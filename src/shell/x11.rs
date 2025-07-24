use std::{cell::RefCell, os::unix::io::OwnedFd};

use smithay::{
    desktop::Window,
    input::pointer::Focus,
    utils::{Logical, Rectangle, SERIAL_COUNTER},
    wayland::{
        selection::{
            data_device::{
                clear_data_device_selection, current_data_device_selection_userdata,
                request_data_device_client_selection, set_data_device_selection,
            },
            primary_selection::{
                clear_primary_selection, current_primary_selection_userdata,
                request_primary_client_selection, set_primary_selection,
            },
            SelectionTarget,
        },
        xwayland_shell::{XWaylandShellHandler, XWaylandShellState},
    },
    xwayland::{
        xwm::{Reorder, ResizeEdge as X11ResizeEdge, XwmId},
        X11Surface, X11Wm, XwmHandler,
    },
};
use tracing::{error, info, trace, warn};

use crate::{focus::KeyboardFocusTarget, state::Backend, StilchState};

use super::{FullscreenSurface, PointerMoveSurfaceGrab, TouchMoveSurfaceGrab, WindowElement};

#[derive(Debug, Default)]
struct OldGeometry(RefCell<Option<Rectangle<i32, Logical>>>);
impl OldGeometry {
    pub fn save(&self, geo: Rectangle<i32, Logical>) {
        *self.0.borrow_mut() = Some(geo);
    }

    pub fn restore(&self) -> Option<Rectangle<i32, Logical>> {
        self.0.borrow_mut().take()
    }
}

impl<BackendData: Backend> XWaylandShellHandler for StilchState<BackendData> {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.protocols.xwayland_shell_state
    }
}

impl<BackendData: Backend> XwmHandler for StilchState<BackendData> {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.xwm
            .as_mut()
            // SAFETY: This is only called after X11 is initialized via start_xwayland()
            .expect("X11 window manager not initialized")
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}
    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Err(e) = window.set_mapped(true) {
            tracing::error!("Failed to set X11 window as mapped: {:?}", e);
            return;
        }
        let window_element = WindowElement(Window::new_x11_window(window));

        // Add to window registry and workspace using the new system
        // Find which virtual output contains this window
        let pointer_loc = self.pointer().current_location();
        let pointer_loc_i32 =
            smithay::utils::Point::from((pointer_loc.x as i32, pointer_loc.y as i32));

        if let Some(virtual_output_id) = self
            .virtual_output_manager
            .virtual_output_at(pointer_loc_i32)
        {
            if let Some(window_id) = self.add_window(window_element.clone(), virtual_output_id) {
                // Configure the X11 window geometry
                if let Some(managed_window) = self.window_registry().get(window_id) {
                    if let Some(xsurface) = managed_window.element.0.x11_surface() {
                        if let Some(bbox) = self.space().element_bbox(&managed_window.element) {
                            if let Err(e) = xsurface.configure(Some(bbox)) {
                                tracing::error!("Failed to configure X11 surface: {:?}", e);
                            }
                        } else {
                            tracing::warn!("No bounding box for X11 window element");
                        }
                    }
                }
            } else {
                // Failed to add window through new system
                error!(
                    "Failed to add X11 window to virtual output {}",
                    virtual_output_id
                );
            }
        } else {
            // No virtual output at pointer location - try to find any virtual output
            let first_vo_id = self
                .virtual_output_manager
                .all_virtual_outputs()
                .next()
                .map(|vo| vo.id());
            if let Some(first_vo_id) = first_vo_id {
                warn!(
                    "No virtual output at pointer location, using first available: {}",
                    first_vo_id
                );
                if let Some(window_id) = self.add_window(window_element.clone(), first_vo_id) {
                    info!(
                        "Successfully added X11 window {} to first available virtual output",
                        window_id
                    );
                    // Configure the X11 window geometry
                    if let Some(managed_window) = self.window_registry().get(window_id) {
                        if let Some(xsurface) = managed_window.element.0.x11_surface() {
                            if let Some(bbox) = self.space().element_bbox(&managed_window.element) {
                                if let Err(e) = xsurface.configure(Some(bbox)) {
                                    tracing::error!("Failed to configure X11 surface: {:?}", e);
                                }
                            } else {
                                tracing::warn!("No bounding box for X11 window element");
                            }
                        }
                    }
                }
            } else {
                error!("No virtual outputs available! Cannot place X11 window.");
            }
        }

        window_element.set_ssd(false);
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        let location = window.geometry().loc;
        let window_element = WindowElement(Window::new_x11_window(window));

        // Override redirect windows bypass normal window management
        // but we still want to track them in the registry
        // They don't belong to any workspace and maintain their own position

        // Override-redirect windows bypass the window manager
        // Use the centralized update method which handles unmanaged windows
        self.window_manager
            .update_element_position(&window_element, location);
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        // Find the window element
        let maybe = self
            .space()
            .elements()
            .find(|e| matches!(e.0.x11_surface(), Some(w) if w == &window))
            .cloned();

        if let Some(elem) = maybe {
            // Check if it's in the registry
            if let Some(window_id) = self.window_registry().find_by_element(&elem) {
                // Use the proper window removal flow
                if let Some(managed_window) = self.window_registry().get(window_id) {
                    let workspace_id = managed_window.workspace;

                    // Remove from workspace
                    if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id)
                    {
                        workspace.remove_window(window_id);
                    }

                    // Remove from window manager (this will emit the event and unmap from space)
                    let (removed, event) = self.window_manager.remove_window(window_id);
                    if let Some(event) = event {
                        self.event_bus.emit_window(event);
                    }
                    if removed {
                        tracing::debug!("Removed X11 window {} from manager", window_id);
                    }

                    // Update layout if needed
                    if let Some(vo_id) = self.virtual_output_at_pointer() {
                        if let Some(vo) = self.virtual_output_manager.get(vo_id) {
                            if vo.active_workspace() == Some(workspace_id.get() as usize) {
                                self.apply_workspace_layout(workspace_id);
                            }
                        }
                    }
                }
            } else {
                // Not in registry (e.g., override redirect), just unmap
                self.space_mut().unmap_elem(&elem)
            }
        }

        if !window.is_override_redirect() {
            if let Err(e) = window.set_mapped(false) {
                tracing::error!("Failed to unmap X11 window: {:?}", e);
            }
        }
    }

    fn destroyed_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        _x: Option<i32>,
        _y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        // we just set the new size, but don't let windows move themselves around freely
        let mut geo = window.geometry();
        if let Some(w) = w {
            geo.size.w = w as i32;
        }
        if let Some(h) = h {
            geo.size.h = h as i32;
        }
        let _ = window.configure(geo);
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        geometry: Rectangle<i32, Logical>,
        _above: Option<u32>,
    ) {
        let Some(elem) = self
            .space()
            .elements()
            .find(|e| matches!(e.0.x11_surface(), Some(w) if w == &window))
            .cloned()
        else {
            return;
        };
        self.window_manager
            .update_element_position(&elem, geometry.loc);
        // TODO: We don't properly handle the order of override-redirect windows here,
        //       they are always mapped top and then never reordered.
    }

    fn maximize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        self.maximize_request_x11(&window);
    }

    fn unmaximize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        let Some(elem) = self
            .space()
            .elements()
            .find(|e| matches!(e.0.x11_surface(), Some(w) if w == &window))
            .cloned()
        else {
            return;
        };

        if let Err(e) = window.set_maximized(false) {
            tracing::error!("Failed to unmaximize X11 window: {:?}", e);
        }
        if let Some(old_geo) = window
            .user_data()
            .get::<OldGeometry>()
            .and_then(|data| data.restore())
        {
            if let Err(e) = window.configure(old_geo) {
                tracing::error!("Failed to restore X11 window geometry: {:?}", e);
            }
            self.window_manager
                .update_element_position(&elem, old_geo.loc);
        }
    }

    fn fullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(elem) = self
            .space()
            .elements()
            .find(|e| matches!(e.0.x11_surface(), Some(w) if w == &window))
        {
            let outputs_for_window = self.space().outputs_for_element(elem);
            let output = outputs_for_window
                .first()
                // The window hasn't been mapped yet, use the primary output instead
                .or_else(|| self.space().outputs().next())
                // SAFETY: We always have at least one output in normal operation
                .expect("No outputs found");
            let geometry = self.space().output_geometry(output).unwrap_or_else(|| {
                tracing::error!("No geometry for output, using default");
                Rectangle::from_size((1920, 1080).into())
            });

            if let Err(e) = window.set_fullscreen(true) {
                tracing::error!("Failed to set X11 window fullscreen: {:?}", e);
            }
            elem.set_ssd(false);
            if let Err(e) = window.configure(geometry) {
                tracing::error!("Failed to configure X11 window geometry: {:?}", e);
            }
            output
                .user_data()
                .insert_if_missing(FullscreenSurface::default);
            output
                .user_data()
                .get::<FullscreenSurface>()
                // SAFETY: We just inserted this above with insert_if_missing
                .expect("FullscreenSurface was just inserted")
                .set(elem.clone());
            trace!("Fullscreening: {:?}", elem);
        }
    }

    fn unfullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        // Find element and output first
        let elem_and_output = self
            .space()
            .elements()
            .find(|e| matches!(e.0.x11_surface(), Some(w) if w == &window))
            .cloned()
            .and_then(|elem| {
                self.space()
                    .outputs()
                    .find(|o| {
                        o.user_data()
                            .get::<FullscreenSurface>()
                            .and_then(|f| f.get())
                            .map(|w| &w == &elem)
                            .unwrap_or(false)
                    })
                    .cloned()
                    .map(|output| (elem, output))
            });

        if let Some((elem, output)) = elem_and_output {
            if let Err(e) = window.set_fullscreen(false) {
                tracing::error!("Failed to unset X11 window fullscreen: {:?}", e);
            }
            elem.set_ssd(false);
            trace!("Unfullscreening: {:?}", elem);
            if let Some(fs_surface) = output.user_data().get::<FullscreenSurface>() {
                fs_surface.clear();
            } else {
                tracing::warn!("No FullscreenSurface data on output");
            }
            if let Some(bbox) = self.space().element_bbox(&elem) {
                if let Err(e) = window.configure(bbox) {
                    tracing::error!("Failed to configure X11 window: {:?}", e);
                }
            } else {
                tracing::warn!("No bounding box for X11 window");
            }
            self.backend_data.reset_buffers(&output);
        }
    }

    fn resize_request(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _button: u32,
        _edges: X11ResizeEdge,
    ) {
        // In a tiling window manager, we don't allow manual window resizing
    }

    fn move_request(&mut self, _xwm: XwmId, _window: X11Surface, _button: u32) {
        // In a tiling window manager, we don't allow manual window movement
    }

    fn allow_selection_access(&mut self, xwm: XwmId, _selection: SelectionTarget) -> bool {
        if let Some(keyboard) = self.seat().get_keyboard() {
            // check that an X11 window is focused
            if let Some(KeyboardFocusTarget::Window(w)) = keyboard.current_focus() {
                if let Some(surface) = w.x11_surface() {
                    if surface.xwm_id() == Some(xwm) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn send_selection(
        &mut self,
        _xwm: XwmId,
        selection: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
    ) {
        match selection {
            SelectionTarget::Clipboard => {
                if let Err(err) = request_data_device_client_selection(self.seat(), mime_type, fd) {
                    error!(
                        ?err,
                        "Failed to request current wayland clipboard for Xwayland",
                    );
                }
            }
            SelectionTarget::Primary => {
                if let Err(err) = request_primary_client_selection(self.seat(), mime_type, fd) {
                    error!(
                        ?err,
                        "Failed to request current wayland primary selection for Xwayland",
                    );
                }
            }
        }
    }

    fn new_selection(&mut self, _xwm: XwmId, selection: SelectionTarget, mime_types: Vec<String>) {
        trace!(?selection, ?mime_types, "Got Selection from X11",);
        // TODO check, that focused windows is X11 window before doing this
        match selection {
            SelectionTarget::Clipboard => {
                set_data_device_selection(&self.display_handle, self.seat(), mime_types, ())
            }
            SelectionTarget::Primary => {
                set_primary_selection(&self.display_handle, self.seat(), mime_types, ())
            }
        }
    }

    fn cleared_selection(&mut self, _xwm: XwmId, selection: SelectionTarget) {
        match selection {
            SelectionTarget::Clipboard => {
                if current_data_device_selection_userdata(self.seat()).is_some() {
                    clear_data_device_selection(&self.display_handle, self.seat())
                }
            }
            SelectionTarget::Primary => {
                if current_primary_selection_userdata(self.seat()).is_some() {
                    clear_primary_selection(&self.display_handle, self.seat())
                }
            }
        }
    }
}

impl<BackendData: Backend> StilchState<BackendData> {
    pub fn maximize_request_x11(&mut self, window: &X11Surface) {
        let Some(elem) = self
            .space()
            .elements()
            .find(|e| matches!(e.0.x11_surface(), Some(w) if w == window))
            .cloned()
        else {
            return;
        };

        let old_geo = self.space().element_bbox(&elem).unwrap_or_else(|| {
            tracing::warn!("No bounding box for element, using default");
            Rectangle::from_size((800, 600).into())
        });
        let outputs_for_window = self.space().outputs_for_element(&elem);
        let output = outputs_for_window
            .first()
            // The window hasn't been mapped yet, use the primary output instead
            .or_else(|| self.space().outputs().next())
            // SAFETY: We always have at least one output in normal operation
            .expect("No outputs found");
        let geometry = self.space().output_geometry(output).unwrap_or_else(|| {
            tracing::error!("No geometry for output, using default");
            Rectangle::from_size((1920, 1080).into())
        });

        if let Err(e) = window.set_maximized(true) {
            tracing::error!("Failed to maximize X11 window: {:?}", e);
        }
        if let Err(e) = window.configure(geometry) {
            tracing::error!("Failed to configure X11 window: {:?}", e);
        }
        window.user_data().insert_if_missing(OldGeometry::default);
        window
            .user_data()
            .get::<OldGeometry>()
            // SAFETY: We just inserted this above with insert_if_missing
            .expect("OldGeometry was just inserted")
            .save(old_geo);
        self.window_manager
            .update_element_position(&elem, geometry.loc);
    }

    pub fn move_request_x11(&mut self, window: &X11Surface) {
        if let Some(touch) = self.seat().get_touch() {
            if let Some(start_data) = touch.grab_start_data() {
                let element = self
                    .space()
                    .elements()
                    .find(|e| matches!(e.0.x11_surface(), Some(w) if w == window));

                if let Some(element) = element {
                    let mut initial_window_location =
                        self.space().element_location(element).unwrap_or_else(|| {
                            tracing::warn!("No location for element, using pointer location");
                            let pos = self.pointer().current_location();
                            (pos.x as i32, pos.y as i32).into()
                        });

                    // If surface is maximized then unmaximize it
                    if window.is_maximized() {
                        if let Err(e) = window.set_maximized(false) {
                            tracing::error!("Failed to unmaximize X11 window: {:?}", e);
                        }
                        let pos = start_data.location;
                        initial_window_location = (pos.x as i32, pos.y as i32).into();
                        if let Some(old_geo) = window
                            .user_data()
                            .get::<OldGeometry>()
                            .and_then(|data| data.restore())
                        {
                            if let Err(e) = window
                                .configure(Rectangle::new(initial_window_location, old_geo.size))
                            {
                                tracing::error!("Failed to configure X11 window: {:?}", e);
                            }
                        }
                    }

                    let grab = TouchMoveSurfaceGrab {
                        start_data,
                        window: element.clone(),
                        initial_window_location,
                    };

                    touch.set_grab(self, grab, SERIAL_COUNTER.next_serial());
                    return;
                }
            }
        }

        // luckily stilch only supports one seat anyway...
        let Some(start_data) = self.pointer().grab_start_data() else {
            return;
        };

        let Some(element) = self
            .space()
            .elements()
            .find(|e| matches!(e.0.x11_surface(), Some(w) if w == window))
        else {
            return;
        };

        let mut initial_window_location =
            self.space().element_location(element).unwrap_or_else(|| {
                tracing::warn!("No location for element, using pointer location");
                let pos = self.pointer().current_location();
                (pos.x as i32, pos.y as i32).into()
            });

        // If surface is maximized then unmaximize it
        if window.is_maximized() {
            if let Err(e) = window.set_maximized(false) {
                tracing::error!("Failed to unmaximize X11 window: {:?}", e);
            }
            let pos = self.pointer().current_location();
            initial_window_location = (pos.x as i32, pos.y as i32).into();
            if let Some(old_geo) = window
                .user_data()
                .get::<OldGeometry>()
                .and_then(|data| data.restore())
            {
                if let Err(e) =
                    window.configure(Rectangle::new(initial_window_location, old_geo.size))
                {
                    tracing::error!("Failed to configure X11 window: {:?}", e);
                }
            }
        }

        let grab = PointerMoveSurfaceGrab {
            start_data,
            window: element.clone(),
            initial_window_location,
        };

        let pointer = self.pointer().clone();
        pointer.set_grab(self, grab, SERIAL_COUNTER.next_serial(), Focus::Clear);
    }
}
