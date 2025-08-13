use std::{
    sync::{atomic::Ordering, Mutex},
    time::Duration,
};

use crate::{
    drawing::*,
    render::*,
    state::{take_presentation_feedback, Backend, StilchState},
};
#[cfg(feature = "egl")]
use smithay::backend::renderer::ImportEgl;
#[cfg(feature = "debug")]
use smithay::backend::{allocator::Fourcc, renderer::ImportMem};

use smithay::{
    backend::{
        allocator::{
            dmabuf::{Dmabuf, DmabufAllocator},
            gbm::{GbmAllocator, GbmBufferFlags},
            vulkan::{ImageUsageFlags, VulkanAllocator},
        },
        egl::{EGLContext, EGLDisplay},
        renderer::{
            damage::OutputDamageTracker, element::AsRenderElements, gles::GlesRenderer, Bind,
            ImportDma, ImportMemWl,
        },
        vulkan::{version::Version, Instance, PhysicalDevice},
        x11::{WindowBuilder, X11Backend, X11Event, X11Surface},
    },
    delegate_dmabuf,
    input::{
        keyboard::LedState,
        pointer::{CursorImageAttributes, CursorImageStatus},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        ash::ext,
        calloop::EventLoop,
        gbm,
        wayland_protocols::wp::presentation_time::server::wp_presentation_feedback,
        wayland_server::{protocol::wl_surface, Display},
    },
    utils::{DeviceFd, IsAlive, Rectangle, Scale},
    wayland::{
        compositor,
        dmabuf::{
            DmabufFeedback, DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState,
            ImportNotifier,
        },
        presentation::Refresh,
    },
};
use tracing::{error, info, trace, warn};

pub const OUTPUT_NAME: &str = "x11";

#[derive(Debug)]
pub struct X11Data {
    render: bool,
    mode: Mode,
    // FIXME: If GlesRenderer is dropped before X11Surface, then the MakeCurrent call inside Gles2Renderer will
    // fail because the X11Surface is keeping gbm alive.
    renderer: GlesRenderer,
    damage_tracker: OutputDamageTracker,
    surface: X11Surface,
    dmabuf_state: DmabufState,
    _dmabuf_global: DmabufGlobal,
    _dmabuf_default_feedback: DmabufFeedback,
    #[cfg(feature = "debug")]
    fps: fps_ticker::Fps,
}

impl DmabufHandler for StilchState<X11Data> {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.backend_data.dmabuf_state
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        if self
            .backend_data
            .renderer
            .import_dmabuf(&dmabuf, None)
            .is_ok()
        {
            let _ = notifier.successful::<StilchState<X11Data>>();
        } else {
            notifier.failed();
        }
    }
}
delegate_dmabuf!(StilchState<X11Data>);

impl Backend for X11Data {
    fn seat_name(&self) -> String {
        "x11".to_owned()
    }
    fn reset_buffers(&mut self, _output: &Output) {
        self.surface.reset_buffers();
    }
    fn early_import(&mut self, _surface: &wl_surface::WlSurface) {}
    fn update_led_state(&mut self, _led_state: LedState) {}
}

pub fn run_x11() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop = EventLoop::try_new()?;
    let display = Display::new()?;
    let mut display_handle = display.handle();

    let backend = X11Backend::new()?;
    let handle = backend.handle();

    // Obtain the DRM node the X server uses for direct rendering.
    let (node, fd) = handle.drm_node()?;

    // Create the gbm device for buffer allocation.
    let device = gbm::Device::new(DeviceFd::from(fd))?;
    // Initialize EGL using the GBM device.
    let egl = unsafe { EGLDisplay::new(device.clone())? };
    // Create the OpenGL context
    let context = EGLContext::new(&egl)?;

    let window = WindowBuilder::new().title("Stilch").build(&handle)?;

    let skip_vulkan = std::env::var("ANVIL_NO_VULKAN")
        .map(|x| {
            x == "1"
                || x.to_lowercase() == "true"
                || x.to_lowercase() == "yes"
                || x.to_lowercase() == "y"
        })
        .unwrap_or(false);

    let vulkan_allocator = if !skip_vulkan {
        Instance::new(Version::VERSION_1_2, None)
            .ok()
            .and_then(|instance| {
                PhysicalDevice::enumerate(&instance)
                    .ok()
                    .and_then(|devices| {
                        devices
                            .filter(|phd| phd.has_device_extension(ext::physical_device_drm::NAME))
                            .find(|phd| {
                                phd.primary_node().unwrap_or(None) == Some(node)
                                    || phd.render_node().unwrap_or(None) == Some(node)
                            })
                    })
            })
            .and_then(|physical_device| {
                VulkanAllocator::new(
                    &physical_device,
                    ImageUsageFlags::COLOR_ATTACHMENT | ImageUsageFlags::SAMPLED,
                )
                .ok()
            })
    } else {
        None
    };

    let surface = match vulkan_allocator {
        // Create the surface for the window.
        Some(vulkan_allocator) => handle.create_surface(
            &window,
            DmabufAllocator(vulkan_allocator),
            context
                .dmabuf_render_formats()
                .iter()
                .map(|format| format.modifier),
        )?,
        None => handle.create_surface(
            &window,
            DmabufAllocator(GbmAllocator::new(device, GbmBufferFlags::RENDERING)),
            context
                .dmabuf_render_formats()
                .iter()
                .map(|format| format.modifier),
        )?,
    };

    #[cfg_attr(not(feature = "egl"), allow(unused_mut))]
    let mut renderer = unsafe { GlesRenderer::new(context)? };

    #[cfg(feature = "egl")]
    if renderer.bind_wl_display(&display.handle()).is_ok() {
        info!("EGL hardware-acceleration enabled");
    }

    let dmabuf_formats = renderer.dmabuf_formats();
    let dmabuf_default_feedback = DmabufFeedbackBuilder::new(node.dev_id(), dmabuf_formats)
        .build()
        .unwrap_or_else(|e| {
            error!("Failed to build dmabuf feedback: {:?}", e);
            std::process::exit(1);
        });
    let mut dmabuf_state = DmabufState::new();
    let dmabuf_global = dmabuf_state.create_global_with_default_feedback::<StilchState<X11Data>>(
        &display.handle(),
        &dmabuf_default_feedback,
    );

    let size = {
        let s = window.size();

        (s.w as i32, s.h as i32).into()
    };

    let mode = Mode {
        size,
        refresh: 60_000,
    };

    #[cfg(feature = "debug")]
    #[allow(deprecated)]
    let fps_image = image::io::Reader::with_format(
        std::io::Cursor::new(FPS_NUMBERS_PNG),
        image::ImageFormat::Png,
    )
    .decode()
    .unwrap_or_else(|e| {
        error!("Failed to decode FPS image: {e}");
        std::process::exit(1);
    });
    #[cfg(feature = "debug")]
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
    #[cfg(feature = "debug")]
    let mut fps_element = FpsElement::new(fps_texture);
    let output = Output::new(
        OUTPUT_NAME.to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "X11".into(),
        },
    );
    let _global = output.create_global::<StilchState<X11Data>>(&display.handle());
    output.change_current_state(Some(mode), None, None, Some((0, 0).into()));
    output.set_preferred(mode);

    let damage_tracker = OutputDamageTracker::from_output(&output);

    let data = X11Data {
        render: true,
        mode,
        surface,
        renderer,
        damage_tracker,
        dmabuf_state,
        _dmabuf_global: dmabuf_global,
        _dmabuf_default_feedback: dmabuf_default_feedback,
        #[cfg(feature = "debug")]
        fps: fps_ticker::Fps::default(),
    };

    let mut state = StilchState::init(display, event_loop.handle(), data, true);
    state
        .protocols
        .shm_state
        .update_formats(state.backend_data.renderer.shm_formats());
    state.space_mut().map_output(&output, (0, 0));

    // Create a virtual output for this physical output
    let output_geometry = Rectangle::from_size(size.to_logical(1));
    let virtual_output_id = state
        .virtual_output_manager
        .create_from_physical(output.clone(), output_geometry);
    state.initialize_virtual_output(virtual_output_id);

    // Initialize tiling area
    state.update_tiling_area_from_output();

    let output_clone = output.clone();
    event_loop
        .handle()
        .insert_source(backend, move |event, _, data| match event {
            X11Event::CloseRequested { .. } => {
                data.running.store(false, Ordering::SeqCst);
            }
            X11Event::Resized { new_size, .. } => {
                let output = &output_clone;
                let size = { (new_size.w as i32, new_size.h as i32).into() };

                data.backend_data.mode = Mode {
                    size,
                    refresh: 60_000,
                };
                if let Some(mode) = output.current_mode() {
                    output.delete_mode(mode);
                }
                output.change_current_state(Some(data.backend_data.mode), None, None, None);
                output.set_preferred(data.backend_data.mode);
                let pointer_location = data.pointer().current_location();
                crate::shell::fixup_positions(data.space_mut(), pointer_location);

                // Update tiling area for new output size
                data.update_tiling_area_from_output();

                data.backend_data.render = true;
            }
            X11Event::PresentCompleted { .. } | X11Event::Refresh { .. } => {
                data.backend_data.render = true;
            }
            X11Event::Input { event, .. } => data.process_input_event_windowed(event, OUTPUT_NAME),
            X11Event::Focus { focused: false, .. } => {
                data.release_all_keys();
            }
            _ => {}
        })
        .map_err(|e| {
            error!("Failed to insert X11 Backend into event loop: {e}");
            Box::new(e) as Box<dyn std::error::Error>
        })?;

    #[cfg(feature = "xwayland")]
    state.start_xwayland();

    // Initialize IPC server
    if let Err(e) = state.init_ipc_server() {
        warn!("Failed to initialize IPC server: {e}");
    }

    info!("Initialization completed, starting the main loop.");

    let mut pointer_element = PointerElement::default();

    while state.running.load(Ordering::SeqCst) {
        if state.backend_data.render {
            profiling::scope!("render_frame");

            let now = state.clock.now();
            let frame_target = now
                + output
                    .current_mode()
                    .map(|mode| Duration::from_secs_f64(1_000f64 / mode.refresh as f64))
                    .unwrap_or_default();
            state.pre_repaint(&output, frame_target);

            // Extract what we need before the mutable borrows
            #[cfg(feature = "debug")]
            let fps = state.backend_data.fps.avg().round() as u32;
            #[cfg(feature = "debug")]
            fps_element.update_fps(fps);

            let show_window_preview = state.show_window_preview;

            // Collect tab bar data before the render closure
            let tab_bar_data = crate::render::collect_tab_bar_data(&state, &output);

            // draw the cursor as relevant
            // reset the cursor if the surface is no longer alive
            let mut reset = false;
            if let CursorImageStatus::Surface(ref surface) = *state.cursor_status() {
                reset = !surface.alive();
            }
            if reset {
                state.input_manager.cursor_status = CursorImageStatus::default_named();
            }
            let cursor_visible = !matches!(state.cursor_status(), CursorImageStatus::Surface(_));

            let scale = Scale::from(output.current_scale().fractional_scale());
            let cursor_hotspot =
                if let CursorImageStatus::Surface(ref surface) = state.cursor_status() {
                    compositor::with_states(surface, |states| {
                        states
                            .data_map
                            .get::<Mutex<CursorImageAttributes>>()
                            .and_then(|m| m.lock().ok())
                            .map(|attrs| attrs.hotspot)
                            .unwrap_or((0, 0).into())
                    })
                } else {
                    (0, 0).into()
                };
            let cursor_pos = state.pointer().current_location();
            let cursor_status_clone = state.cursor_status().clone();
            let dnd_icon_data = state
                .dnd_icon()
                .map(|icon| (icon.surface.clone(), icon.offset));

            let backend_data = &mut state.backend_data;
            let (mut buffer, age) = match backend_data.surface.buffer() {
                Ok(b) => b,
                Err(e) => {
                    error!("gbm device was destroyed: {e}");
                    profiling::finish_frame!();
                    continue;
                }
            };
            let mut fb = match backend_data.renderer.bind(&mut buffer) {
                Ok(fb) => fb,
                Err(err) => {
                    error!("Error while binding buffer: {err}");
                    profiling::finish_frame!();
                    continue;
                }
            };

            #[cfg(feature = "debug")]
            if let Some(renderdoc) = state.renderdoc.as_mut() {
                renderdoc.start_frame_capture(
                    backend_data.renderer.egl_context().get_context_handle(),
                    std::ptr::null(),
                );
            }

            let mut elements: Vec<CustomRenderElements<GlesRenderer>> = Vec::new();

            pointer_element.set_status(cursor_status_clone);
            elements.extend(
                pointer_element.render_elements(
                    &mut backend_data.renderer,
                    (cursor_pos - cursor_hotspot.to_f64())
                        .to_physical(scale)
                        .to_i32_round(),
                    scale,
                    1.0,
                ),
            );

            // draw the dnd icon if any
            if let Some((surface, offset)) = dnd_icon_data {
                let dnd_icon_pos = (cursor_pos + offset.to_f64())
                    .to_physical(scale)
                    .to_i32_round();
                if surface.alive() {
                    elements.extend(AsRenderElements::<GlesRenderer>::render_elements(
                        &smithay::desktop::space::SurfaceTree::from_surface(&surface),
                        &mut backend_data.renderer,
                        dnd_icon_pos,
                        scale,
                        1.0,
                    ));
                }
            }

            #[cfg(feature = "debug")]
            elements.push(CustomRenderElements::Fps(fps_element.clone()));

            let render_res = render_output(
                &output,
                &state.window_manager.space,
                elements,
                &mut backend_data.renderer,
                &mut fb,
                &mut backend_data.damage_tracker,
                age.into(),
                show_window_preview,
                &tab_bar_data,
                &mut state.tab_text_cache,
            );

            match render_res {
                Ok(render_output_result) => {
                    trace!("Finished rendering");
                    let submitted = if let Err(err) = backend_data.surface.submit() {
                        backend_data.surface.reset_buffers();
                        warn!("Failed to submit buffer: {}. Retrying", err);
                        false
                    } else {
                        true
                    };

                    let states = render_output_result.states;
                    #[cfg(feature = "debug")]
                    let rendered = render_output_result.damage.is_some();
                    if render_output_result.damage.is_some() {
                        let mut output_presentation_feedback =
                            take_presentation_feedback(&output, state.space(), &states);
                        output_presentation_feedback.presented(
                            frame_target,
                            output
                                .current_mode()
                                .map(|mode| {
                                    Refresh::fixed(Duration::from_secs_f64(
                                        1_000f64 / mode.refresh as f64,
                                    ))
                                })
                                .unwrap_or(Refresh::Unknown),
                            0,
                            wp_presentation_feedback::Kind::Vsync,
                        )
                    }

                    #[cfg(feature = "debug")]
                    if rendered {
                        if let Some(renderdoc) = state.renderdoc.as_mut() {
                            renderdoc.end_frame_capture(
                                state
                                    .backend_data
                                    .renderer
                                    .egl_context()
                                    .get_context_handle(),
                                std::ptr::null(),
                            );
                        }
                    } else if let Some(renderdoc) = state.renderdoc.as_mut() {
                        renderdoc.discard_frame_capture(
                            state
                                .backend_data
                                .renderer
                                .egl_context()
                                .get_context_handle(),
                            std::ptr::null(),
                        );
                    }

                    state.backend_data.render = !submitted;

                    // Send frame events so that client start drawing their next frame
                    state.post_repaint(&output, frame_target, None, &states);

                    // Execute startup commands after first successful render
                    if !state.startup_done.get() {
                        state.startup_done.set(true);
                        state.execute_startup_commands();
                    }
                }
                Err(err) => {
                    #[cfg(feature = "debug")]
                    if let Some(renderdoc) = state.renderdoc.as_mut() {
                        renderdoc.discard_frame_capture(
                            backend_data.renderer.egl_context().get_context_handle(),
                            std::ptr::null(),
                        );
                    }

                    backend_data.surface.reset_buffers();
                    error!("Rendering error: {err}");
                    // TODO: convert RenderError into SwapBuffersError and skip temporary (will retry) and panic on ContextLost or recreate
                }
            }

            #[cfg(feature = "debug")]
            state.backend_data.fps.tick();
            window.set_cursor_visible(cursor_visible);
            profiling::finish_frame!();
        }

        let result = event_loop.dispatch(Some(Duration::from_millis(16)), &mut state);
        if result.is_err() {
            state.running.store(false, Ordering::SeqCst);
        } else {
            state.space_mut().refresh();
            state.popups_mut().cleanup();
            display_handle.flush_clients().unwrap();
        }
    }
    Ok(())
}
