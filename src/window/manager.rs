//! Window Manager
//!
//! This module consolidates all window management operations including:
//! - Window registry for tracking window metadata
//! - Smithay Space for spatial window management
//! - Popup management
//! - Unified window operations (add, remove, move, resize, etc.)

use smithay::{
    desktop::{PopupManager, Space},
    output::Output,
    utils::{Logical, Point, Rectangle},
};
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::{
    event::WindowEvent,
    shell::WindowElement,
    virtual_output::VirtualOutputId,
    window::{ManagedWindow, WindowId, WindowLayout, WindowRegistry},
    workspace::WorkspaceId,
};

/// Unified window manager combining registry, space, and popup management
#[derive(Debug)]
pub struct WindowManager {
    /// Registry for window metadata
    registry: WindowRegistry,
    /// Smithay space for spatial operations
    pub(crate) space: Space<WindowElement>,
    /// Popup manager
    pub(crate) popups: PopupManager,
    /// Set of windows that have been moved since last frame
    moved_windows: std::collections::HashSet<WindowId>,
}

impl WindowManager {
    /// Create a new window manager
    pub fn new() -> Self {
        Self {
            registry: WindowRegistry::new(),
            space: Space::default(),
            popups: PopupManager::default(),
            moved_windows: std::collections::HashSet::new(),
        }
    }

    /// Get a reference to the space
    pub fn space(&self) -> &Space<WindowElement> {
        &self.space
    }

    /// Get a mutable reference to the space
    pub fn space_mut(&mut self) -> &mut Space<WindowElement> {
        &mut self.space
    }

    /// Get a reference to the popup manager
    pub fn popups(&self) -> &PopupManager {
        &self.popups
    }

    /// Get a mutable reference to the popup manager
    pub fn popups_mut(&mut self) -> &mut PopupManager {
        &mut self.popups
    }

    /// Get a reference to the window registry
    pub fn registry(&self) -> &WindowRegistry {
        &self.registry
    }

    /// Get a mutable reference to the window registry
    pub fn registry_mut(&mut self) -> &mut WindowRegistry {
        &mut self.registry
    }

    /// Add a window to the manager
    /// Returns the window ID and a window created event
    pub fn add_window(
        &mut self,
        window: WindowElement,
        _virtual_output_id: VirtualOutputId,
        workspace: WorkspaceId,
        position: Point<i32, Logical>,
    ) -> Option<(WindowId, WindowEvent)> {
        // Create a managed window
        let managed_window = ManagedWindow::new(window.clone(), workspace);
        let window_id = managed_window.id;

        // Add to registry
        self.registry.insert(managed_window);

        // Map in space (single source of truth for position)
        self.space_mut().map_element(window.clone(), position, true);

        info!(
            "Added window {} to manager at position {:?}",
            window_id, position
        );

        // Create window created event
        let event = WindowEvent::Created {
            window_id,
            workspace,
            initial_position: position,
            timestamp: Instant::now(),
        };

        Some((window_id, event))
    }

    /// Remove a window from the manager
    /// Returns success and optionally a window destroyed event
    pub fn remove_window(&mut self, window_id: WindowId) -> (bool, Option<WindowEvent>) {
        if let Some(managed_window) = self.registry.remove(window_id) {
            let workspace = managed_window.workspace;

            // Unmap from space
            self.space_mut().unmap_elem(&managed_window.element);

            // Remove from moved set
            self.moved_windows.remove(&window_id);

            info!("Removed window {} from manager", window_id);

            // Create window destroyed event
            let event = WindowEvent::Destroyed {
                window_id,
                workspace,
                timestamp: Instant::now(),
            };

            return (true, Some(event));
        }

        warn!("Failed to remove window {} - not found", window_id);
        (false, None)
    }

    /// Move a window to a new position
    /// Returns an optional window moved event if the position changed
    pub fn move_window(
        &mut self,
        window_id: WindowId,
        position: Point<i32, Logical>,
    ) -> Option<WindowEvent> {
        let managed_window = self.registry.get(window_id)?;
        let window_element = managed_window.element.clone();

        // Get current position from Space (single source of truth)
        let old_position = self.space.element_location(&window_element);

        // Check if position actually changed
        if old_position == Some(position) {
            debug!(
                "Window {} already at position {:?}, skipping update",
                window_id, position
            );
            return None;
        }

        // Update position in Space
        self.space_mut().map_element(window_element, position, true);
        self.moved_windows.insert(window_id);

        debug!(
            "Moved window {} from {:?} to {:?}",
            window_id, old_position, position
        );

        // Create window moved event
        old_position.map(|old_pos| WindowEvent::Moved {
            window_id,
            old_position: old_pos,
            new_position: position,
            timestamp: Instant::now(),
        })
    }

    /// Get the position of a window
    pub fn window_position(&self, window_id: WindowId) -> Option<Point<i32, Logical>> {
        self.registry
            .get(window_id)
            .and_then(|mw| self.space().element_location(&mw.element))
    }

    /// Resize a window
    pub fn resize_window(&mut self, window_id: WindowId, size: Rectangle<i32, Logical>) {
        if let Some(managed_window) = self.registry.get_mut(window_id) {
            // Update the ManagedWindow's internal geometry
            match &mut managed_window.layout {
                WindowLayout::Tiled {
                    ref mut geometry, ..
                } => *geometry = size,
                WindowLayout::Floating { ref mut geometry } => *geometry = size,
                WindowLayout::Fullscreen {
                    ref mut geometry, ..
                } => *geometry = size,
            }

            let window_element = &managed_window.element;
            // Handle resize through the window element
            if let Some(toplevel) = window_element.0.toplevel() {
                // XDG windows handle resize through configure events
                toplevel.with_pending_state(|state| {
                    state.size = Some(size.size);
                });
                toplevel.send_pending_configure();
            }
            #[cfg(feature = "xwayland")]
            {
                if let Some(surface) = window_element.0.x11_surface() {
                    // X11 windows can be resized directly
                    let _ = surface.configure(size);
                }
            }
            debug!("Resized window {} to {:?}", window_id, size);
        } else {
            warn!("Cannot resize window {} - not found", window_id);
        }
    }

    /// Set window fullscreen state
    pub fn set_fullscreen(&mut self, window_id: WindowId, fullscreen: bool, output: &Output) {
        // Get the output geometry before mutable borrow
        let output_geo = self
            .space()
            .output_geometry(output)
            .unwrap_or_else(|| Rectangle::from_size((1920, 1080).into()));

        if let Some(managed) = self.registry.get_mut(window_id) {
            if fullscreen && !managed.is_fullscreen() {
                // Enter fullscreen - save current layout (must not be fullscreen)
                if let Some(previous_layout) = managed.layout.as_non_fullscreen() {
                    managed.layout = WindowLayout::Fullscreen {
                        mode: crate::window::FullscreenMode::PhysicalOutput,
                        geometry: output_geo,
                        previous: Box::new(previous_layout),
                    };
                }
            } else if !fullscreen && managed.is_fullscreen() {
                // Exit fullscreen - restore previous layout
                if let WindowLayout::Fullscreen { previous, .. } = &managed.layout {
                    managed.layout = previous.clone().into_layout();
                }
            }

            let window_element = &managed.element;
            if let Some(toplevel) = window_element.0.toplevel() {
                // XDG windows handle fullscreen through configure events
                toplevel.with_pending_state(|state| {
                    if fullscreen {
                        state.states.set(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Fullscreen);
                        state.size = Some(output_geo.size);
                    } else {
                        state.states.unset(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Fullscreen);
                        state.size = None;
                    }
                });
                toplevel.send_pending_configure();
            }
            #[cfg(feature = "xwayland")]
            {
                if let Some(surface) = window_element.0.x11_surface() {
                    let _ = surface.set_fullscreen(fullscreen);
                    if fullscreen {
                        let _ = surface.configure(output_geo);
                    }
                }
            }
        }
    }

    /// Update window workspace assignment
    pub fn set_window_workspace(&mut self, window_id: WindowId, workspace: WorkspaceId) {
        if let Some(managed) = self.registry.get_mut(window_id) {
            managed.workspace = workspace;
            debug!("Moved window {} to workspace {}", window_id, workspace);
        }
    }

    /// Get all windows on a specific workspace
    pub fn windows_on_workspace(&self, workspace: WorkspaceId) -> Vec<WindowId> {
        self.registry
            .windows_in_workspace(workspace)
            .map(|w| w.id)
            .collect()
    }

    /// Get all windows on a specific virtual output
    pub fn windows_on_output(&self, _output_id: VirtualOutputId) -> Vec<WindowId> {
        // For now, return empty vec since we don't track virtual output in ManagedWindow
        // This would need to be implemented by checking workspace locations
        Vec::new()
    }

    /// Refresh space (delegate to space)
    pub fn refresh(&mut self) {
        self.space_mut().refresh();
    }

    /// Map output (delegate to space)
    pub fn map_output(&mut self, output: &Output, location: Point<i32, Logical>) {
        self.space_mut().map_output(output, location);
    }

    /// Unmap output (delegate to space)
    pub fn unmap_output(&mut self, output: &Output) {
        self.space_mut().unmap_output(output);
    }

    /// Update the position of a window element directly (for grab operations)
    /// This should be used when the window is being dragged/resized
    pub fn update_element_position(
        &mut self,
        element: &WindowElement,
        position: Point<i32, Logical>,
    ) {
        // Check if position actually changed
        if self.space.element_location(element) == Some(position) {
            return;
        }

        // Update space directly
        self.space_mut()
            .map_element(element.clone(), position, true);

        // Mark as moved if in registry
        if let Some(window_id) = self.registry.find_by_element(element) {
            self.moved_windows.insert(window_id);
            debug!(
                "Updated element position for window {} to {:?}",
                window_id, position
            );
        }
    }

    /// Clear moved flag for a window
    pub fn clear_moved(&mut self, window_id: WindowId) {
        self.moved_windows.remove(&window_id);
    }

    /// Clear all moved flags
    pub fn clear_all_moved(&mut self) {
        self.moved_windows.clear();
    }

    /// Get list of windows that have moved
    pub fn moved_windows(&self) -> Vec<WindowId> {
        self.moved_windows.iter().cloned().collect()
    }

    /// Check if a window has moved
    pub fn has_moved(&self, window_id: WindowId) -> bool {
        self.moved_windows.contains(&window_id)
    }

    /// Batch update window positions
    /// This is more efficient than individual updates when multiple windows change
    /// Returns a vector of window moved events for positions that changed
    pub fn batch_update_positions(
        &mut self,
        updates: Vec<(WindowId, Point<i32, Logical>)>,
    ) -> Vec<WindowEvent> {
        let mut events = Vec::new();

        for (window_id, position) in updates {
            if let Some(managed_window) = self.registry.get(window_id) {
                let window_element = managed_window.element.clone();

                // Get current position from Space
                let old_position = self.space.element_location(&window_element);

                // Skip if already at position
                if old_position == Some(position) {
                    continue;
                }

                // Update position in Space
                self.space_mut().map_element(window_element, position, true);
                self.moved_windows.insert(window_id);

                // Create window moved event if we had an old position
                if let Some(old_pos) = old_position {
                    events.push(WindowEvent::Moved {
                        window_id,
                        old_position: old_pos,
                        new_position: position,
                        timestamp: Instant::now(),
                    });
                }
            }
        }

        events
    }

    /// Mark windows that have moved as needing updates
    /// This is much simpler without a cache - we just track what moved
    pub fn mark_moved_windows(&mut self) {
        // This method is now mostly a no-op since we track moves as they happen
        // Could be used to detect external position changes if needed
        self.space.refresh();
    }
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}
