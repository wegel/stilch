//! Keyboard input handling

use smithay::{
    backend::input::{Event, InputBackend, KeyState, KeyboardKeyEvent},
    desktop::layer_map_for_output,
    input::keyboard::FilterResult,
    utils::SERIAL_COUNTER as SCOUNTER,
    wayland::{
        compositor::with_states,
        keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitorSeat,
        shell::wlr_layer::{KeyboardInteractivity, Layer as WlrLayer, LayerSurfaceCachedState},
    },
};
use tracing::debug;

#[cfg(feature = "udev")]
use smithay::backend::session::Session;

use crate::{
    keybindings::KeyAction,
    state::{Backend, StilchState},
};

impl<BackendData: Backend> StilchState<BackendData> {
    /// Process keyboard key events for windowed backends
    pub fn on_keyboard_key_windowed<B: InputBackend>(&mut self, evt: B::KeyboardKeyEvent) {
        match self.keyboard_key_to_action::<B>(evt) {
            KeyAction::None => {}
            action => {
                debug!("Windowed backend key action: {:?}", action);
                self.process_common_key_action(action);
            }
        }
    }

    /// Convert keyboard event to action, handling focus and keybindings
    fn keyboard_key_to_action<B: InputBackend>(&mut self, evt: B::KeyboardKeyEvent) -> KeyAction {
        let keycode = evt.key_code();
        let state = evt.state();
        debug!(?keycode, ?state, "key");
        let serial = SCOUNTER.next_serial();
        let time = Event::time_msec(&evt);
        let keyboard = self
            .seat()
            .get_keyboard()
            // SAFETY: We always initialize the keyboard with the seat
            .expect("Keyboard not initialized");

        // Check layer shell exclusive keyboard
        for layer in self.protocols.layer_shell_state.layer_surfaces().rev() {
            let data = with_states(layer.wl_surface(), |states| {
                *states
                    .cached_state
                    .get::<LayerSurfaceCachedState>()
                    .current()
            });
            if data.keyboard_interactivity == KeyboardInteractivity::Exclusive
                && (data.layer == WlrLayer::Top || data.layer == WlrLayer::Overlay)
            {
                let surface = self.space().outputs().find_map(|o| {
                    let map = layer_map_for_output(o);
                    let cloned = map.layers().find(|l| l.layer_surface() == &layer).cloned();
                    cloned
                });
                if let Some(surface) = surface {
                    keyboard.set_focus(self, Some(surface.into()), serial);
                    keyboard.input::<(), _>(self, keycode, state, serial, time, |_, _, _| {
                        FilterResult::Forward
                    });
                    return KeyAction::None;
                };
            }
        }

        let inhibited = self
            .space()
            .element_under(self.pointer().current_location())
            .and_then(|(window, _)| {
                let surface = window.wl_surface()?;
                self.seat()
                    .keyboard_shortcuts_inhibitor_for_surface(&surface)
            })
            .map(|inhibitor| inhibitor.is_active())
            .unwrap_or(false);

        // Process the key input, checking for keybindings
        let action = keyboard.input(
            self,
            keycode,
            state,
            serial,
            time,
            |stilch, modifiers, handle| {
                // Get both modified and raw keysyms
                let modified_keysym = handle.modified_sym();
                let raw_keysyms = handle.raw_syms();
                let raw_keysym = raw_keysyms.first().copied();

                debug!(
                    ?state,
                    mods = ?modifiers,
                    raw_keysym = raw_keysym.map(|k| ::xkbcommon::xkb::keysym_get_name(k)),
                    modified_keysym = ::xkbcommon::xkb::keysym_get_name(modified_keysym),
                    "keysym"
                );

                // Check if keyboard is grabbed
                if keyboard.is_grabbed() && !inhibited {
                    return FilterResult::Forward;
                }

                // Only check keybindings on key press, not release
                if let KeyState::Pressed = state {
                    if !inhibited {
                        // Check if this is a keybinding
                        match stilch.process_keybinding(
                            *modifiers,
                            modified_keysym,
                            raw_keysym,
                            state,
                        ) {
                            FilterResult::Intercept(action) => {
                                // Suppress the raw keysym if available, otherwise the modified one
                                let keysym_to_suppress = raw_keysym.unwrap_or(modified_keysym);
                                stilch
                                    .input_manager
                                    .suppressed_keys
                                    .push(keysym_to_suppress);
                                // Return the action to be handled after this closure
                                return FilterResult::Intercept(action);
                            }
                            FilterResult::Forward => {
                                // Not a keybinding, forward to client
                                return FilterResult::Forward;
                            }
                        }
                    } else {
                        // Forward when inhibited
                        return FilterResult::Forward;
                    }
                } else {
                    // Key release - check if it was suppressed
                    let keysym_to_check = raw_keysym.unwrap_or(modified_keysym);
                    let suppressed = stilch
                        .input_manager
                        .suppressed_keys
                        .contains(&keysym_to_check);
                    if suppressed {
                        stilch
                            .input_manager
                            .suppressed_keys
                            .retain(|k| *k != keysym_to_check);
                        return FilterResult::Intercept(KeyAction::None);
                    } else {
                        return FilterResult::Forward;
                    }
                }
            },
        );

        action.unwrap_or(KeyAction::None)
    }

    // Allow in this method because of existing usage
    #[allow(clippy::uninlined_format_args)]
    fn process_common_key_action(&mut self, action: KeyAction) {
        self.handle_key_action(action);
    }
}

#[cfg(feature = "udev")]
impl StilchState<crate::udev::UdevData> {
    /// Process keyboard key events for udev backend
    pub fn on_keyboard_key<B: InputBackend>(&mut self, evt: B::KeyboardKeyEvent) {
        match self.keyboard_key_to_action::<B>(evt) {
            KeyAction::VtSwitch(vt) => {
                tracing::info!("Switching to vt {vt}");
                if let Err(err) = self.backend_data.session.change_vt(vt) {
                    tracing::error!("Error switching to vt {}: {}", vt, err);
                }
            }
            KeyAction::None => {}
            action => self.process_common_key_action(action),
        }
    }
}
