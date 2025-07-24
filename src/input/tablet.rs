//! Tablet input handling

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Event, InputBackend, ProximityState, TabletToolButtonEvent,
        TabletToolEvent, TabletToolProximityEvent, TabletToolTipEvent, TabletToolTipState,
    },
    reexports::wayland_server::DisplayHandle,
    utils::SERIAL_COUNTER as SCOUNTER,
    wayland::{
        seat::WaylandFocus,
        tablet_manager::{TabletDescriptor, TabletSeatTrait},
    },
};

use crate::state::StilchState;

#[cfg(feature = "udev")]
impl StilchState<crate::udev::UdevData> {
    /// Handle tablet tool axis events (movement)
    pub fn on_tablet_tool_axis<B: InputBackend>(&mut self, evt: B::TabletToolAxisEvent) {
        let tablet_seat = self.seat().tablet_seat();

        let output_geometry = self
            .space()
            .outputs()
            .next()
            .and_then(|o| self.space().output_geometry(o));

        if let Some(rect) = output_geometry {
            let pointer_location = evt.position_transformed(rect.size) + rect.loc.to_f64();
            let pointer = self.pointer().clone();
            let tablet_tool = evt.tool();

            pointer.set_location(pointer_location);

            let under = self.surface_under(pointer_location);
            if let Some((focus, location)) = &under {
                let tablet = tablet_seat.get_tablet(&TabletDescriptor::from(&evt.device()));
                let tool = tablet_seat.get_tool(&tablet_tool);

                if let (Some(tablet), Some(tool)) = (tablet, tool) {
                    if evt.pressure_has_changed() {
                        tool.pressure(evt.pressure());
                    }
                    if evt.distance_has_changed() {
                        tool.distance(evt.distance());
                    }
                    if evt.tilt_has_changed() {
                        tool.tilt(evt.tilt());
                    }
                    if evt.slider_has_changed() {
                        tool.slider_position(evt.slider_position());
                    }
                    if evt.rotation_has_changed() {
                        tool.rotation(evt.rotation());
                    }
                    if evt.wheel_has_changed() {
                        tool.wheel(evt.wheel_delta(), evt.wheel_delta_discrete());
                    }

                    // Convert PointerFocusTarget to WlSurface if possible
                    let wl_surface_focus = focus.wl_surface().map(|cow| cow.into_owned());
                    let focus_with_location = wl_surface_focus.map(|surface| (surface, *location));

                    tool.motion(
                        pointer_location,
                        focus_with_location,
                        &tablet,
                        SCOUNTER.next_serial(),
                        evt.time_msec(),
                    );
                }
            }

            pointer.frame(self);
        }
    }

    /// Handle tablet tool proximity events
    pub fn on_tablet_tool_proximity<B: InputBackend>(
        &mut self,
        dh: &DisplayHandle,
        evt: B::TabletToolProximityEvent,
    ) {
        let tablet_seat = self.seat().tablet_seat();

        let output_geometry = self
            .space()
            .outputs()
            .next()
            .and_then(|o| self.space().output_geometry(o));

        if let Some(rect) = output_geometry {
            let tool = evt.tool();
            tablet_seat.add_tool::<Self>(self, dh, &tool);

            let pointer_location = evt.position_transformed(rect.size) + rect.loc.to_f64();
            let pointer = self.pointer().clone();
            pointer.set_location(pointer_location);
            pointer.frame(self);

            if let (Some(tablet), Some(tool)) = (
                tablet_seat.get_tablet(&TabletDescriptor::from(&evt.device())),
                tablet_seat.get_tool(&tool),
            ) {
                match evt.state() {
                    ProximityState::In => {
                        if let Some((focus, location)) = self.surface_under(pointer_location) {
                            // Convert PointerFocusTarget to WlSurface if possible
                            if let Some(wl_surface) = focus.wl_surface().map(|cow| cow.into_owned())
                            {
                                tool.proximity_in(
                                    pointer_location,
                                    (wl_surface, location),
                                    &tablet,
                                    SCOUNTER.next_serial(),
                                    evt.time_msec(),
                                );
                            }
                        }
                    }
                    ProximityState::Out => {
                        tool.proximity_out(evt.time_msec());
                    }
                }
            }
        }
    }

    /// Handle tablet tool tip events (pen touch)
    pub fn on_tablet_tool_tip<B: InputBackend>(&mut self, evt: B::TabletToolTipEvent) {
        let tool = self.seat().tablet_seat().get_tool(&evt.tool());

        if let Some(tool) = tool {
            match evt.tip_state() {
                TabletToolTipState::Down => {
                    let serial = SCOUNTER.next_serial();
                    tool.tip_down(serial, evt.time_msec());

                    // Change keyboard focus on tip down
                    let pointer_location = self.pointer().current_location();
                    self.update_keyboard_focus(pointer_location, serial);
                }
                TabletToolTipState::Up => {
                    tool.tip_up(evt.time_msec());
                }
            }
        }
    }

    /// Handle tablet tool button events
    pub fn on_tablet_button<B: InputBackend>(&mut self, evt: B::TabletToolButtonEvent) {
        let tool = self.seat().tablet_seat().get_tool(&evt.tool());

        if let Some(tool) = tool {
            tool.button(
                evt.button(),
                evt.button_state(),
                SCOUNTER.next_serial(),
                evt.time_msec(),
            );
        }
    }
}
