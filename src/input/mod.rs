//! Input handling module for stilch
//!
//! This module handles all input events including keyboard, pointer, touch,
//! tablet, and gesture inputs.

mod gesture;
mod keyboard;
pub mod manager;
mod pointer;
mod tablet;
mod touch;

pub use self::manager::InputManager;

use smithay::backend::input::{Device, InputBackend, InputEvent};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::wayland::tablet_manager::TabletSeatTrait;

use crate::state::{Backend, StilchState};

impl<BackendData: Backend> StilchState<BackendData> {
    /// Process input events for windowed backends (winit/x11)
    pub fn process_input_event_windowed<B: InputBackend>(
        &mut self,
        event: InputEvent<B>,
        output_name: &str,
    ) {
        match event {
            InputEvent::Keyboard { event } => self.on_keyboard_key_windowed::<B>(event),
            InputEvent::PointerMotionAbsolute { event } => {
                self.on_pointer_absolute_windowed::<B>(event, output_name)
            }
            InputEvent::PointerButton { event } => self.on_pointer_button::<B>(event),
            InputEvent::PointerAxis { event } => self.on_pointer_axis::<B>(event),
            _ => {} // Other events not handled in windowed mode
        }
    }
}

#[cfg(feature = "udev")]
impl StilchState<crate::udev::UdevData> {
    /// Process input events for udev backend
    pub fn process_input_event<B: InputBackend>(
        &mut self,
        dh: &DisplayHandle,
        event: InputEvent<B>,
    ) {
        match event {
            InputEvent::Keyboard { event, .. } => self.on_keyboard_key::<B>(event),
            InputEvent::PointerMotion { event, .. } => self.on_pointer_move::<B>(dh, event),
            InputEvent::PointerMotionAbsolute { event, .. } => {
                self.on_pointer_move_absolute::<B>(dh, event)
            }
            InputEvent::PointerButton { event, .. } => self.on_pointer_button::<B>(event),
            InputEvent::PointerAxis { event, .. } => self.on_pointer_axis::<B>(event),
            InputEvent::TabletToolAxis { event, .. } => self.on_tablet_tool_axis::<B>(event),
            InputEvent::TabletToolProximity { event, .. } => {
                self.on_tablet_tool_proximity::<B>(dh, event)
            }
            InputEvent::TabletToolTip { event, .. } => self.on_tablet_tool_tip::<B>(event),
            InputEvent::TabletToolButton { event, .. } => self.on_tablet_button::<B>(event),
            InputEvent::GestureSwipeBegin { event, .. } => self.on_gesture_swipe_begin::<B>(event),
            InputEvent::GestureSwipeUpdate { event, .. } => {
                self.on_gesture_swipe_update::<B>(event)
            }
            InputEvent::GestureSwipeEnd { event, .. } => self.on_gesture_swipe_end::<B>(event),
            InputEvent::GesturePinchBegin { event, .. } => self.on_gesture_pinch_begin::<B>(event),
            InputEvent::GesturePinchUpdate { event, .. } => {
                self.on_gesture_pinch_update::<B>(event)
            }
            InputEvent::GesturePinchEnd { event, .. } => self.on_gesture_pinch_end::<B>(event),
            InputEvent::GestureHoldBegin { event, .. } => self.on_gesture_hold_begin::<B>(event),
            InputEvent::GestureHoldEnd { event, .. } => self.on_gesture_hold_end::<B>(event),

            InputEvent::TouchDown { event } => self.on_touch_down::<B>(event),
            InputEvent::TouchUp { event } => self.on_touch_up::<B>(event),
            InputEvent::TouchMotion { event } => self.on_touch_motion::<B>(event),
            InputEvent::TouchFrame { event } => self.on_touch_frame::<B>(event),
            InputEvent::TouchCancel { event } => self.on_touch_cancel::<B>(event),

            InputEvent::DeviceAdded { device } => {
                if device.has_capability(smithay::backend::input::DeviceCapability::TabletTool) {
                    self.seat().tablet_seat().add_tablet::<Self>(
                        dh,
                        &smithay::wayland::tablet_manager::TabletDescriptor::from(&device),
                    );
                }
            }
            InputEvent::DeviceRemoved { device } => {
                if device.has_capability(smithay::backend::input::DeviceCapability::TabletTool) {
                    let tablet_seat = self.seat().tablet_seat();
                    tablet_seat.remove_tablet(
                        &smithay::wayland::tablet_manager::TabletDescriptor::from(&device),
                    );
                    if tablet_seat.count_tablets() == 0 {
                        tablet_seat.clear_tools();
                    }
                }
            }
            _ => {}
        }
    }

    /// Apply input configuration to a device
    pub fn apply_input_config<D: Device>(&self, device: &mut D) {
        use smithay::backend::input::DeviceCapability;

        let device_name = device.name();
        let device_sysname = device
            .syspath()
            .and_then(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();

        // Find matching input config
        let matching_config = self.config.input_configs.iter().find(|config| {
            // Check if identifier matches
            match config.identifier.as_str() {
                "type:keyboard" => device.has_capability(DeviceCapability::Keyboard),
                "type:pointer" => device.has_capability(DeviceCapability::Pointer),
                "type:touchpad" => device.has_capability(DeviceCapability::Touch),
                "*" => true,
                identifier => {
                    // Check exact name match or glob pattern
                    device_name == identifier || device_sysname == identifier
                }
            }
        });

        if let Some(config) = matching_config {
            tracing::info!(
                "Found input config for device '{}' (identifier: '{}')",
                device_name,
                config.identifier
            );

            // Log what we would configure if libinput device methods were available
            if device.has_capability(DeviceCapability::Pointer)
                || device.has_capability(DeviceCapability::Touch)
            {
                if config.accel_speed.is_some() {
                    tracing::debug!("Would set acceleration speed: {:?}", config.accel_speed);
                }
                if config.accel_profile.is_some() {
                    tracing::debug!("Would set acceleration profile: {:?}", config.accel_profile);
                }
                if config.natural_scroll.is_some() {
                    tracing::debug!("Would set natural scrolling: {:?}", config.natural_scroll);
                }
                if config.tap.is_some() {
                    tracing::debug!("Would set tap to click: {:?}", config.tap);
                }
                if config.tap_button_map.is_some() {
                    tracing::debug!("Would set tap button map: {:?}", config.tap_button_map);
                }
                if config.scroll_method.is_some() {
                    tracing::debug!("Would set scroll method: {:?}", config.scroll_method);
                }
                if config.left_handed.is_some() {
                    tracing::debug!("Would set left handed mode: {:?}", config.left_handed);
                }
                if config.middle_emulation.is_some() {
                    tracing::debug!(
                        "Would set middle button emulation: {:?}",
                        config.middle_emulation
                    );
                }
            }

            // Note: The actual device configuration would need to be done at the libinput level,
            // which requires access to the underlying libinput device, not just the smithay Device trait.
            // Keyboard repeat rate/delay and XKB settings are already handled when creating the keyboard.
        } else {
            tracing::debug!("No input config found for device '{}'", device_name);
        }
    }
}
