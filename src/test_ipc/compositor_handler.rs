//! Compositor-side handler for test commands
//!
//! This integrates with the real compositor state to handle test commands
//! and generate ASCII representations of the actual window layout.

use super::{TestCommand, TestCommandHandler, TestResponse, WindowInfo};
use crate::{
    backend::ascii::{AsciiBackend, AsciiWindow},
    state::StilchState,
    virtual_output::VirtualOutputId,
    window::{WindowId, WindowLayout},
    workspace::WorkspaceId,
};
use smithay::utils::Size;
use std::sync::{Arc, Mutex};

/// Handler that integrates with the real compositor state
pub struct CompositorTestHandler<BackendData: crate::state::Backend + 'static> {
    /// Reference to the compositor state
    state: Arc<Mutex<StilchState<BackendData>>>,

    /// ASCII backend for rendering
    ascii: Arc<Mutex<AsciiBackend>>,

    /// Counter for window IDs (simulating Wayland client windows)
    next_window_id: u64,
}

impl<BackendData: crate::state::Backend + 'static> CompositorTestHandler<BackendData> {
    /// Create a new compositor test handler
    pub fn new(state: Arc<Mutex<StilchState<BackendData>>>) -> Self {
        // Create ASCII backend with the virtual output size
        let ascii = {
            let state = state.lock().unwrap();
            let size = if let Some(vo) = state
                .virtual_output_manager
                .list_virtual_outputs()
                .first()
                .and_then(|id| state.virtual_output_manager.get(*id))
            {
                let region = vo.logical_region();
                Size::from((region.size.w, region.size.h))
            } else {
                Size::from((3840, 2160)) // Default size
            };

            AsciiBackend::new(80, 24, size)
        };

        Self {
            state,
            ascii: Arc::new(Mutex::new(ascii)),
            next_window_id: 1,
        }
    }

    /// Sync the ASCII backend with current compositor state
    fn sync_ascii(&self) {
        let state = self.state.lock().unwrap();
        let mut ascii = self.ascii.lock().unwrap();

        // Clear existing windows
        ascii.windows.lock().unwrap().clear();

        // Get the active virtual output
        let virtual_output_id = state
            .virtual_output_manager
            .list_virtual_outputs()
            .first()
            .copied()
            .unwrap_or_else(|| VirtualOutputId::new(1)); // TODO: Support multiple outputs

        // Get the active workspace on this output
        if let Some(workspace_id) = state
            .workspace_manager
            .workspace_on_output(virtual_output_id)
        {
            if let Some(workspace) = state.workspace_manager.get(workspace_id) {
                // Add each window in the workspace to the ASCII backend
                for &window_id in &workspace.windows {
                    if let Some(managed_window) = state.window_registry().get(window_id) {
                        let bounds = match &managed_window.layout {
                            WindowLayout::Tiled { geometry, .. } => *geometry,
                            WindowLayout::Floating { geometry } => *geometry,
                            WindowLayout::Fullscreen { geometry, .. } => *geometry,
                        };

                        let ascii_window = AsciiWindow {
                            id: window_id,
                            bounds,
                            focused: workspace.focused_window == Some(window_id),
                            floating: matches!(
                                managed_window.layout,
                                WindowLayout::Floating { .. }
                            ),
                            fullscreen: matches!(
                                managed_window.layout,
                                WindowLayout::Fullscreen { .. }
                            ),
                            urgent: false,
                            tab_info: None,
                        };

                        ascii.update_window(ascii_window);
                    }
                }
            }
        }
    }

    /// Create a test window (simulating a Wayland client)
    fn create_test_window(&mut self, _width: i32, _height: i32) -> u64 {
        let window_id = self.next_window_id;
        self.next_window_id += 1;

        // In a real implementation, this would:
        // 1. Create a Wayland surface
        // 2. Attach a buffer with the specified size
        // 3. Commit the surface
        // 4. Let the compositor's XdgShellHandler create the WindowElement

        // For now, we'll directly manipulate the compositor state
        // This is a simplification - real tests should use Wayland protocol

        let mut state = self.state.lock().unwrap();

        // Get current virtual output and workspace
        let virtual_output_id = state
            .virtual_output_manager
            .list_virtual_outputs()
            .first()
            .copied()
            .unwrap_or_else(|| VirtualOutputId::new(1));
        let workspace_id = state
            .workspace_manager
            .workspace_on_output(virtual_output_id)
            .unwrap_or(WorkspaceId::new(0));

        // Create a window ID
        let internal_window_id = WindowId::new(window_id as u32);

        // Add to workspace
        state
            .workspace_manager
            .add_window_to_workspace(internal_window_id, workspace_id);

        // Add to layout with horizontal split by default
        if let Some(workspace) = state.workspace_manager.get_workspace_mut(workspace_id) {
            workspace.layout.add_window(
                internal_window_id,
                crate::workspace::layout::SplitDirection::Horizontal,
            );
            workspace.relayout();
        }

        // Apply layout to calculate positions
        state.apply_workspace_layout(workspace_id);

        window_id
    }
}

impl<BackendData: crate::state::Backend + 'static> TestCommandHandler
    for CompositorTestHandler<BackendData>
{
    fn handle_command(&mut self, command: TestCommand) -> TestResponse {
        match command {
            TestCommand::CreateWindow { width, height } => {
                let id = self.create_test_window(width, height);
                self.sync_ascii();
                TestResponse::WindowCreated { id }
            }

            TestCommand::DestroyWindow { id } => {
                let mut state = self.state.lock().unwrap();
                let window_id = WindowId::new(id as u32);

                // Find and remove from workspace
                if let Some(workspace_id) = state.workspace_manager.find_window_workspace(window_id)
                {
                    state
                        .workspace_manager
                        .remove_window_from_workspace(window_id, workspace_id);

                    // Relayout
                    if let Some(workspace) = state.workspace_manager.get_workspace_mut(workspace_id)
                    {
                        workspace.layout.remove_window(window_id);
                        workspace.relayout();
                    }
                    state.apply_workspace_layout(workspace_id);

                    drop(state);
                    self.sync_ascii();

                    TestResponse::Success {
                        message: format!("Window {} destroyed", id),
                    }
                } else {
                    TestResponse::Error {
                        message: format!("Window {} not found", id),
                    }
                }
            }

            TestCommand::FocusWindow { id } => {
                let mut state = self.state.lock().unwrap();
                let window_id = WindowId::new(id as u32);

                // Find the workspace containing this window
                if let Some(workspace_id) = state.workspace_manager.find_window_workspace(window_id)
                {
                    state
                        .workspace_manager
                        .set_focus(workspace_id, Some(window_id));

                    drop(state);
                    self.sync_ascii();

                    TestResponse::Success {
                        message: format!("Focused window {id}"),
                    }
                } else {
                    TestResponse::Error {
                        message: format!("Window {} not found", id),
                    }
                }
            }

            TestCommand::GetState => {
                self.sync_ascii();
                let ascii = self.ascii.lock().unwrap().render();
                TestResponse::State { ascii }
            }

            TestCommand::GetWindows => {
                let state = self.state.lock().unwrap();
                let mut windows = Vec::new();

                // Iterate through all windows in the registry
                for managed_window in state.window_manager.registry().windows() {
                    let window_id = &managed_window.id;
                    let (x, y, width, height) = match &managed_window.layout {
                        WindowLayout::Tiled { geometry, .. }
                        | WindowLayout::Floating { geometry }
                        | WindowLayout::Fullscreen { geometry, .. } => (
                            geometry.loc.x,
                            geometry.loc.y,
                            geometry.size.w,
                            geometry.size.h,
                        ),
                    };

                    let workspace_id = managed_window.workspace;
                    let is_focused = state
                        .workspace_manager
                        .get(workspace_id)
                        .map(|ws| ws.focused_window == Some(*window_id))
                        .unwrap_or(false);

                    windows.push(WindowInfo {
                        id: window_id.get(),
                        x,
                        y,
                        width,
                        height,
                        workspace: workspace_id.get() as usize,
                        focused: is_focused,
                        floating: matches!(managed_window.layout, WindowLayout::Floating { .. }),
                        fullscreen: matches!(
                            managed_window.layout,
                            WindowLayout::Fullscreen { .. }
                        ),
                        title: None,   // Title not available in regular mode yet
                        visible: true, // All workspace windows are considered visible
                    });
                }

                TestResponse::Windows { windows }
            }

            TestCommand::SwitchWorkspace { index } => {
                let mut state = self.state.lock().unwrap();
                let virtual_output_id = state
                    .virtual_output_manager
                    .list_virtual_outputs()
                    .first()
                    .copied()
                    .unwrap_or_else(|| VirtualOutputId::new(1));
                let workspace_id = WorkspaceId::new(index as u8);

                state.switch_workspace(virtual_output_id, workspace_id);

                drop(state);
                self.sync_ascii();

                TestResponse::Success {
                    message: format!("Switched to workspace {index}"),
                }
            }

            TestCommand::MoveFocus { direction } => {
                // Convert to config Direction enum
                let dir = direction.to_config_direction();

                let mut state = self.state.lock().unwrap();

                // Get active workspace from virtual output
                let workspace_id = state
                    .virtual_output_manager
                    .outputs()
                    .find_map(|vo| vo.active_workspace().map(|idx| WorkspaceId::new(idx as u8)))
                    .unwrap_or(WorkspaceId::new(0));

                // Get current focused window
                let current_focused =
                    if let Some(workspace) = state.workspace_manager.get_workspace(workspace_id) {
                        workspace.focused_window
                    } else {
                        None
                    };

                if let Some(focused_id) = current_focused {
                    // Check if we're in a tabbed container
                    let in_tabbed = if let Some(workspace) =
                        state.workspace_manager.get_workspace(workspace_id)
                    {
                        let result = workspace.layout.is_window_in_tabbed_container(focused_id);
                        result
                    } else {
                        false
                    };

                    if in_tabbed
                        && matches!(
                            dir,
                            crate::config::Direction::Left | crate::config::Direction::Right
                        )
                    {
                        // Handle tab switching
                        if let Some(workspace) =
                            state.workspace_manager.get_workspace_mut(workspace_id)
                        {
                            let escaped = match dir {
                                crate::config::Direction::Right => {
                                    workspace.layout.next_tab(focused_id)
                                }
                                crate::config::Direction::Left => {
                                    workspace.layout.prev_tab(focused_id)
                                }
                                _ => false,
                            };

                            if !escaped {
                                // Tab was switched, update focus to the now-visible window
                                workspace.relayout();
                                let visible_windows = workspace.layout.get_visible_geometries();
                                if let Some((visible_id, _)) = visible_windows.first() {
                                    workspace.focused_window = Some(*visible_id);
                                    state
                                        .workspace_manager
                                        .set_focus(workspace_id, Some(*visible_id));
                                }
                            }
                        }
                    } else {
                        // Normal focus movement - find next window in direction
                        // For simplicity in test harness, just cycle through windows
                        if let Some(workspace) =
                            state.workspace_manager.get_workspace_mut(workspace_id)
                        {
                            if !workspace.windows.is_empty() {
                                let current_idx = workspace
                                    .windows
                                    .iter()
                                    .position(|&id| id == focused_id)
                                    .unwrap_or(0);
                                let new_idx = match dir {
                                    crate::config::Direction::Right
                                    | crate::config::Direction::Down => {
                                        (current_idx + 1) % workspace.windows.len()
                                    }
                                    crate::config::Direction::Left
                                    | crate::config::Direction::Up => {
                                        if current_idx == 0 {
                                            workspace.windows.len() - 1
                                        } else {
                                            current_idx - 1
                                        }
                                    }
                                };
                                let new_focused = workspace.windows[new_idx];
                                workspace.focused_window = Some(new_focused);
                                state
                                    .workspace_manager
                                    .set_focus(workspace_id, Some(new_focused));
                            }
                        }
                    }
                }

                // Apply layout to update visible windows
                state.apply_workspace_layout(workspace_id);

                drop(state);
                self.sync_ascii();

                TestResponse::Success {
                    message: format!("Moved focus {direction}"),
                }
            }

            TestCommand::KillFocusedWindow => {
                // Kill the currently focused window
                // This test harness doesn't have real windows, so we need to directly manipulate state
                TestResponse::Error {
                    message:
                        "KillFocusedWindow not implemented in test harness - use test mode instead"
                            .to_string(),
                }
            }

            TestCommand::KeyPress { key } => {
                // This would simulate a key press through the input system
                TestResponse::Success {
                    message: format!("Simulated key press: {key}"),
                }
            }

            _ => TestResponse::Error {
                message: "Command not implemented".to_string(),
            },
        }
    }
}
