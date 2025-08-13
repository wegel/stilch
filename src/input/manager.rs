use smithay::input::{
    keyboard::Keysym,
    pointer::{CursorImageStatus, PointerHandle},
    Seat,
};

use crate::cursor_manager::CursorManager;
use crate::state::{Backend, DndIcon, StilchState};

/// Centralized input management
#[derive(Debug)]
pub struct InputManager<BackendData: Backend + 'static> {
    /// Keys that are currently suppressed
    pub suppressed_keys: Vec<Keysym>,
    /// Current cursor image status
    pub cursor_status: CursorImageStatus,
    /// Cursor manager for loading and caching cursor images
    pub cursor_manager: CursorManager,
    /// Wayland seat for input
    pub seat: Seat<StilchState<BackendData>>,
    /// Pointer handle
    pub pointer: PointerHandle<StilchState<BackendData>>,
    /// Current drag-and-drop icon
    pub dnd_icon: Option<DndIcon>,
}

impl<BackendData: Backend + 'static> InputManager<BackendData> {
    /// Create a new input manager from existing seat and pointer
    pub fn new(
        seat: Seat<StilchState<BackendData>>,
        pointer: PointerHandle<StilchState<BackendData>>,
    ) -> Self {
        Self {
            suppressed_keys: Vec::new(),
            cursor_status: CursorImageStatus::default_named(),
            cursor_manager: CursorManager::new(),
            seat,
            pointer,
            dnd_icon: None,
        }
    }

    /// Get a reference to the seat
    pub fn seat(&self) -> &Seat<StilchState<BackendData>> {
        &self.seat
    }

    /// Get a mutable reference to the seat
    pub fn seat_mut(&mut self) -> &mut Seat<StilchState<BackendData>> {
        &mut self.seat
    }

    /// Get a reference to the pointer
    pub fn pointer(&self) -> &PointerHandle<StilchState<BackendData>> {
        &self.pointer
    }

    /// Get the current cursor status
    pub fn cursor_status(&self) -> &CursorImageStatus {
        &self.cursor_status
    }

    /// Set the cursor status
    pub fn set_cursor_status(&mut self, status: CursorImageStatus) {
        self.cursor_status = status;
    }

    /// Get the current DnD icon
    pub fn dnd_icon(&self) -> Option<&DndIcon> {
        self.dnd_icon.as_ref()
    }

    /// Set the DnD icon
    pub fn set_dnd_icon(&mut self, icon: Option<DndIcon>) {
        self.dnd_icon = icon;
    }

    /// Check if a key is suppressed
    pub fn is_key_suppressed(&self, keysym: Keysym) -> bool {
        self.suppressed_keys.contains(&keysym)
    }

    /// Add a suppressed key
    pub fn suppress_key(&mut self, keysym: Keysym) {
        if !self.suppressed_keys.contains(&keysym) {
            self.suppressed_keys.push(keysym);
        }
    }

    /// Remove a suppressed key
    pub fn unsuppress_key(&mut self, keysym: Keysym) {
        self.suppressed_keys.retain(|&k| k != keysym);
    }

    /// Clear all suppressed keys
    pub fn clear_suppressed_keys(&mut self) {
        self.suppressed_keys.clear();
    }
}
