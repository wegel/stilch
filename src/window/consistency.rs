//! Window state consistency checks
//!
//! Debug-mode checks to ensure window state remains consistent across
//! the multiple places windows are tracked.

use crate::{shell::WindowElement, window::WindowRegistry, workspace::WorkspaceManager};
use smithay::desktop::Space;
use tracing::error;

/// Check consistency of window state across all tracking systems
#[cfg(debug_assertions)]
pub fn check_consistency(
    registry: &WindowRegistry,
    space: &Space<WindowElement>,
    workspace_manager: &WorkspaceManager,
) -> bool {
    let mut is_consistent = true;

    // Check 1: Every window in Space has a Registry entry
    for element in space.elements() {
        let found = registry.iter().any(|(_, w)| &w.element == element);
        if !found {
            error!("Window in Space but not in Registry");
            is_consistent = false;
        }
    }

    // Check 2: Every window in Registry has correct workspace association
    for (window_id, managed_window) in registry.iter() {
        let workspace_id = managed_window.workspace;
        if let Some(workspace) = workspace_manager.get_workspace(workspace_id) {
            if !workspace.windows.contains(&window_id) {
                error!(
                    "Window {} claims workspace {} but workspace doesn't list it",
                    window_id, workspace_id
                );
                is_consistent = false;
            }
        } else {
            error!(
                "Window {} references non-existent workspace {}",
                window_id, workspace_id
            );
            is_consistent = false;
        }
    }

    // Check 3: Every workspace window exists in Registry
    for workspace in workspace_manager.workspaces() {
        for window_id in &workspace.windows {
            if registry.get(*window_id).is_none() {
                error!(
                    "Workspace {} lists window {} but it's not in Registry",
                    workspace.id, window_id
                );
                is_consistent = false;
            }
        }

        // Check 4: Focused window exists
        if let Some(focused_id) = workspace.focused_window {
            if !workspace.windows.contains(&focused_id) {
                error!(
                    "Workspace {} focused window {} not in window list",
                    workspace.id, focused_id
                );
                is_consistent = false;
            }
        }

        // Check 5: Fullscreen window exists and is actually fullscreen
        if let Some(fullscreen_id) = workspace.fullscreen_window {
            if !workspace.windows.contains(&fullscreen_id) {
                error!(
                    "Workspace {} fullscreen window {} not in window list",
                    workspace.id, fullscreen_id
                );
                is_consistent = false;
            }
            if let Some(window) = registry.get(fullscreen_id) {
                if !window.is_fullscreen() {
                    error!(
                        "Workspace {} claims window {} is fullscreen but window state disagrees",
                        workspace.id, fullscreen_id
                    );
                    is_consistent = false;
                }
            }
        }
    }

    if !is_consistent {
        error!("Window state consistency check FAILED");
    }

    is_consistent
}

/// No-op in release builds
#[cfg(not(debug_assertions))]
pub fn check_consistency(
    _registry: &WindowRegistry,
    _space: &Space<WindowElement>,
    _workspace_manager: &WorkspaceManager,
) -> bool {
    true
}
