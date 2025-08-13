//! Seat, input method, and pointer-related protocol handlers

use smithay::{
    backend::input::TabletToolDescriptor,
    delegate_input_method_manager, delegate_keyboard_shortcuts_inhibit,
    delegate_pointer_constraints, delegate_pointer_gestures, delegate_relative_pointer,
    delegate_seat, delegate_tablet_manager, delegate_text_input_manager,
    delegate_virtual_keyboard_manager,
    desktop::{PopupKind, PopupManager},
    input::{
        keyboard::LedState,
        pointer::{CursorImageStatus, PointerHandle},
        Seat, SeatHandler, SeatState,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Rectangle},
    wayland::{
        input_method::{InputMethodHandler, PopupSurface},
        keyboard_shortcuts_inhibit::{
            KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState,
            KeyboardShortcutsInhibitor,
        },
        pointer_constraints::{with_pointer_constraint, PointerConstraintsHandler},
        seat::WaylandFocus, // Trait needed for wl_surface() method
        selection::{data_device::set_data_device_focus, primary_selection::set_primary_focus},
        tablet_manager::TabletSeatHandler,
    },
};
use tracing::warn;

use crate::{
    focus::{KeyboardFocusTarget, PointerFocusTarget},
    state::{Backend, StilchState},
};

impl<BackendData: Backend> SeatHandler for StilchState<BackendData> {
    type KeyboardFocus = KeyboardFocusTarget;
    type PointerFocus = PointerFocusTarget;
    type TouchFocus = PointerFocusTarget;

    fn seat_state(&mut self) -> &mut SeatState<StilchState<BackendData>> {
        &mut self.protocols.seat_state
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, target: Option<&KeyboardFocusTarget>) {
        let dh = &self.display_handle;

        let wl_surface = target.and_then(|t| t.wl_surface());

        if let Some(surface) = wl_surface {
            use smithay::reexports::wayland_server::Resource;
            let client = dh.get_client(surface.as_ref().id()).ok();
            set_data_device_focus(dh, seat, client.clone());
            set_primary_focus(dh, seat, client);
        } else {
            set_data_device_focus(dh, seat, None);
            set_primary_focus(dh, seat, None);
        }
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        self.input_manager.cursor_status = image.clone();
        self.input_manager.cursor_manager.set_cursor_image(image);
    }

    fn led_state_changed(&mut self, _seat: &Seat<Self>, led_state: LedState) {
        self.backend_data.update_led_state(led_state)
    }
}

impl<BackendData: Backend> TabletSeatHandler for StilchState<BackendData> {
    fn tablet_tool_image(&mut self, _tool: &TabletToolDescriptor, image: CursorImageStatus) {
        // Tablet tools can have their own cursors, but for simplicity
        // we'll share the same cursor status with the pointer
        self.input_manager.cursor_status = image.clone();
        self.input_manager.cursor_manager.set_cursor_image(image);
    }
}

impl<BackendData: Backend> InputMethodHandler for StilchState<BackendData> {
    fn new_popup(&mut self, surface: PopupSurface) {
        if let Err(err) = self.popups_mut().track_popup(PopupKind::from(surface)) {
            warn!("Failed to track popup: {err}");
        }
    }

    fn popup_repositioned(&mut self, _: PopupSurface) {}

    fn dismiss_popup(&mut self, surface: PopupSurface) {
        if let Some(parent) = surface.get_parent().map(|parent| parent.surface.clone()) {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.space()
            .elements()
            .find_map(|window| {
                (window.wl_surface().as_deref() == Some(parent)).then(|| {
                    // Get the window's bounding box
                    use smithay::desktop::space::SpaceElement;
                    let loc = self.space().element_location(window).unwrap_or_default();
                    let bbox = SpaceElement::bbox(window);
                    Rectangle::new(loc, bbox.size)
                })
            })
            .unwrap_or_default()
    }
}

impl<BackendData: Backend> KeyboardShortcutsInhibitHandler for StilchState<BackendData> {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        &mut self.protocols.keyboard_shortcuts_inhibit_state
    }

    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        // Just grant the wish for everyone
        inhibitor.activate();
    }
}

impl<BackendData: Backend> PointerConstraintsHandler for StilchState<BackendData> {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        // XXX region
        let Some(current_focus) = pointer.current_focus() else {
            return;
        };
        if current_focus.wl_surface().as_deref() == Some(surface) {
            with_pointer_constraint(surface, pointer, |constraint| {
                if let Some(c) = constraint {
                    c.activate();
                } else {
                    tracing::warn!("No pointer constraint to activate");
                }
            });
        }
    }

    fn cursor_position_hint(
        &mut self,
        _surface: &WlSurface,
        _pointer: &PointerHandle<Self>,
        _location: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) {
        // Handle cursor position hint for pointer constraints
    }
}

delegate_seat!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_tablet_manager!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_text_input_manager!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_input_method_manager!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_keyboard_shortcuts_inhibit!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_virtual_keyboard_manager!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_pointer_gestures!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_relative_pointer!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_pointer_constraints!(@<BackendData: Backend + 'static> StilchState<BackendData>);
