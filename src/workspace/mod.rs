//! Workspace management for stilch
//!
//! Each workspace owns its own Space for window management and can be displayed
//! on any virtual output.

pub mod layout;
mod manager;

pub use layout::LayoutTree;
pub use manager::WorkspaceManager;

use crate::shell::WindowElement;
use crate::virtual_output::VirtualOutputId;
use crate::window::WindowId;
use smithay::desktop::Space;
use smithay::utils::{Logical, Rectangle};

/// Represents where a workspace is currently located
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceLocation {
    /// Workspace is not visible on any output
    Hidden {
        /// The output this workspace was last shown on (for workspace affinity)
        last_output: Option<VirtualOutputId>,
    },
    /// Workspace is visible on a specific virtual output
    Visible {
        output: VirtualOutputId,
        area: Rectangle<i32, Logical>,
    },
}

/// Unique identifier for workspaces
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceId(u8);

impl WorkspaceId {
    pub fn new(id: u8) -> Self {
        WorkspaceId(id)
    }

    pub fn get(&self) -> u8 {
        self.0
    }

    /// Get the display name for this workspace
    /// For workspaces 0-9, this follows the i3/sway convention:
    /// - ID 0 → "1"
    /// - ID 1 → "2"
    /// - ...
    /// - ID 8 → "9"
    /// - ID 9 → "10" (or could be "0" for the tenth workspace)
    pub fn display_name(&self) -> String {
        match self.0 {
            0..=8 => (self.0 + 1).to_string(),
            9 => "10".to_string(),          // Or "0" if you prefer
            _ => format!("{}", self.0 + 1), // For future expansion beyond 10 workspaces
        }
    }
}

impl std::fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A workspace that manages windows in a tiled layout
#[derive(Debug)]
pub struct Workspace {
    /// Unique workspace identifier (0-9)
    pub id: WorkspaceId,
    /// Name of the workspace (can be custom or just the number)
    pub name: String,
    /// Current location of the workspace
    pub location: WorkspaceLocation,
    /// Layout tree for tiling
    pub layout: LayoutTree,
    /// Currently focused window
    pub focused_window: Option<WindowId>,
    /// Windows in this workspace (maintains order for focus cycling)
    pub windows: Vec<WindowId>,
    /// Currently fullscreen window (if any)
    pub fullscreen_window: Option<WindowId>,
    /// Current area of the workspace (updated when shown on a virtual output)
    pub area: Rectangle<i32, Logical>,
    /// Next split direction for new windows
    pub next_split: crate::workspace::layout::SplitDirection,
}

impl Workspace {
    /// Create a new workspace with the given ID
    pub fn new(id: u8, gap: i32) -> Self {
        let id = WorkspaceId::new(id);
        let name = id.to_string();

        // Default area will be updated when workspace is shown
        let default_area = Rectangle::new((0, 0).into(), (1920, 1080).into());

        tracing::debug!(
            "Creating new workspace {} with default area {:?} and gap {}",
            id,
            default_area,
            gap
        );

        Self {
            id,
            name,
            location: WorkspaceLocation::Hidden { last_output: None },
            layout: LayoutTree::new(default_area, gap),
            focused_window: None,
            windows: Vec::new(),
            fullscreen_window: None,
            area: default_area,
            next_split: crate::workspace::layout::SplitDirection::Horizontal,
        }
    }

    /// Show the workspace on a virtual output
    pub fn show_on_output(&mut self, output: VirtualOutputId, area: Rectangle<i32, Logical>) {
        tracing::debug!(
            "Showing workspace {} on output {:?} with area {:?}",
            self.id,
            output,
            area
        );
        self.location = WorkspaceLocation::Visible { output, area };
        self.area = area;
        self.layout.set_area(area);
        // Recalculate all window positions
        self.relayout();
    }

    /// Hide the workspace (remove from output)
    pub fn hide(&mut self) {
        tracing::debug!("Hiding workspace {}", self.id);
        // Remember which output we were on
        let last_output = match self.location {
            WorkspaceLocation::Visible { output, .. } => Some(output),
            WorkspaceLocation::Hidden { last_output } => last_output,
        };
        self.location = WorkspaceLocation::Hidden { last_output };
    }

    /// Check if workspace is visible
    pub fn is_visible(&self) -> bool {
        matches!(self.location, WorkspaceLocation::Visible { .. })
    }

    /// Get the output this workspace is on (if visible)
    pub fn output(&self) -> Option<VirtualOutputId> {
        match self.location {
            WorkspaceLocation::Visible { output, .. } => Some(output),
            WorkspaceLocation::Hidden { .. } => None,
        }
    }

    /// Get the output this workspace is associated with (visible or last shown)
    pub fn associated_output(&self) -> Option<VirtualOutputId> {
        match self.location {
            WorkspaceLocation::Visible { output, .. } => Some(output),
            WorkspaceLocation::Hidden { last_output } => last_output,
        }
    }

    /// Set the associated output for this workspace
    pub fn set_associated_output(&mut self, output: Option<VirtualOutputId>) {
        match &mut self.location {
            WorkspaceLocation::Visible { .. } => {
                // If visible, we don't change the association here
                // It will be updated when the workspace is moved
            }
            WorkspaceLocation::Hidden { last_output } => {
                *last_output = output;
            }
        }
    }

    /// Add a window to this workspace
    pub fn add_window(&mut self, window_id: WindowId) {
        if !self.windows.contains(&window_id) {
            tracing::info!(
                "Adding window {} to workspace {}, current area: {:?}",
                window_id,
                self.id,
                self.area
            );
            self.windows.push(window_id);
            self.layout.add_window(window_id, self.next_split);

            // If this is the first window, focus it
            if self.focused_window.is_none() {
                self.focused_window = Some(window_id);
            }

            // Log the geometry after adding
            if let Some(geom) = self.layout.get_window_geometry(window_id) {
                tracing::info!("Window {} geometry after add: {:?}", window_id, geom);
            } else {
                tracing::warn!("Window {} has no geometry after add!", window_id);
            }
        }
    }

    /// Remove a window from this workspace
    pub fn remove_window(&mut self, window_id: WindowId) -> bool {
        if let Some(pos) = self.windows.iter().position(|&id| id == window_id) {
            self.windows.remove(pos);
            self.layout.remove_window(window_id);

            // Update focus if needed
            if self.focused_window == Some(window_id) {
                self.focused_window = self.layout.find_next_focus();
            }

            // Clear fullscreen if it was this window
            if self.fullscreen_window == Some(window_id) {
                self.fullscreen_window = None;
            }

            true
        } else {
            false
        }
    }

    /// Set the focused window
    pub fn set_focus(&mut self, window_id: Option<WindowId>) {
        self.focused_window = window_id;
    }

    /// Set fullscreen window
    pub fn set_fullscreen_window(&mut self, window_id: Option<WindowId>) {
        if let Some(id) = window_id {
            if self.windows.contains(&id) {
                self.fullscreen_window = Some(id);
            }
        } else {
            self.fullscreen_window = None;
        }
    }

    /// Check if workspace is empty
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// Get window count
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Recalculate layout and apply to space
    pub fn relayout(&mut self) {
        self.layout.calculate_geometries();
    }

    /// Apply the current layout to the Space
    pub fn apply_layout_to_space(
        &mut self,
        window_registry: &crate::window::WindowRegistry,
        space: &mut Space<WindowElement>,
    ) {
        // Check if we have a fullscreen window
        tracing::debug!(
            "apply_layout_to_space: workspace {} fullscreen_window: {:?}",
            self.id,
            self.fullscreen_window
        );
        if let Some(fullscreen_id) = self.fullscreen_window {
            if let Some(managed_window) = window_registry.get(fullscreen_id) {
                tracing::debug!(
                    "Fullscreen window {} layout: {:?}",
                    fullscreen_id,
                    managed_window.layout
                );
                if let crate::window::WindowLayout::Fullscreen { mode, .. } = &managed_window.layout
                {
                    match mode {
                        crate::window::FullscreenMode::Container => {
                            // For container fullscreen, the window should fill its container
                            // In most cases (single root container), this means filling the workspace
                            // TODO: If we have multiple top-level containers, find the specific container

                            // For now, use the workspace area (which is what users expect for container fullscreen)
                            if let Some(toplevel) = managed_window.element.0.toplevel() {
                                // Configure the window to fill the workspace
                                toplevel.with_pending_state(|state| {
                                    state.size = Some(self.area.size);
                                    state.bounds = Some(self.area.size);
                                });
                                if toplevel.is_initial_configure_sent() {
                                    toplevel.send_configure();
                                }
                            }
                            // Map the window to fill the entire workspace area
                            space.map_element(managed_window.element.clone(), self.area.loc, true);
                        }
                        crate::window::FullscreenMode::VirtualOutput => {
                            // For virtual output fullscreen, window should fill the entire virtual output
                            // Map at the workspace area location with proper size
                            if let Some(toplevel) = managed_window.element.0.toplevel() {
                                // Ensure the window is sized to fill the area
                                toplevel.with_pending_state(|state| {
                                    state.size = Some(self.area.size);
                                    state.bounds = Some(self.area.size);
                                });
                                if toplevel.is_initial_configure_sent() {
                                    toplevel.send_configure();
                                }
                            }
                            // Map the window to fill the entire workspace area
                            space.map_element(managed_window.element.clone(), self.area.loc, true);
                        }
                        crate::window::FullscreenMode::PhysicalOutput => {
                            // Physical output fullscreen is handled at the state level
                            // The window is mapped directly to the physical output
                            // so we don't map it here to avoid duplicate mapping
                            // Just return early since the window is already positioned
                            return;
                        }
                    }
                } else {
                    // Window is no longer fullscreen, clear the fullscreen_window field
                    self.fullscreen_window = None;
                }

                // Hide other windows when fullscreen
                for (window_id, _geometry) in self.layout.get_all_geometries() {
                    if window_id != fullscreen_id {
                        if let Some(managed_window) = window_registry.get(window_id) {
                            // Move other windows off-screen or unmap them
                            space.unmap_elem(&managed_window.element);
                        }
                    }
                }
            } else {
                // Fullscreen window no longer exists
                self.fullscreen_window = None;
            }
        } else {
            // Normal layout - no fullscreen
            let visible_geometries = self.layout.get_visible_geometries();
            let all_geometries = self.layout.get_all_geometries();

            tracing::debug!(
                "Applying layout for workspace {}: {} visible windows out of {} total",
                self.id,
                visible_geometries.len(),
                all_geometries.len()
            );

            // First, unmap all windows that should be hidden (e.g., inactive tabs)
            for (window_id, _) in &all_geometries {
                if !visible_geometries.iter().any(|(id, _)| id == window_id) {
                    if let Some(managed_window) = window_registry.get(*window_id) {
                        tracing::debug!("Unmapping hidden window {} (inactive tab)", window_id);
                        space.unmap_elem(&managed_window.element);
                    }
                }
            }

            // Then map only the visible windows
            for (window_id, geometry) in visible_geometries {
                if let Some(managed_window) = window_registry.get(window_id) {
                    tracing::debug!("Mapping visible window {} at {:?}", window_id, geometry);

                    // Update window bounds
                    if let Some(toplevel) = managed_window.element.0.toplevel() {
                        toplevel.with_pending_state(|state| {
                            state.bounds = Some(geometry.size);
                            state.size = Some(geometry.size);
                        });
                        // Send configure to notify the client
                        if toplevel.is_initial_configure_sent() {
                            toplevel.send_configure();
                        }
                    }

                    // Map the window element at the calculated position
                    space.map_element(managed_window.element.clone(), geometry.loc, false);
                }
            }
        }
    }

    /// Move a window in the given direction
    pub fn move_window(
        &mut self,
        window_id: WindowId,
        direction: crate::config::Direction,
    ) -> bool {
        if self.windows.contains(&window_id) {
            self.layout.move_window(window_id, direction)
        } else {
            false
        }
    }

    /// Set the next split direction for new windows
    pub fn set_next_split(&mut self, direction: crate::workspace::layout::SplitDirection) {
        self.next_split = direction;
    }
}
