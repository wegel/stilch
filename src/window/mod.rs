//! Unified window management for stilch
//!
//! This module provides a unified window representation that combines
//! WindowElement with tracking information.

mod consistency;
mod id;
mod manager;
mod registry;

pub use consistency::check_consistency;
pub use id::{ContainerId, WindowId};
pub use manager::WindowManager;
pub use registry::WindowRegistry;

use crate::shell::WindowElement;
use crate::workspace::WorkspaceId;
use smithay::utils::{Logical, Rectangle};

/// Fullscreen modes supported by the window manager
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenMode {
    /// Maximize within current container
    Container,
    /// Fullscreen on virtual output (current behavior)
    VirtualOutput,
    /// Fullscreen on physical display
    PhysicalOutput,
}

/// Non-fullscreen window layouts - used to prevent cycles in fullscreen state
#[derive(Debug, Clone, PartialEq)]
pub enum NonFullscreenLayout {
    /// Window is tiled within a container
    Tiled {
        container: ContainerId,
        geometry: Rectangle<i32, Logical>,
    },
    /// Window is floating above the tiled layout
    Floating { geometry: Rectangle<i32, Logical> },
}

impl NonFullscreenLayout {
    /// Convert to a WindowLayout
    pub fn into_layout(self) -> WindowLayout {
        match self {
            NonFullscreenLayout::Tiled {
                container,
                geometry,
            } => WindowLayout::Tiled {
                container,
                geometry,
            },
            NonFullscreenLayout::Floating { geometry } => WindowLayout::Floating { geometry },
        }
    }
}

/// Represents the layout state of a window - these are mutually exclusive
#[derive(Debug, Clone, PartialEq)]
pub enum WindowLayout {
    /// Window is tiled within a container
    Tiled {
        container: ContainerId,
        geometry: Rectangle<i32, Logical>,
    },
    /// Window is floating above the tiled layout
    Floating { geometry: Rectangle<i32, Logical> },
    /// Window is fullscreen
    Fullscreen {
        mode: FullscreenMode,
        geometry: Rectangle<i32, Logical>,
        /// Previous layout to restore when exiting fullscreen - cannot be another fullscreen
        previous: Box<NonFullscreenLayout>,
    },
}

impl WindowLayout {
    /// Try to convert to a NonFullscreenLayout
    /// Returns None if this is a Fullscreen layout
    pub fn as_non_fullscreen(&self) -> Option<NonFullscreenLayout> {
        match self {
            WindowLayout::Tiled {
                container,
                geometry,
            } => Some(NonFullscreenLayout::Tiled {
                container: *container,
                geometry: *geometry,
            }),
            WindowLayout::Floating { geometry } => Some(NonFullscreenLayout::Floating {
                geometry: *geometry,
            }),
            WindowLayout::Fullscreen { .. } => None,
        }
    }
}

/// Unified window representation that wraps WindowElement with tracking info
#[derive(Debug, Clone)]
pub struct ManagedWindow {
    /// Unique window identifier
    pub id: WindowId,
    /// The actual window element
    pub element: WindowElement,
    /// Current workspace the window is on
    pub workspace: WorkspaceId,
    /// Current layout state of the window
    pub layout: WindowLayout,
}

impl ManagedWindow {
    /// Create a new managed window
    pub fn new(element: WindowElement, workspace: WorkspaceId) -> Self {
        // New windows start without a container assignment
        // The workspace will assign them to a container
        Self {
            id: WindowId::next(),
            element,
            workspace,
            layout: WindowLayout::Tiled {
                container: ContainerId::next(), // Temporary - will be replaced by workspace
                geometry: Rectangle::default(),
            },
        }
    }

    /// Get window title if available
    pub fn title(&self) -> String {
        self.element.title()
    }

    /// Get window app_id if available  
    pub fn app_id(&self) -> Option<String> {
        self.element.app_id()
    }

    /// Check if window is currently fullscreen in any mode
    pub fn is_fullscreen(&self) -> bool {
        matches!(self.layout, WindowLayout::Fullscreen { .. })
    }

    /// Check if window is tiled (not floating and not fullscreen)
    pub fn is_tiled(&self) -> bool {
        matches!(self.layout, WindowLayout::Tiled { .. })
    }

    /// Check if window is floating
    pub fn is_floating(&self) -> bool {
        matches!(self.layout, WindowLayout::Floating { .. })
    }

    /// Get the window's current geometry
    pub fn geometry(&self) -> Rectangle<i32, Logical> {
        match &self.layout {
            WindowLayout::Tiled { geometry, .. } => *geometry,
            WindowLayout::Floating { geometry } => *geometry,
            WindowLayout::Fullscreen { geometry, .. } => *geometry,
        }
    }

    /// Get the window's container if it's tiled
    pub fn container(&self) -> Option<ContainerId> {
        match &self.layout {
            WindowLayout::Tiled { container, .. } => Some(*container),
            _ => None,
        }
    }
}
