//! Protocol state management
//!
//! This module contains all Wayland protocol states in a single struct,
//! separating protocol concerns from the main application state.

use smithay::{
    input::SeatState,
    wayland::{
        commit_timing::CommitTimingManagerState,
        compositor::CompositorState,
        cursor_shape::CursorShapeManagerState,
        fifo::FifoManagerState,
        fractional_scale::FractionalScaleManagerState,
        keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitState,
        output::OutputManagerState,
        presentation::PresentationState,
        selection::{
            data_device::DataDeviceState, primary_selection::PrimarySelectionState,
            wlr_data_control::DataControlState,
        },
        shell::{
            wlr_layer::WlrLayerShellState,
            xdg::{decoration::XdgDecorationState, XdgShellState},
        },
        shm::ShmState,
        single_pixel_buffer::SinglePixelBufferState,
        viewporter::ViewporterState,
        xdg_activation::XdgActivationState,
        xdg_foreign::XdgForeignState,
    },
};

#[cfg(feature = "xwayland")]
use smithay::wayland::xwayland_shell::XWaylandShellState;

use crate::state::{Backend, StilchState};

/// Container for all Wayland protocol states
#[derive(Debug)]
pub struct ProtocolState<BackendData: Backend + 'static> {
    // Core protocols
    pub compositor_state: CompositorState,
    pub shm_state: ShmState,
    pub seat_state: SeatState<StilchState<BackendData>>,

    // Data transfer protocols
    pub data_device_state: DataDeviceState,
    pub primary_selection_state: PrimarySelectionState,
    pub data_control_state: DataControlState,

    // Shell protocols
    pub xdg_shell_state: XdgShellState,
    pub layer_shell_state: WlrLayerShellState,
    pub xdg_decoration_state: XdgDecorationState,

    // Window management protocols
    pub xdg_activation_state: XdgActivationState,
    pub xdg_foreign_state: XdgForeignState,
    pub keyboard_shortcuts_inhibit_state: KeyboardShortcutsInhibitState,

    // Output and presentation protocols
    pub output_manager_state: OutputManagerState,
    pub presentation_state: PresentationState,
    pub fractional_scale_manager_state: FractionalScaleManagerState,
    pub viewporter_state: ViewporterState,

    // Buffer management protocols
    pub single_pixel_buffer_state: SinglePixelBufferState,
    pub fifo_manager_state: FifoManagerState,
    pub commit_timing_manager_state: CommitTimingManagerState,

    // Cursor support
    pub cursor_shape_manager_state: CursorShapeManagerState,

    // XWayland support
    #[cfg(feature = "xwayland")]
    pub xwayland_shell_state: XWaylandShellState,
}

impl<BackendData: Backend + 'static> ProtocolState<BackendData> {
    /// Create a new ProtocolState with all protocols initialized
    pub fn new(
        display_handle: &smithay::reexports::wayland_server::DisplayHandle,
        seat_state: SeatState<StilchState<BackendData>>,
    ) -> Self {
        // Create clock for presentation state
        use smithay::utils::{Clock, Monotonic};
        let clock = Clock::<Monotonic>::new();

        Self {
            // Core protocols
            compositor_state: CompositorState::new_v6::<StilchState<BackendData>>(display_handle),
            shm_state: ShmState::new::<StilchState<BackendData>>(display_handle, vec![]),
            seat_state,

            // Data transfer protocols
            data_device_state: DataDeviceState::new::<StilchState<BackendData>>(display_handle),
            primary_selection_state: PrimarySelectionState::new::<StilchState<BackendData>>(
                display_handle,
            ),
            data_control_state: DataControlState::new::<StilchState<BackendData>, _>(
                display_handle,
                None,
                |_| true,
            ),

            // Shell protocols
            xdg_shell_state: XdgShellState::new::<StilchState<BackendData>>(display_handle),
            layer_shell_state: WlrLayerShellState::new::<StilchState<BackendData>>(display_handle),
            xdg_decoration_state: XdgDecorationState::new::<StilchState<BackendData>>(
                display_handle,
            ),

            // Window management protocols
            xdg_activation_state: XdgActivationState::new::<StilchState<BackendData>>(
                display_handle,
            ),
            xdg_foreign_state: XdgForeignState::new::<StilchState<BackendData>>(display_handle),
            keyboard_shortcuts_inhibit_state: KeyboardShortcutsInhibitState::new::<
                StilchState<BackendData>,
            >(display_handle),

            // Output and presentation protocols
            output_manager_state: OutputManagerState::new_with_xdg_output::<StilchState<BackendData>>(
                display_handle,
            ),
            presentation_state: PresentationState::new::<StilchState<BackendData>>(
                display_handle,
                clock.id() as u32,
            ),
            fractional_scale_manager_state: FractionalScaleManagerState::new::<
                StilchState<BackendData>,
            >(display_handle),
            viewporter_state: ViewporterState::new::<StilchState<BackendData>>(display_handle),

            // Buffer management protocols
            single_pixel_buffer_state: SinglePixelBufferState::new::<StilchState<BackendData>>(
                display_handle,
            ),
            fifo_manager_state: FifoManagerState::new::<StilchState<BackendData>>(display_handle),
            commit_timing_manager_state: CommitTimingManagerState::new::<StilchState<BackendData>>(
                display_handle,
            ),

            // Cursor support
            cursor_shape_manager_state: CursorShapeManagerState::new::<StilchState<BackendData>>(
                display_handle,
            ),

            // XWayland support
            #[cfg(feature = "xwayland")]
            xwayland_shell_state: XWaylandShellState::new::<StilchState<BackendData>>(
                display_handle,
            ),
        }
    }
}
