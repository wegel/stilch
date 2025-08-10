// Allow in this module because of existing usage
#![allow(clippy::uninlined_format_args)]
use std::{
    collections::hash_map::HashMap,
    io,
    ops::Not,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Once,
    },
    time::{Duration, Instant},
};

use crate::{
    drawing::*,
    render::*,
    shell::WindowElement,
    state::{take_presentation_feedback, update_primary_scanout_output, Backend, StilchState},
};
use crate::{
    shell::WindowRenderElement,
    state::{DndIcon, SurfaceDmabufFeedback},
};
#[cfg(feature = "renderer_sync")]
use smithay::backend::drm::compositor::PrimaryPlaneElement;
#[cfg(feature = "egl")]
use smithay::backend::renderer::ImportEgl;
#[cfg(feature = "debug")]
use smithay::backend::renderer::{multigpu::MultiTexture, ImportMem};
use smithay::{
    backend::{
        allocator::{
            dmabuf::Dmabuf,
            format::FormatSet,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            Fourcc, Modifier,
        },
        drm::{
            compositor::{DrmCompositor, FrameFlags},
            exporter::gbm::GbmFramebufferExporter,
            output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements},
            CreateDrmNodeError, DrmAccessError, DrmDevice, DrmDeviceFd, DrmError, DrmEvent,
            DrmEventMetadata, DrmEventTime, DrmNode, DrmSurface, GbmBufferedSurface, NodeType,
        },
        egl::{self, context::ContextPriority, EGLDevice, EGLDisplay},
        input::InputEvent,
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            damage::Error as OutputDamageTrackerError,
            element::{memory::MemoryRenderBuffer, AsRenderElements, RenderElementStates},
            gles::GlesRenderer,
            multigpu::{gbm::GbmGlesBackend, GpuManager, MultiRenderer},
            DebugFlags, ImportDma, ImportMemWl,
        },
        session::{
            libseat::{self, LibSeatSession},
            Event as SessionEvent, Session,
        },
        udev::{all_gpus, primary_gpu, UdevBackend, UdevEvent},
        SwapBuffersError,
    },
    delegate_dmabuf, delegate_drm_lease,
    desktop::{
        space::{Space, SurfaceTree},
        utils::OutputPresentationFeedback,
    },
    input::{
        keyboard::LedState,
        pointer::{CursorImageAttributes, CursorImageStatus},
    },
    output::{Mode as WlMode, Output, PhysicalProperties},
    reexports::{
        calloop::{
            timer::{TimeoutAction, Timer},
            EventLoop, RegistrationToken,
        },
        drm::{
            control::{connector, crtc, Device, ModeTypeFlags},
            Device as _,
        },
        input::{DeviceCapability, Libinput},
        rustix::fs::OFlags,
        wayland_protocols::wp::{
            linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1,
            presentation_time::server::wp_presentation_feedback,
        },
        wayland_server::{backend::GlobalId, protocol::wl_surface, Display, DisplayHandle},
    },
    utils::{DeviceFd, IsAlive, Monotonic, Point, Scale, Time, Transform},
    utils::{Logical, Rectangle, Size},
    wayland::{
        compositor,
        dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier},
        drm_lease::{
            DrmLease, DrmLeaseBuilder, DrmLeaseHandler, DrmLeaseRequest, DrmLeaseState,
            LeaseRejected,
        },
        drm_syncobj::{supports_syncobj_eventfd, DrmSyncobjHandler, DrmSyncobjState},
        presentation::Refresh,
    },
};
use smithay_drm_extras::{
    display_info,
    drm_scanner::{DrmScanEvent, DrmScanner},
};
use tracing::{debug, error, info, trace, warn};

// we cannot simply pick the first supported format of the intersection of *all* formats, because:
// - we do not want something like Abgr4444, which looses color information, if something better is available
// - some formats might perform terribly
// - we might need some work-arounds, if one supports modifiers, but the other does not
//
// So lets just pick `ARGB2101010` (10-bit) or `ARGB8888` (8-bit) for now, they are widely supported.
const SUPPORTED_FORMATS: &[Fourcc] = &[
    Fourcc::Abgr2101010,
    Fourcc::Argb2101010,
    Fourcc::Abgr8888,
    Fourcc::Argb8888,
];
const SUPPORTED_FORMATS_8BIT_ONLY: &[Fourcc] = &[Fourcc::Abgr8888, Fourcc::Argb8888];

type UdevRenderer<'a> = MultiRenderer<
    'a,
    'a,
    GbmGlesBackend<GlesRenderer, DrmDeviceFd>,
    GbmGlesBackend<GlesRenderer, DrmDeviceFd>,
>;

#[derive(Debug, PartialEq)]
pub struct UdevOutputId {
    pub device_id: DrmNode,
    pub crtc: crtc::Handle,
}

pub struct UdevData {
    pub session: LibSeatSession,
    dh: DisplayHandle,
    dmabuf_state: Option<(DmabufState, DmabufGlobal)>,
    syncobj_state: Option<DrmSyncobjState>,
    primary_gpu: DrmNode,
    gpus: GpuManager<GbmGlesBackend<GlesRenderer, DrmDeviceFd>>,
    backends: HashMap<DrmNode, BackendData>,
    pointer_images: Vec<(xcursor::parser::Image, MemoryRenderBuffer)>,
    pointer_element: PointerElement,
    #[cfg(feature = "debug")]
    fps_texture: Option<MultiTexture>,
    pointer_image: crate::cursor::Cursor,
    debug_flags: DebugFlags,
    keyboards: Vec<smithay::reexports::input::Device>,
    outputs_needing_render: HashMap<(DrmNode, crtc::Handle), ()>,
    render_idle_scheduled: Arc<AtomicBool>,
}

impl UdevData {
    pub fn set_debug_flags(&mut self, flags: DebugFlags) {
        if self.debug_flags != flags {
            self.debug_flags = flags;

            for (_, backend) in self.backends.iter_mut() {
                for (_, surface) in backend.surfaces.iter_mut() {
                    surface.drm_output.set_debug_flags(flags);
                }
            }
        }
    }

    pub fn debug_flags(&self) -> DebugFlags {
        self.debug_flags
    }
}

impl DmabufHandler for StilchState<UdevData> {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self
            .backend_data
            .dmabuf_state
            .as_mut()
            // SAFETY: dmabuf_state is always initialized before this handler is registered
            .expect("DmaBuf state should be initialized for udev backend")
            .0
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        if self
            .backend_data
            .gpus
            .single_renderer(&self.backend_data.primary_gpu)
            .and_then(|mut renderer| renderer.import_dmabuf(&dmabuf, None))
            .is_ok()
        {
            dmabuf.set_node(self.backend_data.primary_gpu);
            let _ = notifier.successful::<StilchState<UdevData>>();
        } else {
            notifier.failed();
        }
    }
}
delegate_dmabuf!(StilchState<UdevData>);

impl Backend for UdevData {
    const HAS_RELATIVE_MOTION: bool = true;
    const HAS_GESTURES: bool = true;

    fn seat_name(&self) -> String {
        self.session.seat()
    }

    fn reset_buffers(&mut self, output: &Output) {
        if let Some(id) = output.user_data().get::<UdevOutputId>() {
            if let Some(gpu) = self.backends.get_mut(&id.device_id) {
                if let Some(surface) = gpu.surfaces.get_mut(&id.crtc) {
                    surface.drm_output.reset_buffers();
                }
            }
        }
    }

    fn early_import(&mut self, surface: &wl_surface::WlSurface) {
        if let Err(err) = self.gpus.early_import(self.primary_gpu, surface) {
            warn!("Early buffer import failed: {err}");
        }
    }

    fn update_led_state(&mut self, led_state: LedState) {
        for keyboard in self.keyboards.iter_mut() {
            keyboard.led_update(led_state.into());
        }
    }

    fn request_render(&mut self) {
        // Mark all outputs as needing render
        // In a more sophisticated implementation, we'd track which specific outputs need render
        for (node, backend) in self.backends.iter() {
            for &crtc in backend.surfaces.keys() {
                self.outputs_needing_render.insert((*node, crtc), ());
            }
        }
    }

    fn should_schedule_render(&self) -> bool {
        // Only schedule if not already scheduled
        !self.render_idle_scheduled.load(Ordering::Acquire)
    }
}

pub fn run_udev(enable_test_ipc: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop =
        EventLoop::try_new().map_err(|e| format!("Failed to create event loop: {e}"))?;
    let display = Display::new().map_err(|e| format!("Failed to create Wayland display: {e}"))?;
    let mut display_handle = display.handle();

    /*
     * Initialize session
     */
    let (session, notifier) = match LibSeatSession::new() {
        Ok(ret) => ret,
        Err(err) => {
            return Err(format!("Could not initialize a session: {err}").into());
        }
    };

    /*
     * Initialize the compositor
     */
    let primary_gpu = if let Ok(var) = std::env::var("ANVIL_DRM_DEVICE") {
        DrmNode::from_path(var).map_err(|e| format!("Invalid DRM device path: {e}"))?
    } else {
        primary_gpu(session.seat())
            .ok()
            .and_then(|path| {
                path.and_then(|p| {
                    DrmNode::from_path(p)
                        .ok()?
                        .node_with_type(NodeType::Render)?
                        .ok()
                })
            })
            .or_else(|| {
                all_gpus(session.seat())
                    .ok()?
                    .into_iter()
                    .find_map(|x| DrmNode::from_path(x).ok())
            })
            .ok_or_else(|| "No GPU device found")?
    };
    info!("Using {} as primary gpu.", primary_gpu);

    let gpus = GpuManager::new(GbmGlesBackend::with_context_priority(ContextPriority::High))
        .map_err(|e| format!("Failed to initialize GPU manager: {e}"))?;

    let data = UdevData {
        dh: display_handle.clone(),
        dmabuf_state: None,
        syncobj_state: None,
        session,
        primary_gpu,
        gpus,
        backends: HashMap::new(),
        pointer_image: crate::cursor::Cursor::load(),
        pointer_images: Vec::new(),
        pointer_element: PointerElement::default(),
        #[cfg(feature = "debug")]
        fps_texture: None,
        debug_flags: DebugFlags::empty(),
        keyboards: Vec::new(),
        outputs_needing_render: HashMap::new(),
        render_idle_scheduled: Arc::new(AtomicBool::new(false)),
    };
    let mut state = StilchState::init(display, event_loop.handle(), data, true);

    /*
     * Initialize the udev backend
     */
    let udev_backend = UdevBackend::new(&state.seat_name)
        .map_err(|e| format!("Failed to initialize udev backend: {:?}", e))?;

    /*
     * Initialize libinput backend
     */
    let mut libinput_context = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(
        state.backend_data.session.clone().into(),
    );
    libinput_context
        .udev_assign_seat(&state.seat_name)
        .map_err(|_| "Failed to assign udev seat")?;
    let libinput_backend = LibinputInputBackend::new(libinput_context.clone());

    /*
     * Bind all our objects that get driven by the event loop
     */
    event_loop
        .handle()
        .insert_source(libinput_backend, move |mut event, _, data| {
            let dh = data.backend_data.dh.clone();
            if let InputEvent::DeviceAdded { device } = &mut event {
                // Configure the device to disable "disable-while-typing" feature
                // This prevents the cursor from being blocked during typing
                if device.has_capability(DeviceCapability::Pointer) {
                    // The device is already a libinput::Device, we can call methods directly
                    // Check if the device supports disable-while-typing configuration
                    if device.config_dwt_is_available() {
                        // Disable the "disable-while-typing" feature
                        if let Err(e) = device.config_dwt_set_enabled(false) {
                            warn!(
                                "Failed to disable 'disable-while-typing' for pointer device: {:?}",
                                e
                            );
                        } else {
                            info!(
                                "Disabled 'disable-while-typing' for pointer device: {}",
                                device.name()
                            );
                        }
                    }
                }

                if device.has_capability(DeviceCapability::Keyboard) {
                    if let Some(led_state) = data
                        .seat()
                        .get_keyboard()
                        .map(|keyboard| keyboard.led_state())
                    {
                        device.led_update(led_state.into());
                    }
                    data.backend_data.keyboards.push(device.clone());
                }
            } else if let InputEvent::DeviceRemoved { ref device } = event {
                if device.has_capability(DeviceCapability::Keyboard) {
                    data.backend_data.keyboards.retain(|item| item != device);
                }
            }

            data.process_input_event(&dh, event)
        })
        .map_err(|e| {
            error!("Failed to insert libinput source: {:?}", e);
            format!("Failed to insert libinput source: {e}")
        })?;

    event_loop
        .handle()
        .insert_source(notifier, move |event, &mut (), data| match event {
            SessionEvent::PauseSession => {
                libinput_context.suspend();
                info!("pausing session");

                for backend in data.backend_data.backends.values_mut() {
                    backend.drm_output_manager.pause();
                    backend.active_leases.clear();
                    if let Some(lease_global) = backend.leasing_global.as_mut() {
                        lease_global.suspend();
                    }
                }
            }
            SessionEvent::ActivateSession => {
                info!("resuming session");

                if let Err(err) = libinput_context.resume() {
                    error!("Failed to resume libinput context: {:?}", err);
                }
                // Collect outputs that need rendering first
                let mut outputs_to_render = Vec::new();

                for (node, backend) in data
                    .backend_data
                    .backends
                    .iter_mut()
                    .map(|(handle, backend)| (*handle, backend))
                {
                    // if we do not care about flicking (caused by modesetting) we could just
                    // pass true for disable connectors here. this would make sure our drm
                    // device is in a known state (all connectors and planes disabled).
                    // but for demonstration we choose a more optimistic path by leaving the
                    // state as is and assume it will just work. If this assumption fails
                    // we will try to reset the state when trying to queue a frame.
                    if let Err(e) = backend.drm_output_manager.lock().activate(false) {
                        error!("Failed to activate drm backend: {e}");
                        // Continue anyway - the backend may still be partially functional
                    }
                    if let Some(lease_global) = backend.leasing_global.as_mut() {
                        lease_global.resume::<StilchState<UdevData>>();
                    }
                    // Collect outputs on this device that need render
                    for &crtc in backend.surfaces.keys() {
                        outputs_to_render.push((node, crtc));
                    }
                }

                // Mark all collected outputs as needing render
                let has_outputs = !outputs_to_render.is_empty();
                for (node, crtc) in outputs_to_render {
                    data.backend_data
                        .outputs_needing_render
                        .insert((node, crtc), ());
                }

                // Schedule render if we have outputs that need it
                if has_outputs {
                    data.schedule_render();
                }
            }
        })
        .map_err(|e| {
            error!("Failed to insert session notifier source: {:?}", e);
            format!("Failed to insert session notifier source: {e}")
        })?;

    // We try to initialize the primary node before others to make sure
    // any display only node can fall back to the primary node for rendering
    let primary_node = primary_gpu
        .node_with_type(NodeType::Primary)
        .and_then(|node| node.ok());
    let primary_device = udev_backend.device_list().find(|(device_id, _)| {
        primary_node
            .map(|primary_node| *device_id == primary_node.dev_id())
            .unwrap_or(false)
            || *device_id == primary_gpu.dev_id()
    });

    if let Some((device_id, path)) = primary_device {
        match DrmNode::from_dev_id(device_id) {
            Ok(node) => {
                if let Err(e) = state.device_added(node, path) {
                    error!("Failed to initialize primary node: {e}");
                }
            }
            Err(e) => {
                error!(
                    "Failed to get primary node from device id {}: {}",
                    device_id, e
                );
            }
        }
    }

    let primary_device_id = primary_device.map(|(device_id, _)| device_id);
    for (device_id, path) in udev_backend.device_list() {
        if Some(device_id) == primary_device_id {
            continue;
        }

        if let Err(err) = DrmNode::from_dev_id(device_id)
            .map_err(DeviceAddError::DrmNode)
            .and_then(|node| state.device_added(node, path))
        {
            error!("Skipping device {device_id}: {err}");
        }
    }
    state.protocols.shm_state.update_formats(
        state
            .backend_data
            .gpus
            .single_renderer(&primary_gpu)
            .map_err(|e| {
                error!("Failed to get single renderer: {:?}", e);
                format!("Failed to get single renderer: {e}")
            })?
            .shm_formats(),
    );

    #[cfg_attr(not(feature = "egl"), allow(unused_mut))]
    let mut renderer = state
        .backend_data
        .gpus
        .single_renderer(&primary_gpu)
        .unwrap_or_else(|e| {
            tracing::error!("FATAL: Failed to get GPU renderer: {e}");
            tracing::error!("The compositor cannot continue without a working GPU renderer");
            // This is a critical initialization failure - we cannot continue
            std::process::exit(1);
        });

    #[cfg(feature = "debug")]
    {
        #[allow(deprecated)]
        let fps_image = image::io::Reader::with_format(
            std::io::Cursor::new(FPS_NUMBERS_PNG),
            image::ImageFormat::Png,
        )
        .decode()
        .map_err(|e| {
            error!("Failed to decode FPS numbers image: {:?}", e);
            format!("Failed to decode FPS numbers image: {e}")
        })?;
        let fps_texture = renderer
            .import_memory(
                &fps_image.to_rgba8(),
                Fourcc::Abgr8888,
                (fps_image.width() as i32, fps_image.height() as i32).into(),
                false,
            )
            .map_err(|e| {
                error!("Unable to upload FPS texture: {e}");
                Box::new(e) as Box<dyn std::error::Error>
            })?;

        for backend in state.backend_data.backends.values_mut() {
            for surface in backend.surfaces.values_mut() {
                surface.fps_element = Some(FpsElement::new(fps_texture.clone()));
            }
        }
        state.backend_data.fps_texture = Some(fps_texture);
    }

    #[cfg(feature = "egl")]
    {
        info!(
            ?primary_gpu,
            "Trying to initialize EGL Hardware Acceleration",
        );
        match renderer.bind_wl_display(&display_handle) {
            Ok(_) => info!("EGL hardware-acceleration enabled"),
            Err(err) => info!(?err, "Failed to initialize EGL hardware-acceleration"),
        }
    }

    // init dmabuf support with format list from our primary gpu
    let dmabuf_formats = renderer.dmabuf_formats();
    let default_feedback = DmabufFeedbackBuilder::new(primary_gpu.dev_id(), dmabuf_formats)
        .build()
        .map_err(|e| {
            error!("Failed to build dmabuf feedback: {:?}", e);
            format!("Failed to build dmabuf feedback: {e}")
        })?;
    let mut dmabuf_state = DmabufState::new();
    let global = dmabuf_state.create_global_with_default_feedback::<StilchState<UdevData>>(
        &display_handle,
        &default_feedback,
    );
    state.backend_data.dmabuf_state = Some((dmabuf_state, global));

    let gpus = &mut state.backend_data.gpus;
    state
        .backend_data
        .backends
        .iter_mut()
        .for_each(|(node, backend_data)| {
            // Update the per drm surface dmabuf feedback
            backend_data.surfaces.values_mut().for_each(|surface_data| {
                surface_data.dmabuf_feedback = surface_data.dmabuf_feedback.take().or_else(|| {
                    surface_data.drm_output.with_compositor(|compositor| {
                        get_surface_dmabuf_feedback(
                            primary_gpu,
                            surface_data.render_node,
                            *node,
                            gpus,
                            compositor.surface(),
                        )
                    })
                });
            });
        });

    // Expose syncobj protocol if supported by primary GPU
    if let Some(primary_node) = state
        .backend_data
        .primary_gpu
        .node_with_type(NodeType::Primary)
        .and_then(|x| x.ok())
    {
        if let Some(backend) = state.backend_data.backends.get(&primary_node) {
            let import_device = backend.drm_output_manager.device().device_fd().clone();
            if supports_syncobj_eventfd(&import_device) {
                let syncobj_state =
                    DrmSyncobjState::new::<StilchState<UdevData>>(&display_handle, import_device);
                state.backend_data.syncobj_state = Some(syncobj_state);
            }
        }
    }

    event_loop
        .handle()
        .insert_source(udev_backend, move |event, _, data| match event {
            UdevEvent::Added { device_id, path } => {
                if let Err(err) = DrmNode::from_dev_id(device_id)
                    .map_err(DeviceAddError::DrmNode)
                    .and_then(|node| data.device_added(node, &path))
                {
                    error!("Skipping device {device_id}: {err}");
                }
            }
            UdevEvent::Changed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    data.device_changed(node)
                }
            }
            UdevEvent::Removed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    data.device_removed(node)
                }
            }
        })
        .map_err(|e| {
            error!("Failed to insert udev source: {:?}", e);
            format!("Failed to insert udev source: {e}")
        })?;

    /*
     * Start XWayland if supported
     */
    #[cfg(feature = "xwayland")]
    state.start_xwayland();

    /*
     * Initialize IPC server
     */
    if let Err(e) = state.init_ipc_server() {
        warn!("Failed to initialize IPC server: {e}");
    }

    /*
     * Initialize test IPC server if requested
     */
    if enable_test_ipc {
        let socket_path = std::env::var("STILCH_TEST_SOCKET")
            .unwrap_or_else(|_| "/tmp/stilch-test.sock".to_string());

        if let Err(e) =
            crate::test_ipc_server::init_test_ipc_server(&mut state, &mut event_loop, socket_path)
        {
            warn!("Failed to initialize test IPC server: {e}");
        } else {
            info!("Test IPC server started successfully");
        }
    }

    /*
     * And run our loop
     */

    tracing::info!("Starting main event loop");
    while state.running.load(Ordering::SeqCst) {
        let result = event_loop.dispatch(None, &mut state);
        if result.is_err() {
            tracing::error!("Event loop dispatch failed, exiting");
            state.running.store(false, Ordering::SeqCst);
        } else {
            state.space_mut().refresh();
            state.popups_mut().cleanup();
            display_handle.flush_clients().unwrap();

            // Process any pending renders
            state.process_pending_renders();

            // Execute startup commands after first successful dispatch
            if !state.startup_done.get() {
                state.startup_done.set(true);
                state.execute_startup_commands();
            }
        }
    }

    Ok(())
}

impl DrmLeaseHandler for StilchState<UdevData> {
    fn drm_lease_state(&mut self, node: DrmNode) -> &mut DrmLeaseState {
        self.backend_data
            .backends
            .get_mut(&node)
            // SAFETY: DrmLeaseHandler is only called for nodes we've registered
            .expect("DRM node not found in backends - this is a bug")
            .leasing_global
            .as_mut()
            // SAFETY: leasing_global is always initialized when DRM lease is enabled
            .expect("Leasing global not initialized - this is a bug")
    }

    fn lease_request(
        &mut self,
        node: DrmNode,
        request: DrmLeaseRequest,
    ) -> Result<DrmLeaseBuilder, LeaseRejected> {
        let backend = self
            .backend_data
            .backends
            .get(&node)
            .ok_or(LeaseRejected::default())?;

        let drm_device = backend.drm_output_manager.device();
        let mut builder = DrmLeaseBuilder::new(drm_device);
        for conn in request.connectors {
            if let Some((_, crtc)) = backend
                .non_desktop_connectors
                .iter()
                .find(|(handle, _)| *handle == conn)
            {
                builder.add_connector(conn);
                builder.add_crtc(*crtc);
                let planes = drm_device.planes(crtc).map_err(LeaseRejected::with_cause)?;
                let (primary_plane, primary_plane_claim) = planes
                    .primary
                    .iter()
                    .find_map(|plane| {
                        drm_device
                            .claim_plane(plane.handle, *crtc)
                            .map(|claim| (plane, claim))
                    })
                    .ok_or_else(LeaseRejected::default)?;
                builder.add_plane(primary_plane.handle, primary_plane_claim);
                if let Some((cursor, claim)) = planes.cursor.iter().find_map(|plane| {
                    drm_device
                        .claim_plane(plane.handle, *crtc)
                        .map(|claim| (plane, claim))
                }) {
                    builder.add_plane(cursor.handle, claim);
                }
            } else {
                tracing::warn!(
                    ?conn,
                    "Lease requested for desktop connector, denying request"
                );
                return Err(LeaseRejected::default());
            }
        }

        Ok(builder)
    }

    fn new_active_lease(&mut self, node: DrmNode, lease: DrmLease) {
        if let Some(backend) = self.backend_data.backends.get_mut(&node) {
            backend.active_leases.push(lease);
        } else {
            tracing::error!("Backend not found for node {:?} when adding lease", node);
        }
    }

    fn lease_destroyed(&mut self, node: DrmNode, lease: u32) {
        if let Some(backend) = self.backend_data.backends.get_mut(&node) {
            backend.active_leases.retain(|l| l.id() != lease);
        } else {
            tracing::error!(
                "Backend not found for node {:?} when destroying lease {}",
                node,
                lease
            );
        }
    }
}

delegate_drm_lease!(StilchState<UdevData>);

impl DrmSyncobjHandler for StilchState<UdevData> {
    fn drm_syncobj_state(&mut self) -> Option<&mut DrmSyncobjState> {
        self.backend_data.syncobj_state.as_mut()
    }
}
smithay::delegate_drm_syncobj!(StilchState<UdevData>);

pub type RenderSurface =
    GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, Option<OutputPresentationFeedback>>;

pub type GbmDrmCompositor = DrmCompositor<
    GbmAllocator<DrmDeviceFd>,
    GbmDevice<DrmDeviceFd>,
    Option<OutputPresentationFeedback>,
    DrmDeviceFd,
>;

struct SurfaceData {
    dh: DisplayHandle,
    _device_id: DrmNode,
    render_node: Option<DrmNode>,
    output: Output,
    global: Option<GlobalId>,
    drm_output: DrmOutput<
        GbmAllocator<DrmDeviceFd>,
        GbmFramebufferExporter<DrmDeviceFd>,
        Option<OutputPresentationFeedback>,
        DrmDeviceFd,
    >,
    disable_direct_scanout: bool,
    #[cfg(feature = "debug")]
    fps: fps_ticker::Fps,
    #[cfg(feature = "debug")]
    fps_element: Option<FpsElement<MultiTexture>>,
    dmabuf_feedback: Option<SurfaceDmabufFeedback>,
    last_presentation_time: Option<Time<Monotonic>>,
    vblank_throttle_timer: Option<RegistrationToken>,
}

impl Drop for SurfaceData {
    fn drop(&mut self) {
        self.output.leave_all();
        if let Some(global) = self.global.take() {
            self.dh.remove_global::<StilchState<UdevData>>(global);
        }
    }
}

struct BackendData {
    surfaces: HashMap<crtc::Handle, SurfaceData>,
    non_desktop_connectors: Vec<(connector::Handle, crtc::Handle)>,
    leasing_global: Option<DrmLeaseState>,
    active_leases: Vec<DrmLease>,
    drm_output_manager: DrmOutputManager<
        GbmAllocator<DrmDeviceFd>,
        GbmFramebufferExporter<DrmDeviceFd>,
        Option<OutputPresentationFeedback>,
        DrmDeviceFd,
    >,
    drm_scanner: DrmScanner,
    render_node: Option<DrmNode>,
    registration_token: RegistrationToken,
}

#[derive(Debug, thiserror::Error)]
enum DeviceAddError {
    #[error("Failed to open device using libseat: {0}")]
    DeviceOpen(libseat::Error),
    #[error("Failed to initialize drm device: {0}")]
    DrmDevice(DrmError),
    #[error("Failed to initialize gbm device: {0}")]
    GbmDevice(std::io::Error),
    #[error("Failed to access drm node: {0}")]
    DrmNode(CreateDrmNodeError),
    #[error("Failed to add device to GpuManager: {0}")]
    AddNode(egl::Error),
    #[error("The device has no render node")]
    NoRenderNode,
    #[error("Primary GPU is missing")]
    PrimaryGpuMissing,
}

fn get_surface_dmabuf_feedback(
    primary_gpu: DrmNode,
    render_node: Option<DrmNode>,
    scanout_node: DrmNode,
    gpus: &mut GpuManager<GbmGlesBackend<GlesRenderer, DrmDeviceFd>>,
    surface: &DrmSurface,
) -> Option<SurfaceDmabufFeedback> {
    let primary_formats = gpus.single_renderer(&primary_gpu).ok()?.dmabuf_formats();
    let render_formats = if let Some(render_node) = render_node {
        gpus.single_renderer(&render_node).ok()?.dmabuf_formats()
    } else {
        FormatSet::default()
    };

    let all_render_formats = primary_formats
        .iter()
        .chain(render_formats.iter())
        .copied()
        .collect::<FormatSet>();

    let planes = surface.planes().clone();

    // We limit the scan-out tranche to formats we can also render from
    // so that there is always a fallback render path available in case
    // the supplied buffer can not be scanned out directly
    let planes_formats = surface
        .plane_info()
        .formats
        .iter()
        .copied()
        .chain(planes.overlay.into_iter().flat_map(|p| p.formats))
        .collect::<FormatSet>()
        .intersection(&all_render_formats)
        .copied()
        .collect::<FormatSet>();

    let builder = DmabufFeedbackBuilder::new(primary_gpu.dev_id(), primary_formats);
    let render_feedback = if let Some(render_node) = render_node {
        builder
            .clone()
            .add_preference_tranche(render_node.dev_id(), None, render_formats.clone())
            .build()
            .map_err(|e| {
                error!("Failed to build render feedback: {:?}", e);
                e
            })
            .ok()?
    } else {
        builder
            .clone()
            .build()
            .map_err(|e| {
                error!("Failed to build render feedback: {:?}", e);
                e
            })
            .ok()?
    };

    let scanout_feedback = builder
        .add_preference_tranche(
            surface
                .device_fd()
                .dev_id()
                .map_err(|e| {
                    error!("Failed to get device id: {:?}", e);
                    e
                })
                .ok()?,
            Some(zwp_linux_dmabuf_feedback_v1::TrancheFlags::Scanout),
            planes_formats,
        )
        .add_preference_tranche(scanout_node.dev_id(), None, render_formats)
        .build()
        .map_err(|e| {
            error!("Failed to build scanout feedback: {:?}", e);
            e
        })
        .ok()?;

    Some(SurfaceDmabufFeedback {
        render_feedback,
        scanout_feedback,
    })
}

impl StilchState<UdevData> {
    pub fn schedule_render(&mut self) {
        self.backend_data.request_render();

        // Schedule idle callback if not already scheduled
        if !self
            .backend_data
            .render_idle_scheduled
            .load(Ordering::Acquire)
        {
            self.backend_data
                .render_idle_scheduled
                .store(true, Ordering::Release);
            self.handle.insert_idle(|state| {
                state
                    .backend_data
                    .render_idle_scheduled
                    .store(false, Ordering::Release);
                state.process_pending_renders();
            });
        }
    }

    fn process_pending_renders(&mut self) {
        // Get all outputs that need rendering
        let outputs_to_render: Vec<_> = self.backend_data.outputs_needing_render.drain().collect();

        if outputs_to_render.is_empty() {
            return;
        }

        tracing::trace!("Processing {} pending renders", outputs_to_render.len());

        for ((node, crtc), _) in outputs_to_render {
            self.render_surface(node, crtc, self.clock.now());
        }
    }

    fn device_added(&mut self, node: DrmNode, path: &Path) -> Result<(), DeviceAddError> {
        // Try to open the device
        let fd = self
            .backend_data
            .session
            .open(
                path,
                OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
            )
            .map_err(DeviceAddError::DeviceOpen)?;

        let fd = DrmDeviceFd::new(DeviceFd::from(fd));

        let (drm, notifier) =
            DrmDevice::new(fd.clone(), true).map_err(DeviceAddError::DrmDevice)?;
        let gbm = GbmDevice::new(fd).map_err(DeviceAddError::GbmDevice)?;

        let registration_token = self
            .handle
            .insert_source(
                notifier,
                move |event, metadata, data: &mut StilchState<_>| match event {
                    DrmEvent::VBlank(crtc) => {
                        profiling::scope!("vblank", &format!("{crtc:?}"));
                        data.frame_finish(node, crtc, metadata);
                    }
                    DrmEvent::Error(error) => {
                        error!("{:?}", error);
                    }
                },
            )
            .map_err(|e| {
                error!("Failed to insert drm notifier source: {:?}", e);
                DeviceAddError::AddNode(egl::Error::DisplayNotSupported)
            })?;

        let mut try_initialize_gpu = || {
            let display = unsafe { EGLDisplay::new(gbm.clone()).map_err(DeviceAddError::AddNode)? };
            let egl_device =
                EGLDevice::device_for_display(&display).map_err(DeviceAddError::AddNode)?;

            if egl_device.is_software() {
                return Err(DeviceAddError::NoRenderNode);
            }

            let render_node = egl_device
                .try_get_render_node()
                .ok()
                .flatten()
                .unwrap_or(node);
            self.backend_data
                .gpus
                .as_mut()
                .add_node(render_node, gbm.clone())
                .map_err(DeviceAddError::AddNode)?;

            std::result::Result::<DrmNode, DeviceAddError>::Ok(render_node)
        };

        let render_node = try_initialize_gpu()
            .inspect_err(|err| {
                warn!(?err, "failed to initialize gpu");
            })
            .ok();

        let allocator = render_node
            .is_some()
            .then(|| {
                GbmAllocator::new(
                    gbm.clone(),
                    GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
                )
            })
            .or_else(|| {
                self.backend_data
                    .backends
                    .get(&self.backend_data.primary_gpu)
                    .or_else(|| {
                        self.backend_data.backends.values().find(|backend| {
                            backend.render_node == Some(self.backend_data.primary_gpu)
                        })
                    })
                    .map(|backend| backend.drm_output_manager.allocator().clone())
            })
            .ok_or(DeviceAddError::PrimaryGpuMissing)?;

        let framebuffer_exporter = GbmFramebufferExporter::new(gbm.clone(), render_node.into());

        let color_formats = if std::env::var("ANVIL_DISABLE_10BIT").is_ok() {
            SUPPORTED_FORMATS_8BIT_ONLY
        } else {
            SUPPORTED_FORMATS
        };
        let mut renderer = self
            .backend_data
            .gpus
            .single_renderer(&render_node.unwrap_or(self.backend_data.primary_gpu))
            .map_err(|e| {
                error!("Failed to get single renderer: {:?}", e);
                DeviceAddError::AddNode(egl::Error::DisplayNotSupported)
            })?;
        let render_formats = renderer
            .as_mut()
            .egl_context()
            .dmabuf_render_formats()
            .iter()
            .filter(|format| render_node.is_some() || format.modifier == Modifier::Linear)
            .copied()
            .collect::<FormatSet>();

        let drm_output_manager = DrmOutputManager::new(
            drm,
            allocator,
            framebuffer_exporter,
            Some(gbm),
            color_formats.iter().copied(),
            render_formats,
        );

        self.backend_data.backends.insert(
            node,
            BackendData {
                registration_token,
                drm_output_manager,
                drm_scanner: DrmScanner::new(),
                non_desktop_connectors: Vec::new(),
                render_node,
                surfaces: HashMap::new(),
                leasing_global: DrmLeaseState::new::<StilchState<UdevData>>(
                    &self.display_handle,
                    &node,
                )
                .inspect_err(|err| {
                    warn!(?err, "Failed to initialize drm lease global for: {}", node);
                })
                .ok(),
                active_leases: Vec::new(),
            },
        );

        self.device_changed(node);

        Ok(())
    }

    fn calculate_suggested_scale(physical_size_mm: (i32, i32), pixel_size: (i32, i32)) -> f64 {
        // Calculate DPI and suggest appropriate scaling
        // Avoid division by zero
        if physical_size_mm.0 <= 0 || physical_size_mm.1 <= 0 {
            return 1.0;
        }

        let dpi_x = (pixel_size.0 as f64 * 25.4) / physical_size_mm.0 as f64;
        let dpi_y = (pixel_size.1 as f64 * 25.4) / physical_size_mm.1 as f64;
        let dpi = (dpi_x + dpi_y) / 2.0; // Average DPI

        info!(
            "Display DPI calculation: {}x{} pixels, {}x{}mm physical = {:.1} DPI",
            pixel_size.0, pixel_size.1, physical_size_mm.0, physical_size_mm.1, dpi
        );

        // Common DPI thresholds for scaling
        // Note: 1080p on typical laptop screens (14-15.6") is around 140-157 DPI and doesn't need scaling
        let scale = match dpi {
            dpi if dpi >= 192.0 => 2.0,  // High DPI (4K at 24" or smaller)
            dpi if dpi >= 168.0 => 1.5,  // Medium-high DPI (QHD at 14" or 1080p at 10")
            dpi if dpi >= 160.0 => 1.25, // Only for very dense displays
            _ => 1.0,                    // Standard DPI (includes all 1080p laptops 13"+)
        };

        info!("Selected scale factor: {} for DPI {:.1}", scale, dpi);
        scale
    }

    fn connector_connected(
        &mut self,
        node: DrmNode,
        connector: connector::Info,
        crtc: crtc::Handle,
    ) {
        // Calculate x position before the mutable borrow
        let x = {
            let space = self.space();
            space.outputs().fold(0, |acc, o| {
                acc + space.output_geometry(o).map(|geo| geo.size.w).unwrap_or(0)
            })
        };

        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        let render_node = device.render_node.unwrap_or(self.backend_data.primary_gpu);
        let mut renderer = match self.backend_data.gpus.single_renderer(&render_node) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to get single renderer: {:?}", e);
                return;
            }
        };

        let output_name = format!(
            "{}-{}",
            connector.interface().as_str(),
            connector.interface_id()
        );
        info!(?crtc, "Trying to setup connector {}", output_name,);

        let drm_device = device.drm_output_manager.device();

        let non_desktop = drm_device
            .get_properties(connector.handle())
            .ok()
            .and_then(|props| {
                let (info, value) = props
                    .into_iter()
                    .filter_map(|(handle, value)| {
                        let info = drm_device.get_property(handle).ok()?;

                        Some((info, value))
                    })
                    .find(|(info, _)| info.name().to_str() == Ok("non-desktop"))?;

                info.value_type().convert_value(value).as_boolean()
            })
            .unwrap_or(false);

        let display_info = display_info::for_connector(drm_device, connector.handle());

        let make = display_info
            .as_ref()
            .and_then(|info| info.make())
            .unwrap_or_else(|| "Unknown".into());

        let model = display_info
            .as_ref()
            .and_then(|info| info.model())
            .unwrap_or_else(|| "Unknown".into());

        if non_desktop {
            info!(
                "Connector {} is non-desktop, setting up for leasing",
                output_name
            );
            device
                .non_desktop_connectors
                .push((connector.handle(), crtc));
            if let Some(lease_state) = device.leasing_global.as_mut() {
                lease_state.add_connector::<StilchState<UdevData>>(
                    connector.handle(),
                    output_name,
                    format!("{make} {model}"),
                );
            }
        } else {
            let mode_id = connector
                .modes()
                .iter()
                .position(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
                .unwrap_or(0);

            let drm_mode = connector.modes()[mode_id];
            let wl_mode = WlMode::from(drm_mode);

            let (phys_w, phys_h) = connector.size().unwrap_or((0, 0));
            let output = Output::new(
                output_name.clone(),
                PhysicalProperties {
                    size: (phys_w as i32, phys_h as i32).into(),
                    subpixel: connector.subpixel().into(),
                    make,
                    model,
                },
            );
            let global = output.create_global::<StilchState<UdevData>>(&self.display_handle);

            let position = (x, 0).into();

            output.set_preferred(wl_mode);

            // Determine scale from config or auto-detect
            let scale = {
                // First check if there's a configured scale for this output
                let configured_scale = self
                    .config
                    .outputs
                    .iter()
                    .find(|o| o.name == output_name)
                    .and_then(|o| o.scale);

                if let Some(scale) = configured_scale {
                    info!(
                        "Using configured scale {} for output {}",
                        scale, output_name
                    );
                    scale
                } else {
                    // Auto-detect scale based on DPI
                    let pixel_size = (drm_mode.size().0 as i32, drm_mode.size().1 as i32);
                    let suggested_scale =
                        Self::calculate_suggested_scale((phys_w as i32, phys_h as i32), pixel_size);
                    info!(
                        "Auto-detected scale {} for output {} ({}x{} pixels, {}x{}mm physical)",
                        suggested_scale, output_name, pixel_size.0, pixel_size.1, phys_w, phys_h
                    );
                    suggested_scale
                }
            };

            output.change_current_state(
                Some(wl_mode),
                None,
                Some(smithay::output::Scale::Fractional(scale)),
                Some(position),
            );

            output.user_data().insert_if_missing(|| UdevOutputId {
                crtc,
                device_id: node,
            });

            #[cfg(feature = "debug")]
            let fps_element = self.backend_data.fps_texture.clone().map(FpsElement::new);

            let driver = match drm_device.get_driver() {
                Ok(driver) => driver,
                Err(err) => {
                    warn!("Failed to query drm driver: {err}");
                    return;
                }
            };

            let mut planes = match drm_device.planes(&crtc) {
                Ok(planes) => planes,
                Err(err) => {
                    warn!("Failed to query crtc planes: {err}");
                    return;
                }
            };

            // Using an overlay plane on a nvidia card breaks
            if driver
                .name()
                .to_string_lossy()
                .to_lowercase()
                .contains("nvidia")
                || driver
                    .description()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains("nvidia")
            {
                planes.overlay = vec![];
            }

            let drm_output = match device
                .drm_output_manager
                .lock()
                .initialize_output::<_, OutputRenderElements<UdevRenderer<'_>, WindowRenderElement<UdevRenderer<'_>>>>(
                    crtc,
                    drm_mode,
                    &[connector.handle()],
                    &output,
                    Some(planes),
                    &mut renderer,
                    &DrmOutputRenderElements::default(),
                ) {
                Ok(drm_output) => drm_output,
                Err(err) => {
                    warn!("Failed to initialize drm output: {err}");
                    return;
                }
            };

            let disable_direct_scanout = std::env::var("ANVIL_DISABLE_DIRECT_SCANOUT").is_ok();

            let dmabuf_feedback = drm_output.with_compositor(|compositor| {
                compositor.set_debug_flags(self.backend_data.debug_flags);

                get_surface_dmabuf_feedback(
                    self.backend_data.primary_gpu,
                    device.render_node,
                    node,
                    &mut self.backend_data.gpus,
                    compositor.surface(),
                )
            });

            let surface = SurfaceData {
                dh: self.display_handle.clone(),
                _device_id: node,
                render_node: device.render_node,
                output: output.clone(),
                global: Some(global),
                drm_output,
                disable_direct_scanout,
                #[cfg(feature = "debug")]
                fps: fps_ticker::Fps::default(),
                #[cfg(feature = "debug")]
                fps_element,
                dmabuf_feedback,
                last_presentation_time: None,
                vblank_throttle_timer: None,
            };

            device.surfaces.insert(crtc, surface);

            // Check if this is a 4K display and split it
            let logical_size = wl_mode.size.to_logical(1);
            let output_geometry = Rectangle::<i32, Logical>::new(position, logical_size);

            // Store output reference before moving it
            let output_ref = output.clone();

            // First check if this output is part of ANY virtual output config
            let mut handled_by_virtual_config = false;

            // Clone the config to avoid borrow issues
            let virtual_configs = self.config.virtual_outputs.clone();

            // Check if this output is mentioned in any virtual config
            let output_has_virtual_configs = virtual_configs
                .iter()
                .any(|vc| vc.outputs.contains(&output_name));

            if output_has_virtual_configs {
                info!("Output {} has virtual output configurations", output_name);
                handled_by_virtual_config = true;

                // Process ALL virtual configs for this output
                for virtual_config in &virtual_configs {
                    if !virtual_config.outputs.contains(&output_name) {
                        continue;
                    }

                    info!(
                        "Creating virtual output '{}' for {}",
                        virtual_config.name, output_name
                    );

                    // For single-output virtual configs, we can create immediately
                    // For multi-output configs, wait for all outputs
                    let all_outputs_available = if virtual_config.outputs.len() == 1 {
                        true // Single output config, and we have it
                    } else {
                        virtual_config
                            .outputs
                            .iter()
                            .all(|name| self.space().outputs().any(|o| o.name() == *name))
                    };

                    if all_outputs_available {
                        info!(
                            "All outputs for virtual output '{}' are available, creating it",
                            virtual_config.name
                        );

                        // Collect all physical outputs
                        let physical_outputs: Vec<Output> = virtual_config
                            .outputs
                            .iter()
                            .filter_map(|name| {
                                self.space().outputs().find(|o| o.name() == *name).cloned()
                            })
                            .collect();

                        // Convert region config to Rectangle if specified
                        // Region is specified in physical pixels, convert to logical
                        let region = virtual_config.region.as_ref().map(|r| {
                            let scale_factor = scale; // We already have the scale from earlier
                            Rectangle::new(
                                Point::from((
                                    (r.x as f64 / scale_factor) as i32,
                                    (r.y as f64 / scale_factor) as i32,
                                )),
                                Size::from((
                                    (r.width as f64 / scale_factor) as i32,
                                    (r.height as f64 / scale_factor) as i32,
                                )),
                            )
                        });

                        // If no region specified, use the current output's geometry
                        let region = region.unwrap_or(output_geometry);

                        // Check if this virtual output already exists
                        let existing_vo = self
                            .virtual_output_manager
                            .all_virtual_outputs()
                            .find(|vo| vo.name() == virtual_config.name)
                            .map(|vo| vo.id());

                        if existing_vo.is_none() {
                            // For virtual outputs with regions, we need to ensure the physical output is passed
                            // Use the current output being connected
                            let physical_outputs_to_use = if virtual_config.outputs.len() == 1 {
                                vec![output.clone()] // Use the actual output object, not the collected one
                            } else {
                                physical_outputs
                            };

                            let virtual_output_id =
                                self.virtual_output_manager.create_virtual_output(
                                    virtual_config.name.clone(),
                                    physical_outputs_to_use,
                                    region,
                                );

                            info!(
                                "Created virtual output '{}' with id {:?}",
                                virtual_config.name, virtual_output_id
                            );
                            self.initialize_virtual_output(virtual_output_id);
                            handled_by_virtual_config = true;
                        } else {
                            info!("Virtual output '{}' already exists", virtual_config.name);
                            handled_by_virtual_config = true;
                        }
                    } else {
                        info!(
                            "Not all outputs for virtual output '{}' are available yet",
                            virtual_config.name
                        );
                        // Keep handled_by_virtual_config = true so we don't create default splits
                    }
                }
            }

            if !handled_by_virtual_config {
                // Check if this output has split configuration
                let should_split = self
                    .config
                    .outputs
                    .iter()
                    .find(|o| o.name == output_name || o.name == "*")
                    .and_then(|o| o.split.clone());

                if let Some((split_type, count)) = should_split {
                    info!(
                        "Splitting {} display based on config: {:?} into {} parts",
                        output_name, split_type, count
                    );
                    let virtual_outputs = self.virtual_output_manager.split_physical(
                        output,
                        output_geometry,
                        split_type,
                        count,
                    );
                    info!("Created {} virtual outputs", virtual_outputs.len());

                    // Initialize each virtual output with its own workspace
                    for (i, vo_id) in virtual_outputs.iter().enumerate() {
                        // Assign workspace 0 to first output, workspace 1 to second, etc.
                        let workspace_id = crate::workspace::WorkspaceId::new(i as u8);
                        info!(
                            "Initializing virtual output {} with workspace {} (display name: {})",
                            vo_id,
                            workspace_id,
                            workspace_id.display_name()
                        );

                        if let Some(vo) = self.virtual_output_manager.get(*vo_id) {
                            let area = vo.logical_region();
                            if let Err(e) = self.workspace_manager.show_workspace_on_output(
                                workspace_id,
                                *vo_id,
                                area,
                            ) {
                                error!("Failed to initialize workspace on virtual output: {:?}", e);
                            } else {
                                self.virtual_output_manager.set_active_workspace(*vo_id, i);
                            }
                        }
                    }
                } else {
                    // Create a single virtual output for this physical output
                    let virtual_output_id = self
                        .virtual_output_manager
                        .create_from_physical(output, output_geometry);
                    self.initialize_virtual_output(virtual_output_id);
                }
            }

            // Map the output in the space
            self.space_mut().map_output(&output_ref, position);

            // Update tiling area for new output
            self.update_tiling_area_from_output();

            // Schedule initial render for new output
            self.backend_data
                .outputs_needing_render
                .insert((node, crtc), ());
            trace!("Marked new output for initial render: {:?}", crtc);
            self.schedule_render();
        }
    }

    fn connector_disconnected(
        &mut self,
        node: DrmNode,
        connector: connector::Info,
        crtc: crtc::Handle,
    ) {
        // Extract what we need before device borrow
        let maybe_output = {
            let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
                device
            } else {
                return;
            };

            if let Some(pos) = device
                .non_desktop_connectors
                .iter()
                .position(|(handle, _)| *handle == connector.handle())
            {
                let _ = device.non_desktop_connectors.remove(pos);
                if let Some(leasing_state) = device.leasing_global.as_mut() {
                    leasing_state.withdraw_connector(connector.handle());
                }
                None
            } else if let Some(surface) = device.surfaces.remove(&crtc) {
                Some(surface.output.clone())
            } else {
                None
            }
        };

        // Now we can use self mutably
        if let Some(output) = maybe_output {
            // Remove any virtual outputs associated with this physical output
            let removed_virtual = self.virtual_output_manager.remove_physical_output(&output);
            for vo_id in removed_virtual {
                info!(
                    "Removed virtual output {:?} due to physical output disconnection",
                    vo_id
                );
                // Clean up workspace associations if needed
                // The workspace manager should handle this automatically when the virtual output is gone
            }

            self.space_mut().unmap_output(&output);
            self.space_mut().refresh();
        }

        let device = match self.backend_data.backends.get_mut(&node) {
            Some(d) => d,
            None => {
                error!("Device not found for node {:?}", node);
                return;
            }
        };
        let render_node = device.render_node.unwrap_or(self.backend_data.primary_gpu);
        let mut renderer = match self.backend_data.gpus.single_renderer(&render_node) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to get single renderer: {:?}", e);
                return;
            }
        };
        let _ = device.drm_output_manager.lock().try_to_restore_modifiers::<_, OutputRenderElements<
            UdevRenderer<'_>,
            WindowRenderElement<UdevRenderer<'_>>,
        >>(
            &mut renderer,
            // FIXME: For a flicker free operation we should return the actual elements for this output..
            // Instead we just use black to "simulate" a modeset :)
            &DrmOutputRenderElements::default(),
        );
    }

    fn device_changed(&mut self, node: DrmNode) {
        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        let scan_result = match device
            .drm_scanner
            .scan_connectors(device.drm_output_manager.device())
        {
            Ok(scan_result) => scan_result,
            Err(err) => {
                tracing::warn!(?err, "Failed to scan connectors");
                return;
            }
        };

        for event in scan_result {
            match event {
                DrmScanEvent::Connected {
                    connector,
                    crtc: Some(crtc),
                } => {
                    self.connector_connected(node, connector, crtc);
                }
                DrmScanEvent::Disconnected {
                    connector,
                    crtc: Some(crtc),
                } => {
                    self.connector_disconnected(node, connector, crtc);
                }
                _ => {}
            }
        }

        // fixup window coordinates
        let pointer_location = self.pointer().current_location();
        crate::shell::fixup_positions(self.space_mut(), pointer_location);
    }

    fn device_removed(&mut self, node: DrmNode) {
        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        let crtcs: Vec<_> = device
            .drm_scanner
            .crtcs()
            .map(|(info, crtc)| (info.clone(), crtc))
            .collect();

        for (connector, crtc) in crtcs {
            self.connector_disconnected(node, connector, crtc);
        }

        debug!("Surfaces dropped");

        // drop the backends on this side
        if let Some(mut backend_data) = self.backend_data.backends.remove(&node) {
            if let Some(mut leasing_global) = backend_data.leasing_global.take() {
                leasing_global.disable_global::<StilchState<UdevData>>();
            }

            if let Some(render_node) = backend_data.render_node {
                self.backend_data.gpus.as_mut().remove_node(&render_node);
            }

            self.handle.remove(backend_data.registration_token);

            debug!("Dropping device");
        }

        let pointer_location = self.pointer().current_location();
        crate::shell::fixup_positions(self.space_mut(), pointer_location);
    }

    fn frame_finish(
        &mut self,
        dev_id: DrmNode,
        crtc: crtc::Handle,
        metadata: &mut Option<DrmEventMetadata>,
    ) {
        profiling::scope!("frame_finish", &format!("{crtc:?}"));
        tracing::debug!("frame_finish called for crtc {:?}", crtc);

        // Find the output before device borrow
        let output = self
            .space()
            .outputs()
            .find(|o| {
                o.user_data().get::<UdevOutputId>()
                    == Some(&UdevOutputId {
                        device_id: dev_id,
                        crtc,
                    })
            })
            .cloned();

        let device_backend = match self.backend_data.backends.get_mut(&dev_id) {
            Some(backend) => backend,
            None => {
                error!("Trying to finish frame on non-existent backend {dev_id}");
                return;
            }
        };

        let surface = match device_backend.surfaces.get_mut(&crtc) {
            Some(surface) => surface,
            None => {
                error!("Trying to finish frame on non-existent crtc {:?}", crtc);
                return;
            }
        };

        if let Some(timer_token) = surface.vblank_throttle_timer.take() {
            self.handle.remove(timer_token);
        }

        let output = if let Some(output) = output {
            output
        } else {
            // somehow we got called with an invalid output
            return;
        };

        let Some(frame_duration) = output
            .current_mode()
            .map(|mode| Duration::from_secs_f64(1_000f64 / mode.refresh as f64))
        else {
            return;
        };

        let tp = metadata.as_ref().and_then(|metadata| match metadata.time {
            smithay::backend::drm::DrmEventTime::Monotonic(tp) => tp.is_zero().not().then_some(tp),
            smithay::backend::drm::DrmEventTime::Realtime(_) => None,
        });

        let seq = metadata
            .as_ref()
            .map(|metadata| metadata.sequence)
            .unwrap_or(0);

        let (clock, flags) = if let Some(tp) = tp {
            (
                tp.into(),
                wp_presentation_feedback::Kind::Vsync
                    | wp_presentation_feedback::Kind::HwClock
                    | wp_presentation_feedback::Kind::HwCompletion,
            )
        } else {
            (self.clock.now(), wp_presentation_feedback::Kind::Vsync)
        };

        let vblank_remaining_time = surface
            .last_presentation_time
            .map(|last_presentation_time| {
                frame_duration.saturating_sub(Time::elapsed(&last_presentation_time, clock))
            });

        if let Some(vblank_remaining_time) = vblank_remaining_time {
            if vblank_remaining_time > frame_duration / 2 {
                static WARN_ONCE: Once = Once::new();
                WARN_ONCE.call_once(|| {
                    warn!("display running faster than expected, throttling vblanks and disabling HwClock")
                });
                let throttled_time = tp
                    .map(|tp| tp.saturating_add(vblank_remaining_time))
                    .unwrap_or(Duration::ZERO);
                let throttled_metadata = DrmEventMetadata {
                    sequence: seq,
                    time: DrmEventTime::Monotonic(throttled_time),
                };
                let timer_token = self
                    .handle
                    .insert_source(
                        Timer::from_duration(vblank_remaining_time),
                        move |_, _, data| {
                            data.frame_finish(dev_id, crtc, &mut Some(throttled_metadata));
                            TimeoutAction::Drop
                        },
                    )
                    .map_err(|e| {
                        error!("Failed to register vblank throttle timer: {e}");
                        // Non-fatal: we just won't throttle properly
                    })
                    .ok();
                surface.vblank_throttle_timer = timer_token;
                return;
            }
        }
        surface.last_presentation_time = Some(clock);

        let submit_result = surface
            .drm_output
            .frame_submitted()
            .map_err(Into::<SwapBuffersError>::into);

        let schedule_render = match submit_result {
            Ok(user_data) => {
                if let Some(mut feedback) = user_data.flatten() {
                    feedback.presented(clock, Refresh::fixed(frame_duration), seq as u64, flags);
                }

                true
            }
            Err(err) => {
                warn!("Error during rendering: {:?}", err);
                match err {
                    SwapBuffersError::AlreadySwapped => true,
                    // If the device has been deactivated do not reschedule, this will be done
                    // by session resume
                    SwapBuffersError::TemporaryFailure(err)
                        if matches!(
                            err.downcast_ref::<DrmError>(),
                            Some(&DrmError::DeviceInactive)
                        ) =>
                    {
                        false
                    }
                    SwapBuffersError::TemporaryFailure(err) => matches!(
                        err.downcast_ref::<DrmError>(),
                        Some(DrmError::Access(DrmAccessError {
                            source,
                            ..
                        })) if source.kind() == io::ErrorKind::PermissionDenied
                    ),
                    SwapBuffersError::ContextLost(err) => {
                        tracing::error!("Rendering context lost: {err}");
                        tracing::error!("Will attempt to recover on next frame");
                        // Return false to skip this frame, scheduler will retry
                        false
                    }
                }
            }
        };

        if schedule_render {
            // Mark output as needing render
            self.backend_data
                .outputs_needing_render
                .insert((dev_id, crtc), ());
            trace!("Marked output for render after vblank: {:?}", crtc);
            self.schedule_render();
        }
    }

    fn render_surface(&mut self, node: DrmNode, crtc: crtc::Handle, frame_target: Time<Monotonic>) {
        profiling::scope!("render_surface", &format!("{crtc:?}"));

        let output = if let Some(output) = self.space().outputs().find(|o| {
            o.user_data().get::<UdevOutputId>()
                == Some(&UdevOutputId {
                    device_id: node,
                    crtc,
                })
        }) {
            output.clone()
        } else {
            // somehow we got called with an invalid output
            tracing::warn!("No output found for crtc {:?}", crtc);
            return;
        };

        self.pre_repaint(&output, frame_target);

        let start = Instant::now();

        // Extract values before device borrow
        let pointer_location = self.pointer().current_location();
        let show_window_preview = self.show_window_preview;
        let dnd_icon = self.dnd_icon().cloned();

        // Collect tab bar data before mutable borrows
        let tab_bar_data = crate::render::collect_tab_bar_data(self, &output);

        // TODO get scale from the rendersurface when supporting HiDPI
        let frame = self
            .backend_data
            .pointer_image
            .get_image(1 /*scale*/, self.clock.now().into());

        let device = if let Some(device) = self.backend_data.backends.get_mut(&node) {
            device
        } else {
            return;
        };

        let surface = if let Some(surface) = device.surfaces.get_mut(&crtc) {
            surface
        } else {
            return;
        };

        let primary_gpu = self.backend_data.primary_gpu;
        let render_node = surface.render_node.unwrap_or(primary_gpu);
        let mut renderer = match if primary_gpu == render_node {
            self.backend_data.gpus.single_renderer(&render_node)
        } else {
            let format = surface.drm_output.format();
            self.backend_data
                .gpus
                .renderer(&primary_gpu, &render_node, format)
        } {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to get renderer: {:?}", e);
                return;
            }
        };

        let pointer_images = &mut self.backend_data.pointer_images;
        let pointer_image = pointer_images
            .iter()
            .find_map(|(image, texture)| {
                if image == &frame {
                    Some(texture.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                let buffer = MemoryRenderBuffer::from_slice(
                    &frame.pixels_rgba,
                    Fourcc::Argb8888,
                    (frame.width as i32, frame.height as i32),
                    1,
                    Transform::Normal,
                    None,
                );
                pointer_images.push((frame, buffer.clone()));
                buffer
            });

        // Get space reference
        let space = &self.window_manager.space;
        let pointer_element = &mut self.backend_data.pointer_element;
        let cursor_status = &mut self.input_manager.cursor_status;

        let result = render_surface(
            surface,
            &mut renderer,
            space,
            &output,
            pointer_location,
            &pointer_image,
            pointer_element,
            &dnd_icon,
            cursor_status,
            show_window_preview,
            &tab_bar_data,
        );
        let reschedule = match result {
            Ok((has_rendered, states)) => {
                let dmabuf_feedback = surface.dmabuf_feedback.clone();
                self.post_repaint(&output, frame_target, dmabuf_feedback, &states);
                !has_rendered
            }
            Err(err) => {
                warn!("Error during rendering: {:#?}", err);
                match err {
                    SwapBuffersError::AlreadySwapped => false,
                    SwapBuffersError::TemporaryFailure(err) => match err.downcast_ref::<DrmError>()
                    {
                        Some(DrmError::DeviceInactive) => true,
                        Some(DrmError::Access(DrmAccessError { source, .. })) => {
                            source.kind() == io::ErrorKind::PermissionDenied
                        }
                        _ => false,
                    },
                    SwapBuffersError::ContextLost(err) => match err.downcast_ref::<DrmError>() {
                        Some(DrmError::TestFailed(_)) => {
                            // reset the complete state, disabling all connectors and planes in case we hit a test failed
                            // most likely we hit this after a tty switch when a foreign master changed CRTC <-> connector bindings
                            // and we run in a mismatch
                            if let Err(e) = device.drm_output_manager.device_mut().reset_state() {
                                error!("Failed to reset drm device: {}. Device may be in inconsistent state", e);
                                // Continue anyway - the device might recover on next frame
                            }
                            true
                        }
                        _ => {
                            tracing::error!("Unrecoverable rendering error: {err}");
                            false
                        }
                    },
                }
            }
        };

        if reschedule {
            // Rendering failed due to temporary error - mark output as needing render
            self.backend_data
                .outputs_needing_render
                .insert((node, crtc), ());
            trace!(
                "Marked output for re-render due to temporary failure: {:?}",
                crtc
            );
            self.schedule_render();
        } else {
            let elapsed = start.elapsed();
            tracing::trace!(?elapsed, "rendered surface");
        }

        profiling::finish_frame!();
    }
}

#[allow(clippy::too_many_arguments)]
#[profiling::function]
fn render_surface<'a>(
    surface: &'a mut SurfaceData,
    renderer: &mut UdevRenderer<'a>,
    space: &Space<WindowElement>,
    output: &Output,
    pointer_location: Point<f64, Logical>,
    pointer_image: &MemoryRenderBuffer,
    pointer_element: &mut PointerElement,
    dnd_icon: &Option<DndIcon>,
    cursor_status: &mut CursorImageStatus,
    show_window_preview: bool,
    tab_bar_data: &[crate::render::TabBarData],
) -> Result<(bool, RenderElementStates), SwapBuffersError> {
    let output_geometry = space.output_geometry(output).ok_or_else(|| {
        error!(
            "Failed to get output geometry for output {:?}",
            output.name()
        );
        SwapBuffersError::ContextLost(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Output geometry not found",
        )))
    })?;
    let scale = Scale::from(output.current_scale().fractional_scale());

    let mut custom_elements: Vec<CustomRenderElements<_>> = Vec::new();

    // Add tab bar elements for tabbed containers
    // TODO: Need to pass workspace/state info to render_surface to enable tab bars
    // let tab_bar_elements = crate::render::generate_tab_bar_elements(state, output);
    // custom_elements.extend(tab_bar_elements);

    if output_geometry.to_f64().contains(pointer_location) {
        let cursor_hotspot = if let CursorImageStatus::Surface(ref surface) = cursor_status {
            compositor::with_states(surface, |states| {
                states
                    .data_map
                    .get::<Mutex<CursorImageAttributes>>()
                    .and_then(|mutex| mutex.lock().ok())
                    .map(|attrs| attrs.hotspot)
                    .unwrap_or_else(|| (0, 0).into())
            })
        } else {
            (0, 0).into()
        };
        let cursor_pos = pointer_location - output_geometry.loc.to_f64();

        // set cursor
        pointer_element.set_buffer(pointer_image.clone());

        // draw the cursor as relevant
        {
            // reset the cursor if the surface is no longer alive
            let mut reset = false;
            if let CursorImageStatus::Surface(ref surface) = *cursor_status {
                reset = !surface.alive();
            }
            if reset {
                *cursor_status = CursorImageStatus::default_named();
            }

            pointer_element.set_status(cursor_status.clone());
        }

        custom_elements.extend(
            pointer_element.render_elements(
                renderer,
                (cursor_pos - cursor_hotspot.to_f64())
                    .to_physical(scale)
                    .to_i32_round(),
                scale,
                1.0,
            ),
        );

        // draw the dnd icon if applicable
        {
            if let Some(icon) = dnd_icon.as_ref() {
                let dnd_icon_pos = (cursor_pos + icon.offset.to_f64())
                    .to_physical(scale)
                    .to_i32_round();
                if icon.surface.alive() {
                    custom_elements.extend(AsRenderElements::<UdevRenderer<'a>>::render_elements(
                        &SurfaceTree::from_surface(&icon.surface),
                        renderer,
                        dnd_icon_pos,
                        scale,
                        1.0,
                    ));
                }
            }
        }
    }

    #[cfg(feature = "debug")]
    if let Some(element) = surface.fps_element.as_mut() {
        element.update_fps(surface.fps.avg().round() as u32);
        surface.fps.tick();
        custom_elements.push(CustomRenderElements::Fps(element.clone()));
    }

    let (elements, clear_color) = output_elements(
        output,
        space,
        custom_elements,
        renderer,
        show_window_preview,
        tab_bar_data,
    );

    let frame_mode = if surface.disable_direct_scanout {
        FrameFlags::empty()
    } else {
        FrameFlags::DEFAULT
    };
    let (rendered, states) = surface
        .drm_output
        .render_frame(renderer, &elements, clear_color, frame_mode)
        .map(|render_frame_result| {
            #[cfg(feature = "renderer_sync")]
            if let PrimaryPlaneElement::Swapchain(element) = render_frame_result.primary_element {
                element.sync.wait();
            }
            (!render_frame_result.is_empty, render_frame_result.states)
        })
        .map_err(|err| match err {
            smithay::backend::drm::compositor::RenderFrameError::PrepareFrame(err) => {
                SwapBuffersError::from(err)
            }
            smithay::backend::drm::compositor::RenderFrameError::RenderFrame(
                OutputDamageTrackerError::Rendering(err),
            ) => SwapBuffersError::from(err),
            _ => unreachable!(),
        })?;

    update_primary_scanout_output(space, output, dnd_icon, cursor_status, &states);

    if rendered {
        let output_presentation_feedback = take_presentation_feedback(output, space, &states);
        tracing::debug!("Queuing frame for output");
        surface
            .drm_output
            .queue_frame(Some(output_presentation_feedback))
            .map_err(Into::<SwapBuffersError>::into)?;
    } else {
        tracing::debug!("Not queuing frame - no damage");
    }

    Ok((rendered, states))
}
