//! Touch input handling

use smithay::{
    backend::input::{AbsolutePositionEvent, Event, InputBackend, TouchEvent},
    input::touch::{DownEvent, MotionEvent as TouchMotionEvent, UpEvent},
    utils::{Logical, Point, SERIAL_COUNTER as SCOUNTER},
};

use crate::state::StilchState;

#[cfg(feature = "udev")]
impl StilchState<crate::udev::UdevData> {
    /// Handle touch down events
    pub fn on_touch_down<B: InputBackend>(&mut self, evt: B::TouchDownEvent) {
        let Some(handle) = self.seat().get_touch() else {
            return;
        };

        let Some(touch_location) = self.touch_location_transformed::<B, _>(&evt) else {
            return;
        };

        let serial = SCOUNTER.next_serial();
        self.update_keyboard_focus(touch_location, serial);

        let under = self.surface_under(touch_location);
        handle.down(
            self,
            under,
            &DownEvent {
                slot: evt.slot(),
                location: touch_location,
                serial,
                time: evt.time_msec(),
            },
        );
    }

    /// Handle touch up events
    pub fn on_touch_up<B: InputBackend>(&mut self, evt: B::TouchUpEvent) {
        let Some(handle) = self.seat().get_touch() else {
            return;
        };

        let serial = SCOUNTER.next_serial();
        handle.up(
            self,
            &UpEvent {
                slot: evt.slot(),
                serial,
                time: evt.time_msec(),
            },
        );
    }

    /// Handle touch motion events
    pub fn on_touch_motion<B: InputBackend>(&mut self, evt: B::TouchMotionEvent) {
        let Some(handle) = self.seat().get_touch() else {
            return;
        };

        let Some(touch_location) = self.touch_location_transformed::<B, _>(&evt) else {
            return;
        };

        let under = self.surface_under(touch_location);
        handle.motion(
            self,
            under,
            &TouchMotionEvent {
                slot: evt.slot(),
                location: touch_location,
                time: evt.time_msec(),
            },
        );
    }

    /// Handle touch frame events
    pub fn on_touch_frame<B: InputBackend>(&mut self, _evt: B::TouchFrameEvent) {
        let Some(handle) = self.seat().get_touch() else {
            return;
        };

        handle.frame(self);
    }

    /// Handle touch cancel events
    pub fn on_touch_cancel<B: InputBackend>(&mut self, _evt: B::TouchCancelEvent) {
        let Some(handle) = self.seat().get_touch() else {
            return;
        };

        handle.cancel(self);
    }
    /// Transform touch location to logical coordinates
    fn touch_location_transformed<B: InputBackend, E: AbsolutePositionEvent<B>>(
        &self,
        evt: &E,
    ) -> Option<Point<f64, Logical>> {
        // Get the first output's geometry for transformation
        let output_geometry = self
            .space()
            .outputs()
            .next()
            .and_then(|o| self.space().output_geometry(o));

        output_geometry.map(|rect| evt.position_transformed(rect.size) + rect.loc.to_f64())
    }
}
