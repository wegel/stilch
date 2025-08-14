//! Test mode for the compositor
//!
//! When run with --test, the compositor starts with an ASCII backend
//! and an IPC server that accepts commands from test clients.

use smithay::{
    backend::renderer::pixman::PixmanRenderer,
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, Mode as CallMode, PostAction},
        wayland_server::Display,
    },
    utils::{Logical, Point, Rectangle, Size},
};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{error, info};

use crate::{
    backend::ascii::AsciiBackend,
    state::{Backend as BackendTrait, StilchState},
};

pub struct TestBackendData {
    pub ascii_backend: Arc<Mutex<AsciiBackend>>,
    pub renderer: PixmanRenderer,
    seat_name: String,
}

impl BackendTrait for TestBackendData {
    fn seat_name(&self) -> String {
        self.seat_name.clone()
    }

    fn reset_buffers(&mut self, _output: &Output) {
        // No real buffers in test mode
    }

    fn early_import(
        &mut self,
        _surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        // No import needed for test mode
    }

    fn update_led_state(&mut self, _led_state: smithay::input::keyboard::LedState) {
        // No LEDs in test mode
    }
}

// NO MOCK WINDOWS - WE USE REAL WINDOWS ONLY

/// Per-connection state for IPC clients
struct ClientConnection {
    stream: UnixStream,
    buffer: String,
    ascii_backend: Arc<Mutex<AsciiBackend>>,
}

impl ClientConnection {
    fn new(stream: UnixStream, ascii_backend: Arc<Mutex<AsciiBackend>>) -> Self {
        Self {
            stream,
            buffer: String::new(),
            ascii_backend,
        }
    }
}

/// Simple IPC handler that processes commands directly in the main thread
struct TestIpcHandler {
    listener: UnixListener,
    #[allow(dead_code)]
    ascii_backend: Arc<Mutex<AsciiBackend>>,
}

impl TestIpcHandler {
    fn new(
        socket_path: &PathBuf,
        ascii_backend: Arc<Mutex<AsciiBackend>>,
    ) -> std::io::Result<Self> {
        // Remove old socket if it exists
        let _ = std::fs::remove_file(socket_path);

        let listener = UnixListener::bind(socket_path)?;
        listener.set_nonblocking(true)?;

        Ok(Self {
            listener,
            ascii_backend,
        })
    }

    fn process_client_data<BackendData: BackendTrait + 'static>(
        connection: &mut ClientConnection,
        state: &mut StilchState<BackendData>,
    ) -> std::io::Result<()> {
        use std::io::{Read, Write};

        // Read available data into buffer
        let mut temp_buffer = [0u8; 4096];
        let mut read_something = false;

        loop {
            match connection.stream.read(&mut temp_buffer) {
                Ok(0) if !read_something => {
                    // Connection closed
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "Connection closed",
                    ));
                }
                Ok(0) => {
                    // No more data right now, but we already read something
                    break;
                }
                Ok(n) => {
                    // Add data to buffer
                    connection
                        .buffer
                        .push_str(&String::from_utf8_lossy(&temp_buffer[..n]));
                    read_something = true;

                    // Check if we have complete command(s) to process
                    if connection.buffer.contains('\n') {
                        break; // Process what we have
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No more data available right now
                    break;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        // Process all complete lines in the buffer
        while let Some(newline_pos) = connection.buffer.find('\n') {
            let line = connection.buffer.drain(..=newline_pos).collect::<String>();
            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            // Parse and process command
            let command: crate::test_ipc::TestCommand = match serde_json::from_str(line) {
                Ok(cmd) => cmd,
                Err(e) => {
                    eprintln!("Failed to parse command: {e}");
                    let error_response = crate::test_ipc::TestResponse::Error {
                        message: format!("Failed to parse command: {e}"),
                    };
                    let response_json = serde_json::to_string(&error_response).unwrap();
                    writeln!(connection.stream, "{}", response_json)?;
                    connection.stream.flush()?;
                    continue;
                }
            };

            // Process command and generate response
            let response = match command {
                crate::test_ipc::TestCommand::GetState => {
                    // Sync ASCII backend with REAL windows from the compositor state
                    let mut ascii = connection.ascii_backend.lock().unwrap();

                    // Clear existing windows in ASCII
                    let window_ids: Vec<_> =
                        ascii.windows.lock().unwrap().keys().cloned().collect();
                    for id in window_ids {
                        ascii.remove_window(id);
                    }

                    // Get REAL windows from the window registry
                    let window_count = state.window_manager.registry().len();
                    info!("Window registry has {} windows", window_count);

                    for managed_window in state.window_manager.registry().windows() {
                        let window_id = managed_window.id;

                        // Get the actual geometry from the window's layout
                        let geometry = match &managed_window.layout {
                            crate::window::WindowLayout::Tiled { geometry, .. } => geometry,
                            crate::window::WindowLayout::Floating { geometry } => geometry,
                            crate::window::WindowLayout::Fullscreen { geometry, .. } => geometry,
                        };

                        info!("Window {} geometry: {:?}", window_id, geometry);

                        let is_focused = state
                            .focused_window()
                            .map(|w| {
                                // Check if this window element matches the focused one
                                state.window_manager.registry().find_by_element(&w)
                                    == Some(window_id)
                            })
                            .unwrap_or(false);

                        let is_floating = matches!(
                            &managed_window.layout,
                            crate::window::WindowLayout::Floating { .. }
                        );
                        let is_fullscreen = matches!(
                            &managed_window.layout,
                            crate::window::WindowLayout::Fullscreen { .. }
                        );

                        ascii.update_window(crate::backend::ascii::AsciiWindow {
                            id: window_id,
                            bounds: *geometry,
                            focused: is_focused,
                            floating: is_floating,
                            fullscreen: is_fullscreen,
                            urgent: false,
                            tab_info: None,
                        });
                    }

                    // Render and return ASCII state
                    crate::test_ipc::TestResponse::State {
                        ascii: ascii.render(),
                    }
                }

                crate::test_ipc::TestCommand::CreateWindow { .. } => {
                    // We don't create windows via IPC - real applications must create real Wayland windows
                    crate::test_ipc::TestResponse::Error {
                        message: "Windows must be created by real Wayland clients. Launch a real application.".to_string(),
                    }
                }

                crate::test_ipc::TestCommand::FocusWindow { id } => {
                    // Focus a real window by ID
                    let window_id = crate::window::WindowId::new(id as u32);

                    // Find the window element for this ID
                    if let Some(managed_window) = state.window_manager.registry().get(window_id) {
                        // Use the compositor's actual focus_window method
                        let window_element = managed_window.element.clone();
                        info!("Focusing window {id}");
                        state.focus_window(&window_element);

                        crate::test_ipc::TestResponse::Success {
                            message: format!("Focused window {id}"),
                        }
                    } else {
                        crate::test_ipc::TestResponse::Error {
                            message: format!("Window {} not found", id),
                        }
                    }
                }

                crate::test_ipc::TestCommand::GetFocusedWindow => {
                    // Get the currently focused window
                    let focused_id = state
                        .focused_window()
                        .and_then(|w| state.window_manager.registry().find_by_element(&w))
                        .map(|id| id.get() as u64);

                    crate::test_ipc::TestResponse::FocusedWindow { id: focused_id }
                }

                crate::test_ipc::TestCommand::GetWorkspaces => {
                    // Get all workspaces and their state
                    let workspaces: Vec<_> = (0..10)
                        .map(|i| {
                            let workspace_id = crate::workspace::WorkspaceId::new(i);
                            let workspace = state.workspace_manager.get_workspace(workspace_id);

                            let (visible, output, window_count) = if let Some(ws) = workspace {
                                let visible = ws.is_visible();
                                let output = ws.output().map(|o| format!("output-{}", o.get()));
                                let window_count = ws.window_count();
                                (visible, output, window_count)
                            } else {
                                (false, None, 0)
                            };

                            // Check if this workspace is focused
                            let focused = state
                                .virtual_output_manager
                                .all_virtual_outputs()
                                .any(|vo| vo.active_workspace() == Some(i as usize));

                            let workspace_id = crate::workspace::WorkspaceId::new(i);
                            crate::test_ipc::WorkspaceInfo {
                                id: workspace_id
                                    .display_name()
                                    .parse()
                                    .unwrap_or(i as usize + 1),
                                name: workspace_id.display_name(),
                                visible,
                                output,
                                window_count,
                                focused,
                            }
                        })
                        .collect();

                    crate::test_ipc::TestResponse::Workspaces { workspaces }
                }

                crate::test_ipc::TestCommand::GetWindows => {
                    // Get REAL windows from the registry
                    info!(
                        "GetWindows: Registry has {} windows",
                        state.window_manager.registry().len()
                    );
                    let windows: Vec<_> = state
                        .window_manager
                        .registry()
                        .windows()
                        .map(|managed_window| {
                            let window_id = managed_window.id.get();
                            let geometry = match &managed_window.layout {
                                crate::window::WindowLayout::Tiled { geometry, .. } => geometry,
                                crate::window::WindowLayout::Floating { geometry } => geometry,
                                crate::window::WindowLayout::Fullscreen { geometry, .. } => {
                                    geometry
                                }
                            };

                            let is_focused = state
                                .focused_window()
                                .map(|w| {
                                    state.window_manager.registry().find_by_element(&w)
                                        == Some(managed_window.id)
                                })
                                .unwrap_or(false);

                            let is_floating = matches!(
                                &managed_window.layout,
                                crate::window::WindowLayout::Floating { .. }
                            );
                            let is_fullscreen = matches!(
                                &managed_window.layout,
                                crate::window::WindowLayout::Fullscreen { .. }
                            );

                            // Get window title from the window element if available
                            let title = managed_window.element.0.toplevel().and_then(|toplevel| {
                                use smithay::wayland::compositor::with_states;
                                use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
                                let surface = toplevel.wl_surface();
                                with_states(&surface, |states| {
                                    states.data_map.get::<XdgToplevelSurfaceData>().and_then(
                                        |data| {
                                            let data = data.lock().unwrap();
                                            data.app_id.clone()
                                        },
                                    )
                                })
                            });

                            // Check if window is visible (mapped in space)
                            let is_visible = state.space().elements().any(|elem| {
                                state.window_manager.registry().find_by_element(elem)
                                    == Some(managed_window.id)
                            });

                            crate::test_ipc::WindowInfo {
                                id: window_id,
                                x: geometry.loc.x,
                                y: geometry.loc.y,
                                width: geometry.size.w,
                                height: geometry.size.h,
                                workspace: managed_window
                                    .workspace
                                    .display_name()
                                    .parse()
                                    .unwrap_or(managed_window.workspace.get() as usize + 1),
                                focused: is_focused,
                                floating: is_floating,
                                fullscreen: is_fullscreen,
                                title,
                                visible: is_visible,
                            }
                        })
                        .collect();

                    crate::test_ipc::TestResponse::Windows { windows }
                }

                crate::test_ipc::TestCommand::SwitchWorkspace { index } => {
                    // Switch to the specified workspace
                    if index < 10 {
                        // Find the first virtual output (we typically have one in test mode)
                        let vo_id = state
                            .virtual_output_manager
                            .all_virtual_outputs()
                            .next()
                            .map(|vo| vo.id());

                        if let Some(vo_id) = vo_id {
                            let workspace_id = crate::workspace::WorkspaceId::new(index as u8);
                            state.switch_workspace(vo_id, workspace_id);

                            crate::test_ipc::TestResponse::Success {
                                message: format!("Switched to workspace {index}"),
                            }
                        } else {
                            crate::test_ipc::TestResponse::Error {
                                message: "No virtual output available".to_string(),
                            }
                        }
                    } else {
                        crate::test_ipc::TestResponse::Error {
                            message: format!("Invalid workspace index: {index}"),
                        }
                    }
                }

                crate::test_ipc::TestCommand::MoveFocus { direction } => {
                    // Implement focus movement using the same logic as keybindings
                    let dir = direction.to_config_direction();

                    // Use the same KeyAction that would be triggered by a keybinding
                    use crate::keybindings::KeyAction;
                    state.handle_key_action(KeyAction::Focus(dir));

                    crate::test_ipc::TestResponse::Success {
                        message: format!("Moved focus {direction}"),
                    }
                }

                crate::test_ipc::TestCommand::ClickAt { x, y } => {
                    // Simulate a pointer click at the given location
                    use smithay::{
                        input::pointer::ButtonEvent,
                        reexports::wayland_server::protocol::wl_pointer,
                        utils::{Logical, Point, SERIAL_COUNTER},
                    };

                    // First, move the pointer to the location
                    let location = Point::<f64, Logical>::from((x as f64, y as f64));
                    info!("ClickAt: Moving pointer to ({}, {})", x, y);
                    state.pointer().set_location(location);

                    // Check what's under the pointer
                    let under = state.surface_under(location);
                    info!("ClickAt: Surface under pointer: {:?}", under.is_some());

                    // Update keyboard focus based on the click location
                    let serial = SERIAL_COUNTER.next_serial();
                    info!("ClickAt: Calling update_keyboard_focus");
                    state.update_keyboard_focus(location, serial);

                    // Also simulate the button event for any grab handlers
                    let pointer = state.pointer().clone();
                    pointer.button(
                        state,
                        &ButtonEvent {
                            button: 0x110, // BTN_LEFT
                            state: wl_pointer::ButtonState::Pressed.try_into().unwrap(),
                            serial,
                            time: 0,
                        },
                    );
                    pointer.frame(state);

                    // Release
                    let serial = SERIAL_COUNTER.next_serial();
                    pointer.button(
                        state,
                        &ButtonEvent {
                            button: 0x110, // BTN_LEFT
                            state: wl_pointer::ButtonState::Released.try_into().unwrap(),
                            serial,
                            time: 1,
                        },
                    );
                    pointer.frame(state);

                    // Check focus after click
                    let focused = state.focused_window();
                    info!(
                        "ClickAt: Focused window after click: {:?}",
                        focused.is_some()
                    );

                    crate::test_ipc::TestResponse::Success {
                        message: format!("Clicked at ({}, {})", x, y),
                    }
                }

                crate::test_ipc::TestCommand::KillFocusedWindow => {
                    // Kill the currently focused window (same as Super+Q)
                    // In test mode, we need to actually remove the window from the workspace
                    // since there's no real XDG toplevel to be destroyed

                    tracing::info!("KillFocusedWindow command received");

                    if let Some(focused) = state.focused_window() {
                        tracing::info!("Found focused window");
                        if let Some(window_id) = state.window_registry().find_by_element(&focused) {
                            if let Some(managed_window) = state.window_registry().get(window_id) {
                                let workspace_id = managed_window.workspace;

                                // Remove from workspace (this also removes from layout tree)
                                if let Some(workspace) =
                                    state.workspace_manager.get_workspace_mut(workspace_id)
                                {
                                    workspace.remove_window(window_id);

                                    // Find next window to focus
                                    let next_focus = workspace.layout.find_next_focus();
                                    workspace.focused_window = next_focus;

                                    // Relayout
                                    workspace.relayout();
                                }

                                // Remove from window registry
                                state.window_registry_mut().remove(window_id);

                                // Unmap from space
                                state.space_mut().unmap_elem(&focused);

                                // Apply the workspace layout
                                state.apply_workspace_layout(workspace_id);

                                // Focus next window if available
                                if let Some(workspace) =
                                    state.workspace_manager.get_workspace(workspace_id)
                                {
                                    if let Some(next_id) = workspace.focused_window {
                                        let next_element = state
                                            .window_registry()
                                            .get(next_id)
                                            .map(|w| w.element.clone());
                                        if let Some(element) = next_element {
                                            state.focus_window(&element);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    crate::test_ipc::TestResponse::Success {
                        message: "Killed focused window".to_string(),
                    }
                }

                crate::test_ipc::TestCommand::MoveWindow { id, direction } => {
                    // Move a specific window in a direction
                    let window_id = crate::window::WindowId::new(id as u32);
                    let dir = direction.to_config_direction();

                    // Get the window's workspace
                    if let Some(managed_window) = state.window_manager.registry().get(window_id) {
                        let workspace_id = managed_window.workspace;
                        if state.workspace_manager.move_window_in_workspace(
                            window_id,
                            workspace_id,
                            dir,
                        ) {
                            state.apply_workspace_layout(workspace_id);
                            crate::test_ipc::TestResponse::Success {
                                message: format!("Moved window {} {}", id, direction),
                            }
                        } else {
                            crate::test_ipc::TestResponse::Error {
                                message: format!("Failed to move window {} {}", id, direction),
                            }
                        }
                    } else {
                        crate::test_ipc::TestResponse::Error {
                            message: format!("Window {} not found", id),
                        }
                    }
                }

                crate::test_ipc::TestCommand::SetSplitDirection { direction } => {
                    // Set the split direction for the next window
                    let split_dir = direction.to_layout_split();

                    // Get the current workspace and set its next split direction
                    let workspace_info = state
                        .virtual_output_manager
                        .all_virtual_outputs()
                        .next()
                        .and_then(|vo| vo.active_workspace().map(|idx| idx as u8));

                    if let Some(workspace_idx) = workspace_info {
                        let workspace_id = crate::workspace::WorkspaceId::new(workspace_idx);
                        if let Some(workspace) =
                            state.workspace_manager.get_workspace_mut(workspace_id)
                        {
                            workspace.set_next_split(split_dir);
                            crate::test_ipc::TestResponse::Success {
                                message: format!("Set split direction to {direction}"),
                            }
                        } else {
                            crate::test_ipc::TestResponse::Error {
                                message: "Workspace not found".to_string(),
                            }
                        }
                    } else {
                        crate::test_ipc::TestResponse::Error {
                            message: "No active workspace".to_string(),
                        }
                    }
                }

                crate::test_ipc::TestCommand::SetLayout { mode } => {
                    // Handle layout command
                    let layout_cmd = mode.to_layout_command();

                    if let Some(layout_cmd) = layout_cmd {
                        state.handle_layout_command(layout_cmd);
                        crate::test_ipc::TestResponse::Success {
                            message: format!("Set layout to {mode}"),
                        }
                    } else {
                        crate::test_ipc::TestResponse::Error {
                            message: format!("Unknown layout mode: {mode}"),
                        }
                    }
                }

                crate::test_ipc::TestCommand::Fullscreen => {
                    state.toggle_fullscreen(crate::window::FullscreenMode::VirtualOutput);
                    crate::test_ipc::TestResponse::Success {
                        message: "Toggled fullscreen (virtual output)".to_string(),
                    }
                }

                crate::test_ipc::TestCommand::FullscreenContainer => {
                    println!("TEST: FullscreenContainer command received");
                    state.toggle_fullscreen(crate::window::FullscreenMode::Container);
                    println!("TEST: toggle_fullscreen(Container) called");
                    crate::test_ipc::TestResponse::Success {
                        message: "Toggled container fullscreen".to_string(),
                    }
                }

                crate::test_ipc::TestCommand::FullscreenVirtualOutput => {
                    state.toggle_fullscreen(crate::window::FullscreenMode::VirtualOutput);
                    crate::test_ipc::TestResponse::Success {
                        message: "Toggled virtual output fullscreen".to_string(),
                    }
                }

                crate::test_ipc::TestCommand::FullscreenPhysicalOutput => {
                    state.toggle_fullscreen(crate::window::FullscreenMode::PhysicalOutput);
                    crate::test_ipc::TestResponse::Success {
                        message: "Toggled physical output fullscreen".to_string(),
                    }
                }

                crate::test_ipc::TestCommand::GetAsciiSnapshot {
                    show_ids,
                    show_focus,
                } => {
                    // Get ASCII snapshot with options
                    let mut ascii = connection.ascii_backend.lock().unwrap();

                    // Sync with current state first
                    let window_ids: Vec<_> =
                        ascii.windows.lock().unwrap().keys().cloned().collect();
                    for id in window_ids {
                        ascii.remove_window(id);
                    }

                    // Get windows from ALL outputs' active workspaces
                    for output in state.virtual_output_manager.outputs() {
                        if let Some(workspace_id) = output.active_workspace_id() {
                            if let Some(workspace) =
                                state.workspace_manager.get_workspace(workspace_id)
                            {
                                // Get the geometries from the layout tree (only visible windows)
                                let visible_geometries = workspace.layout.get_visible_geometries();

                                for (window_id, geometry) in visible_geometries {
                                    if let Some(managed_window) =
                                        state.window_manager.registry().get(window_id)
                                    {
                                        let is_focused = state
                                            .focused_window()
                                            .map(|w| {
                                                state.window_manager.registry().find_by_element(&w)
                                                    == Some(window_id)
                                            })
                                            .unwrap_or(false);

                                        let is_floating = matches!(
                                            &managed_window.layout,
                                            crate::window::WindowLayout::Floating { .. }
                                        );
                                        let is_fullscreen = matches!(
                                            &managed_window.layout,
                                            crate::window::WindowLayout::Fullscreen { .. }
                                        );

                                        ascii.update_window(crate::backend::ascii::AsciiWindow {
                                            id: window_id,
                                            bounds: geometry,
                                            focused: is_focused && show_focus,
                                            floating: is_floating,
                                            fullscreen: is_fullscreen,
                                            urgent: false,
                                            tab_info: None, // TODO: Detect tab containers from layout tree
                                        });
                                    }
                                }
                            }
                        }
                    }

                    let snapshot = if show_ids {
                        ascii.render() // IDs are shown in the render
                    } else {
                        ascii.render() // TODO: Add option to hide IDs
                    };

                    crate::test_ipc::TestResponse::AsciiSnapshot {
                        snapshot,
                        width: ascii.width(),
                        height: ascii.height(),
                    }
                }

                crate::test_ipc::TestCommand::MoveWindowToWorkspace {
                    window_id,
                    workspace,
                } => {
                    // Use the compositor's actual method for moving windows between workspaces
                    let window = crate::window::WindowId::new(window_id as u32);
                    let target_workspace = crate::workspace::WorkspaceId::new(workspace as u8);

                    // Check if window exists first
                    if state.window_manager.registry().get(window).is_some() {
                        state.move_window_to_workspace_by_id(window, target_workspace);

                        crate::test_ipc::TestResponse::Success {
                            message: format!(
                                "Moved window {} to workspace {}",
                                window_id, workspace
                            ),
                        }
                    } else {
                        crate::test_ipc::TestResponse::Error {
                            message: format!("Window {} not found", window_id),
                        }
                    }
                }

                crate::test_ipc::TestCommand::MoveFocusedWindowToWorkspace { workspace } => {
                    // Move the focused window to another workspace
                    let target_workspace = crate::workspace::WorkspaceId::new(workspace as u8);

                    // Get the focused window
                    if let Some(focused_element) = state.focused_window() {
                        if let Some(window_id) = state
                            .window_manager
                            .registry()
                            .find_by_element(&focused_element)
                        {
                            // This is what happens when user presses Shift+Super+<number>
                            state.move_window_to_workspace_by_id(window_id, target_workspace);

                            crate::test_ipc::TestResponse::Success {
                                message: format!("Moved focused window to workspace {workspace}"),
                            }
                        } else {
                            crate::test_ipc::TestResponse::Error {
                                message: "Could not find focused window ID".to_string(),
                            }
                        }
                    } else {
                        crate::test_ipc::TestResponse::Error {
                            message: "No window is focused".to_string(),
                        }
                    }
                }

                crate::test_ipc::TestCommand::DestroyWindow { id } => {
                    // Close a window by ID - use the SAME method as the real compositor
                    info!("DestroyWindow command for window {id}");
                    let window_id = crate::window::WindowId::new(id as u32);

                    // Find the window element for this ID
                    if let Some(managed_window) = state.window_manager.registry().get(window_id) {
                        let window_elem = managed_window.element.clone();
                        info!("Found window {} in registry", id);

                        // Use the SAME close_window method as keybindings
                        state.close_window(&window_elem);

                        // Flush the display to ensure the close event is sent
                        let _ = state.display_handle.flush_clients();

                        crate::test_ipc::TestResponse::Success {
                            message: format!("Sent close request to window {id}"),
                        }
                    } else {
                        crate::test_ipc::TestResponse::Error {
                            message: format!("Window {} not found", id),
                        }
                    }
                }

                crate::test_ipc::TestCommand::GetOutputs => {
                    // Get all virtual outputs
                    let outputs: Vec<_> = state
                        .virtual_output_manager
                        .all_virtual_outputs()
                        .map(|vo| {
                            let geometry = vo.logical_region();
                            crate::test_ipc::OutputInfo {
                                id: vo.id().get() as u64,
                                x: geometry.loc.x,
                                y: geometry.loc.y,
                                width: geometry.size.w,
                                height: geometry.size.h,
                                name: format!("Virtual-{}", vo.id().get()),
                            }
                        })
                        .collect();

                    crate::test_ipc::TestResponse::Outputs { outputs }
                }

                crate::test_ipc::TestCommand::MoveWorkspaceToOutput { direction } => {
                    // Parse direction
                    let dir = direction.to_config_direction();

                    // Use the compositor's move_workspace_to_output method
                    state.move_workspace_to_output(dir);
                    crate::test_ipc::TestResponse::Success {
                        message: format!("Moved workspace to output {direction}"),
                    }
                }

                crate::test_ipc::TestCommand::MoveMouse { x, y } => {
                    // Move pointer to position
                    use smithay::utils::{Logical, Point};

                    let target = Point::<f64, Logical>::from((x as f64, y as f64));

                    // If physical layout manager is active, let it handle the movement
                    // Otherwise use clamping
                    let final_position = if state.physical_layout.is_some() {
                        info!("Physical layout manager is active, not clamping");
                        // Don't clamp - let the physical layout manager handle boundaries
                        target
                    } else {
                        info!("No physical layout manager, using clamping");
                        // Apply the same clamping logic as production code
                        state.clamp_pointer_location(target)
                    };

                    // Set the pointer location
                    state.pointer().set_location(final_position);

                    // Update physical layout manager if present
                    if let Some(ref mut physical_layout) = state.physical_layout {
                        physical_layout.set_logical_position(final_position);
                    }

                    info!(
                        "MoveMouse: Requested ({}, {}), positioned at ({}, {})",
                        x, y, final_position.x, final_position.y
                    );

                    crate::test_ipc::TestResponse::Success {
                        message: format!(
                            "Moved mouse to ({}, {})",
                            final_position.x as i32, final_position.y as i32
                        ),
                    }
                }

                crate::test_ipc::TestCommand::GetCursorPosition => {
                    // Get current cursor position
                    let location = state.pointer().current_location();
                    info!(
                        "GetCursorPosition: Current location is ({}, {})",
                        location.x, location.y
                    );

                    // Return as a Success response with the position in the message
                    // The test can parse this
                    crate::test_ipc::TestResponse::Success {
                        message: serde_json::to_string(&serde_json::json!({
                            "data": {
                                "x": location.x,
                                "y": location.y
                            }
                        }))
                        .unwrap(),
                    }
                }

                _ => {
                    info!("Unhandled test command: {:?}", command);
                    crate::test_ipc::TestResponse::Error {
                        message: "Command not implemented".to_string(),
                    }
                }
            };

            // Send response
            let response_json = serde_json::to_string(&response).unwrap();
            writeln!(connection.stream, "{}", response_json)?;
            connection.stream.flush()?;
        }

        Ok(())
    }
}

/// Configuration for test mode
pub struct TestModeConfig {
    /// Width of the ASCII grid in characters
    pub ascii_width: usize,
    /// Height of the ASCII grid in characters  
    pub ascii_height: usize,
    /// Logical width of the virtual output
    pub logical_width: i32,
    /// Logical height of the virtual output
    pub logical_height: i32,
    /// Additional outputs to create (for multi-output testing)
    pub additional_outputs: Vec<Rectangle<i32, Logical>>,
}

impl Default for TestModeConfig {
    fn default() -> Self {
        Self {
            ascii_width: 160,
            ascii_height: 90,
            logical_width: 3840,
            logical_height: 2160,
            additional_outputs: Vec::new(),
        }
    }
}

/// Run the compositor in test mode with ASCII backend
pub fn run_test_mode(config: TestModeConfig) {
    // Allow overriding the socket path via environment variable
    let socket_path =
        std::env::var("STILCH_TEST_SOCKET").unwrap_or_else(|_| "/tmp/stilch-test.sock".to_string());

    info!("Starting compositor in test mode with ASCII backend...");

    info!(
        "ASCII grid: {}x{}, Logical space: {}x{}",
        config.ascii_width, config.ascii_height, config.logical_width, config.logical_height
    );
    info!("Test socket: {socket_path}");
    info!("Use test_client to send commands\n");

    // Create event loop and display
    let mut event_loop = EventLoop::try_new().unwrap();
    let display = Display::new().unwrap();
    let display_handle = display.handle();

    // Create a virtual output for the ASCII backend
    let output_name = "ascii";
    let output = Output::new(
        output_name.to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Test".to_string(),
            model: "ASCII".to_string(),
        },
    );

    // Set mode based on config
    let logical_size = Size::from((config.logical_width, config.logical_height));
    let physical_size = Size::from((config.logical_width, config.logical_height)); // For test mode, physical = logical
    let mode = Mode {
        size: physical_size,
        refresh: 60_000,
    };
    output.change_current_state(Some(mode), None, None, None);
    output.set_preferred(mode);
    output.create_global::<StilchState<TestBackendData>>(&display_handle);

    // Create ASCII backend with configurable dimensions
    let ascii_backend = Arc::new(Mutex::new(AsciiBackend::new(
        config.ascii_width,
        config.ascii_height,
        logical_size,
    )));

    // Create a pixman renderer for software rendering
    // This provides the minimal rendering capabilities that Wayland clients need
    let renderer = PixmanRenderer::new().expect("Failed to create pixman renderer");

    // Create test backend data
    let backend_data = TestBackendData {
        ascii_backend: ascii_backend.clone(),
        renderer,
        seat_name: "test-seat".to_string(),
    };

    // Initialize compositor state
    // IMPORTANT: Pass true to listen on Wayland socket so real apps can connect!
    let mut state = StilchState::init(display, event_loop.handle(), backend_data, true);

    // CRITICAL: Update SHM formats - pixman supports standard formats
    // This tells clients which buffer formats are supported
    use smithay::reexports::wayland_server::protocol::wl_shm::Format;
    state.protocols.shm_state.update_formats(vec![
        Format::Argb8888,
        Format::Xrgb8888,
        Format::Rgb888,
        Format::Bgr888,
        Format::Rgba8888,
        Format::Bgra8888,
    ]);

    // Apply scale from config if available (same as udev backend)
    let scale = state
        .config
        .outputs
        .iter()
        .find(|o| o.name == output_name || o.name == "*")
        .and_then(|o| o.scale)
        .unwrap_or(1.0);

    if scale != 1.0 {
        info!("Applying scale {} to test output '{}'", scale, output_name);
        output.change_current_state(
            None, // Keep existing mode
            None, // Keep existing transform
            Some(smithay::output::Scale::Fractional(scale)),
            None, // Keep existing location
        );
    }

    // Map output to space
    state.space_mut().map_output(&output, (0, 0));

    // Check if there are virtual output configs for this output
    let output_name = "ascii";
    let mut handled_by_virtual_config = false;

    // Clone the config to avoid borrow issues
    let virtual_configs = state.config.virtual_outputs.clone();
    for virtual_config in &virtual_configs {
        if virtual_config.outputs.contains(&output_name.to_string()) {
            info!(
                "Output {} is part of virtual output config '{}'",
                output_name, virtual_config.name
            );
            handled_by_virtual_config = true;

            // Convert region config to Rectangle if specified
            let scale_factor = 1.0; // Test mode uses 1:1 scale
            let region = if let Some(region_config) = &virtual_config.region {
                // Region is specified in physical pixels, convert to logical
                Rectangle::new(
                    Point::from((
                        (region_config.x as f64 / scale_factor) as i32,
                        (region_config.y as f64 / scale_factor) as i32,
                    )),
                    Size::from((
                        (region_config.width as f64 / scale_factor) as i32,
                        (region_config.height as f64 / scale_factor) as i32,
                    )),
                )
            } else {
                // Use full output
                Rectangle::from_size(logical_size)
            };

            // Create virtual output with the specified region
            let vo_id = state
                .virtual_output_manager
                .create_from_physical(output.clone(), region);
            state.initialize_virtual_output(vo_id);
            info!(
                "Created virtual output {} '{}' with region {:?}",
                vo_id, virtual_config.name, region
            );
        }
    }

    // If no virtual configs, create default virtual output
    if !handled_by_virtual_config {
        // Adjust logical size based on scale
        let scaled_logical_size = if scale != 1.0 {
            let scaled_width = (logical_size.w as f64 / scale) as i32;
            let scaled_height = (logical_size.h as f64 / scale) as i32;
            Size::from((scaled_width, scaled_height))
        } else {
            logical_size
        };
        info!(
            "Creating default virtual output with logical size {:?} (scale {})",
            scaled_logical_size, scale
        );

        let output_geometry = Rectangle::from_size(scaled_logical_size);
        let virtual_output_id = state
            .virtual_output_manager
            .create_from_physical(output.clone(), output_geometry);
        state.initialize_virtual_output(virtual_output_id);

        // Initialize workspace on the first virtual output
        let workspace_id = crate::workspace::WorkspaceId::new(0); // First workspace is index 0
        if let Some(vo) = state.virtual_output_manager.get(virtual_output_id) {
            let area = vo.logical_region();
            info!("Virtual output logical region: {:?}", area);
            if let Err(e) = state.workspace_manager.show_workspace_on_output(
                workspace_id,
                virtual_output_id,
                area,
            ) {
                error!("Failed to initialize workspace on virtual output: {:?}", e);
            } else {
                info!(
                    "Successfully showed workspace {} on virtual output {} with area {:?}",
                    workspace_id, virtual_output_id, area
                );
                state
                    .virtual_output_manager
                    .set_active_workspace(virtual_output_id, 0); // First workspace is index 0
            }
        } else {
            error!("Virtual output {} not found!", virtual_output_id);
        }
    }

    // Create additional outputs if configured
    for (i, output_rect) in config.additional_outputs.iter().enumerate() {
        // Create a new physical output for testing
        let additional_output = Output::new(
            format!("TEST-{}", i + 2), // TEST-2, TEST-3, etc.
            PhysicalProperties {
                size: (output_rect.size.w, output_rect.size.h).into(),
                subpixel: smithay::output::Subpixel::Unknown,
                make: "Test".to_string(),
                model: format!("ASCII-{}", i + 2),
            },
        );

        let mode = smithay::output::Mode {
            size: (output_rect.size.w, output_rect.size.h).into(),
            refresh: 60_000,
        };

        additional_output.change_current_state(Some(mode), None, None, None);
        additional_output.set_preferred(mode);
        additional_output.create_global::<StilchState<TestBackendData>>(&display_handle);

        // Map the additional output in space
        state
            .space_mut()
            .map_output(&additional_output, (output_rect.loc.x, output_rect.loc.y));

        // Create virtual output for this physical output
        let vo_id = state
            .virtual_output_manager
            .create_from_physical(additional_output.clone(), *output_rect);
        state.initialize_virtual_output(vo_id);

        info!(
            "Created additional virtual output {} at {:?}",
            vo_id, output_rect
        );

        // Update the ASCII backend to know about the full extent
        if let Ok(mut ascii) = ascii_backend.lock() {
            let total_width = output_rect.loc.x + output_rect.size.w;
            let total_height = output_rect.loc.y + output_rect.size.h;
            ascii.update_total_size(total_width, total_height);
        }
    }

    // Initialize physical layout if configured
    // First collect all the physical display configurations
    let mut physical_displays = Vec::new();
    {
        let outputs = state.space().outputs().cloned().collect::<Vec<_>>();
        info!(
            "Checking {} outputs for physical layout configuration",
            outputs.len()
        );
        for output in outputs {
            let output_name = output.name();
            info!(
                "Checking output '{}' for physical layout config",
                output_name
            );

            // Check if this output has physical layout configuration
            if let Some(output_config) = state.config.outputs.iter().find(|o| o.name == output_name)
            {
                info!("Found config for output '{}'", output_name);
                if let (Some(physical_size_mm), Some(physical_position_mm)) = (
                    output_config.physical_size_mm,
                    output_config.physical_position_mm,
                ) {
                    // Get output properties
                    let current_mode =
                        output
                            .current_mode()
                            .unwrap_or_else(|| smithay::output::Mode {
                                size: (1920, 1080).into(),
                                refresh: 60_000,
                            });
                    let scale = output.current_scale().fractional_scale();
                    let transform = output.current_transform();
                    let position = state
                        .space()
                        .output_geometry(&output)
                        .map(|g| g.loc)
                        .unwrap_or_else(|| smithay::utils::Point::from((0, 0)));
                    let logical_size = current_mode.size.to_f64().to_logical(scale).to_i32_round();

                    // Create PhysicalDisplay entry for this output
                    let physical_display = crate::physical_layout::PhysicalDisplay {
                        name: output_name.clone(),
                        pixel_size: current_mode.size.into(),
                        physical_size_mm: smithay::utils::Size::from((
                            physical_size_mm.0,
                            physical_size_mm.1,
                        )),
                        physical_position_mm: smithay::utils::Point::from((
                            physical_position_mm.0,
                            physical_position_mm.1,
                        )),
                        scale,
                        transform,
                        logical_position: position,
                        logical_size,
                    };

                    info!(
                        "Adding test output '{}' to physical layout: {}x{}mm at ({}, {})mm, scale {}",
                        output_name,
                        physical_size_mm.0, physical_size_mm.1,
                        physical_position_mm.0, physical_position_mm.1,
                        scale
                    );

                    physical_displays.push(physical_display);
                }
            } else {
                info!("No config found for output '{}'", output_name);
            }
        }
    }

    // Now add all the displays to the physical layout manager
    if !physical_displays.is_empty() {
        // Initialize physical layout manager if not already done
        if state.physical_layout.is_none() {
            state.physical_layout = Some(crate::physical_layout::PhysicalLayoutManager::new());
            info!("Initialized PhysicalLayoutManager for test mode");
        }

        if let Some(ref mut physical_layout) = state.physical_layout {
            for display in physical_displays {
                physical_layout.add_display(display);
            }
        }
    }

    state.update_tiling_area_from_output();

    if let Err(e) = state.init_ipc_server() {
        error!("Failed to initialize IPC server: {e}");
    }

    // Create test IPC handler for ASCII commands
    let socket_path_buf = PathBuf::from(&socket_path);
    let ipc_handler = match TestIpcHandler::new(&socket_path_buf, ascii_backend.clone()) {
        Ok(handler) => handler,
        Err(e) => {
            error!("Failed to create IPC handler: {e}");
            return;
        }
    };

    info!("Test IPC server listening on {:?}", socket_path);

    // Add IPC listener to event loop
    let listener = ipc_handler.listener;
    let ascii_for_source = ascii_backend.clone();

    // Track active connections - we'll register each as its own event source
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex as StdMutex};
    let next_conn_id = Arc::new(StdMutex::new(0usize));
    let active_connections: Arc<StdMutex<HashMap<usize, ClientConnection>>> =
        Arc::new(StdMutex::new(HashMap::new()));

    let next_id_for_listener = next_conn_id.clone();
    let connections_for_listener = active_connections.clone();
    let handle_for_listener = event_loop.handle();

    // Add listener source for accepting new connections
    event_loop
        .handle()
        .insert_source(
            Generic::new(listener, Interest::READ, CallMode::Level),
            move |_, listener, _state: &mut StilchState<TestBackendData>| {
                // Try to accept new connections
                while let Ok((stream, _)) = listener.accept() {
                    info!("Test client connected");
                    stream.set_nonblocking(true).unwrap();

                    // Get a unique ID for this connection
                    let conn_id = {
                        let mut id = next_id_for_listener.lock().unwrap();
                        let current = *id;
                        *id += 1;
                        current
                    };

                    // Create the connection object
                    let connection = ClientConnection::new(
                        stream.try_clone().unwrap(),
                        ascii_for_source.clone(),
                    );
                    connections_for_listener
                        .lock()
                        .unwrap()
                        .insert(conn_id, connection);

                    // Register this stream as an event source
                    let conn_for_source = active_connections.clone();
                    handle_for_listener
                        .insert_source(
                            Generic::new(stream, Interest::READ, CallMode::Level),
                            move |_, _stream, state: &mut StilchState<TestBackendData>| {
                                let mut connections = conn_for_source.lock().unwrap();
                                if let Some(connection) = connections.get_mut(&conn_id) {
                                    match TestIpcHandler::process_client_data(connection, state) {
                                        Ok(_) => Ok(PostAction::Continue),
                                        Err(e) => {
                                            if e.kind() != std::io::ErrorKind::UnexpectedEof
                                                && e.kind() != std::io::ErrorKind::BrokenPipe
                                            {
                                                error!("Error handling test client: {e}");
                                            }
                                            connections.remove(&conn_id);
                                            Ok(PostAction::Remove)
                                        }
                                    }
                                } else {
                                    Ok(PostAction::Remove)
                                }
                            },
                        )
                        .expect("Failed to add client connection to event loop");
                }
                Ok(PostAction::Continue)
            },
        )
        .expect("Failed to add IPC listener to event loop");

    // Main event loop
    info!("Test mode compositor initialized, starting event loop...");

    loop {
        // Process events
        let timeout = Some(std::time::Duration::from_millis(16)); // ~60 FPS
        let result = event_loop.dispatch(timeout, &mut state);

        if let Err(e) = result {
            error!("Event loop error: {e}");
            break;
        }

        // Flush any pending client events
        let _ = state.display_handle.flush_clients();

        // Check if we should exit
        if !state.running.load(std::sync::atomic::Ordering::SeqCst) {
            info!("Shutting down test mode compositor...");
            break;
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);
    info!("Test mode compositor shut down");
}
