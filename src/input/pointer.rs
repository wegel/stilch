//! Pointer (mouse) input handling

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, Event, InputBackend, PointerAxisEvent,
        PointerButtonEvent, PointerMotionEvent,
    },
    input::pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent},
    output::Output,
    reexports::wayland_server::{protocol::wl_pointer, Resource},
    utils::{Logical, Point, Serial, SERIAL_COUNTER as SCOUNTER},
    wayland::{
        pointer_constraints::{with_pointer_constraint, PointerConstraint},
        seat::WaylandFocus,
    },
};
use tracing::debug;

use crate::{
    focus::PointerFocusTarget,
    state::{Backend, StilchState},
};

impl<BackendData: Backend> StilchState<BackendData> {
    /// Handle pointer button events
    pub fn on_pointer_button<B: InputBackend>(&mut self, evt: B::PointerButtonEvent) {
        let serial = SCOUNTER.next_serial();
        let button = evt.button_code();

        let state = wl_pointer::ButtonState::from(evt.state());

        if wl_pointer::ButtonState::Pressed == state {
            self.update_keyboard_focus(self.pointer().current_location(), serial);
        };
        let pointer = self.pointer().clone();
        pointer.button(
            self,
            &ButtonEvent {
                button,
                state: state.try_into().unwrap_or_else(|_| {
                    tracing::error!("Invalid button state");
                    smithay::backend::input::ButtonState::Released
                }),
                serial,
                time: evt.time_msec(),
            },
        );
        pointer.frame(self);
    }

    /// Handle pointer axis (scroll) events
    pub fn on_pointer_axis<B: InputBackend>(&mut self, evt: B::PointerAxisEvent) {
        let horizontal_amount = evt
            .amount(Axis::Horizontal)
            .unwrap_or_else(|| evt.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.);
        let vertical_amount = evt
            .amount(Axis::Vertical)
            .unwrap_or_else(|| evt.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.);
        let horizontal_amount_discrete = evt.amount_v120(Axis::Horizontal);
        let vertical_amount_discrete = evt.amount_v120(Axis::Vertical);

        {
            let mut frame = AxisFrame::new(evt.time_msec()).source(evt.source());
            if horizontal_amount != 0.0 {
                frame = frame
                    .relative_direction(Axis::Horizontal, evt.relative_direction(Axis::Horizontal));
                frame = frame.value(Axis::Horizontal, horizontal_amount);
            }
            if let Some(discrete) = horizontal_amount_discrete {
                frame = frame.v120(Axis::Horizontal, discrete as i32);
            }
            if vertical_amount != 0.0 {
                frame = frame
                    .relative_direction(Axis::Vertical, evt.relative_direction(Axis::Vertical));
                frame = frame.value(Axis::Vertical, vertical_amount);
            }
            if let Some(discrete) = vertical_amount_discrete {
                frame = frame.v120(Axis::Vertical, discrete as i32);
            }
            if evt.source() == AxisSource::Finger {
                if evt.amount(Axis::Horizontal) == Some(0.0) {
                    frame = frame.stop(Axis::Horizontal);
                }
                if evt.amount(Axis::Vertical) == Some(0.0) {
                    frame = frame.stop(Axis::Vertical);
                }
            }
            let pointer = self.pointer().clone();
            pointer.axis(self, frame);
            pointer.frame(self);
        }
    }

    /// Handle absolute pointer motion for windowed backends
    pub fn on_pointer_absolute_windowed<B: InputBackend>(
        &mut self,
        evt: B::PointerMotionAbsoluteEvent,
        output_name: &str,
    ) {
        let output = self
            .space()
            .outputs()
            .find(|o| o.name() == output_name)
            .cloned();
        if let Some(output) = output {
            self.on_pointer_move_absolute_windowed::<B>(evt, &output);
        }
    }

    fn on_pointer_move_absolute_windowed<B: InputBackend>(
        &mut self,
        evt: B::PointerMotionAbsoluteEvent,
        output: &Output,
    ) {
        let output_geo = match self.space().output_geometry(output) {
            Some(geo) => geo,
            None => {
                tracing::warn!("Output has no geometry, skipping absolute pointer motion");
                return;
            }
        };

        let pos = evt.position_transformed(output_geo.size) + output_geo.loc.to_f64();
        let serial = SCOUNTER.next_serial();

        let pointer = self.pointer().clone();
        let under = self.surface_under(pos);

        // Update keyboard focus if focus_follows_mouse is enabled
        if self.config.focus_follows_mouse() {
            if let Some((focus, _)) = under.as_ref() {
                // Only update focus if we're hovering over a different window
                let current_focus = self.seat().get_keyboard().unwrap().current_focus();
                let should_update_focus = match current_focus {
                    Some(current) => {
                        // Convert current keyboard focus to pointer focus target for comparison
                        let current_as_pointer: PointerFocusTarget = current.into();
                        current_as_pointer != *focus
                    }
                    None => true,
                };

                if should_update_focus {
                    self.update_keyboard_focus(pos, serial);
                }
            }
        }

        pointer.motion(
            self,
            under,
            &MotionEvent {
                location: pos,
                serial,
                time: evt.time_msec(),
            },
        );
        pointer.frame(self);
    }
}

#[cfg(feature = "udev")]
impl StilchState<crate::udev::UdevData> {
    /// Handle relative pointer motion
    pub fn on_pointer_move<B: InputBackend>(
        &mut self,
        _dh: &smithay::reexports::wayland_server::DisplayHandle,
        evt: B::PointerMotionEvent,
    ) {
        let mut pointer_location = self.pointer().current_location();
        let serial = SCOUNTER.next_serial();

        let pointer = self.pointer().clone();

        // Handle pointer constraints
        let mut pointer_locked = false;
        let mut confine_region = None;
        if let Some((surface, surface_loc)) = pointer
            .current_focus()
            .and_then(|target| Some((target.wl_surface()?.into_owned(), target.surface_loc()?)))
        {
            with_pointer_constraint(&surface, &pointer, |constraint| match constraint {
                Some(constraint) if constraint.is_active() => match &*constraint {
                    PointerConstraint::Locked(_locked) => {
                        pointer_locked = true;
                    }
                    PointerConstraint::Confined(confine) => {
                        confine_region = confine.region().cloned();
                    }
                },
                _ => {}
            });

            // Handle confined pointer
            if let Some(region) = confine_region {
                let new_location = pointer_location + evt.delta();
                let new_location_within_surface =
                    (new_location.to_i32_round() - surface_loc).to_f64();
                if !region.contains(new_location_within_surface.to_i32_round()) {
                    // Pointer would be outside region, don't move it
                    return;
                }
            }
        }

        if pointer_locked {
            // For locked pointer, we don't move the cursor but still send relative motion
            pointer.relative_motion(
                self,
                None,
                &RelativeMotionEvent {
                    delta: evt.delta(),
                    delta_unaccel: evt.delta_unaccel(),
                    utime: evt.time(),
                },
            );
            pointer.frame(self);

            // Still need to redraw for cursor changes
            return;
        }

        // Use physical layout manager if available for DPI-aware cursor movement
        pointer_location = if let Some(ref mut physical_layout) = self.physical_layout {
            // Physical layout manager handles gaps and boundaries itself
            physical_layout.handle_relative_motion(pointer_location, evt.delta())
        } else {
            // Only clamp when not using physical layout
            let new_location = pointer_location + evt.delta();
            self.clamp_pointer_location(new_location)
        };

        let under = self.surface_under(pointer_location);

        // Update keyboard focus if focus_follows_mouse is enabled
        if self.config.focus_follows_mouse() {
            if let Some((focus, _)) = under.as_ref() {
                // Only update focus if we're hovering over a different window
                let current_focus = self.seat().get_keyboard().unwrap().current_focus();
                let should_update_focus = match current_focus {
                    Some(current) => {
                        // Convert current keyboard focus to pointer focus target for comparison
                        let current_as_pointer: PointerFocusTarget = current.into();
                        current_as_pointer != *focus
                    }
                    None => true,
                };

                if should_update_focus {
                    self.update_keyboard_focus(pointer_location, serial);
                }
            }
        }

        pointer.motion(
            self,
            under,
            &MotionEvent {
                location: pointer_location,
                serial,
                time: evt.time_msec(),
            },
        );
        pointer.relative_motion(
            self,
            None,
            &RelativeMotionEvent {
                delta: evt.delta(),
                delta_unaccel: evt.delta_unaccel(),
                utime: evt.time(),
            },
        );
        pointer.frame(self);

        // Queue redraw for outputs where cursor is visible
    }

    /// Handle absolute pointer motion
    pub fn on_pointer_move_absolute<B: InputBackend>(
        &mut self,
        _dh: &smithay::reexports::wayland_server::DisplayHandle,
        evt: B::PointerMotionAbsoluteEvent,
    ) {
        let serial = SCOUNTER.next_serial();

        // For absolute motion, we need to determine which output it's on
        // This is typically for touch/tablet input which is output-specific

        let max_x = self.space().outputs().fold(0, |acc, o| {
            acc + self
                .space()
                .output_geometry(o)
                .map(|g| g.size.w)
                .unwrap_or(0)
        });

        let max_y = self
            .space()
            .outputs()
            .map(|o| {
                self.space()
                    .output_geometry(o)
                    .map(|g| g.size.h)
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0);

        let pos = evt.position();
        // Convert normalized coordinates to pixel coordinates
        let x = pos.x * max_x as f64;
        let y = pos.y * max_y as f64;
        let location = Point::from((x, y));

        // Clamp to screen boundaries
        let location = self.clamp_pointer_location(location);

        // Update physical layout manager's position if available
        if let Some(ref mut physical_layout) = self.physical_layout {
            physical_layout.set_logical_position(location);
        }

        let pointer = self.pointer().clone();
        let under = self.surface_under(location);

        // Update keyboard focus if focus_follows_mouse is enabled
        if self.config.focus_follows_mouse() {
            if let Some((focus, _)) = under.as_ref() {
                // Only update focus if we're hovering over a different window
                let current_focus = self.seat().get_keyboard().unwrap().current_focus();
                let should_update_focus = match current_focus {
                    Some(current) => {
                        // Convert current keyboard focus to pointer focus target for comparison
                        let current_as_pointer: PointerFocusTarget = current.into();
                        current_as_pointer != *focus
                    }
                    None => true,
                };

                if should_update_focus {
                    self.update_keyboard_focus(location, serial);
                }
            }
        }

        pointer.motion(
            self,
            under,
            &MotionEvent {
                location,
                serial,
                time: evt.time_msec(),
            },
        );
        pointer.frame(self);

        // Queue redraw for outputs where cursor is visible
    }
}

impl<BackendData: Backend> StilchState<BackendData> {
    /// Update keyboard focus when pointer is clicked
    pub(crate) fn update_keyboard_focus(&mut self, location: Point<f64, Logical>, serial: Serial) {
        tracing::info!("update_keyboard_focus called at location: {:?}", location);
        let keyboard = match self.seat().get_keyboard() {
            Some(kb) => kb,
            None => {
                tracing::warn!("No keyboard available for focus update");
                return;
            }
        };
        // change the keyboard focus unless the pointer or keyboard is grabbed
        // We test for any matching surface type here but always use the root
        // (in case of a window the toplevel) surface for the focus.
        // So for example if a user clicks on a subsurface or popup the toplevel
        // will receive the keyboard focus.
        let target = self.surface_under(location);
        tracing::info!("Surface under location: {:?}", target.is_some());
        if let Some((target, _loc)) = target {
            debug!("Focusing on {:?}", target);
            // If a parent surface has a keyboard grab, this prohibits changing keyboard focus
            if let Some(parent) = target.toplevel_surface() {
                if keyboard
                    .current_focus()
                    .map(|f| f.same_surface(&parent))
                    .unwrap_or(false)
                {
                    return;
                }
                if keyboard.is_grabbed() && !keyboard.has_grab(parent.id().protocol_id().into()) {
                    return;
                }
            }

            #[cfg(feature = "xwayland")]
            if let PointerFocusTarget::X11Surface(x11_surface) = &target {
                if !x11_surface.is_override_redirect() {
                    if let Some(xwm) = self.xwm.as_mut() {
                        if let Err(e) = xwm.raise_window(x11_surface) {
                            tracing::warn!("Failed to raise X11 window: {:?}", e);
                        }
                    } else {
                        tracing::warn!("XWM not available to raise window");
                    }
                }
            }

            // Check if clicked surface has a keyboard grab
            if !keyboard.is_grabbed() {
                // Find the window element that was clicked
                let window_element = match &target {
                    PointerFocusTarget::WlSurface(surface) => {
                        // Get the toplevel surface in case we clicked on a subsurface
                        let toplevel = target.toplevel_surface().unwrap_or_else(|| surface.clone());

                        // Try to find the corresponding window
                        let found = self
                            .space()
                            .elements()
                            .find(|w| {
                                w.wl_surface()
                                    .map(|s| s.as_ref() == &toplevel)
                                    .unwrap_or(false)
                            })
                            .cloned();
                        tracing::info!("Found window for WlSurface: {:?}", found.is_some());
                        found
                    }
                    #[cfg(feature = "xwayland")]
                    PointerFocusTarget::X11Surface(surface) => {
                        // Try to find the corresponding window for X11 surface
                        let found = self
                            .space()
                            .elements()
                            .find(|w| w.0.x11_surface().map(|s| s == surface).unwrap_or(false))
                            .cloned();
                        tracing::info!("Found window for X11Surface: {:?}", found.is_some());
                        found
                    }
                    PointerFocusTarget::SSD(_) => {
                        tracing::info!("Click on SSD, no window focus");
                        None
                    }
                };

                if let Some(window) = window_element {
                    // Use the proper focus_window method that handles all the necessary updates
                    tracing::info!("Calling focus_window for the clicked window");
                    self.focus_window(&window);
                } else {
                    tracing::warn!("No window element found for the clicked surface");
                }
            }
        } else {
            // Clear focus when clicking on empty space
            keyboard.set_focus(self, None, serial);
        }
    }
}
