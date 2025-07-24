//! Gesture input handling

use smithay::{
    backend::input::{
        Event, GestureBeginEvent, GestureEndEvent, GesturePinchUpdateEvent as _,
        GestureSwipeUpdateEvent as _, InputBackend,
    },
    input::pointer::{
        GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent, GesturePinchEndEvent,
        GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
        GestureSwipeUpdateEvent,
    },
};

use crate::state::StilchState;

#[cfg(feature = "udev")]
impl StilchState<crate::udev::UdevData> {
    /// Handle gesture swipe begin
    pub fn on_gesture_swipe_begin<B: InputBackend>(&mut self, evt: B::GestureSwipeBeginEvent) {
        let pointer = self.pointer().clone();
        pointer.gesture_swipe_begin(
            self,
            &GestureSwipeBeginEvent {
                serial: smithay::utils::SERIAL_COUNTER.next_serial(),
                time: evt.time_msec(),
                fingers: evt.fingers(),
            },
        );
    }

    /// Handle gesture swipe update
    pub fn on_gesture_swipe_update<B: InputBackend>(&mut self, evt: B::GestureSwipeUpdateEvent) {
        let pointer = self.pointer().clone();
        pointer.gesture_swipe_update(
            self,
            &GestureSwipeUpdateEvent {
                time: evt.time_msec(),
                delta: evt.delta(),
            },
        );
    }

    /// Handle gesture swipe end
    pub fn on_gesture_swipe_end<B: InputBackend>(&mut self, evt: B::GestureSwipeEndEvent) {
        let pointer = self.pointer().clone();
        pointer.gesture_swipe_end(
            self,
            &GestureSwipeEndEvent {
                serial: smithay::utils::SERIAL_COUNTER.next_serial(),
                time: evt.time_msec(),
                cancelled: evt.cancelled(),
            },
        );
    }

    /// Handle gesture pinch begin
    pub fn on_gesture_pinch_begin<B: InputBackend>(&mut self, evt: B::GesturePinchBeginEvent) {
        let pointer = self.pointer().clone();
        pointer.gesture_pinch_begin(
            self,
            &GesturePinchBeginEvent {
                serial: smithay::utils::SERIAL_COUNTER.next_serial(),
                time: evt.time_msec(),
                fingers: evt.fingers(),
            },
        );
    }

    /// Handle gesture pinch update
    pub fn on_gesture_pinch_update<B: InputBackend>(&mut self, evt: B::GesturePinchUpdateEvent) {
        let pointer = self.pointer().clone();
        pointer.gesture_pinch_update(
            self,
            &GesturePinchUpdateEvent {
                time: evt.time_msec(),
                delta: evt.delta(),
                scale: evt.scale(),
                rotation: evt.rotation(),
            },
        );
    }

    /// Handle gesture pinch end
    pub fn on_gesture_pinch_end<B: InputBackend>(&mut self, evt: B::GesturePinchEndEvent) {
        let pointer = self.pointer().clone();
        pointer.gesture_pinch_end(
            self,
            &GesturePinchEndEvent {
                serial: smithay::utils::SERIAL_COUNTER.next_serial(),
                time: evt.time_msec(),
                cancelled: evt.cancelled(),
            },
        );
    }

    /// Handle gesture hold begin
    pub fn on_gesture_hold_begin<B: InputBackend>(&mut self, evt: B::GestureHoldBeginEvent) {
        let pointer = self.pointer().clone();
        pointer.gesture_hold_begin(
            self,
            &GestureHoldBeginEvent {
                serial: smithay::utils::SERIAL_COUNTER.next_serial(),
                time: evt.time_msec(),
                fingers: evt.fingers(),
            },
        );
    }

    /// Handle gesture hold end
    pub fn on_gesture_hold_end<B: InputBackend>(&mut self, evt: B::GestureHoldEndEvent) {
        let pointer = self.pointer().clone();
        pointer.gesture_hold_end(
            self,
            &GestureHoldEndEvent {
                serial: smithay::utils::SERIAL_COUNTER.next_serial(),
                time: evt.time_msec(),
                cancelled: evt.cancelled(),
            },
        );
    }
}
