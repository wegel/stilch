//! Type-safe workspace state
//!
//! This module provides types that make invalid workspace states unrepresentable.

use crate::window::WindowId;

/// Represents the window state of a workspace
#[derive(Debug, Clone)]
pub enum WorkspaceWindows {
    /// No windows in the workspace
    Empty,
    /// One or more windows with guaranteed valid focus
    WithWindows {
        /// All windows in the workspace (maintains order)
        windows: Vec<WindowId>,
        /// Index of the focused window (guaranteed valid)
        focused_index: usize,
        /// Index of fullscreen window if any
        fullscreen_index: Option<usize>,
    }
}

impl WorkspaceWindows {
    /// Create an empty workspace
    pub fn new() -> Self {
        WorkspaceWindows::Empty
    }

    /// Add a window to the workspace
    pub fn add_window(&mut self, window_id: WindowId) {
        match self {
            WorkspaceWindows::Empty => {
                *self = WorkspaceWindows::WithWindows {
                    windows: vec![window_id],
                    focused_index: 0,
                    fullscreen_index: None,
                };
            }
            WorkspaceWindows::WithWindows { windows, focused_index, .. } => {
                if !windows.contains(&window_id) {
                    windows.push(window_id);
                    // New window becomes focused
                    *focused_index = windows.len() - 1;
                }
            }
        }
    }

    /// Remove a window from the workspace
    pub fn remove_window(&mut self, window_id: WindowId) -> bool {
        match self {
            WorkspaceWindows::Empty => false,
            WorkspaceWindows::WithWindows { windows, focused_index, fullscreen_index } => {
                if let Some(pos) = windows.iter().position(|&id| id == window_id) {
                    windows.remove(pos);
                    
                    // If we removed the only window, become empty
                    if windows.is_empty() {
                        *self = WorkspaceWindows::Empty;
                        return true;
                    }
                    
                    // Adjust focused index
                    if pos < *focused_index {
                        *focused_index -= 1;
                    } else if pos == *focused_index {
                        // We removed the focused window, focus the next logical one
                        *focused_index = (*focused_index).min(windows.len() - 1);
                    }
                    
                    // Adjust fullscreen index
                    if let Some(fs_idx) = fullscreen_index {
                        if pos < *fs_idx {
                            *fullscreen_index = Some(*fs_idx - 1);
                        } else if pos == *fs_idx {
                            // Removed the fullscreen window
                            *fullscreen_index = None;
                        }
                    }
                    
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Get the currently focused window
    pub fn focused_window(&self) -> Option<WindowId> {
        match self {
            WorkspaceWindows::Empty => None,
            WorkspaceWindows::WithWindows { windows, focused_index, .. } => {
                windows.get(*focused_index).copied()
            }
        }
    }

    /// Get the fullscreen window if any
    pub fn fullscreen_window(&self) -> Option<WindowId> {
        match self {
            WorkspaceWindows::Empty => None,
            WorkspaceWindows::WithWindows { windows, fullscreen_index, .. } => {
                fullscreen_index.and_then(|idx| windows.get(idx).copied())
            }
        }
    }

    /// Set which window is focused
    pub fn set_focus(&mut self, window_id: WindowId) -> bool {
        match self {
            WorkspaceWindows::Empty => false,
            WorkspaceWindows::WithWindows { windows, focused_index, .. } => {
                if let Some(pos) = windows.iter().position(|&id| id == window_id) {
                    *focused_index = pos;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Set which window is fullscreen
    pub fn set_fullscreen(&mut self, window_id: Option<WindowId>) -> bool {
        match self {
            WorkspaceWindows::Empty => false,
            WorkspaceWindows::WithWindows { windows, fullscreen_index, .. } => {
                if let Some(id) = window_id {
                    if let Some(pos) = windows.iter().position(|&wid| wid == id) {
                        *fullscreen_index = Some(pos);
                        true
                    } else {
                        false
                    }
                } else {
                    *fullscreen_index = None;
                    true
                }
            }
        }
    }

    /// Check if the workspace is empty
    pub fn is_empty(&self) -> bool {
        matches!(self, WorkspaceWindows::Empty)
    }

    /// Get the number of windows
    pub fn len(&self) -> usize {
        match self {
            WorkspaceWindows::Empty => 0,
            WorkspaceWindows::WithWindows { windows, .. } => windows.len(),
        }
    }

    /// Get all windows
    pub fn windows(&self) -> &[WindowId] {
        match self {
            WorkspaceWindows::Empty => &[],
            WorkspaceWindows::WithWindows { windows, .. } => windows,
        }
    }

    /// Check if a window exists in this workspace
    pub fn contains(&self, window_id: WindowId) -> bool {
        match self {
            WorkspaceWindows::Empty => false,
            WorkspaceWindows::WithWindows { windows, .. } => windows.contains(&window_id),
        }
    }

    /// Get the first window (for fallback focus)
    pub fn first(&self) -> Option<WindowId> {
        match self {
            WorkspaceWindows::Empty => None,
            WorkspaceWindows::WithWindows { windows, .. } => windows.first().copied(),
        }
    }
}