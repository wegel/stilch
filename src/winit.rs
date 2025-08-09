use std::{
    sync::{atomic::Ordering, Mutex},
    time::Duration,
};

#[cfg(feature = "egl")]
use smithay::backend::renderer::ImportEgl;
#[cfg(feature = "debug")]
use smithay::{
    backend::{allocator::Fourcc, renderer::ImportMem},
    reexports::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle},
};

use smithay::{
    backend::{
        allocator::dmabuf::Dmabuf,
        egl::EGLDevice,
        renderer::{
            damage::{Error as OutputDamageTrackerError, OutputDamageTracker},
            element::AsRenderElements,
            gles::GlesRenderer,
            ImportDma, ImportMemWl,
        },
        winit::{self, WinitEvent, WinitGraphicsBackend},
        SwapBuffersError,
    },
    delegate_dmabuf,
    input::{
        keyboard::LedState,
        pointer::{CursorImageAttributes, CursorImageStatus},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::EventLoop,
        wayland_protocols::wp::presentation_time::server::wp_presentation_feedback,
        wayland_server::{protocol::wl_surface, Display},
        winit::platform::pump_events::PumpStatus,
    },
    utils::{IsAlive, Point, Rectangle, Scale, Size, Transform},
    wayland::{
        compositor,
        dmabuf::{
            DmabufFeedback, DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState,
            ImportNotifier,
        },
        presentation::Refresh,
    },
};
use tracing::{error, info, warn};

use crate::state::{take_presentation_feedback, Backend, StilchState};
use crate::{drawing::*, render::*};

pub const OUTPUT_NAME: &str = "winit";

pub struct WinitData {
    backend: WinitGraphicsBackend<GlesRenderer>,
    damage_tracker: OutputDamageTracker,
    dmabuf_state: (DmabufState, DmabufGlobal, Option<DmabufFeedback>),
    full_redraw: u8,
    #[cfg(feature = "debug")]
    pub fps: fps_ticker::Fps,
}

impl DmabufHandler for StilchState<WinitData> {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.backend_data.dmabuf_state.0
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        if self
            .backend_data
            .backend
            .renderer()
            .import_dmabuf(&dmabuf, None)
            .is_ok()
        {
            let _ = notifier.successful::<StilchState<WinitData>>();
        } else {
            notifier.failed();
        }
    }
}
delegate_dmabuf!(StilchState<WinitData>);

impl Backend for WinitData {
    fn seat_name(&self) -> String {
        String::from("winit")
    }
    fn reset_buffers(&mut self, _output: &Output) {
        self.full_redraw = 4;
    }
    fn early_import(&mut self, _surface: &wl_surface::WlSurface) {}
    fn update_led_state(&mut self, _led_state: LedState) {}
}

pub fn run_winit() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop = EventLoop::try_new()?;
    let display = Display::new()?;
    let mut display_handle = display.handle();

    #[cfg_attr(not(feature = "egl"), allow(unused_mut))]
    let (mut backend, mut winit) = match winit::init::<GlesRenderer>() {
        Ok(ret) => ret,
        Err(err) => {
            error!("Failed to initialize Winit backend: {err}");
            return Err(Box::new(err));
        }
    };
    let size = backend.window_size();

    let mode = Mode {
        size,
        refresh: 60_000,
    };
    let output = Output::new(
        OUTPUT_NAME.to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "Winit".into(),
        },
    );
    let _global = output.create_global::<StilchState<WinitData>>(&display.handle());
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

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
    let fps_texture = backend
        .renderer()
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

    let render_node = EGLDevice::device_for_display(backend.renderer().egl_context().display())
        .and_then(|device| device.try_get_render_node());

    let dmabuf_default_feedback = match render_node {
        Ok(Some(node)) => {
            let dmabuf_formats = backend.renderer().dmabuf_formats();
            match DmabufFeedbackBuilder::new(node.dev_id(), dmabuf_formats).build() {
                Ok(feedback) => Some(feedback),
                Err(e) => {
                    warn!("Failed to build dmabuf feedback: {e}");
                    None
                }
            }
        }
        Ok(None) => {
            warn!("failed to query render node, dmabuf will use v3");
            None
        }
        Err(err) => {
            warn!(?err, "failed to egl device for display, dmabuf will use v3");
            None
        }
    };

    // if we failed to build dmabuf feedback we fall back to dmabuf v3
    // Note: egl on Mesa requires either v4 or wl_drm (initialized with bind_wl_display)
    let dmabuf_state = if let Some(default_feedback) = dmabuf_default_feedback {
        let mut dmabuf_state = DmabufState::new();
        let dmabuf_global = dmabuf_state
            .create_global_with_default_feedback::<StilchState<WinitData>>(
                &display.handle(),
                &default_feedback,
            );
        (dmabuf_state, dmabuf_global, Some(default_feedback))
    } else {
        let dmabuf_formats = backend.renderer().dmabuf_formats();
        let mut dmabuf_state = DmabufState::new();
        let dmabuf_global =
            dmabuf_state.create_global::<StilchState<WinitData>>(&display.handle(), dmabuf_formats);
        (dmabuf_state, dmabuf_global, None)
    };

    #[cfg(feature = "egl")]
    if backend
        .renderer()
        .bind_wl_display(&display.handle())
        .is_ok()
    {
        info!("EGL hardware-acceleration enabled");
    };

    let data = {
        let damage_tracker = OutputDamageTracker::from_output(&output);

        WinitData {
            backend,
            damage_tracker,
            dmabuf_state,
            full_redraw: 0,
            #[cfg(feature = "debug")]
            fps: fps_ticker::Fps::default(),
        }
    };
    let mut state = StilchState::init(display, event_loop.handle(), data, true);
    state
        .protocols
        .shm_state
        .update_formats(state.backend_data.backend.renderer().shm_formats());
    state.space_mut().map_output(&output, (0, 0));

    // Check configuration for output settings
    let logical_size = size.to_logical(1);
    let output_name = output.name();
    let output_geometry = Rectangle::from_size(logical_size);

    // First check if this output is part of a virtual output config
    let mut handled_by_virtual_config = false;

    // Clone the config to avoid borrow issues
    let virtual_configs = state.config.virtual_outputs.clone();
    for virtual_config in &virtual_configs {
        if virtual_config.outputs.contains(&output_name)
            || virtual_config.outputs.contains(&"winit".to_string())
        {
            info!(
                "Output {} is part of virtual output config '{}'",
                output_name, virtual_config.name
            );

            // For single-output configs or winit, create immediately
            let physical_outputs = vec![output.clone()];

            // Convert region config to Rectangle if specified
            // Region is specified in physical pixels, convert to logical
            let scale_factor = output.current_scale().fractional_scale();
            let region = virtual_config
                .region
                .as_ref()
                .map(|r| {
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
                })
                .unwrap_or(output_geometry);

            // Check if this virtual output already exists
            let existing_vo = state
                .virtual_output_manager
                .all_virtual_outputs()
                .find(|vo| vo.name() == virtual_config.name)
                .map(|vo| vo.id());

            if existing_vo.is_none() {
                let virtual_output_id = state.virtual_output_manager.create_virtual_output(
                    virtual_config.name.clone(),
                    physical_outputs,
                    region,
                );

                info!(
                    "Created virtual output '{}' with id {:?}",
                    virtual_config.name, virtual_output_id
                );
                state.initialize_virtual_output(virtual_output_id);
                handled_by_virtual_config = true;
            }
        }
    }

    if !handled_by_virtual_config {
        // Check if this output has split configuration
        let should_split = state
            .config
            .outputs
            .iter()
            .find(|o| o.name == output_name || o.name == "winit" || o.name == "*")
            .and_then(|o| o.split.clone());

        if let Some((split_type, count)) = should_split {
            info!(
                "Splitting {} display based on config: {:?} into {} parts",
                output_name, split_type, count
            );
            let virtual_outputs = state.virtual_output_manager.split_physical(
                output.clone(),
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

                if let Some(vo) = state.virtual_output_manager.get(*vo_id) {
                    let area = vo.logical_region();
                    if let Err(e) =
                        state
                            .workspace_manager
                            .show_workspace_on_output(workspace_id, *vo_id, area)
                    {
                        error!("Failed to initialize workspace on virtual output: {:?}", e);
                    } else {
                        state.virtual_output_manager.set_active_workspace(*vo_id, i);
                    }
                }
            }
        } else {
            // Create a single virtual output for this physical output
            let virtual_output_id = state
                .virtual_output_manager
                .create_from_physical(output.clone(), output_geometry);
            state.initialize_virtual_output(virtual_output_id);
        }
    }

    // Initialize tiling area
    state.update_tiling_area_from_output();

    // Initialize IPC server
    if let Err(e) = state.init_ipc_server() {
        warn!("Failed to initialize IPC server: {e}");
    }

    #[cfg(feature = "xwayland")]
    state.start_xwayland();

    info!("Initialization completed, starting the main loop.");

    let mut pointer_element = PointerElement::default();

    while state.running.load(Ordering::SeqCst) {
        let status = winit.dispatch_new_events(|event| match event {
            WinitEvent::Resized { size, .. } => {
                // We only have one output
                let output = match state.space().outputs().next() {
                    Some(o) => o.clone(),
                    None => {
                        warn!("No output found when handling resize");
                        return;
                    }
                };
                state.space_mut().map_output(&output, (0, 0));
                let mode = Mode {
                    size,
                    refresh: 60_000,
                };
                output.change_current_state(Some(mode), None, None, None);
                output.set_preferred(mode);
                let pointer_location = state.pointer().current_location();
                crate::shell::fixup_positions(state.space_mut(), pointer_location);

                // Update tiling area for new output size
                state.update_tiling_area_from_output();
            }
            WinitEvent::Input(event) => state.process_input_event_windowed(event, OUTPUT_NAME),
            _ => (),
        });

        if let PumpStatus::Exit(_) = status {
            state.running.store(false, Ordering::SeqCst);
            break;
        }

        // drawing logic
        {
            let now = state.clock.now();
            let frame_target = now
                + output
                    .current_mode()
                    .map(|mode| Duration::from_secs_f64(1_000f64 / mode.refresh as f64))
                    .unwrap_or_default();
            state.pre_repaint(&output, frame_target);

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

            pointer_element.set_status(state.cursor_status().clone());

            #[cfg(feature = "debug")]
            let fps = state.backend_data.fps.avg().round() as u32;
            #[cfg(feature = "debug")]
            fps_element.update_fps(fps);

            // Extract values we need before mutable borrows
            let show_window_preview = state.show_window_preview;
            let dnd_icon = state
                .dnd_icon()
                .map(|icon| (icon.surface.clone(), icon.offset));
            let scale = Scale::from(output.current_scale().fractional_scale());

            // Collect tab bar data
            let tab_bar_data = crate::render::collect_tab_bar_data(&state, &output);
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

            // Now take the mutable borrow
            let backend = &mut state.backend_data.backend;

            // Handle full redraw
            let full_redraw = &mut state.backend_data.full_redraw;
            *full_redraw = full_redraw.saturating_sub(1);
            let should_full_redraw = *full_redraw > 0;

            let age = if should_full_redraw {
                0
            } else {
                backend.buffer_age().unwrap_or(0)
            };

            #[cfg(feature = "debug")]
            let mut renderdoc = state.renderdoc.as_mut();

            // Now get space and damage tracker
            let space = &mut state.window_manager.space;
            let damage_tracker = &mut state.backend_data.damage_tracker;
            #[cfg(feature = "debug")]
            let window_handle = backend
                .window()
                .window_handle()
                .map(|handle| {
                    if let RawWindowHandle::Wayland(handle) = handle.as_raw() {
                        handle.surface.as_ptr()
                    } else {
                        std::ptr::null_mut()
                    }
                })
                .unwrap_or_else(|_| std::ptr::null_mut());
            let render_res = backend.bind().and_then(|(renderer, mut fb)| {
                #[cfg(feature = "debug")]
                if let Some(renderdoc) = renderdoc.as_mut() {
                    renderdoc.start_frame_capture(
                        renderer.egl_context().get_context_handle(),
                        window_handle,
                    );
                }

                let mut elements = Vec::<CustomRenderElements<GlesRenderer>>::new();

                elements.extend(
                    pointer_element.render_elements(
                        renderer,
                        (cursor_pos - cursor_hotspot.to_f64())
                            .to_physical(scale)
                            .to_i32_round(),
                        scale,
                        1.0,
                    ),
                );

                // draw the dnd icon if any
                if let Some((surface, offset)) = dnd_icon {
                    let dnd_icon_pos = (cursor_pos + offset.to_f64())
                        .to_physical(scale)
                        .to_i32_round();
                    if surface.alive() {
                        elements.extend(AsRenderElements::<GlesRenderer>::render_elements(
                            &smithay::desktop::space::SurfaceTree::from_surface(&surface),
                            renderer,
                            dnd_icon_pos,
                            scale,
                            1.0,
                        ));
                    }
                }

                #[cfg(feature = "debug")]
                elements.push(CustomRenderElements::Fps(fps_element.clone()));

                let res = render_output(
                    &output,
                    space,
                    elements,
                    renderer,
                    &mut fb,
                    damage_tracker,
                    age,
                    show_window_preview,
                    &tab_bar_data,
                )
                .map_err(|err| match err {
                    OutputDamageTrackerError::Rendering(err) => err.into(),
                    _ => unreachable!(),
                });

                res
            });

            match render_res {
                Ok(render_output_result) => {
                    let has_rendered = render_output_result.damage.is_some();
                    if let Some(damage) = render_output_result.damage {
                        if let Err(err) = backend.submit(Some(damage)) {
                            warn!("Failed to submit buffer: {err}");
                        }
                    }

                    #[cfg(feature = "debug")]
                    if let Some(renderdoc) = renderdoc.as_mut() {
                        renderdoc.end_frame_capture(
                            backend.renderer().egl_context().get_context_handle(),
                            backend
                                .window()
                                .window_handle()
                                .map(|handle| {
                                    if let RawWindowHandle::Wayland(handle) = handle.as_raw() {
                                        handle.surface.as_ptr()
                                    } else {
                                        std::ptr::null_mut()
                                    }
                                })
                                .unwrap_or_else(|_| std::ptr::null_mut()),
                        );
                    }

                    backend.window().set_cursor_visible(cursor_visible);

                    let states = render_output_result.states;
                    if has_rendered {
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

                    // Send frame events so that client start drawing their next frame
                    state.post_repaint(&output, frame_target, None, &states);

                    // Execute startup commands after first successful render
                    if !state.startup_done.get() {
                        state.startup_done.set(true);
                        state.execute_startup_commands();
                    }
                }
                Err(SwapBuffersError::ContextLost(err)) => {
                    #[cfg(feature = "debug")]
                    if let Some(renderdoc) = renderdoc.as_mut() {
                        renderdoc.discard_frame_capture(
                            backend.renderer().egl_context().get_context_handle(),
                            backend
                                .window()
                                .window_handle()
                                .map(|handle| {
                                    if let RawWindowHandle::Wayland(handle) = handle.as_raw() {
                                        handle.surface.as_ptr()
                                    } else {
                                        std::ptr::null_mut()
                                    }
                                })
                                .unwrap_or_else(|_| std::ptr::null_mut()),
                        );
                    }

                    error!("Critical Rendering Error: {err}");
                    state.running.store(false, Ordering::SeqCst);
                }
                Err(err) => warn!("Rendering error: {err}"),
            }
        }

        let result = event_loop.dispatch(Some(Duration::from_millis(1)), &mut state);
        if result.is_err() {
            state.running.store(false, Ordering::SeqCst);
        } else {
            state.space_mut().refresh();
            state.popups_mut().cleanup();
            display_handle.flush_clients().unwrap();
        }

        #[cfg(feature = "debug")]
        state.backend_data.fps.tick();
    }
    Ok(())
}
