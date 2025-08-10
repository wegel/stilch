//! Miscellaneous protocol handlers (output, selection, shm, etc.)

use std::os::unix::io::OwnedFd;

use smithay::{
    delegate_data_control, delegate_fractional_scale, delegate_output, delegate_presentation,
    delegate_primary_selection, delegate_security_context, delegate_shm, delegate_viewporter,
    delegate_xdg_activation, delegate_xdg_decoration, delegate_xdg_foreign,
    input::Seat,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    wayland::{
        fractional_scale::FractionalScaleHandler,
        output::OutputHandler,
        security_context::{SecurityContext, SecurityContextHandler},
        selection::{
            primary_selection::{PrimarySelectionHandler, PrimarySelectionState},
            wlr_data_control::{DataControlHandler, DataControlState},
            SelectionHandler, SelectionSource, SelectionTarget,
        },
        shell::xdg::decoration::XdgDecorationHandler,
        shm::{ShmHandler, ShmState},
        xdg_activation::{
            XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
        },
        xdg_foreign::{XdgForeignHandler, XdgForeignState},
    },
};
use tracing::warn;

use crate::state::{Backend, StilchState};

impl<BackendData: Backend> OutputHandler for StilchState<BackendData> {}

impl<BackendData: Backend> SelectionHandler for StilchState<BackendData> {
    type SelectionUserData = ();

    #[cfg(feature = "xwayland")]
    fn new_selection(
        &mut self,
        ty: SelectionTarget,
        source: Option<SelectionSource>,
        _seat: Seat<Self>,
    ) {
        if let Some(xwm) = self.xwm.as_mut() {
            if let Err(err) = xwm.new_selection(ty, source.map(|source| source.mime_types())) {
                warn!(?err, ?ty, "Failed to set Xwayland selection");
            }
        }
    }

    #[cfg(feature = "xwayland")]
    fn send_selection(
        &mut self,
        ty: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Self>,
        _user_data: &(),
    ) {
        if let Some(xwm) = self.xwm.as_mut() {
            if let Err(err) = xwm.send_selection(ty, mime_type, fd, self.handle.clone()) {
                warn!(?err, "Failed to send primary (X11 -> Wayland)");
            }
        }
    }
}

impl<BackendData: Backend> PrimarySelectionHandler for StilchState<BackendData> {
    fn primary_selection_state(&mut self) -> &mut PrimarySelectionState {
        &mut self.protocols.primary_selection_state
    }
}

impl<BackendData: Backend> DataControlHandler for StilchState<BackendData> {
    fn data_control_state(&mut self) -> &mut DataControlState {
        &mut self.protocols.data_control_state
    }
}

impl<BackendData: Backend> ShmHandler for StilchState<BackendData> {
    fn shm_state(&self) -> &ShmState {
        &self.protocols.shm_state
    }
}

impl<BackendData: Backend> XdgActivationHandler for StilchState<BackendData> {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.protocols.xdg_activation_state
    }

    fn request_activation(
        &mut self,
        _token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        if token_data.timestamp.elapsed() < std::time::Duration::from_secs(10) {
            // Just grant the wish
            let w = self
                .space()
                .elements()
                .find(|window| {
                    window
                        .wl_surface()
                        .map(|s| s.as_ref() == &surface)
                        .unwrap_or(false)
                })
                .cloned();
            if let Some(window) = w {
                self.space_mut().raise_element(&window, true);
            }
        } else {
            tracing::info!("Activation request was too old, ignoring");
        }
    }
}

impl<BackendData: Backend> XdgDecorationHandler for StilchState<BackendData> {
    fn new_decoration(&mut self, toplevel: smithay::wayland::shell::xdg::ToplevelSurface) {
        use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
        // Tell clients we'll handle decorations (but we won't actually draw any)
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });
    }

    fn request_mode(
        &mut self,
        toplevel: smithay::wayland::shell::xdg::ToplevelSurface,
        _mode: smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode,
    ) {
        use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;

        // Always tell clients we're doing server-side decorations
        // This prevents them from drawing their own decorations
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });

        // If the initial configure has been sent, send a new configure
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }

    fn unset_mode(&mut self, toplevel: smithay::wayland::shell::xdg::ToplevelSurface) {
        use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
        // Even when unset, claim we're doing server-side decorations
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });

        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }
}

impl<BackendData: Backend> FractionalScaleHandler for StilchState<BackendData> {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        // find the matching window and output
        let window = self.space().elements().find(|window| {
            window
                .wl_surface()
                .map(|s| s.as_ref() == &surface)
                .unwrap_or(false)
        });
        if let Some(window) = window {
            if let Some(output) = self.space().outputs_for_element(window).first() {
                use smithay::wayland::compositor::with_states;
                let scale = output.current_scale().fractional_scale();
                with_states(&surface, |data| {
                    smithay::wayland::fractional_scale::with_fractional_scale(
                        data,
                        |fractional_scale| {
                            fractional_scale.set_preferred_scale(scale);
                        },
                    );
                });
            }
        }
    }
}

impl<BackendData: Backend + 'static> SecurityContextHandler for StilchState<BackendData> {
    fn context_created(
        &mut self,
        _source: smithay::wayland::security_context::SecurityContextListenerSource,
        _context: SecurityContext,
    ) {
        // Handle security context creation
        // The actual client insertion is handled elsewhere
    }
}

// XWaylandKeyboardGrabHandler implementation remains in state/main.rs
// due to complex integration with XWayland state

impl<BackendData: Backend> XdgForeignHandler for StilchState<BackendData> {
    fn xdg_foreign_state(&mut self) -> &mut XdgForeignState {
        &mut self.protocols.xdg_foreign_state
    }
}

// ViewporterHandler and PresentationHandler don't exist as traits in current Smithay
// These protocols are handled automatically by their respective state objects

delegate_output!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_primary_selection!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_data_control!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_shm!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_xdg_activation!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_xdg_decoration!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_fractional_scale!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_security_context!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_xdg_foreign!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_viewporter!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_presentation!(@<BackendData: Backend + 'static> StilchState<BackendData>);
