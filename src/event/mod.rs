//! Event system for stilch
//!
//! This module provides a centralized event system for the compositor,
//! enabling better decoupling and more flexible event handling.

pub mod ipc_handler;

use crate::virtual_output::VirtualOutputId;
use crate::window::WindowId;
use crate::workspace::WorkspaceId;
use smithay::utils::{Logical, Point, Rectangle};
use std::time::Instant;

/// Window-related events
#[derive(Debug, Clone)]
pub enum WindowEvent {
    /// A new window was created
    Created {
        window_id: WindowId,
        workspace: WorkspaceId,
        initial_position: Point<i32, Logical>,
        timestamp: Instant,
    },

    /// A window was destroyed
    Destroyed {
        window_id: WindowId,
        workspace: WorkspaceId,
        timestamp: Instant,
    },

    /// A window was moved
    Moved {
        window_id: WindowId,
        old_position: Point<i32, Logical>,
        new_position: Point<i32, Logical>,
        timestamp: Instant,
    },

    /// A window was resized
    Resized {
        window_id: WindowId,
        old_size: Rectangle<i32, Logical>,
        new_size: Rectangle<i32, Logical>,
        timestamp: Instant,
    },

    /// A window gained focus
    Focused {
        window_id: WindowId,
        timestamp: Instant,
    },

    /// A window lost focus
    Unfocused {
        window_id: WindowId,
        timestamp: Instant,
    },

    /// A window changed workspace
    WorkspaceChanged {
        window_id: WindowId,
        old_workspace: WorkspaceId,
        new_workspace: WorkspaceId,
        timestamp: Instant,
    },

    /// A window entered fullscreen
    FullscreenEntered {
        window_id: WindowId,
        mode: crate::window::FullscreenMode,
        timestamp: Instant,
    },

    /// A window exited fullscreen
    FullscreenExited {
        window_id: WindowId,
        timestamp: Instant,
    },
}

/// Workspace-related events
#[derive(Debug, Clone)]
pub enum WorkspaceEvent {
    /// Workspace was switched
    Switched {
        old_workspace: WorkspaceId,
        new_workspace: WorkspaceId,
        virtual_output: VirtualOutputId,
        timestamp: Instant,
    },

    /// Workspace layout changed
    LayoutChanged {
        workspace: WorkspaceId,
        timestamp: Instant,
    },
}

/// Input-related events
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Keyboard key pressed
    KeyPressed {
        keysym: smithay::input::keyboard::Keysym,
        modifiers: smithay::input::keyboard::ModifiersState,
        timestamp: Instant,
    },

    /// Keyboard key released
    KeyReleased {
        keysym: smithay::input::keyboard::Keysym,
        timestamp: Instant,
    },

    /// Pointer moved
    PointerMoved {
        position: Point<f64, Logical>,
        timestamp: Instant,
    },

    /// Pointer button pressed
    PointerButtonPressed {
        button: u32,
        position: Point<f64, Logical>,
        timestamp: Instant,
    },

    /// Pointer button released
    PointerButtonReleased {
        button: u32,
        position: Point<f64, Logical>,
        timestamp: Instant,
    },
}

/// Layout-related events
#[derive(Debug, Clone)]
pub enum LayoutEvent {
    /// Layout recalculation requested
    RecalculationRequested {
        workspace: WorkspaceId,
        timestamp: Instant,
    },

    /// Layout applied
    Applied {
        workspace: WorkspaceId,
        windows_affected: Vec<WindowId>,
        timestamp: Instant,
    },
}

/// IPC-related events
#[derive(Debug, Clone)]
pub enum IpcEvent {
    /// IPC client connected
    ClientConnected { client_id: u64, timestamp: Instant },

    /// IPC client disconnected
    ClientDisconnected { client_id: u64, timestamp: Instant },

    /// IPC command received
    CommandReceived {
        client_id: u64,
        command: String,
        timestamp: Instant,
    },
}

/// Combined event type
#[derive(Debug, Clone)]
pub enum Event {
    Window(WindowEvent),
    Workspace(WorkspaceEvent),
    Input(InputEvent),
    Layout(LayoutEvent),
    Ipc(IpcEvent),
    StateUpdate(ipc_handler::StateUpdateEvent),
}

/// Event handler trait
pub trait EventHandler {
    /// Handle an event
    fn handle_event(&mut self, event: &Event);
}

/// Event listener trait for specific event types
pub trait EventListener<E> {
    /// Called when an event occurs
    fn on_event(&mut self, event: &E);
}

/// Event bus for distributing events to handlers
#[derive(Default)]
pub struct EventBus {
    handlers: Vec<Box<dyn EventHandler>>,
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("handlers_count", &self.handlers.len())
            .finish()
    }
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Register an event handler
    pub fn register_handler(&mut self, handler: Box<dyn EventHandler>) {
        self.handlers.push(handler);
    }

    /// Emit an event to all registered handlers
    pub fn emit(&mut self, event: Event) {
        for handler in &mut self.handlers {
            handler.handle_event(&event);
        }
    }

    /// Emit a window event
    pub fn emit_window(&mut self, event: WindowEvent) {
        self.emit(Event::Window(event));
    }

    /// Emit a workspace event
    pub fn emit_workspace(&mut self, event: WorkspaceEvent) {
        self.emit(Event::Workspace(event));
    }

    /// Emit an input event
    pub fn emit_input(&mut self, event: InputEvent) {
        self.emit(Event::Input(event));
    }

    /// Emit a layout event
    pub fn emit_layout(&mut self, event: LayoutEvent) {
        self.emit(Event::Layout(event));
    }

    /// Emit an IPC event
    pub fn emit_ipc(&mut self, event: IpcEvent) {
        self.emit(Event::Ipc(event));
    }
}
