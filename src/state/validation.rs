//! State validation module for ensuring consistency across the compositor
//!
//! This module provides validation to ensure that:
//! - Windows reference valid workspaces
//! - Workspaces contain only windows that reference them back
//! - Virtual outputs show valid workspaces
//! - No orphaned references exist

use crate::{
    virtual_output::VirtualOutputManager,
    window::{ManagedWindow, WindowId},
    workspace::{WorkspaceId, WorkspaceManager},
};
use std::collections::{HashMap, HashSet};

/// Errors that can occur during state validation
#[derive(Debug)]
pub enum ValidationError {
    /// Window references a workspace that doesn't exist
    WindowReferencesInvalidWorkspace {
        window: WindowId,
        workspace: WorkspaceId,
    },
    /// Workspace contains a window that doesn't reference it back
    WorkspaceContainsOrphanWindow {
        workspace: WorkspaceId,
        window: WindowId,
    },
    /// Window exists in workspace but not in registry
    WindowInWorkspaceButNotInRegistry {
        workspace: WorkspaceId,
        window: WindowId,
    },
    /// Window exists in registry but not in any workspace
    WindowInRegistryButNotInWorkspace {
        window: WindowId,
        claimed_workspace: WorkspaceId,
    },
    /// Virtual output shows a non-existent workspace
    VirtualOutputShowsInvalidWorkspace {
        output: crate::virtual_output::VirtualOutputId,
        workspace: WorkspaceId,
    },
    /// Multiple workspaces claim the same window
    WindowInMultipleWorkspaces {
        window: WindowId,
        workspaces: Vec<WorkspaceId>,
    },
}

/// Result type for validation operations
pub type ValidationResult = Result<(), Vec<ValidationError>>;

/// Trait for types that can validate their internal consistency
pub trait ValidateConsistency {
    /// Validate internal consistency, returning errors if any invariants are violated
    fn validate_consistency(&self) -> ValidationResult;
}

/// Validate workspace-window bidirectional consistency
pub fn validate_workspace_window_consistency(
    workspace_manager: &WorkspaceManager,
    windows: &[ManagedWindow],
) -> ValidationResult {
    let mut errors = Vec::new();

    // Build a map of window IDs to their claimed workspace
    let window_workspace_map: HashMap<WindowId, WorkspaceId> =
        windows.iter().map(|w| (w.id, w.workspace)).collect();

    // Track which windows we've seen in workspaces
    let mut windows_in_workspaces = HashSet::new();
    let mut window_workspace_locations: HashMap<WindowId, Vec<WorkspaceId>> = HashMap::new();

    // Check each workspace
    for workspace_id in 0..10u8 {
        let workspace_id = WorkspaceId::new(workspace_id);

        if let Some(workspace) = workspace_manager.get(workspace_id) {
            // Check each window in the workspace
            for &window_id in &workspace.windows {
                // Track that we've seen this window
                windows_in_workspaces.insert(window_id);

                // Track which workspaces contain this window
                window_workspace_locations
                    .entry(window_id)
                    .or_insert_with(Vec::new)
                    .push(workspace_id);

                // Check if window exists in registry
                if let Some(&claimed_workspace) = window_workspace_map.get(&window_id) {
                    // Window exists in registry - check it claims this workspace
                    if claimed_workspace != workspace_id {
                        errors.push(ValidationError::WorkspaceContainsOrphanWindow {
                            workspace: workspace_id,
                            window: window_id,
                        });
                    }
                } else {
                    // Window doesn't exist in registry at all
                    errors.push(ValidationError::WindowInWorkspaceButNotInRegistry {
                        workspace: workspace_id,
                        window: window_id,
                    });
                }
            }
        }
    }

    // Check for windows in multiple workspaces
    for (window_id, workspaces) in window_workspace_locations {
        if workspaces.len() > 1 {
            errors.push(ValidationError::WindowInMultipleWorkspaces {
                window: window_id,
                workspaces,
            });
        }
    }

    // Check that all windows in registry are in their claimed workspace
    for window in windows {
        if !windows_in_workspaces.contains(&window.id) {
            errors.push(ValidationError::WindowInRegistryButNotInWorkspace {
                window: window.id,
                claimed_workspace: window.workspace,
            });
        }

        // Also check that the claimed workspace exists
        if workspace_manager.get(window.workspace).is_none() {
            errors.push(ValidationError::WindowReferencesInvalidWorkspace {
                window: window.id,
                workspace: window.workspace,
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Validate virtual output consistency
pub fn validate_virtual_output_consistency(
    virtual_output_manager: &VirtualOutputManager,
    workspace_manager: &WorkspaceManager,
) -> ValidationResult {
    let mut errors = Vec::new();

    for output in virtual_output_manager.outputs() {
        if let Some(workspace_idx) = output.active_workspace() {
            let workspace_id = WorkspaceId::new(workspace_idx as u8);

            // Check that the workspace exists
            if workspace_manager.get(workspace_id).is_none() {
                errors.push(ValidationError::VirtualOutputShowsInvalidWorkspace {
                    output: output.id(),
                    workspace: workspace_id,
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Full state validation combining all consistency checks
pub fn validate_full_state<BackendData: crate::state::Backend + 'static>(
    state: &crate::state::StilchState<BackendData>,
) -> ValidationResult {
    let mut all_errors = Vec::new();

    // Get all windows from the registry
    let windows: Vec<_> = state.window_registry().windows().cloned().collect();

    // Validate workspace-window consistency
    if let Err(errors) = validate_workspace_window_consistency(&state.workspace_manager, &windows) {
        all_errors.extend(errors);
    }

    // Validate virtual output consistency
    if let Err(errors) =
        validate_virtual_output_consistency(&state.virtual_output_manager, &state.workspace_manager)
    {
        all_errors.extend(errors);
    }

    if all_errors.is_empty() {
        Ok(())
    } else {
        Err(all_errors)
    }
}

#[cfg(debug_assertions)]
/// Debug helper to log validation errors
pub fn log_validation_errors(errors: &[ValidationError]) {
    for error in errors {
        tracing::error!("State validation error: {:?}", error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_display() {
        let error = ValidationError::WindowReferencesInvalidWorkspace {
            window: WindowId::new(1),
            workspace: WorkspaceId::new(5),
        };

        // Just ensure it can be formatted
        let _ = format!("{:?}", error);
    }
}
