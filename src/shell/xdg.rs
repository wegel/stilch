use std::cell::RefCell;

use smithay::{
    desktop::{
        find_popup_root_surface, get_popup_toplevel_coords, layer_map_for_output,
        space::SpaceElement, PopupKeyboardGrab, PopupKind, PopupPointerGrab, PopupUngrabStrategy,
        Space, Window, WindowSurfaceType,
    },
    input::{pointer::Focus, Seat},
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            protocol::{wl_output, wl_seat, wl_surface::WlSurface},
            Resource,
        },
    },
    utils::{Logical, Point, Serial},
    wayland::{
        compositor::{self, with_states},
        seat::WaylandFocus,
        shell::xdg::{
            Configure, PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler,
            XdgShellState, XdgToplevelSurfaceData,
        },
    },
};
use tracing::{debug, error, info, trace, warn};

use crate::{
    focus::KeyboardFocusTarget,
    shell::TouchMoveSurfaceGrab,
    state::{StilchState, Backend},
};

use super::{PointerMoveSurfaceGrab, ResizeEdge, ResizeState, SurfaceData, WindowElement};

impl<BackendData: Backend> XdgShellHandler for StilchState<BackendData> {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.protocols.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        info!(
            "New toplevel surface created: {:?}",
            surface.wl_surface().id()
        );

        // Log the call stack to understand why we're being called multiple times
        let backtrace = std::backtrace::Backtrace::capture();
        if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            trace!("new_toplevel called from:\n{backtrace}");
        }
        // Do not send a configure here, the initial configure
        // of a xdg_surface has to be sent during the commit if
        // the surface is not already configured

        // Log surface details BEFORE creating window
        let wl_surface = surface.wl_surface();
        let surface_id = wl_surface.id().protocol_id();

        // Use the ObjectId's internal pointer for comparison
        let wl_surface_object_ptr = format!("{:?}", wl_surface.id());
        let surface_client = wl_surface.client();
        let client_ptr = surface_client
            .as_ref()
            .map(|c| format!("{:p}", c as *const _))
            .unwrap_or_else(|| "null".to_string());

        debug!("==================== NEW TOPLEVEL ====================");
        debug!(
            "wl_surface object ptr: {} (id: wl_surface@{})",
            wl_surface_object_ptr, surface_id
        );
        debug!("client ptr: {client_ptr}");
        debug!("======================================================");

        // Check if we already have a window for this surface
        // Compare using object pointers since IDs are per-client
        let surface_obj_id = wl_surface.id();
        let existing_count = self
            .space()
            .elements()
            .filter(|elem| {
                elem.0
                    .toplevel()
                    .map(|t| t.wl_surface().id() == surface_obj_id)
                    .unwrap_or(false)
            })
            .count();

        if existing_count > 0 {
            tracing::error!(
                "BUG: new_toplevel called with already mapped surface! wl_surface@{}",
                surface_id
            );
            tracing::error!(
                "This surface already has {} window(s) in the space",
                existing_count
            );
            tracing::error!(
                "This is a serious bug - the same surface is being mapped multiple times"
            );
            return; // Don't create duplicate windows
        }

        // Check our window registry
        let registry_count = self.window_registry().len();
        debug!("Currently tracking {} windows in registry", registry_count);

        debug!(
            "Creating Window::new_wayland_window with surface wl_surface@{}",
            surface_id
        );
        let window = WindowElement(Window::new_wayland_window(surface.clone()));
        let window_ptr = format!("{:p}", &window.0 as *const _);

        // Disable decorations by default (i3/sway style - only borders, no title bars)
        window.set_ssd(false);
        let window_surface = window
            .0
            .toplevel()
            .map(|t| {
                let ws = t.wl_surface();
                let ws_id = ws.id().protocol_id();
                (format!("{:p}", ws as *const _), ws_id)
            })
            .unwrap_or_else(|| ("null".to_string(), 0));
        debug!(
            "Created new window: Window ptr: {}, window's wl_surface ptr: {}, wl_surface@{}",
            window_ptr, window_surface.0, window_surface.1
        );

        // Find which virtual output should contain this window (based on pointer location)
        let pointer_loc = self.pointer().current_location();
        let pointer_loc_i32 = Point::from((pointer_loc.x as i32, pointer_loc.y as i32));

        debug!("New window at pointer location: {:?}", pointer_loc_i32);

        if let Some(virtual_output_id) = self
            .virtual_output_manager
            .virtual_output_at(pointer_loc_i32)
        {
            if let Some(virtual_output) = self.virtual_output_manager.get_mut(virtual_output_id) {
                debug!(
                    "Placing window in virtual output: {} (region: {:?})",
                    virtual_output.name(),
                    virtual_output.logical_region()
                );

                // Get the active workspace
                if let Some(workspace_idx) = virtual_output.active_workspace() {
                    info!(
                        "Placing window in workspace {} on virtual output {}",
                        workspace_idx, virtual_output_id
                    );

                    // Add window to workspace
                    if let Some(window_id) = self.add_window(window.clone(), virtual_output_id) {
                        info!("Successfully added window {} to workspace", window_id);
                    } else {
                        error!("Failed to add window to workspace!");
                    }

                    // Update IPC state
                    self.update_ipc_workspace_state();
                } else {
                    // No active workspace - this shouldn't happen, but create a workspace if needed
                    error!(
                        "No active workspace on virtual output {}",
                        virtual_output_id
                    );
                    let workspace_id = crate::workspace::WorkspaceId::new(0); // First workspace is index 0
                    self.switch_workspace(virtual_output_id, workspace_id);

                    // Try again
                    if let Some(window_id) = self.add_window(window.clone(), virtual_output_id) {
                        info!(
                            "Successfully added window {} to newly created workspace",
                            window_id
                        );
                    }
                }
            } else {
                // Virtual output not found - this is a serious error
                error!("Virtual output {} not found!", virtual_output_id);
            }
        } else {
            // No virtual output found at pointer location - try to find any virtual output
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
                if let Some(window_id) = self.add_window(window.clone(), first_vo_id) {
                    info!(
                        "Successfully added window {} to first available virtual output",
                        window_id
                    );
                }
            } else {
                error!("No virtual outputs available! Cannot place window.");
            }
        }

        // Set focus to the new window
        if let Some(keyboard) = self.seat().get_keyboard() {
            keyboard.set_focus(
                self,
                Some(crate::focus::KeyboardFocusTarget::Window(window.0.clone())),
                smithay::utils::SERIAL_COUNTER.next_serial(),
            );

            // Move cursor to center of new window
            if let Some(loc) = self.space().element_location(&window) {
                let geo = window.geometry();
                let center = smithay::utils::Point::<f64, smithay::utils::Logical>::from((
                    (loc.x + geo.size.w / 2) as f64,
                    (loc.y + geo.size.h / 2) as f64,
                ));
                self.pointer().set_location(center);
            }
        }

        // Debug dump all windows before adding post commit hook
        self.debug_dump_all_windows();

        // Log before adding post commit hook
        debug!("Adding post commit hook for wl_surface@{surface_id}");

        // Check if this surface already has a post-commit hook
        let hook_surface = surface.wl_surface();
        let hook_surface_ptr = format!("{:p}", hook_surface as *const _);
        debug!("Post-commit hook surface ptr: {hook_surface_ptr}");

        compositor::add_post_commit_hook(hook_surface, |state: &mut Self, _, surface| {
            let surface_id = surface.id().protocol_id();
            tracing::debug!("Post commit hook called for wl_surface@{surface_id}");
            handle_toplevel_commit(state.space_mut(), surface);
        });
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        // Do not send a configure here, the initial configure
        // of a xdg_surface has to be sent during the commit if
        // the surface is not already configured

        self.unconstrain_popup(&surface);

        if let Err(err) = self.popups_mut().track_popup(PopupKind::from(surface)) {
            warn!("Failed to track popup: {err}");
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        tracing::info!(
            "toplevel_destroyed called for surface {:?}",
            surface.wl_surface().id()
        );

        // Find the window element for this surface
        let window_element = self
            .space()
            .elements()
            .find(|elem| {
                elem.0
                    .toplevel()
                    .map(|t| t.wl_surface() == surface.wl_surface())
                    .unwrap_or(false)
            })
            .cloned();

        if let Some(window_element) = window_element {
            tracing::info!("Found window element for destroyed toplevel");
            // Find the window in the registry
            if let Some(window_id) = self.window_registry().find_by_element(&window_element) {
                // Get the workspace this window is in
                let workspace_id = self.window_registry().get(window_id).map(|mw| mw.workspace);

                if let Some(workspace_id) = workspace_id {
                    // Find which virtual output this workspace is on
                    let virtual_output_id =
                        self.workspace_manager.find_workspace_location(workspace_id);

                    // Remove window from workspace first
                    if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id)
                    {
                        workspace.remove_window(window_id);
                    }

                    // Find the next window to focus AFTER removing (so we don't pick the removed window)
                    let next_focus = self
                        .workspace_manager
                        .get_workspace(workspace_id)
                        .and_then(|ws| ws.layout.find_next_focus());

                    tracing::debug!(
                        "After removing window {}, next focus candidate: {:?}",
                        window_id,
                        next_focus
                    );

                    // Remove from window manager (this will emit the event)
                    let (removed, event) = self.window_manager.remove_window(window_id);
                    if let Some(event) = event {
                        self.event_bus.emit_window(event);
                    }
                    if removed {
                        debug!("Removed window {} from manager", window_id);
                    }

                    // Debug check consistency after removal
                    #[cfg(debug_assertions)]
                    self.check_consistency();

                    // Check if we need to update positions and focus
                    let needs_update = virtual_output_id
                        .and_then(|vo_id| {
                            self.virtual_output_manager.get(vo_id).map(|vo| {
                                vo.active_workspace() == Some(workspace_id.get() as usize)
                            })
                        })
                        .unwrap_or(false);

                    if needs_update {
                        tracing::info!(
                            "Window closed in active workspace {}, updating positions",
                            workspace_id
                        );

                        // Apply new layout
                        if let Some(workspace) =
                            self.workspace_manager.get_workspace_mut(workspace_id)
                        {
                            workspace.relayout();
                        }
                        self.apply_workspace_layout(workspace_id);

                        // Update IPC state
                        self.update_ipc_workspace_state();

                        // Log current focus state
                        let current_focus = self.focused_window();
                        tracing::info!(
                            "Current focused window before focus transfer: {:?}",
                            current_focus
                                .as_ref()
                                .and_then(|w| self.window_registry().find_by_element(w))
                        );

                        // Focus next window
                        if let Some(next_window_id) = next_focus {
                            let focus_info = self
                                .window_registry()
                                .get(next_window_id)
                                .map(|mw| (mw.element.clone(), mw.element.0.clone()));

                            if let Some((window_elem, _window)) = focus_info {
                                tracing::info!(
                                    "Focusing next window {} after closing window",
                                    next_window_id
                                );
                                // Use the public focus_window method which updates workspace tracking
                                self.focus_window(&window_elem);

                                // Also move cursor to the newly focused window
                                if let Some(loc) = self.space().element_location(&window_elem) {
                                    let geo = window_elem.geometry();
                                    let center = smithay::utils::Point::<
                                        f64,
                                        smithay::utils::Logical,
                                    >::from((
                                        (loc.x + geo.size.w / 2) as f64,
                                        (loc.y + geo.size.h / 2) as f64,
                                    ));
                                    self.pointer().set_location(center);
                                }

                                // Verify focus was set
                                let new_focus = self.focused_window();
                                tracing::info!(
                                    "Focus after transfer: {:?}",
                                    new_focus
                                        .as_ref()
                                        .and_then(|w| self.window_registry().find_by_element(w))
                                );
                            } else {
                                tracing::warn!(
                                    "Could not find window element for window_id {}",
                                    next_window_id
                                );
                            }
                        } else {
                            // No window to focus, clear keyboard focus
                            tracing::debug!(
                                "No window to focus after closing window, clearing keyboard focus"
                            );
                            if let Some(keyboard) = self.seat().get_keyboard() {
                                keyboard.set_focus(
                                    self,
                                    None,
                                    smithay::utils::SERIAL_COUNTER.next_serial(),
                                );
                            }
                        }
                    }
                }
            }

            // The window has already been unmapped from space by window_manager.remove_window()
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, _surface: ToplevelSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // In a tiling window manager, we don't allow manual window movement
    }

    fn resize_request(
        &mut self,
        _surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        _serial: Serial,
        _edges: xdg_toplevel::ResizeEdge,
    ) {
        // In a tiling window manager, we don't allow manual window resizing
    }

    fn ack_configure(&mut self, surface: WlSurface, configure: Configure) {
        if let Configure::Toplevel(configure) = configure {
            if let Some(serial) = with_states(&surface, |states| {
                if let Some(data) = states.data_map.get::<RefCell<SurfaceData>>() {
                    if let ResizeState::WaitingForFinalAck(_, serial) = data.borrow().resize_state {
                        return Some(serial);
                    }
                }

                None
            }) {
                // When the resize grab is released the surface
                // resize state will be set to WaitingForFinalAck
                // and the client will receive a configure request
                // without the resize state to inform the client
                // resizing has finished. Here we will wait for
                // the client to acknowledge the end of the
                // resizing. To check if the surface was resizing
                // before sending the configure we need to use
                // the current state as the received acknowledge
                // will no longer have the resize state set
                let is_resizing = with_states(&surface, |states| {
                    states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()
                        .and_then(|data| data.lock().ok())
                        .map(|data| data.current.states.contains(xdg_toplevel::State::Resizing))
                        .unwrap_or(false)
                });

                if configure.serial >= serial && is_resizing {
                    with_states(&surface, |states| {
                        if let Some(surface_data) = states
                            .data_map
                            .get::<RefCell<SurfaceData>>() {
                            let mut data = surface_data.borrow_mut();
                            if let ResizeState::WaitingForFinalAck(resize_data, _) = data.resize_state {
                                data.resize_state = ResizeState::WaitingForCommit(resize_data);
                            } else {
                                tracing::error!("Unexpected resize state during ack_configure");
                            }
                        }
                    });
                }
            }

            // Don't apply decoration mode from configure - we handle decorations ourselves
            // In i3/sway style, we don't use title bars for regular tiled windows
        }
    }

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        wl_output: Option<wl_output::WlOutput>,
    ) {
        if surface
            .current_state()
            .capabilities
            .contains(xdg_toplevel::WmCapabilities::Fullscreen)
        {
            let wl_surface = surface.wl_surface();

            // Find the window element
            let window = self
                .space()
                .elements()
                .find(|window| {
                    window
                        .wl_surface()
                        .map(|s| &*s == wl_surface)
                        .unwrap_or(false)
                })
                .cloned();

            if let Some(window) = window {
                if let Some(window_id) = self.window_registry().find_by_element(&window) {
                    // Determine fullscreen mode based on whether output was specified
                    let mode = if wl_output.is_some() {
                        crate::window::FullscreenMode::PhysicalOutput
                    } else {
                        crate::window::FullscreenMode::VirtualOutput
                    };

                    self.set_window_fullscreen(window_id, true, mode);
                    return;
                }
            }
        }

        // The protocol demands us to always reply with a configure,
        // regardless of we fulfilled the request or not
        if surface.is_initial_configure_sent() {
            surface.send_configure();
        } else {
            // Will be sent during initial configure
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if !surface
            .current_state()
            .states
            .contains(xdg_toplevel::State::Fullscreen)
        {
            return;
        }

        let wl_surface = surface.wl_surface();

        // Find the window element
        let window = self
            .space()
            .elements()
            .find(|window| {
                window
                    .wl_surface()
                    .map(|s| &*s == wl_surface)
                    .unwrap_or(false)
            })
            .cloned();

        if let Some(window) = window {
            if let Some(window_id) = self.window_registry().find_by_element(&window) {
                // Clear fullscreen mode
                self.set_window_fullscreen(
                    window_id,
                    false,
                    crate::window::FullscreenMode::VirtualOutput,
                );
                return;
            }
        }

        surface.send_pending_configure();
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        // NOTE: This should use layer-shell when it is implemented to
        // get the correct maximum size
        if surface
            .current_state()
            .capabilities
            .contains(xdg_toplevel::WmCapabilities::Maximize)
        {
            let Some(window) = self.window_for_surface(surface.wl_surface()) else {
                tracing::warn!("Maximize request for unknown surface");
                return;
            };
            let outputs_for_window = self.space().outputs_for_element(&window);
            let Some(output) = outputs_for_window
                .first()
                // The window hasn't been mapped yet, use the primary output instead
                .or_else(|| self.space().outputs().next())
            else {
                tracing::error!("No outputs found for maximize request");
                return;
            };
            let Some(geometry) = self.space().output_geometry(output) else {
                tracing::error!("No geometry for output during maximize");
                return;
            };

            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Maximized);
                state.size = Some(geometry.size);
            });
            // Find window ID and use window manager for proper tracking
            if let Some(window_id) = self.window_registry().find_by_element(&window) {
                self.window_manager.move_window(window_id, geometry.loc);
            } else {
                self.window_manager
                    .update_element_position(&window, geometry.loc);
            }
        }

        // The protocol demands us to always reply with a configure,
        // regardless of we fulfilled the request or not
        if surface.is_initial_configure_sent() {
            surface.send_configure();
        } else {
            // Will be sent during initial configure
        }
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        if !surface
            .current_state()
            .states
            .contains(xdg_toplevel::State::Maximized)
        {
            return;
        }

        surface.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Maximized);
            state.size = None;
        });
        surface.send_pending_configure();
    }

    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let Some(seat) = Seat::<StilchState<BackendData>>::from_resource(&seat) else {
            tracing::error!("Invalid seat in grab request");
            return;
        };
        let kind = PopupKind::Xdg(surface);
        if let Some(root) = find_popup_root_surface(&kind).ok().and_then(|root| {
            self.space()
                .elements()
                .find(|w| w.wl_surface().map(|s| *s == root).unwrap_or(false))
                .cloned()
                .map(KeyboardFocusTarget::from)
                .or_else(|| {
                    self.space()
                        .outputs()
                        .find_map(|o| {
                            let map = layer_map_for_output(o);
                            map.layer_for_surface(&root, WindowSurfaceType::TOPLEVEL)
                                .cloned()
                        })
                        .map(KeyboardFocusTarget::LayerSurface)
                })
        }) {
            let ret = self.popups_mut().grab_popup(root, kind, &seat, serial);

            if let Ok(mut grab) = ret {
                if let Some(keyboard) = seat.get_keyboard() {
                    if keyboard.is_grabbed()
                        && !(keyboard.has_grab(serial)
                            || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
                    {
                        grab.ungrab(PopupUngrabStrategy::All);
                        return;
                    }
                    keyboard.set_focus(self, grab.current_grab(), serial);
                    keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
                }
                if let Some(pointer) = seat.get_pointer() {
                    if pointer.is_grabbed()
                        && !(pointer.has_grab(serial)
                            || pointer
                                .has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
                    {
                        grab.ungrab(PopupUngrabStrategy::All);
                        return;
                    }
                    pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
                }
            }
        }
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        // The title is already stored in the XdgToplevelSurfaceData by Smithay
        // We just need to log it and trigger any necessary updates
        tracing::info!(
            "Window title changed for surface {:?}",
            surface.wl_surface().id()
        );

        // TODO: Trigger tab bar update if this window is in a tabbed container
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        // The app_id is already stored in the XdgToplevelSurfaceData by Smithay
        // We just need to log it and trigger any necessary updates
        tracing::info!(
            "Window app_id changed for surface {:?}",
            surface.wl_surface().id()
        );

        // TODO: Trigger tab bar update if this window is in a tabbed container
    }
}

impl<BackendData: Backend> StilchState<BackendData> {
    fn debug_dump_all_windows(&self) {
        tracing::debug!("============ WINDOW STATE DUMP ============");
        tracing::debug!("Total space elements: {}", self.space().elements().count());
        tracing::debug!(
            "Total windows in registry: {}",
            self.window_registry().len()
        );

        // Dump all space elements
        tracing::debug!("\nSpace elements:");
        for (idx, elem) in self.space().elements().enumerate() {
            let elem_ptr = format!("{:p}", &elem.0 as *const _);
            let window_id = self.window_registry().find_by_element(elem);
            if let Some(toplevel) = elem.0.toplevel() {
                let surface = toplevel.wl_surface();
                let surface_id = surface.id().protocol_id();
                let surface_ptr = format!("{:p}", surface as *const _);
                let client_id = surface.client().map(|c| format!("{:?}", c.id()));
                tracing::debug!(
                    "  [{}] Window ptr: {}, wl_surface@{} (ptr: {}), client: {}, registry id: {:?}",
                    idx,
                    elem_ptr,
                    surface_id,
                    surface_ptr,
                    client_id.as_deref().unwrap_or("None"),
                    window_id
                );
            } else {
                tracing::debug!(
                    "  [{}] Window ptr: {} (no toplevel), registry id: {:?}",
                    idx,
                    elem_ptr,
                    window_id
                );
            }
        }

        // Dump window registry
        tracing::debug!("\nWindow registry:");
        for (window_id, managed_window) in self.window_registry().iter() {
            tracing::debug!(
                "  WindowId {} -> workspace: {}, layout: {:?}",
                window_id,
                managed_window.workspace,
                managed_window.layout
            );
        }
        tracing::debug!("=========================================");
    }
    pub fn move_request_xdg(
        &mut self,
        surface: &ToplevelSurface,
        seat: &Seat<Self>,
        serial: Serial,
    ) {
        if let Some(touch) = seat.get_touch() {
            if touch.has_grab(serial) {
                let Some(start_data) = touch.grab_start_data() else {
                    tracing::warn!("Touch grab without start data");
                    return;
                };

                // If the client disconnects after requesting a move
                // we can just ignore the request
                let Some(window) = self.window_for_surface(surface.wl_surface()) else {
                    return;
                };

                // If the focus was for a different surface, ignore the request.
                if let Some(ref focus) = start_data.focus {
                    if !focus.0.same_client_as(&surface.wl_surface().id()) {
                        return;
                    }
                } else {
                    return;
                }

                let Some(initial_window_location) = self.space().element_location(&window) else {
                    tracing::error!("Window has no location in space");
                    return;
                };
                let mut initial_window_location = initial_window_location;

                // If surface is maximized then unmaximize it
                let current_state = surface.current_state();
                if current_state
                    .states
                    .contains(xdg_toplevel::State::Maximized)
                {
                    surface.with_pending_state(|state| {
                        state.states.unset(xdg_toplevel::State::Maximized);
                        state.size = None;
                    });

                    surface.send_configure();

                    // NOTE: In real compositor mouse location should be mapped to a new window size
                    // For example, you could:
                    // 1) transform mouse pointer position from compositor space to window space (location relative)
                    // 2) divide the x coordinate by width of the window to get the percentage
                    //   - 0.0 would be on the far left of the window
                    //   - 0.5 would be in middle of the window
                    //   - 1.0 would be on the far right of the window
                    // 3) multiply the percentage by new window width
                    // 4) by doing that, drag will look a lot more natural
                    //
                    // but for stilch needs setting location to pointer location is fine
                    initial_window_location = start_data.location.to_i32_round();
                }

                let grab = TouchMoveSurfaceGrab {
                    start_data,
                    window,
                    initial_window_location,
                };

                touch.set_grab(self, grab, serial);
                return;
            }
        }

        let Some(pointer) = seat.get_pointer() else {
            tracing::debug!("Move request without pointer");
            return;
        };

        // Check that this surface has a click grab.
        if !pointer.has_grab(serial) {
            return;
        }

        let Some(start_data) = pointer.grab_start_data() else {
            tracing::warn!("Pointer grab without start data");
            return;
        };

        // If the client disconnects after requesting a move
        // we can just ignore the request
        let Some(window) = self.window_for_surface(surface.wl_surface()) else {
            return;
        };

        // If the focus was for a different surface, ignore the request.
        if let Some(ref focus) = start_data.focus {
            if !focus.0.same_client_as(&surface.wl_surface().id()) {
                return;
            }
        } else {
            return;
        }

        let Some(initial_window_location) = self.space().element_location(&window) else {
            tracing::error!("Window has no location in space for move");
            return;
        };
        let mut initial_window_location = initial_window_location;

        // If surface is maximized then unmaximize it
        let current_state = surface.current_state();
        if current_state
            .states
            .contains(xdg_toplevel::State::Maximized)
        {
            surface.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Maximized);
                state.size = None;
            });

            surface.send_configure();

            // NOTE: In real compositor mouse location should be mapped to a new window size
            // For example, you could:
            // 1) transform mouse pointer position from compositor space to window space (location relative)
            // 2) divide the x coordinate by width of the window to get the percentage
            //   - 0.0 would be on the far left of the window
            //   - 0.5 would be in middle of the window
            //   - 1.0 would be on the far right of the window
            // 3) multiply the percentage by new window width
            // 4) by doing that, drag will look a lot more natural
            //
            // but for stilch needs setting location to pointer location is fine
            let pos = pointer.current_location();
            initial_window_location = (pos.x as i32, pos.y as i32).into();
        }

        let grab = PointerMoveSurfaceGrab {
            start_data,
            window,
            initial_window_location,
        };

        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self.window_for_surface(&root) else {
            return;
        };

        let mut outputs_for_window = self.space().outputs_for_element(&window);
        if outputs_for_window.is_empty() {
            return;
        }

        // Get a union of all outputs' geometries.
        let Some(first_output) = outputs_for_window.pop() else {
            tracing::error!("outputs_for_window was empty after check");
            return;
        };
        let Some(mut outputs_geo) = self.space().output_geometry(&first_output) else {
            tracing::error!("No geometry for output");
            return;
        };
        for output in outputs_for_window {
            if let Some(geo) = self.space().output_geometry(&output) {
                outputs_geo = outputs_geo.merge(geo);
            }
        }

        let Some(window_geo) = self.space().element_geometry(&window) else {
            tracing::error!("No geometry for window");
            return;
        };

        // The target geometry for the positioner should be relative to its parent's geometry, so
        // we will compute that here.
        let mut target = outputs_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

/// Should be called on `WlSurface::commit` of xdg toplevel
fn handle_toplevel_commit(space: &mut Space<WindowElement>, surface: &WlSurface) -> Option<()> {
    let window = space
        .elements()
        .find(|w| w.wl_surface().as_deref() == Some(surface))
        .cloned()?;

    let mut window_loc = space.element_location(&window)?;
    let geometry = window.geometry();

    let new_loc: Point<Option<i32>, Logical> =
        with_states(window.wl_surface().as_deref()?, |states| {
            let data = states.data_map.get::<RefCell<SurfaceData>>()?.borrow_mut();

            if let ResizeState::Resizing(resize_data) = data.resize_state {
                let edges = resize_data.edges;
                let loc = resize_data.initial_window_location;
                let size = resize_data.initial_window_size;

                // If the window is being resized by top or left, its location must be adjusted
                // accordingly.
                edges.intersects(ResizeEdge::TOP_LEFT).then(|| {
                    let new_x = edges
                        .intersects(ResizeEdge::LEFT)
                        .then_some(loc.x + (size.w - geometry.size.w));

                    let new_y = edges
                        .intersects(ResizeEdge::TOP)
                        .then_some(loc.y + (size.h - geometry.size.h));

                    (new_x, new_y).into()
                })
            } else {
                None
            }
        })?;

    if let Some(new_x) = new_loc.x {
        window_loc.x = new_x;
    }
    if let Some(new_y) = new_loc.y {
        window_loc.y = new_y;
    }

    if new_loc.x.is_some() || new_loc.y.is_some() {
        // If TOP or LEFT side of the window got resized, we have to move it
        space.map_element(window, window_loc, false);
    }

    Some(())
}
