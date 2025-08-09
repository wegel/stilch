//! Data device and drag-and-drop protocol handlers

use std::os::unix::io::OwnedFd;

use smithay::{
    delegate_data_device,
    input::{
        pointer::{CursorImageStatus, CursorImageSurfaceData},
        Seat,
    },
    reexports::wayland_server::protocol::{wl_data_source::WlDataSource, wl_surface::WlSurface},
    utils::Point,
    wayland::{
        compositor::with_states,
        selection::data_device::{
            ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
        },
    },
};

use crate::state::{Backend, DndIcon, StilchState};

impl<BackendData: Backend> DataDeviceHandler for StilchState<BackendData> {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.protocols.data_device_state
    }
}

impl<BackendData: Backend> ClientDndGrabHandler for StilchState<BackendData> {
    fn started(
        &mut self,
        _source: Option<WlDataSource>,
        icon: Option<WlSurface>,
        _seat: Seat<Self>,
    ) {
        let offset =
            if let CursorImageStatus::Surface(ref surface) = self.input_manager.cursor_status {
                with_states(surface, |states| {
                    let hotspot = states
                        .data_map
                        .get::<CursorImageSurfaceData>()
                        .and_then(|data| data.lock().ok())
                        .map(|data| data.hotspot)
                        .unwrap_or_else(|| {
                            tracing::warn!("No cursor hotspot data available");
                            (0, 0).into()
                        });
                    Point::from((-hotspot.x, -hotspot.y))
                })
            } else {
                (0, 0).into()
            };
        self.input_manager.dnd_icon = icon.map(|surface| DndIcon { surface, offset });
    }

    fn dropped(&mut self, _target: Option<WlSurface>, _validated: bool, _seat: Seat<Self>) {
        self.input_manager.dnd_icon = None;
    }
}

impl<BackendData: Backend> ServerDndGrabHandler for StilchState<BackendData> {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {
        unreachable!("stilch doesn't do server-side grabs");
    }
}

delegate_data_device!(@<BackendData: Backend + 'static> StilchState<BackendData>);
