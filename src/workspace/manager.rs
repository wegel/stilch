//! Workspace manager that owns all workspaces

use super::{Workspace, WorkspaceId};
use crate::virtual_output::VirtualOutputId;
use crate::window::WindowId;
use smithay::utils::{Logical, Rectangle};

/// Manages all workspaces in the compositor
#[derive(Debug)]
pub struct WorkspaceManager {
    /// All workspaces (0-9 by default)
    workspaces: Vec<Workspace>,
}

impl WorkspaceManager {
    /// Create a new workspace manager with 10 workspaces (0-9)
    pub fn new(gap: i32) -> Self {
        let workspaces = (0..10).map(|i| Workspace::new(i, gap)).collect();

        Self { workspaces }
    }

    /// Get a workspace by ID
    pub fn get(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|ws| ws.id == id)
    }

    /// Get a mutable workspace by ID
    pub fn get_mut(&mut self, id: WorkspaceId) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|ws| ws.id == id)
    }

    /// Get all workspaces
    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    /// Get the workspace currently on a virtual output
    pub fn workspace_on_output(&self, output_id: VirtualOutputId) -> Option<WorkspaceId> {
        self.workspaces
            .iter()
            .find(|ws| ws.output() == Some(output_id))
            .map(|ws| ws.id)
    }

    /// Get the virtual output a workspace is currently on
    pub fn workspace_location(&self, workspace_id: WorkspaceId) -> Option<VirtualOutputId> {
        self.get(workspace_id).and_then(|ws| ws.output())
    }

    /// Get the virtual output a workspace is associated with (visible or last shown)
    pub fn workspace_association(&self, workspace_id: WorkspaceId) -> Option<VirtualOutputId> {
        self.get(workspace_id).and_then(|ws| ws.associated_output())
    }

    /// Show a workspace on a virtual output
    pub fn show_workspace_on_output(
        &mut self,
        workspace_id: WorkspaceId,
        output_id: VirtualOutputId,
        output_area: Rectangle<i32, Logical>,
    ) -> Result<(), ShowWorkspaceError> {
        tracing::debug!(
            "Showing workspace {} on output {} with area {:?}",
            workspace_id,
            output_id,
            output_area
        );

        // Check if workspace exists
        if !self.workspaces.iter().any(|ws| ws.id == workspace_id) {
            return Err(ShowWorkspaceError::WorkspaceNotFound);
        }

        // Hide any workspace currently on the target output
        for ws in &mut self.workspaces {
            if ws.output() == Some(output_id) {
                ws.hide();
            }
        }

        // Show the workspace on the output
        if let Some(workspace) = self.get_mut(workspace_id) {
            workspace.show_on_output(output_id, output_area);
        }

        Ok(())
    }

    /// Hide a workspace (remove from its output)
    pub fn hide_workspace(&mut self, workspace_id: WorkspaceId) {
        if let Some(workspace) = self.get_mut(workspace_id) {
            workspace.hide();
        }
    }

    /// Add a window to a workspace
    pub fn add_window_to_workspace(
        &mut self,
        window_id: WindowId,
        workspace_id: WorkspaceId,
    ) -> bool {
        if let Some(workspace) = self.get_mut(workspace_id) {
            workspace.add_window(window_id);
            true
        } else {
            false
        }
    }

    /// Remove a window from a workspace
    pub fn remove_window_from_workspace(
        &mut self,
        window_id: WindowId,
        workspace_id: WorkspaceId,
    ) -> bool {
        if let Some(workspace) = self.get_mut(workspace_id) {
            workspace.remove_window(window_id)
        } else {
            false
        }
    }

    /// Move a window between workspaces
    pub fn move_window(&mut self, window_id: WindowId, from: WorkspaceId, to: WorkspaceId) -> bool {
        // Remove from source workspace
        if !self.remove_window_from_workspace(window_id, from) {
            return false;
        }

        // Add to destination workspace
        self.add_window_to_workspace(window_id, to)
    }

    /// Move a window within its workspace in a given direction
    pub fn move_window_in_workspace(
        &mut self,
        window_id: WindowId,
        workspace_id: WorkspaceId,
        direction: crate::config::Direction,
    ) -> bool {
        if let Some(workspace) = self.get_mut(workspace_id) {
            workspace.move_window(window_id, direction)
        } else {
            false
        }
    }

    /// Set focus on a workspace
    pub fn set_focus(&mut self, workspace_id: WorkspaceId, window_id: Option<WindowId>) {
        if let Some(workspace) = self.get_mut(workspace_id) {
            workspace.set_focus(window_id);
        }
    }

    /// Find which workspace a window is on
    pub fn find_window_workspace(&self, window_id: WindowId) -> Option<WorkspaceId> {
        self.workspaces
            .iter()
            .find(|ws| ws.windows.contains(&window_id))
            .map(|ws| ws.id)
    }

    /// Get workspace statistics
    pub fn workspace_stats(&self) -> Vec<WorkspaceStats> {
        self.workspaces
            .iter()
            .map(|ws| WorkspaceStats {
                id: ws.id,
                window_count: ws.window_count(),
                is_visible: ws.is_visible(),
                on_output: ws.output(),
                has_focus: ws.focused_window.is_some(),
            })
            .collect()
    }

    /// Find which virtual output a workspace is currently located on
    pub fn find_workspace_location(&self, workspace_id: WorkspaceId) -> Option<VirtualOutputId> {
        self.workspace_location(workspace_id)
    }

    /// Associate a workspace with a specific output (for move workspace to output)
    pub fn associate_workspace_with_output(
        &mut self,
        workspace_id: WorkspaceId,
        output_id: VirtualOutputId,
    ) {
        if let Some(workspace) = self.get_mut(workspace_id) {
            workspace.set_associated_output(Some(output_id));
        }
    }

    /// Get a workspace by ID (alias for better naming)
    pub fn get_workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.get(id)
    }

    /// Get a mutable workspace by ID (alias for better naming)
    pub fn get_workspace_mut(&mut self, id: WorkspaceId) -> Option<&mut Workspace> {
        self.get_mut(id)
    }
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new(10) // Default gap of 10 pixels
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceStats {
    pub id: WorkspaceId,
    pub window_count: usize,
    pub is_visible: bool,
    pub on_output: Option<VirtualOutputId>,
    pub has_focus: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowWorkspaceError {
    WorkspaceNotFound,
}
