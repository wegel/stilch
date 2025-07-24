//! Type-safe resize state machine
//!
//! This module implements a type-state pattern for window resize operations,
//! ensuring that state transitions are valid at compile time.

#![allow(dead_code)]

use smithay::utils::{Logical, Point, Serial, Size};

/// Data associated with a resize operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResizeData {
    /// The edges that are being resized
    pub edges: ResizeEdge,
    /// The initial window size before resize started
    pub initial_size: Size<i32, Logical>,
    /// The initial window location before resize started
    pub initial_location: Point<i32, Logical>,
    /// The current size during resize
    pub current_size: Size<i32, Logical>,
}

/// Edges that can be resized
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    /// Resize only the top edge
    Top,
    /// Resize only the bottom edge
    Bottom,
    /// Resize only the left edge
    Left,
    /// Resize only the right edge
    Right,
    /// Resize top-left corner
    TopLeft,
    /// Resize top-right corner
    TopRight,
    /// Resize bottom-left corner
    BottomLeft,
    /// Resize bottom-right corner
    BottomRight,
}

impl ResizeEdge {
    /// Check if the top edge is being resized
    pub fn has_top(&self) -> bool {
        matches!(
            self,
            ResizeEdge::Top | ResizeEdge::TopLeft | ResizeEdge::TopRight
        )
    }

    /// Check if the bottom edge is being resized
    pub fn has_bottom(&self) -> bool {
        matches!(
            self,
            ResizeEdge::Bottom | ResizeEdge::BottomLeft | ResizeEdge::BottomRight
        )
    }

    /// Check if the left edge is being resized
    pub fn has_left(&self) -> bool {
        matches!(
            self,
            ResizeEdge::Left | ResizeEdge::TopLeft | ResizeEdge::BottomLeft
        )
    }

    /// Check if the right edge is being resized
    pub fn has_right(&self) -> bool {
        matches!(
            self,
            ResizeEdge::Right | ResizeEdge::TopRight | ResizeEdge::BottomRight
        )
    }
}

// Type states for the resize operation
pub struct NotResizing;
pub struct Resizing {
    pub data: ResizeData,
}
pub struct WaitingForAck {
    pub data: ResizeData,
    pub serial: Serial,
}
pub struct WaitingForCommit {
    pub data: ResizeData,
}

/// Type-safe resize state machine
pub enum ResizeState {
    Idle(NotResizing),
    Active(ActiveResize),
}

pub enum ActiveResize {
    Resizing(Resizing),
    WaitingForAck(WaitingForAck),
    WaitingForCommit(WaitingForCommit),
}

// Type-safe state transitions
impl NotResizing {
    /// Start a resize operation
    pub fn start_resize(self, data: ResizeData) -> Resizing {
        Resizing { data }
    }
}

impl Resizing {
    /// Update the current size during resize
    pub fn update_size(&mut self, new_size: Size<i32, Logical>) {
        self.data.current_size = new_size;
    }

    /// Finish resizing and wait for acknowledgment
    pub fn finish(self, serial: Serial) -> WaitingForAck {
        WaitingForAck {
            data: self.data,
            serial,
        }
    }

    /// Cancel the resize operation
    pub fn cancel(self) -> NotResizing {
        NotResizing
    }
}

impl WaitingForAck {
    /// Acknowledgment received, wait for commit
    pub fn ack_received(self) -> WaitingForCommit {
        WaitingForCommit { data: self.data }
    }

    /// Timeout or error, cancel the operation
    pub fn timeout(self) -> NotResizing {
        NotResizing
    }
}

impl WaitingForCommit {
    /// Commit received, resize complete
    pub fn commit_received(self) -> NotResizing {
        NotResizing
    }

    /// Timeout or error, cancel the operation
    pub fn timeout(self) -> NotResizing {
        NotResizing
    }
}

// Helper methods for the enum wrapper
impl ResizeState {
    pub fn new() -> Self {
        ResizeState::Idle(NotResizing)
    }

    pub fn is_resizing(&self) -> bool {
        matches!(self, ResizeState::Active(_))
    }

    pub fn get_data(&self) -> Option<&ResizeData> {
        match self {
            ResizeState::Idle(_) => None,
            ResizeState::Active(active) => match active {
                ActiveResize::Resizing(r) => Some(&r.data),
                ActiveResize::WaitingForAck(w) => Some(&w.data),
                ActiveResize::WaitingForCommit(w) => Some(&w.data),
            },
        }
    }
}

impl Default for ResizeState {
    fn default() -> Self {
        Self::new()
    }
}
