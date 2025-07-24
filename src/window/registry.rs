//! Window registry for tracking all windows in the compositor

use super::{ManagedWindow, WindowId};
use crate::shell::WindowElement;
use crate::workspace::WorkspaceId;
use smithay::desktop::Window;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::seat::WaylandFocus;
use std::collections::HashMap;

/// Central registry for all windows in the compositor
#[derive(Debug, Default)]
pub struct WindowRegistry {
    /// Map from WindowId to ManagedWindow
    windows: HashMap<WindowId, ManagedWindow>,
    /// Map from WlSurface to WindowId for quick lookups
    surface_to_id: HashMap<WlSurface, WindowId>,
    /// Map from smithay Window to WindowId
    window_to_id: HashMap<Window, WindowId>,
}

impl WindowRegistry {
    /// Create a new empty window registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new window in the registry
    pub fn insert(&mut self, window: ManagedWindow) -> WindowId {
        let id = window.id;

        // Store surface mapping
        if let Some(surface) = window.element.0.wl_surface() {
            self.surface_to_id.insert(surface.into_owned(), id);
        }

        // Store window mapping
        self.window_to_id.insert(window.element.0.clone(), id);

        // Store the managed window
        self.windows.insert(id, window);

        id
    }

    /// Remove a window from the registry
    pub fn remove(&mut self, id: WindowId) -> Option<ManagedWindow> {
        if let Some(window) = self.windows.remove(&id) {
            // Clean up surface mapping
            if let Some(surface) = window.element.0.wl_surface() {
                self.surface_to_id.remove(&surface.into_owned());
            }

            // Clean up window mapping
            self.window_to_id.remove(&window.element.0);

            Some(window)
        } else {
            None
        }
    }

    /// Get a window by ID
    pub fn get(&self, id: WindowId) -> Option<&ManagedWindow> {
        self.windows.get(&id)
    }

    /// Get a mutable window by ID
    pub fn get_mut(&mut self, id: WindowId) -> Option<&mut ManagedWindow> {
        self.windows.get_mut(&id)
    }

    /// Find window ID by surface
    pub fn find_by_surface(&self, surface: &WlSurface) -> Option<WindowId> {
        self.surface_to_id.get(surface).copied()
    }

    /// Find window ID by smithay Window
    pub fn find_by_window(&self, window: &Window) -> Option<WindowId> {
        self.window_to_id.get(window).copied()
    }

    /// Find window ID by WindowElement
    pub fn find_by_element(&self, element: &WindowElement) -> Option<WindowId> {
        self.find_by_window(&element.0)
    }

    /// Get all windows
    pub fn windows(&self) -> impl Iterator<Item = &ManagedWindow> {
        self.windows.values()
    }

    /// Get all windows mutably
    pub fn windows_mut(&mut self) -> impl Iterator<Item = &mut ManagedWindow> {
        self.windows.values_mut()
    }

    /// Get all windows in a specific workspace
    pub fn windows_in_workspace(
        &self,
        workspace: WorkspaceId,
    ) -> impl Iterator<Item = &ManagedWindow> {
        self.windows
            .values()
            .filter(move |w| w.workspace == workspace)
    }

    /// Get all window IDs
    pub fn window_ids(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.windows.keys().copied()
    }

    /// Update workspace for a window
    pub fn set_workspace(&mut self, id: WindowId, workspace: WorkspaceId) -> bool {
        if let Some(window) = self.windows.get_mut(&id) {
            window.workspace = workspace;
            true
        } else {
            false
        }
    }

    /// Get the count of windows
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// Iterate over all windows
    pub fn iter(&self) -> impl Iterator<Item = (WindowId, &ManagedWindow)> {
        self.windows.iter().map(|(id, window)| (*id, window))
    }

    /// Clear all windows from the registry
    pub fn clear(&mut self) {
        self.windows.clear();
        self.surface_to_id.clear();
        self.window_to_id.clear();
    }
}
