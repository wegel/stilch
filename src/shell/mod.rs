use once_cell::sync::Lazy;
use std::cell::RefCell;

#[cfg(feature = "xwayland")]
use smithay::xwayland::XWaylandClientData;

#[cfg(feature = "udev")]
use smithay::wayland::drm_syncobj::DrmSyncobjCachedState;

use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    desktop::{
        layer_map_for_output, space::SpaceElement, LayerSurface, PopupKind, PopupManager, Space,
        Window, WindowSurfaceType,
    },
    input::pointer::{CursorImageStatus, CursorImageSurfaceData},
    output::Output,
    reexports::{
        calloop::Interest,
        wayland_server::{
            protocol::{wl_buffer::WlBuffer, wl_output, wl_surface::WlSurface},
            Client, Resource,
        },
    },
    utils::{IsAlive, Logical, Point, Rectangle, Size},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            add_blocker, add_pre_commit_hook, get_parent, is_sync_subsurface, with_states,
            with_surface_tree_upward, BufferAssignment, CompositorClientState, CompositorHandler,
            CompositorState, SurfaceAttributes, TraversalAction,
        },
        dmabuf::get_dmabuf,
        shell::{
            wlr_layer::{
                Layer, LayerSurface as WlrLayerSurface, LayerSurfaceData, WlrLayerShellHandler,
                WlrLayerShellState,
            },
            xdg::XdgToplevelSurfaceData,
        },
    },
};

use crate::{
    state::{Backend, StilchState},
    ClientState,
};

mod element;
mod grabs;
mod resize_state;
pub(crate) mod ssd;
#[cfg(feature = "xwayland")]
mod x11;
mod xdg;

pub use self::element::*;
pub use self::grabs::*;

#[derive(Default)]
pub struct FullscreenSurface(RefCell<Option<WindowElement>>);

impl FullscreenSurface {
    pub fn set(&self, window: WindowElement) {
        *self.0.borrow_mut() = Some(window);
    }

    pub fn get(&self) -> Option<WindowElement> {
        let mut window = self.0.borrow_mut();
        if window.as_ref().map(|w| !w.alive()).unwrap_or(false) {
            *window = None;
        }
        window.clone()
    }

    pub fn clear(&self) -> Option<WindowElement> {
        self.0.borrow_mut().take()
    }
}

// Static poison state for error cases where we can't return a proper state
// This is better than panicking, but operations using this state may not work correctly
static POISON_COMPOSITOR_STATE: Lazy<CompositorClientState> =
    Lazy::new(|| CompositorClientState::default());

impl<BackendData: Backend> BufferHandler for StilchState<BackendData> {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl<BackendData: Backend> CompositorHandler for StilchState<BackendData> {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.protocols.compositor_state
    }
    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        #[cfg(feature = "xwayland")]
        if let Some(state) = client.get_data::<XWaylandClientData>() {
            return &state.compositor_state;
        }
        if let Some(state) = client.get_data::<ClientState>() {
            return &state.compositor_state;
        }

        // This should never happen in normal operation
        // Both regular Wayland clients and XWayland clients should have their state
        tracing::error!(
            "CRITICAL: Unknown client data type for client {:?} - returning poison state. \
             This is a bug in the compositor and may cause incorrect behavior.",
            client.id()
        );

        // Return a static poison state instead of panicking
        // This allows the compositor to continue running, though the client may not work correctly
        &*POISON_COMPOSITOR_STATE
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        let surface_id = surface.id().protocol_id();
        let surface_object_id = format!("{:?}", surface.id());
        let surface_ptr = format!("{:p}", surface as *const _);
        let client = surface.client();
        let client_info = client.as_ref().map(|c| format!("id={:?}", c.id()));

        tracing::debug!(
            "CompositorHandler::new_surface called: {} (protocol_id={}, ptr={}), client: {}",
            surface_object_id,
            surface_id,
            surface_ptr,
            client_info.as_deref().unwrap_or("None")
        );

        add_pre_commit_hook::<Self, _>(surface, move |state, _dh, surface| {
            #[cfg(feature = "udev")]
            let mut acquire_point = None;
            let maybe_dmabuf = with_states(surface, |surface_data| {
                #[cfg(feature = "udev")]
                acquire_point.clone_from(
                    &surface_data
                        .cached_state
                        .get::<DrmSyncobjCachedState>()
                        .pending()
                        .acquire_point,
                );
                surface_data
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .pending()
                    .buffer
                    .as_ref()
                    .and_then(|assignment| match assignment {
                        BufferAssignment::NewBuffer(buffer) => get_dmabuf(buffer).cloned().ok(),
                        _ => None,
                    })
            });
            if let Some(dmabuf) = maybe_dmabuf {
                #[cfg(feature = "udev")]
                if let Some(acquire_point) = acquire_point {
                    if let Ok((blocker, source)) = acquire_point.generate_blocker() {
                        let client = match surface.client() {
                            Some(client) => client,
                            None => {
                                tracing::warn!("Surface has no client");
                                return;
                            }
                        };
                        let res = state.handle.insert_source(source, move |_, _, data| {
                            let dh = data.display_handle.clone();
                            data.client_compositor_state(&client)
                                .blocker_cleared(data, &dh);
                            Ok(())
                        });
                        if res.is_ok() {
                            add_blocker(surface, blocker);
                            return;
                        }
                    }
                }
                if let Ok((blocker, source)) = dmabuf.generate_blocker(Interest::READ) {
                    if let Some(client) = surface.client() {
                        let res = state.handle.insert_source(source, move |_, _, data| {
                            let dh = data.display_handle.clone();
                            data.client_compositor_state(&client)
                                .blocker_cleared(data, &dh);
                            Ok(())
                        });
                        if res.is_ok() {
                            add_blocker(surface, blocker);
                        }
                    }
                }
            }
        });
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        self.backend_data.early_import(surface);

        // Only request render if the surface has a new buffer or explicit damage
        let needs_render = with_states(surface, |states| {
            let mut attrs = states.cached_state.get::<SurfaceAttributes>();
            let current = attrs.current();

            // Check if there's a new buffer or damage regions
            matches!(current.buffer, Some(BufferAssignment::NewBuffer(_)))
                || !current.damage.is_empty()
        });

        if needs_render {
            tracing::trace!(
                "Surface {:?} needs render (new buffer or damage)",
                surface.id()
            );
            self.backend_data.request_render();
        }

        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self.window_for_surface(&root) {
                window.0.on_commit();

                if &root == surface {
                    let buffer_offset = with_states(surface, |states| {
                        states
                            .cached_state
                            .get::<SurfaceAttributes>()
                            .current()
                            .buffer_delta
                            .take()
                    });

                    if let Some(buffer_offset) = buffer_offset {
                        let current_loc = match self.space().element_location(&window) {
                            Some(loc) => loc,
                            None => {
                                tracing::warn!("Window has no location in space");
                                return;
                            }
                        };
                        self.window_manager
                            .update_element_position(&window, current_loc + buffer_offset);
                    }
                }
            }
            // Check if this is a layer surface and queue redraw for its output
            else {
                // Check all outputs for layer surfaces
                for output in self.space().outputs().cloned().collect::<Vec<_>>() {
                    let map = layer_map_for_output(&output);
                    if map
                        .layer_for_surface(&root, WindowSurfaceType::TOPLEVEL)
                        .is_some()
                    {
                        break;
                    }
                }
            }
        }
        self.popups_mut().commit(surface);

        if matches!(self.cursor_status(), CursorImageStatus::Surface(cursor_surface) if cursor_surface == surface)
        {
            // Queue redraw on output where cursor is
            with_states(surface, |states| {
                let cursor_image_attributes = states.data_map.get::<CursorImageSurfaceData>();

                if let Some(mut cursor_image_attributes) =
                    cursor_image_attributes.and_then(|attrs| attrs.lock().ok())
                {
                    let buffer_delta = states
                        .cached_state
                        .get::<SurfaceAttributes>()
                        .current()
                        .buffer_delta
                        .take();
                    if let Some(buffer_delta) = buffer_delta {
                        tracing::trace!(hotspot = ?cursor_image_attributes.hotspot, ?buffer_delta, "decrementing cursor hotspot");
                        cursor_image_attributes.hotspot -= buffer_delta;
                    }
                }
            });
        }

        if matches!(self.dnd_icon(), Some(icon) if &icon.surface == surface) {
            let Some(dnd_icon) = self.input_manager.dnd_icon.as_mut() else {
                tracing::error!("DND icon not found when expected");
                return;
            };
            with_states(&dnd_icon.surface, |states| {
                let buffer_delta = states
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .current()
                    .buffer_delta
                    .take()
                    .unwrap_or_default();
                tracing::trace!(offset = ?dnd_icon.offset, ?buffer_delta, "moving dnd offset");
                dnd_icon.offset += buffer_delta;
            });
        }

        // Extract space and popups manager directly from window manager
        ensure_initial_configure(
            surface,
            &self.window_manager.space,
            &mut self.window_manager.popups,
        );
    }
}

impl<BackendData: Backend> WlrLayerShellHandler for StilchState<BackendData> {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.protocols.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        wl_output: Option<wl_output::WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        let output = wl_output
            .as_ref()
            .and_then(Output::from_resource)
            .unwrap_or_else(|| {
                self.space().outputs().next().cloned().unwrap_or_else(|| {
                    tracing::error!("No outputs available for layer surface");
                    // Create a dummy output as fallback
                    Output::new(
                        "dummy".to_string(),
                        smithay::output::PhysicalProperties {
                            size: (0, 0).into(),
                            subpixel: smithay::output::Subpixel::Unknown,
                            make: "Unknown".to_string(),
                            model: "Unknown".to_string(),
                        },
                    )
                })
            });
        let mut map = layer_map_for_output(&output);
        if let Err(e) = map.map_layer(&LayerSurface::new(surface, namespace)) {
            tracing::error!("Failed to map layer surface: {:?}", e);
        } else {
            // Queue redraw for the output when a new layer surface is added
        }
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        // Find and unmap the layer
        for output in self.space().outputs() {
            let layer_to_unmap = {
                let map = layer_map_for_output(output);
                let layers: Vec<_> = map.layers().cloned().collect();
                layers
                    .into_iter()
                    .find(|layer| layer.layer_surface() == &surface)
            };

            if let Some(layer) = layer_to_unmap {
                let mut map = layer_map_for_output(output);
                map.unmap_layer(&layer);
                break;
            }
        }
    }
}

impl<BackendData: Backend> StilchState<BackendData> {
    pub fn get_window_id(&self, window: &Window) -> Option<usize> {
        window.user_data().get::<usize>().copied()
    }
    pub fn window_for_surface(&self, surface: &WlSurface) -> Option<WindowElement> {
        self.space()
            .elements()
            .find(|window| window.wl_surface().map(|s| &*s == surface).unwrap_or(false))
            .cloned()
    }

    pub fn update_window_positions_for_virtual_output(
        &mut self,
        virtual_output_id: crate::virtual_output::VirtualOutputId,
    ) {
        self.update_window_positions_for_workspace(virtual_output_id, None);
    }

    pub fn update_window_positions_for_workspace(
        &mut self,
        _virtual_output_id: crate::virtual_output::VirtualOutputId,
        _workspace_idx: Option<usize>,
    ) {
        // This method has been largely replaced by the new system
        // The old tiling layout is no longer used
    }

    pub fn update_tiling_area_from_output(&mut self) {
        tracing::info!("=== update_tiling_area_from_output called ===");

        // Calculate effective exclusive zones for all virtual outputs
        let virtual_outputs: Vec<_> = self
            .virtual_output_manager
            .all_virtual_outputs()
            .map(|vo| (vo.id(), vo.logical_region(), vo.physical_outputs().to_vec()))
            .collect();

        tracing::info!("Found {} virtual outputs", virtual_outputs.len());

        for (vo_id, vo_region, physical_outputs) in virtual_outputs {
            tracing::info!(
                "Processing virtual output {} with region {:?}",
                vo_id,
                vo_region
            );

            // Calculate the most restrictive exclusive zone from all physical outputs
            let mut effective_area = vo_region;

            for physical_output in &physical_outputs {
                // Arrange the layer map to ensure exclusive zones are calculated
                layer_map_for_output(physical_output).arrange();

                let layer_map = layer_map_for_output(physical_output);
                let non_exclusive = layer_map.non_exclusive_zone();

                tracing::info!(
                    "Physical output {:?} has non_exclusive zone: {:?}",
                    physical_output.name(),
                    non_exclusive
                );

                // The non_exclusive zone is the area NOT reserved by panels
                // It's in physical output local coordinates
                let physical_geo = self
                    .space()
                    .output_geometry(physical_output)
                    .unwrap_or_else(|| Rectangle::from_size((0, 0).into()));

                // The physical output's full size in logical coords
                let physical_size = physical_geo.size;

                // Calculate exclusive zones (what panels reserve) from the non-exclusive zone
                // These are the amounts reserved on each edge
                let exclusive_top = non_exclusive.loc.y;
                let exclusive_left = non_exclusive.loc.x;
                let exclusive_bottom =
                    physical_size.h - (non_exclusive.loc.y + non_exclusive.size.h);
                let exclusive_right =
                    physical_size.w - (non_exclusive.loc.x + non_exclusive.size.w);

                tracing::info!("Physical output {:?}: exclusive zones - top: {}, left: {}, bottom: {}, right: {}",
                             physical_output.name(), exclusive_top, exclusive_left, exclusive_bottom, exclusive_right);

                // Apply exclusions to our virtual output region
                // Only apply if the virtual output touches that edge of the physical output

                // Top edge - if virtual output starts at physical top
                if vo_region.loc.y == physical_geo.loc.y && exclusive_top > 0 {
                    let new_y = effective_area.loc.y + exclusive_top;
                    let new_h = effective_area.size.h - exclusive_top;
                    if new_h > 0 {
                        effective_area.loc.y = new_y;
                        effective_area.size.h = new_h;
                        tracing::info!("Applied top exclusion: {} pixels", exclusive_top);
                    }
                }

                // Left edge - if virtual output starts at physical left
                if vo_region.loc.x == physical_geo.loc.x && exclusive_left > 0 {
                    let new_x = effective_area.loc.x + exclusive_left;
                    let new_w = effective_area.size.w - exclusive_left;
                    if new_w > 0 {
                        effective_area.loc.x = new_x;
                        effective_area.size.w = new_w;
                        tracing::info!("Applied left exclusion: {} pixels", exclusive_left);
                    }
                }

                // Bottom edge - if virtual output extends to physical bottom
                if vo_region.loc.y + vo_region.size.h >= physical_geo.loc.y + physical_size.h - 1
                    && exclusive_bottom > 0
                {
                    let new_h = effective_area.size.h - exclusive_bottom;
                    if new_h > 0 {
                        effective_area.size.h = new_h;
                        tracing::info!("Applied bottom exclusion: {} pixels", exclusive_bottom);
                    }
                }

                // Right edge - if virtual output extends to physical right
                if vo_region.loc.x + vo_region.size.w >= physical_geo.loc.x + physical_size.w - 1
                    && exclusive_right > 0
                {
                    let new_w = effective_area.size.w - exclusive_right;
                    if new_w > 0 {
                        effective_area.size.w = new_w;
                        tracing::info!("Applied right exclusion: {} pixels", exclusive_right);
                    }
                }
            }

            tracing::info!(
                "Final effective area for virtual output {}: {:?}",
                vo_id,
                effective_area
            );

            // Store the calculated area for this virtual output
            self.virtual_output_exclusive_zones
                .insert(vo_id, effective_area);
        }
    }

    /// Focus a specific virtual output (useful for focusing empty VOs)
    pub fn focus_virtual_output(
        &mut self,
        virtual_output_id: crate::virtual_output::VirtualOutputId,
    ) {
        if let Some(vo) = self.virtual_output_manager.get(virtual_output_id) {
            let region = vo.logical_region();
            let center = Point::<f64, Logical>::from((
                (region.loc.x + region.size.w / 2) as f64,
                (region.loc.y + region.size.h / 2) as f64,
            ));

            tracing::info!("Focusing virtual output {:?} at {:?}", vo.name(), center);
            self.pointer().set_location(center);

            // Clear keyboard focus since we're focusing an empty area
            if let Some(keyboard) = self.seat().get_keyboard() {
                keyboard.set_focus(self, None, smithay::utils::SERIAL_COUNTER.next_serial());
            }
        }
    }

    pub fn switch_to_workspace(
        &mut self,
        virtual_output_id: crate::virtual_output::VirtualOutputId,
        workspace_idx: usize,
    ) {
        // Use the new workspace system
        let workspace_id = crate::workspace::WorkspaceId::new(workspace_idx as u8);
        self.switch_workspace(virtual_output_id, workspace_id);

        tracing::info!("Switched to workspace {}", workspace_idx + 1);
    }

    /// Warp pointer to center of window
    pub fn warp_pointer_to_window(&mut self, elem: &WindowElement) {
        let window_loc = self.space().element_location(elem).unwrap_or_default();
        let window_geo = elem.geometry();
        let center_x = window_loc.x + window_geo.size.w / 2;
        let center_y = window_loc.y + window_geo.size.h / 2;
        let center_point = Point::from((center_x as f64, center_y as f64));

        // Get surface under the center point
        let under = self.surface_under(center_point);

        // Use cloned pointer to avoid borrow issues
        let pointer = self.pointer().clone();
        pointer.motion(
            self,
            under,
            &smithay::input::pointer::MotionEvent {
                location: center_point,
                serial: smithay::utils::SERIAL_COUNTER.next_serial(),
                time: self.clock.now().as_millis() as u32,
            },
        );
        pointer.frame(self);
    }

    pub fn move_window_direction(
        &mut self,
        window_element: WindowElement,
        direction: crate::config::Direction,
    ) {
        // Find the window in the registry
        let window_id = match self.window_registry().find_by_element(&window_element) {
            Some(id) => id,
            None => {
                tracing::warn!("Window element not found in window registry");
                return;
            }
        };

        // Get the managed window to find its workspace
        let workspace_id = match self.window_registry().get(window_id) {
            Some(managed_window) => managed_window.workspace,
            None => {
                tracing::warn!("Window not found in registry");
                return;
            }
        };

        // Find which virtual output this workspace is on
        let virtual_output_id = match self.workspace_manager.find_workspace_location(workspace_id) {
            Some(vo_id) => vo_id,
            None => {
                tracing::warn!("Workspace not found on any virtual output");
                return;
            }
        };

        // Move the window in the workspace
        let moved = self
            .workspace_manager
            .get_workspace_mut(workspace_id)
            .map(|workspace| workspace.move_window(window_id, direction))
            .unwrap_or(false);

        if moved {
            tracing::info!("Moved window {:?}", direction);
            // Apply the new layout if this workspace is visible
            if let Some(vo) = self.virtual_output_manager.get(virtual_output_id) {
                if vo.active_workspace() == Some(workspace_id.get() as usize) {
                    self.apply_workspace_layout(workspace_id);
                }
            }
        } else {
            tracing::info!("Could not move window {:?}", direction);
        }
    }

    pub fn set_split_direction(&mut self, direction: crate::workspace::layout::SplitDirection) {
        // Get the current virtual output based on pointer location
        if let Some(virtual_output_id) = self.virtual_output_at_pointer() {
            // Get the active workspace on this virtual output
            if let Some(workspace_id) = self
                .workspace_manager
                .workspace_on_output(virtual_output_id)
            {
                if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id) {
                    workspace.set_next_split(direction);
                    tracing::info!(
                        "Set split direction to {:?} for workspace {}",
                        direction,
                        workspace_id
                    );
                }
            }
        }
    }

    pub fn set_split_direction_automatic(&mut self) {
        // Get the current virtual output based on pointer location
        if let Some(virtual_output_id) = self.virtual_output_at_pointer() {
            // Get the active workspace on this virtual output
            if let Some(workspace_id) = self
                .workspace_manager
                .workspace_on_output(virtual_output_id)
            {
                if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id) {
                    // For automatic mode, choose based on workspace aspect ratio
                    let direction = if workspace.area.size.w > workspace.area.size.h {
                        crate::workspace::layout::SplitDirection::Horizontal
                    } else {
                        crate::workspace::layout::SplitDirection::Vertical
                    };
                    workspace.set_next_split(direction);
                    tracing::info!(
                        "Set automatic split direction to {:?} for workspace {}",
                        direction,
                        workspace_id
                    );
                }
            }
        }
    }

    pub fn debug_swap_windows(&mut self) {
        tracing::info!("============ DEBUG SWAP WINDOWS ============");

        // Get the current virtual output based on pointer location
        let pointer_loc = self.pointer().current_location();
        let pointer_loc_i32 = Point::from((pointer_loc.x as i32, pointer_loc.y as i32));

        if let Some(virtual_output_id) = self
            .virtual_output_manager
            .virtual_output_at(pointer_loc_i32)
        {
            if let Some(virtual_output) = self.virtual_output_manager.get(virtual_output_id) {
                if let Some(workspace_idx) = virtual_output.active_workspace() {
                    let workspace_id = crate::workspace::WorkspaceId::new(workspace_idx as u8);
                    if let Some(workspace) = self.workspace_manager.get_workspace(workspace_id) {
                        tracing::info!(
                            "Current workspace has {} windows",
                            workspace.window_count()
                        );

                        if workspace.window_count() >= 2 {
                            // Get the first two windows from the new workspace
                            let window_ids: Vec<_> =
                                workspace.windows.iter().take(2).cloned().collect();
                            if window_ids.len() >= 2 {
                                let window_id1 = window_ids[0];
                                let window_id2 = window_ids[1];

                                tracing::info!("Window 1: id={window_id1}");
                                tracing::info!("Window 2: id={window_id2}");

                                // Find their WindowElements using new window registry
                                let elem1 = self
                                    .window_registry()
                                    .get(window_id1)
                                    .map(|mw| mw.element.clone());
                                let elem2 = self
                                    .window_registry()
                                    .get(window_id2)
                                    .map(|mw| mw.element.clone());

                                if let (Some(elem1), Some(elem2)) = (elem1, elem2) {
                                    // Get their current locations
                                    let loc1 = self.space().element_location(&elem1);
                                    let loc2 = self.space().element_location(&elem2);

                                    tracing::info!("Element 1 location: {:?}", loc1);
                                    tracing::info!("Element 2 location: {:?}", loc2);

                                    if let (Some(loc1), Some(loc2)) = (loc1, loc2) {
                                        // Swap their positions
                                        tracing::info!("Swapping positions...");
                                        self.window_manager.update_element_position(&elem1, loc2);
                                        self.window_manager.update_element_position(&elem2, loc1);

                                        tracing::info!("After swap:");
                                        tracing::info!(
                                            "Element 1 new location: {:?}",
                                            self.space().element_location(&elem1)
                                        );
                                        tracing::info!(
                                            "Element 2 new location: {:?}",
                                            self.space().element_location(&elem2)
                                        );
                                    } else {
                                        tracing::warn!("Could not get locations for windows");
                                    }
                                } else {
                                    tracing::warn!(
                                        "Could not find WindowElements for tracked windows"
                                    );
                                }
                            } else {
                                tracing::info!("Not enough windows to swap (need at least 2)");
                            }
                        } else {
                            tracing::info!("Not enough windows to swap (need at least 2)");
                        }
                    }
                }
            }
        }

        tracing::info!("========================================");
    }
}

#[derive(Default)]
pub struct SurfaceData {
    pub geometry: Option<Rectangle<i32, Logical>>,
    pub resize_state: ResizeState,
}

fn ensure_initial_configure(
    surface: &WlSurface,
    space: &Space<WindowElement>,
    popups: &mut PopupManager,
) {
    with_surface_tree_upward(
        surface,
        (),
        |_, _, _| TraversalAction::DoChildren(()),
        |_, states, _| {
            states
                .data_map
                .insert_if_missing(|| RefCell::new(SurfaceData::default()));
        },
        |_, _, _| true,
    );

    if let Some(window) = space
        .elements()
        .find(|window| window.wl_surface().map(|s| &*s == surface).unwrap_or(false))
        .cloned()
    {
        // send the initial configure if relevant
        #[cfg_attr(not(feature = "xwayland"), allow(irrefutable_let_patterns))]
        if let Some(toplevel) = window.0.toplevel() {
            let initial_configure_sent = with_states(surface, |states| {
                states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .and_then(|data| data.lock().ok())
                    .map(|data| data.initial_configure_sent)
                    .unwrap_or(false)
            });
            if !initial_configure_sent {
                toplevel.send_configure();
            }
        }

        with_states(surface, |states| {
            let data = states.data_map.get::<RefCell<SurfaceData>>();
            if let Some(data) = data {
                let mut data = data.borrow_mut();

                // Finish resizing.
                if let ResizeState::WaitingForCommit(_) = data.resize_state {
                    data.resize_state = ResizeState::NotResizing;
                }
            }
        });

        return;
    }

    if let Some(popup) = popups.find_popup(surface) {
        let popup = match popup {
            PopupKind::Xdg(ref popup) => popup,
            // Doesn't require configure
            PopupKind::InputMethod(ref _input_popup) => {
                return;
            }
        };

        if !popup.is_initial_configure_sent() {
            // NOTE: This should never fail as the initial configure is always
            // allowed.
            popup.send_configure().expect("initial configure failed");
        }

        return;
    };

    if let Some(output) = space.outputs().find(|o| {
        let map = layer_map_for_output(o);
        map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
            .is_some()
    }) {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<LayerSurfaceData>()
                .and_then(|data| data.lock().ok())
                .map(|data| data.initial_configure_sent)
                .unwrap_or(false)
        });

        let mut map = layer_map_for_output(output);

        // arrange the layers before sending the initial configure
        // to respect any size the client may have sent
        map.arrange();
        // send the initial configure if relevant
        if !initial_configure_sent {
            if let Some(layer) = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL) {
                layer.layer_surface().send_configure();
            }
        }
    };
}

// TODO: Remove this function once fixup_positions is refactored to use WindowRegistry
// This is only used for orphaned windows when outputs change
fn place_new_window(
    space: &mut Space<WindowElement>,
    pointer_location: Point<f64, Logical>,
    window: &WindowElement,
    activate: bool,
) {
    // place the window at a random location on same output as pointer
    // or if there is not output in a [0;800]x[0;800] square
    use rand::distributions::{Distribution, Uniform};

    let output = space
        .output_under(pointer_location)
        .next()
        .or_else(|| space.outputs().next())
        .cloned();
    let output_geometry = output
        .and_then(|o| {
            let geo = space.output_geometry(&o)?;
            let map = layer_map_for_output(&o);
            let zone = map.non_exclusive_zone();
            Some(Rectangle::new(geo.loc + zone.loc, zone.size))
        })
        .unwrap_or_else(|| Rectangle::from_size((800, 800).into()));

    // set the initial toplevel bounds
    #[allow(irrefutable_let_patterns)]
    if let Some(toplevel) = window.0.toplevel() {
        toplevel.with_pending_state(|state| {
            state.bounds = Some(output_geometry.size);
        });
    }

    let max_x = output_geometry.loc.x + (((output_geometry.size.w as f32) / 3.0) * 2.0) as i32;
    let max_y = output_geometry.loc.y + (((output_geometry.size.h as f32) / 3.0) * 2.0) as i32;
    let x_range = Uniform::new(output_geometry.loc.x, max_x);
    let y_range = Uniform::new(output_geometry.loc.y, max_y);
    let mut rng = rand::thread_rng();
    let x = x_range.sample(&mut rng);
    let y = y_range.sample(&mut rng);

    space.map_element(window.clone(), (x, y), activate);
}

pub fn fixup_positions(space: &mut Space<WindowElement>, pointer_location: Point<f64, Logical>) {
    fixup_positions_with_config(space, pointer_location, &[]);
}

pub fn fixup_positions_with_config(
    space: &mut Space<WindowElement>,
    pointer_location: Point<f64, Logical>,
    output_configs: &[crate::config::OutputConfig],
) {
    // fixup outputs
    let mut offset = Point::<i32, Logical>::from((0, 0));
    for output in space.outputs().cloned().collect::<Vec<_>>().into_iter() {
        let output_name = output.name();
        
        // Check if this output has a configured position
        let configured_position = output_configs
            .iter()
            .find(|c| c.name == output_name)
            .and_then(|c| c.position);
        
        if let Some((x, y)) = configured_position {
            // Use configured position
            space.map_output(&output, Point::from((x, y)));
        } else {
            // Use automatic horizontal layout
            let size = space
                .output_geometry(&output)
                .map(|geo| geo.size)
                .unwrap_or_else(|| Size::from((0, 0)));
            space.map_output(&output, offset);
            offset.x += size.w;
        }
        
        layer_map_for_output(&output).arrange();
    }

    // fixup windows
    let mut orphaned_windows = Vec::new();
    let outputs = space
        .outputs()
        .flat_map(|o| {
            let geo = space.output_geometry(o)?;
            let map = layer_map_for_output(o);
            let zone = map.non_exclusive_zone();
            Some(Rectangle::new(geo.loc + zone.loc, zone.size))
        })
        .collect::<Vec<_>>();
    for window in space.elements() {
        let window_location = match space.element_location(window) {
            Some(loc) => loc,
            None => continue,
        };
        let geo_loc = window.bbox().loc + window_location;

        if !outputs.iter().any(|o_geo| o_geo.contains(geo_loc)) {
            orphaned_windows.push(window.clone());
        }
    }
    for window in orphaned_windows.into_iter() {
        place_new_window(space, pointer_location, &window, false);
    }
}
