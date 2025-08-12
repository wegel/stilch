use std::{
    collections::HashMap,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use tracing::{debug, error, info, warn};

use smithay::{
    backend::renderer::element::{
        default_primary_scanout_output_compare, utils::select_dmabuf_feedback, RenderElementStates,
    },
    delegate_compositor, delegate_layer_shell, delegate_xdg_shell,
    desktop::{
        space::SpaceElement,
        utils::{
            surface_presentation_feedback_flags_from_states, surface_primary_scanout_output,
            update_surface_primary_scanout_output, with_surfaces_surface_tree,
            OutputPresentationFeedback,
        },
        Space,
    },
    input::{
        keyboard::{Keysym, LedState, XkbConfig},
        pointer::{CursorImageStatus, PointerHandle},
        Seat, SeatState,
    },
    output::Output,
    reexports::{
        calloop::{generic::Generic, Interest, LoopHandle, Mode, PostAction},
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Client, Display, DisplayHandle, Resource,
        },
    },
    utils::{Clock, Logical, Monotonic, Point, Rectangle, Time, SERIAL_COUNTER as SCOUNTER},
    wayland::{
        commit_timing::CommitTimerBarrierStateUserData,
        compositor::{CompositorClientState, CompositorHandler},
        dmabuf::DmabufFeedback,
        fifo::FifoBarrierCachedState,
        fractional_scale::with_fractional_scale,
        input_method::InputMethodManagerState,
        pointer_constraints::PointerConstraintsState,
        pointer_gestures::PointerGesturesState,
        relative_pointer::RelativePointerManagerState,
        security_context::{SecurityContext, SecurityContextState},
        socket::ListeningSocketSource,
        tablet_manager::TabletManagerState,
        text_input::TextInputManagerState,
        virtual_keyboard::VirtualKeyboardManagerState,
    },
};

#[cfg(feature = "xwayland")]
use crate::cursor::Cursor;
use crate::{
    command::CommandExecutor,
    config::Config,
    event::{ipc_handler::IpcEventHandler, EventBus},
    focus::KeyboardFocusTarget, // Import handlers module
    ipc::IpcServer,
    shell::WindowElement,
    virtual_output::VirtualOutputManager,
};

use super::ProtocolState;
#[cfg(feature = "xwayland")]
use smithay::{
    delegate_xwayland_keyboard_grab, delegate_xwayland_shell,
    utils::Size,
    wayland::xwayland_keyboard_grab::{XWaylandKeyboardGrabHandler, XWaylandKeyboardGrabState},
    xwayland::{X11Wm, XWayland, XWaylandEvent},
};

#[derive(Debug, Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
    pub security_context: Option<SecurityContext>,
}
impl ClientData for ClientState {
    /// Notification that a client was initialized
    fn initialized(&self, _client_id: ClientId) {}
    /// Notification that a client is disconnected
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

#[derive(Debug)]
pub struct StilchState<BackendData: Backend + 'static> {
    pub backend_data: BackendData,
    pub socket_name: Option<String>,
    pub display_handle: DisplayHandle,
    pub running: Arc<AtomicBool>,
    pub handle: LoopHandle<'static, StilchState<BackendData>>,

    // desktop
    pub window_manager: crate::window::WindowManager,
    pub virtual_output_manager: VirtualOutputManager,
    pub virtual_output_exclusive_zones:
        HashMap<crate::virtual_output::VirtualOutputId, Rectangle<i32, Logical>>,
    pub config: Config,
    pub ipc_server: Option<Arc<IpcServer>>,

    // smithay state
    pub protocols: ProtocolState<BackendData>,

    // Workspace management
    pub workspace_manager: crate::workspace::WorkspaceManager,

    // Input management
    pub input_manager: crate::input::InputManager<BackendData>,

    // Event system
    pub event_bus: EventBus,

    // Command executor for undo/redo support
    pub command_executor: CommandExecutor<BackendData>,

    pub seat_name: String,
    pub clock: Clock<Monotonic>,

    #[cfg(feature = "xwayland")]
    pub xwm: Option<X11Wm>,
    #[cfg(feature = "xwayland")]
    pub xdisplay: Option<u32>,

    #[cfg(feature = "debug")]
    pub renderdoc: Option<renderdoc::RenderDoc<renderdoc::V141>>,

    pub show_window_preview: bool,
    pub startup_done: std::cell::Cell<bool>,
}

#[derive(Debug, Clone)]
pub struct DndIcon {
    pub surface: WlSurface,
    pub offset: Point<i32, Logical>,
}

// =================================================================================
// Protocol handler implementations have been moved to src/handlers/ for organization:
// - handlers/data_device.rs: DataDeviceHandler, ClientDndGrabHandler, ServerDndGrabHandler
// - handlers/seat.rs: SeatHandler, TabletSeatHandler, InputMethodHandler, etc.
// - handlers/misc.rs: OutputHandler, SelectionHandler, ShmHandler, XdgActivationHandler, etc.
// =================================================================================

// Smithay delegate macros - these wire up the protocol handlers
delegate_compositor!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_xdg_shell!(@<BackendData: Backend + 'static> StilchState<BackendData>);
delegate_layer_shell!(@<BackendData: Backend + 'static> StilchState<BackendData>);

smithay::delegate_single_pixel_buffer!(@<BackendData: Backend + 'static> StilchState<BackendData>);
smithay::delegate_fifo!(@<BackendData: Backend + 'static> StilchState<BackendData>);
smithay::delegate_commit_timing!(@<BackendData: Backend + 'static> StilchState<BackendData>);
smithay::delegate_cursor_shape!(@<BackendData: Backend + 'static> StilchState<BackendData>);

#[cfg(feature = "xwayland")]
impl<BackendData: Backend + 'static> XWaylandKeyboardGrabHandler for StilchState<BackendData> {
    fn keyboard_focus_for_xsurface(&self, _surface: &WlSurface) -> Option<KeyboardFocusTarget> {
        // Return the appropriate keyboard focus target for the X11 surface
        None
    }
}

#[cfg(feature = "xwayland")]
delegate_xwayland_keyboard_grab!(@<BackendData: Backend + 'static> StilchState<BackendData>);
#[cfg(feature = "xwayland")]
delegate_xwayland_shell!(@<BackendData: Backend + 'static> StilchState<BackendData>);

fn load_config() -> Config {
    // Check if a specific config file was provided via environment variable
    if let Ok(config_file) = std::env::var("STILCH_CONFIG_FILE") {
        let path = Path::new(&config_file);
        if path.exists() {
            info!("Loading config from environment variable: {:?}", path);
            match Config::load_from_file(path) {
                Ok(config) => {
                    info!(
                        "Successfully loaded config with {} keybindings",
                        config.keybindings.len()
                    );
                    return config;
                }
                Err(e) => error!("Failed to load config from {:?}: {}", path, e),
            }
        } else {
            error!(
                "Config file specified in STILCH_CONFIG_FILE does not exist: {:?}",
                path
            );
        }
    }

    // Try to load config from various locations
    let mut config_paths = vec![
        Path::new("./stilch.conf").to_path_buf(),
        Path::new("/etc/stilch/config").to_path_buf(),
    ];

    if let Ok(home) = std::env::var("HOME") {
        config_paths.push(Path::new(&home).join(".config/stilch/config"));
    }

    for path in &config_paths {
        if path.exists() {
            info!("Loading config from: {:?}", path);
            match Config::load_from_file(path) {
                Ok(config) => {
                    info!(
                        "Successfully loaded config with {} keybindings",
                        config.keybindings.len()
                    );
                    for binding in &config.keybindings {
                        debug!(
                            "  Keybinding: {:?} + {:?} -> {:?}",
                            binding.modifiers, binding.key, binding.command
                        );
                    }
                    debug!("Variables: {:?}", config.variables);
                    return config;
                }
                Err(e) => error!("Failed to load config from {:?}: {}", path, e),
            }
        }
    }

    warn!("No config file found, using defaults");
    Config::default()
}

impl<BackendData: Backend + 'static> StilchState<BackendData> {
    pub fn init(
        display: Display<StilchState<BackendData>>,
        handle: LoopHandle<'static, StilchState<BackendData>>,
        backend_data: BackendData,
        listen_on_socket: bool,
    ) -> StilchState<BackendData> {
        let dh = display.handle();

        let clock = Clock::new();

        // init wayland clients
        let socket_name = if listen_on_socket {
            // Check if we should use a specific socket name (for testing)
            let source = if let Ok(socket_name) = std::env::var("STILCH_WAYLAND_SOCKET") {
                info!("Using specified Wayland socket: {socket_name}");
                ListeningSocketSource::with_name(&socket_name).unwrap_or_else(|e| {
                    error!(
                        "Failed to create Wayland socket with specified name '{}': {:?}",
                        socket_name, e
                    );
                    std::process::exit(1);
                })
            } else {
                ListeningSocketSource::new_auto().unwrap_or_else(|e| {
                    error!("Failed to create Wayland socket automatically: {:?}", e);
                    std::process::exit(1);
                })
            };
            let socket_name = source.socket_name().to_string_lossy().into_owned();
            handle
                .insert_source(source, |client_stream, _, data| {
                    info!("New Wayland client connecting!");
                    if let Err(err) = data
                        .display_handle
                        .insert_client(client_stream, Arc::new(ClientState::default()))
                    {
                        warn!("Error adding wayland client: {err}");
                    } else {
                        info!(
                            "Successfully added Wayland client - client should now request globals"
                        );
                    };
                })
                .unwrap_or_else(|e| {
                    error!("Failed to init wayland socket source: {:?}", e);
                    std::process::exit(1);
                });
            info!(name = socket_name, "Listening on wayland socket");
            Some(socket_name)
        } else {
            None
        };
        handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, data| {
                    profiling::scope!("dispatch_clients");
                    // Safety: we don't drop the display
                    unsafe {
                        if let Err(e) = display.get_mut().dispatch_clients(data) {
                            tracing::error!("Failed to dispatch clients: {:?}", e);
                        }
                    }
                    Ok(PostAction::Continue)
                },
            )
            .unwrap_or_else(|e| {
                error!("Failed to init wayland server source: {:?}", e);
                std::process::exit(1);
            });

        // init globals
        let mut seat_state = SeatState::new();

        // Load configuration
        let config = load_config();

        // init input
        let seat_name = backend_data.seat_name();
        let mut seat = seat_state.new_wl_seat(&dh, seat_name.clone());

        let pointer = seat.add_pointer();

        // Get keyboard config from the first keyboard input config, or use defaults
        let keyboard_config = config
            .input_configs
            .iter()
            .find(|c| c.identifier == "type:keyboard" || c.identifier == "*");

        let mut delay = 200;
        let mut rate = 25;

        // Create XkbConfig with leaked strings to ensure 'static lifetime
        let xkb_config = if let Some(cfg) = keyboard_config {
            if let Some(d) = cfg.repeat_delay {
                delay = d as i32;
            }
            if let Some(r) = cfg.repeat_rate {
                rate = r as i32;
            }

            // Leak the strings to create 'static references
            // This is safe since the compositor runs for the entire program lifetime
            let layout: &'static str = if let Some(ref l) = cfg.xkb_layout {
                Box::leak(l.clone().into_boxed_str())
            } else {
                ""
            };

            let variant: &'static str = if let Some(ref v) = cfg.xkb_variant {
                Box::leak(v.clone().into_boxed_str())
            } else {
                ""
            };

            let model: &'static str = if let Some(ref m) = cfg.xkb_model {
                Box::leak(m.clone().into_boxed_str())
            } else {
                ""
            };

            let options = cfg.xkb_options.clone();

            info!(
                "Configuring keyboard with layout: '{}', variant: '{}', model: '{}'",
                layout, variant, model
            );

            XkbConfig {
                rules: "", // Use default rules
                model,
                layout,
                variant,
                options,
            }
        } else {
            XkbConfig::default()
        };

        seat.add_keyboard(xkb_config, delay, rate)
            .unwrap_or_else(|e| {
                error!("Failed to initialize the keyboard: {:?}", e);
                std::process::exit(1);
            });

        // Create protocol state container
        let protocols = ProtocolState::new(&dh, seat_state);

        // Initialize additional protocol states that aren't in ProtocolState
        TextInputManagerState::new::<Self>(&dh);
        InputMethodManagerState::new::<Self, _>(&dh, |_client| true);
        VirtualKeyboardManagerState::new::<Self, _>(&dh, |_client| true);
        // Expose global only if backend supports relative motion events
        if BackendData::HAS_RELATIVE_MOTION {
            RelativePointerManagerState::new::<Self>(&dh);
        }
        PointerConstraintsState::new::<Self>(&dh);
        if BackendData::HAS_GESTURES {
            PointerGesturesState::new::<Self>(&dh);
        }
        TabletManagerState::new::<Self>(&dh);
        SecurityContextState::new::<Self, _>(&dh, |client| {
            client
                .get_data::<ClientState>()
                .map_or(true, |client_state| client_state.security_context.is_none())
        });

        #[cfg(feature = "xwayland")]
        XWaylandKeyboardGrabState::new::<Self>(&dh.clone());

        let input_manager = crate::input::InputManager::new(seat, pointer);

        let inner_gap = config.gaps.inner.unwrap_or(10);

        StilchState {
            backend_data,
            display_handle: dh,
            socket_name,
            running: Arc::new(AtomicBool::new(true)),
            handle,
            window_manager: crate::window::WindowManager::new(),
            virtual_output_manager: VirtualOutputManager::new(),
            virtual_output_exclusive_zones: HashMap::new(),
            config,
            ipc_server: None,
            protocols,
            workspace_manager: crate::workspace::WorkspaceManager::new(inner_gap),
            input_manager,
            event_bus: EventBus::new(),
            command_executor: CommandExecutor::new(),
            seat_name,
            clock,

            #[cfg(feature = "xwayland")]
            xwm: None,
            #[cfg(feature = "xwayland")]
            xdisplay: None,
            #[cfg(feature = "debug")]
            renderdoc: renderdoc::RenderDoc::new().ok(),
            show_window_preview: false,
            startup_done: std::cell::Cell::new(false),
        }
    }

    pub fn execute_startup_commands(&self) {
        for cmd in &self.config.startup_commands {
            info!("Executing startup command: {cmd}");

            // Set WAYLAND_DISPLAY environment variable
            let wayland_display = self.socket_name.as_deref().unwrap_or("wayland-1");

            match std::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .env("WAYLAND_DISPLAY", wayland_display)
                .env(
                    "XDG_RUNTIME_DIR",
                    std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string()),
                )
                .stderr(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
            {
                Ok(mut child) => {
                    info!("Started process with PID: {}", child.id());

                    // Spawn a thread to log any output from the process
                    if let Some(stderr) = child.stderr.take() {
                        std::thread::spawn(move || {
                            use std::io::{BufRead, BufReader};
                            let reader = BufReader::new(stderr);
                            for line in reader.lines() {
                                if let Ok(line) = line {
                                    warn!("Startup command stderr: {line}");
                                }
                            }
                        });
                    }
                }
                Err(e) => {
                    error!("Failed to execute startup command '{}': {}", cmd, e);
                }
            }
        }
    }

    #[cfg(feature = "xwayland")]
    pub fn start_xwayland(&mut self) {
        use std::process::Stdio;

        use smithay::wayland::compositor::CompositorHandler;

        let (xwayland, client) = XWayland::spawn(
            &self.display_handle,
            None,
            std::iter::empty::<(String, String)>(),
            true,
            Stdio::null(),
            Stdio::null(),
            |_| (),
        )
        .unwrap_or_else(|e| {
            error!("Failed to start XWayland: {:?}", e);
            error!("Compositor will continue without XWayland support");
            // Return a dummy value since XWayland is optional
            std::process::exit(1);
        });

        let ret = self
            .handle
            .insert_source(xwayland, move |event, _, data| match event {
                XWaylandEvent::Ready {
                    x11_socket,
                    display_number,
                } => {
                    let xwayland_scale = std::env::var("ANVIL_XWAYLAND_SCALE")
                        .ok()
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(1.);
                    data.client_compositor_state(&client)
                        .set_client_scale(xwayland_scale);
                    let mut wm =
                        match X11Wm::start_wm(data.handle.clone(), x11_socket, client.clone()) {
                            Ok(wm) => wm,
                            Err(e) => {
                                error!("Failed to attach X11 Window Manager: {:?}", e);
                                return;
                            }
                        };

                    let cursor = Cursor::load();
                    let image = cursor.get_image(1, Duration::ZERO);
                    wm.set_cursor(
                        &image.pixels_rgba,
                        Size::from((image.width as u16, image.height as u16)),
                        Point::from((image.xhot as u16, image.yhot as u16)),
                    )
                    .unwrap_or_else(|e| {
                        error!("Failed to set xwayland default cursor: {:?}", e);
                        // Non-critical error, continue without cursor
                    });
                    data.xwm = Some(wm);
                    data.xdisplay = Some(display_number);
                }
                XWaylandEvent::Error => {
                    warn!("XWayland crashed on startup");
                }
            });
        if let Err(e) = ret {
            tracing::error!(
                "Failed to insert the XWaylandSource into the event loop: {}",
                e
            );
        }
    }
}

impl<BackendData: Backend + 'static> StilchState<BackendData> {
    pub fn init_ipc_server(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let ipc_server = Arc::new(IpcServer::new()?);
        let runtime = tokio::runtime::Runtime::new()?;

        runtime.block_on(ipc_server.start())?;

        // Keep the runtime alive
        std::thread::spawn(move || {
            runtime.block_on(async {
                // Keep runtime alive
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                }
            });
        });

        self.ipc_server = Some(ipc_server);

        // Register IPC event handler
        let handler = Box::new(IpcEventHandler::new(self.ipc_server.clone()));
        self.event_bus.register_handler(handler);

        // Send initial workspace state
        self.update_ipc_workspace_state();

        Ok(())
    }

    pub fn update_ipc_workspace_state(&self) {
        if let Some(ipc_server) = &self.ipc_server {
            let mut workspaces = Vec::new();

            // Iterate through all global workspaces
            for idx in 0..10 {
                let workspace_id = crate::workspace::WorkspaceId::new(idx as u8);
                if let Some(workspace) = self.workspace_manager.get_workspace(workspace_id) {
                    // Check which output this workspace is on
                    let location = self.workspace_manager.workspace_location(workspace_id);
                    let is_active = location.is_some()
                        && location
                            .and_then(|loc| self.virtual_output_manager.get(loc))
                            .map(|vo| vo.active_workspace() == Some(idx))
                            .unwrap_or(false);

                    // Check if any window in this workspace has urgency hints
                    let urgent = workspace.windows.iter().any(|window_id| {
                        self.window_registry()
                            .get(*window_id)
                            .and_then(|managed_window| managed_window.element.0.toplevel())
                            .map(|_toplevel| {
                                // Check if the window has the urgent hint set
                                // For now, return false as urgency hints require additional protocol support
                                false
                            })
                            .unwrap_or(false)
                    });

                    workspaces.push(crate::ipc::WorkspaceInfo {
                        id: workspace_id.display_name().parse().unwrap_or(idx + 1), // Use display name
                        active: is_active,
                        windows: workspace.window_count(),
                        urgent,
                    });
                }
            }

            // Send global workspace state (using virtual output 0 for compatibility)
            // Use the first virtual output for workspace updates
            // TODO: Handle multiple virtual outputs properly
            if let Some(first_vo_id) = self
                .virtual_output_manager
                .list_virtual_outputs()
                .first()
                .copied()
            {
                ipc_server.send_workspace_update(first_vo_id, workspaces);
            }
        }
    }

    pub fn pre_repaint(&mut self, output: &Output, frame_target: impl Into<Time<Monotonic>>) {
        let frame_target = frame_target.into();

        #[allow(clippy::mutable_key_type)]
        let mut clients: HashMap<ClientId, Client> = HashMap::new();
        self.space().elements().for_each(|window| {
            window.with_surfaces(|surface, states| {
                if let Some(mut commit_timer_state) = states
                    .data_map
                    .get::<CommitTimerBarrierStateUserData>()
                    .and_then(|commit_timer| commit_timer.lock().ok())
                {
                    commit_timer_state.signal_until(frame_target);
                    if let Some(client) = surface.client() {
                        clients.insert(client.id(), client);
                    }
                }
            });
        });

        let map = smithay::desktop::layer_map_for_output(output);
        for layer_surface in map.layers() {
            layer_surface.with_surfaces(|surface, states| {
                if let Some(mut commit_timer_state) = states
                    .data_map
                    .get::<CommitTimerBarrierStateUserData>()
                    .and_then(|commit_timer| commit_timer.lock().ok())
                {
                    commit_timer_state.signal_until(frame_target);
                    if let Some(client) = surface.client() {
                        clients.insert(client.id(), client);
                    }
                }
            });
        }
        // Drop the lock to the layer map before calling blocker_cleared, which might end up
        // calling the commit handler which in turn again could access the layer map.
        std::mem::drop(map);

        if let CursorImageStatus::Surface(ref surface) = *self.cursor_status() {
            with_surfaces_surface_tree(surface, |surface, states| {
                if let Some(mut commit_timer_state) = states
                    .data_map
                    .get::<CommitTimerBarrierStateUserData>()
                    .and_then(|commit_timer| commit_timer.lock().ok())
                {
                    commit_timer_state.signal_until(frame_target);
                    if let Some(client) = surface.client() {
                        clients.insert(client.id(), client);
                    }
                }
            });
        }

        if let Some(surface) = self.dnd_icon().map(|icon| &icon.surface) {
            with_surfaces_surface_tree(surface, |surface, states| {
                if let Some(mut commit_timer_state) = states
                    .data_map
                    .get::<CommitTimerBarrierStateUserData>()
                    .and_then(|commit_timer| commit_timer.lock().ok())
                {
                    commit_timer_state.signal_until(frame_target);
                    if let Some(client) = surface.client() {
                        clients.insert(client.id(), client);
                    }
                }
            });
        }

        let dh = self.display_handle.clone();
        for client in clients.into_values() {
            self.client_compositor_state(&client)
                .blocker_cleared(self, &dh);
        }
    }

    pub fn post_repaint(
        &mut self,
        output: &Output,
        time: impl Into<Duration>,
        dmabuf_feedback: Option<SurfaceDmabufFeedback>,
        render_element_states: &RenderElementStates,
    ) {
        let time = time.into();
        let throttle = Some(Duration::from_secs(1));

        #[allow(clippy::mutable_key_type)]
        let mut clients: HashMap<ClientId, Client> = HashMap::new();

        self.space().elements().for_each(|window| {
            window.with_surfaces(|surface, states| {
                let primary_scanout_output = surface_primary_scanout_output(surface, states);

                if let Some(output) = primary_scanout_output.as_ref() {
                    with_fractional_scale(states, |fraction_scale| {
                        fraction_scale
                            .set_preferred_scale(output.current_scale().fractional_scale());
                    });
                }

                if primary_scanout_output
                    .as_ref()
                    .map(|o| o == output)
                    .unwrap_or(true)
                {
                    let fifo_barrier = states
                        .cached_state
                        .get::<FifoBarrierCachedState>()
                        .current()
                        .barrier
                        .take();

                    if let Some(fifo_barrier) = fifo_barrier {
                        fifo_barrier.signal();
                        if let Some(client) = surface.client() {
                            clients.insert(client.id(), client);
                        }
                    }
                }
            });

            if self.space().outputs_for_element(window).contains(output) {
                window.send_frame(output, time, throttle, surface_primary_scanout_output);
                if let Some(dmabuf_feedback) = dmabuf_feedback.as_ref() {
                    window.send_dmabuf_feedback(
                        output,
                        surface_primary_scanout_output,
                        |surface, _| {
                            select_dmabuf_feedback(
                                surface,
                                render_element_states,
                                &dmabuf_feedback.render_feedback,
                                &dmabuf_feedback.scanout_feedback,
                            )
                        },
                    );
                }
            }
        });
        let map = smithay::desktop::layer_map_for_output(output);
        for layer_surface in map.layers() {
            layer_surface.with_surfaces(|surface, states| {
                let primary_scanout_output = surface_primary_scanout_output(surface, states);

                if let Some(output) = primary_scanout_output.as_ref() {
                    with_fractional_scale(states, |fraction_scale| {
                        fraction_scale
                            .set_preferred_scale(output.current_scale().fractional_scale());
                    });
                }

                if primary_scanout_output
                    .as_ref()
                    .map(|o| o == output)
                    .unwrap_or(true)
                {
                    let fifo_barrier = states
                        .cached_state
                        .get::<FifoBarrierCachedState>()
                        .current()
                        .barrier
                        .take();

                    if let Some(fifo_barrier) = fifo_barrier {
                        fifo_barrier.signal();
                        if let Some(client) = surface.client() {
                            clients.insert(client.id(), client);
                        }
                    }
                }
            });

            layer_surface.send_frame(output, time, throttle, surface_primary_scanout_output);
            if let Some(dmabuf_feedback) = dmabuf_feedback.as_ref() {
                layer_surface.send_dmabuf_feedback(
                    output,
                    surface_primary_scanout_output,
                    |surface, _| {
                        select_dmabuf_feedback(
                            surface,
                            render_element_states,
                            &dmabuf_feedback.render_feedback,
                            &dmabuf_feedback.scanout_feedback,
                        )
                    },
                );
            }
        }
        // Drop the lock to the layer map before calling blocker_cleared, which might end up
        // calling the commit handler which in turn again could access the layer map.
        std::mem::drop(map);

        if let CursorImageStatus::Surface(ref surface) = *self.cursor_status() {
            with_surfaces_surface_tree(surface, |surface, states| {
                let primary_scanout_output = surface_primary_scanout_output(surface, states);

                if let Some(output) = primary_scanout_output.as_ref() {
                    with_fractional_scale(states, |fraction_scale| {
                        fraction_scale
                            .set_preferred_scale(output.current_scale().fractional_scale());
                    });
                }

                if primary_scanout_output
                    .as_ref()
                    .map(|o| o == output)
                    .unwrap_or(true)
                {
                    let fifo_barrier = states
                        .cached_state
                        .get::<FifoBarrierCachedState>()
                        .current()
                        .barrier
                        .take();

                    if let Some(fifo_barrier) = fifo_barrier {
                        fifo_barrier.signal();
                        if let Some(client) = surface.client() {
                            clients.insert(client.id(), client);
                        }
                    }
                }
            });
        }

        if let Some(surface) = self.dnd_icon().map(|icon| &icon.surface) {
            with_surfaces_surface_tree(surface, |surface, states| {
                let primary_scanout_output = surface_primary_scanout_output(surface, states);

                if let Some(output) = primary_scanout_output.as_ref() {
                    with_fractional_scale(states, |fraction_scale| {
                        fraction_scale
                            .set_preferred_scale(output.current_scale().fractional_scale());
                    });
                }

                if primary_scanout_output
                    .as_ref()
                    .map(|o| o == output)
                    .unwrap_or(true)
                {
                    let fifo_barrier = states
                        .cached_state
                        .get::<FifoBarrierCachedState>()
                        .current()
                        .barrier
                        .take();

                    if let Some(fifo_barrier) = fifo_barrier {
                        fifo_barrier.signal();
                        if let Some(client) = surface.client() {
                            clients.insert(client.id(), client);
                        }
                    }
                }
            });
        }

        let dh = self.display_handle.clone();
        for client in clients.into_values() {
            self.client_compositor_state(&client)
                .blocker_cleared(self, &dh);
        }
    }

    // Window management helper methods

    /// Find a window by its element
    pub fn find_window_by_element(
        &self,
        element: &crate::shell::WindowElement,
    ) -> Option<crate::window::WindowId> {
        self.window_registry().find_by_element(element)
    }

    /// Get a managed window by ID
    pub fn get_window(&self, id: crate::window::WindowId) -> Option<&crate::window::ManagedWindow> {
        self.window_registry().get(id)
    }

    /// Get a mutable managed window by ID
    pub fn get_window_mut(
        &mut self,
        id: crate::window::WindowId,
    ) -> Option<&mut crate::window::ManagedWindow> {
        self.window_registry_mut().get_mut(id)
    }

    /// Move a window to a different workspace using the new window registry
    pub fn move_window_to_workspace_by_id(
        &mut self,
        window_id: crate::window::WindowId,
        target_workspace_id: crate::workspace::WorkspaceId,
    ) {
        info!(
            "Moving window {} from workspace to workspace {}",
            window_id, target_workspace_id
        );

        // Get the managed window to find current workspace
        let (element, source_workspace_id) = match self.window_registry().get(window_id) {
            Some(mw) => (mw.element.clone(), mw.workspace),
            None => {
                tracing::warn!("Window {} not found in registry", window_id);
                return;
            }
        };

        if source_workspace_id == target_workspace_id {
            tracing::debug!("Window already in target workspace");
            return;
        }

        info!(
            "Moving window {} from workspace {} to workspace {}",
            window_id, source_workspace_id, target_workspace_id
        );

        // Check if the window being moved is currently focused
        let was_focused = self
            .focused_window()
            .map(|focused| self.window_registry().find_by_element(&focused) == Some(window_id))
            .unwrap_or(false);

        // Remove from source workspace
        if let Some(source_workspace) = self
            .workspace_manager
            .get_workspace_mut(source_workspace_id)
        {
            source_workspace.remove_window(window_id);

            // Unmap from space if source workspace is visible
            self.space_mut().unmap_elem(&element);
        }

        // Check if source workspace is visible and needs relayout
        if let Some(source_vo_id) = self
            .workspace_manager
            .find_workspace_location(source_workspace_id)
        {
            if let Some(source_vo) = self.virtual_output_manager.get(source_vo_id) {
                if source_vo.active_workspace() == Some(source_workspace_id.get() as usize) {
                    // Relayout source workspace after removing window
                    if let Some(source_workspace) = self
                        .workspace_manager
                        .get_workspace_mut(source_workspace_id)
                    {
                        source_workspace.relayout();
                    }
                    self.apply_workspace_layout(source_workspace_id);
                }
            }
        }

        // Update window registry
        if !self
            .window_registry_mut()
            .set_workspace(window_id, target_workspace_id)
        {
            tracing::warn!("Failed to update window workspace in registry");
            return;
        }

        // Add to target workspace
        if let Some(target_workspace) = self
            .workspace_manager
            .get_workspace_mut(target_workspace_id)
        {
            target_workspace.add_window(window_id);
            target_workspace.relayout();

            // Check if target workspace is visible on any output
            if let Some(vo_id) = self
                .workspace_manager
                .find_workspace_location(target_workspace_id)
            {
                if let Some(vo) = self.virtual_output_manager.get(vo_id) {
                    if vo.active_workspace() == Some(target_workspace_id.get() as usize) {
                        // Apply layout if workspace is visible
                        self.apply_workspace_layout(target_workspace_id);
                    }
                }
            }
        }

        // If the moved window was focused and the source workspace is still visible,
        // we need to update focus to a remaining window in the source workspace
        if was_focused {
            // Check if source workspace is visible
            if let Some(source_vo_id) = self
                .workspace_manager
                .find_workspace_location(source_workspace_id)
            {
                if let Some(source_vo) = self.virtual_output_manager.get(source_vo_id) {
                    if source_vo.active_workspace() == Some(source_workspace_id.get() as usize) {
                        // Source workspace is visible, focus next window in it
                        if let Some(source_workspace) =
                            self.workspace_manager.get_workspace(source_workspace_id)
                        {
                            if let Some(next_window_id) = source_workspace.focused_window {
                                // Focus the window that the workspace selected
                                let next_element = self
                                    .window_registry()
                                    .get(next_window_id)
                                    .map(|w| w.element.clone());
                                if let Some(element) = next_element {
                                    self.focus_window(&element);
                                    info!(
                                        "Focused window {} in source workspace after moving window",
                                        next_window_id
                                    );
                                }
                            } else if !source_workspace.windows.is_empty() {
                                // Workspace has windows but no focused window, focus the first one
                                let first_window_id = source_workspace.windows[0];
                                let first_element = self
                                    .window_registry()
                                    .get(first_window_id)
                                    .map(|w| w.element.clone());
                                if let Some(element) = first_element {
                                    self.focus_window(&element);
                                    info!("Focused first window {} in source workspace after moving window", first_window_id);
                                }
                            }
                            // If workspace is empty, focus remains nowhere (which is correct)
                        }
                    }
                }
            }
        }

        // Debug check consistency after move
        #[cfg(debug_assertions)]
        self.check_consistency();

        debug!(
            "Moved window {} from workspace {} to {}",
            window_id,
            source_workspace_id.get(),
            target_workspace_id.get()
        );
        self.update_ipc_workspace_state();
    }

    // Workspace management methods

    /// Switch to a workspace on a virtual output
    pub fn switch_workspace(
        &mut self,
        virtual_output_id: crate::virtual_output::VirtualOutputId,
        workspace_id: crate::workspace::WorkspaceId,
    ) {
        debug!(
            "Switching virtual output {} to workspace {}",
            virtual_output_id, workspace_id
        );

        // Recalculate exclusive zones before switching
        self.update_tiling_area_from_output();

        // Check if this workspace is associated with another output
        if let Some(associated_output) = self.workspace_manager.workspace_association(workspace_id)
        {
            if associated_output != virtual_output_id {
                info!(
                    "Workspace {} is associated with virtual output {}, switching to that output",
                    workspace_id, associated_output
                );

                // Switch the associated output to show this workspace
                self.switch_workspace(associated_output, workspace_id);

                // Move pointer focus to the center of that output
                if let Some(output) = self.virtual_output_manager.get(associated_output) {
                    let output_rect = output.logical_region();
                    let center = Point::from((
                        output_rect.loc.x + output_rect.size.w / 2,
                        output_rect.loc.y + output_rect.size.h / 2,
                    ));

                    // Move the pointer to the center of the target output
                    self.pointer().set_location(center.to_f64());

                    // Focus the first window in the target workspace
                    if let Some(workspace) = self.workspace_manager.get(workspace_id) {
                        if let Some(first_window_id) = workspace.windows.first() {
                            let element = self
                                .window_registry()
                                .get(*first_window_id)
                                .map(|mw| mw.element.clone());
                            if let Some(element) = element {
                                self.focus_window(&element);
                                info!(
                                    "Focused first window in workspace {} on output {}",
                                    workspace_id, associated_output
                                );
                            }
                        } else {
                            // No windows in workspace, clear keyboard focus
                            if let Some(keyboard) = self.seat().get_keyboard() {
                                keyboard.set_focus(self, None, SCOUNTER.next_serial());
                                info!(
                                    "No windows in workspace {}, cleared keyboard focus",
                                    workspace_id
                                );
                            }
                        }
                    }

                    info!(
                        "Moved pointer and keyboard focus to virtual output {} at {:?}",
                        associated_output, center
                    );
                }

                return;
            }
        }

        // Update the virtual output's active workspace
        self.virtual_output_manager
            .set_active_workspace(virtual_output_id, workspace_id.get() as usize);

        // Get the output's area
        if let Some(output) = self.virtual_output_manager.get(virtual_output_id) {
            // Use the calculated exclusive zone area if available, otherwise use the full area
            let area = self
                .virtual_output_exclusive_zones
                .get(&virtual_output_id)
                .copied()
                .unwrap_or_else(|| output.logical_region());

            // Get the current workspace (if any) to hide its windows
            let previous_workspace_id = self
                .workspace_manager
                .workspace_on_output(virtual_output_id);
            if let Some(current_workspace_id) = previous_workspace_id {
                if current_workspace_id == workspace_id {
                    return; // Already on this workspace
                }

                // Hide windows from current workspace
                let windows_to_hide: Vec<_> = self
                    .workspace_manager
                    .get(current_workspace_id)
                    .map(|ws| {
                        ws.windows
                            .iter()
                            .filter_map(|window_id| {
                                self.window_registry()
                                    .get(*window_id)
                                    .map(|mw| mw.element.clone())
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                for window_elem in windows_to_hide {
                    self.space_mut().unmap_elem(&window_elem);
                }
            }

            // Show the workspace on this output
            if let Err(e) = self.workspace_manager.show_workspace_on_output(
                workspace_id,
                virtual_output_id,
                area,
            ) {
                warn!("Failed to switch to workspace {}: {:?}", workspace_id, e);
                return;
            }

            // Show windows from new workspace
            self.apply_workspace_layout(workspace_id);

            // Focus the first window in the workspace (or the previously focused one)
            if let Some(workspace) = self.workspace_manager.get(workspace_id) {
                let window_to_focus = workspace
                    .focused_window
                    .or_else(|| workspace.windows.first().copied());

                if let Some(window_id) = window_to_focus {
                    let element = self
                        .window_registry()
                        .get(window_id)
                        .map(|mw| mw.element.clone());
                    if let Some(element) = element {
                        self.focus_window(&element);
                        debug!("Focused window {} in workspace {}", window_id, workspace_id);

                        // Update pointer focus to match the newly focused window
                        // This ensures mouse events go to the correct window
                        let pointer = self.pointer().clone();
                        let pointer_loc = pointer.current_location();
                        let surface_under = self.surface_under(pointer_loc);
                        pointer.motion(
                            self,
                            surface_under,
                            &smithay::input::pointer::MotionEvent {
                                location: pointer_loc,
                                serial: SCOUNTER.next_serial(),
                                time: self.clock.now().as_millis() as u32,
                            },
                        );
                        pointer.frame(self);
                    }
                } else {
                    // No windows in workspace, clear keyboard focus
                    if let Some(keyboard) = self.seat().get_keyboard() {
                        keyboard.set_focus(self, None, SCOUNTER.next_serial());
                        debug!(
                            "No windows in workspace {}, cleared keyboard focus",
                            workspace_id
                        );
                    }

                    // Also update pointer focus when workspace is empty
                    let pointer = self.pointer().clone();
                    let pointer_loc = pointer.current_location();
                    let surface_under = self.surface_under(pointer_loc);
                    pointer.motion(
                        self,
                        surface_under,
                        &smithay::input::pointer::MotionEvent {
                            location: pointer_loc,
                            serial: SCOUNTER.next_serial(),
                            time: self.clock.now().as_millis() as u32,
                        },
                    );
                    pointer.frame(self);
                }
            }

            debug!(
                "Switched virtual output {} to workspace {}",
                virtual_output_id, workspace_id
            );

            // Emit workspace switched event
            if let Some(prev_workspace) = previous_workspace_id {
                self.event_bus
                    .emit_workspace(crate::event::WorkspaceEvent::Switched {
                        old_workspace: prev_workspace,
                        new_workspace: workspace_id,
                        virtual_output: virtual_output_id,
                        timestamp: std::time::Instant::now(),
                    });
            }

            // Update IPC state (the event handler will do this now, but keep for backwards compatibility)
            self.update_ipc_workspace_state();
        }
    }

    /// Get the current workspace for a virtual output
    pub fn current_workspace(
        &self,
        virtual_output_id: crate::virtual_output::VirtualOutputId,
    ) -> Option<crate::workspace::WorkspaceId> {
        self.workspace_manager
            .workspace_on_output(virtual_output_id)
    }

    /// Update layout for all active workspaces
    pub fn update_all_workspace_layouts(&mut self) {
        // Get all active workspaces from virtual outputs
        let active_workspaces: Vec<_> = self
            .virtual_output_manager
            .all_virtual_outputs()
            .filter_map(|vo| {
                let workspace_idx = vo.active_workspace()?;
                Some((
                    vo.id(),
                    crate::workspace::WorkspaceId::new(workspace_idx as u8),
                ))
            })
            .collect();

        for (vo_id, workspace_id) in active_workspaces {
            // Use the calculated exclusive zone area if available
            if let Some(&area) = self.virtual_output_exclusive_zones.get(&vo_id) {
                if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id) {
                    workspace.area = area;
                    workspace.layout.set_area(area);
                }
                self.apply_workspace_layout(workspace_id);
            }
        }
    }

    /// Initialize a virtual output with a default workspace
    pub fn initialize_virtual_output(
        &mut self,
        virtual_output_id: crate::virtual_output::VirtualOutputId,
    ) {
        info!("Initializing virtual output {virtual_output_id}");

        // Update exclusive zones when a new virtual output is initialized
        self.update_tiling_area_from_output();

        // Check if this virtual output already has a workspace
        if self
            .workspace_manager
            .workspace_on_output(virtual_output_id)
            .is_none()
        {
            // No workspace assigned yet
            // If this is the first virtual output OR no workspaces are shown anywhere, use workspace 0
            let all_workspaces_hidden = (0..10).all(|i| {
                let ws_id = crate::workspace::WorkspaceId::new(i);
                self.workspace_manager.workspace_location(ws_id).is_none()
            });

            info!(
                "Virtual output {} has no workspace. Output count: {}, all workspaces hidden: {}",
                virtual_output_id,
                self.virtual_output_manager.outputs().count(),
                all_workspaces_hidden
            );

            if self.virtual_output_manager.outputs().count() <= 1 || all_workspaces_hidden {
                info!(
                    "Assigning workspace 1 to virtual output {}",
                    virtual_output_id
                );
                let workspace_id = crate::workspace::WorkspaceId::new(0); // First workspace is index 0
                self.switch_workspace(virtual_output_id, workspace_id);
            }
        } else {
            info!(
                "Virtual output {} already has a workspace",
                virtual_output_id
            );
        }
    }

    /// Add a new window to the workspace system
    pub fn add_window(
        &mut self,
        window: crate::shell::WindowElement,
        virtual_output_id: crate::virtual_output::VirtualOutputId,
    ) -> Option<crate::window::WindowId> {
        // Get the current workspace for this virtual output
        let workspace_id = match self
            .workspace_manager
            .workspace_on_output(virtual_output_id)
        {
            Some(ws_id) => ws_id,
            None => {
                // No workspace active on this output - activate workspace 1
                warn!(
                    "No workspace active on virtual output {}, activating workspace 1",
                    virtual_output_id
                );
                let workspace_id = crate::workspace::WorkspaceId::new(0); // First workspace is index 0

                // Get the virtual output's area and ensure workspace is properly initialized
                if let Some(output) = self.virtual_output_manager.get(virtual_output_id) {
                    let area = output.logical_region();
                    // Show workspace on output with proper area
                    if let Err(e) = self.workspace_manager.show_workspace_on_output(
                        workspace_id,
                        virtual_output_id,
                        area,
                    ) {
                        error!("Failed to create workspace on virtual output: {:?}", e);
                    }
                }

                // Update virtual output's active workspace
                self.virtual_output_manager
                    .set_active_workspace(virtual_output_id, workspace_id.get() as usize);

                workspace_id
            }
        };

        debug!(
            "Adding new window to virtual output {} workspace {}",
            virtual_output_id, workspace_id
        );

        // Get initial position (will be updated by layout)
        let initial_position = Point::from((0, 0));

        // Add window through window manager
        let (window_id, event) = self.window_manager.add_window(
            window.clone(),
            virtual_output_id,
            workspace_id,
            initial_position,
        )?;

        // Emit the window created event
        self.event_bus.emit_window(event);

        // Add to workspace - ensure consistency between registry and workspace
        if !self
            .workspace_manager
            .add_window_to_workspace(window_id, workspace_id)
        {
            tracing::error!(
                "Failed to add window {} to workspace {} - workspace doesn't exist",
                window_id,
                workspace_id
            );
            // Remove from registry to maintain consistency
            self.window_registry_mut().remove(window_id);
            return None;
        }

        // Debug check consistency
        #[cfg(debug_assertions)]
        self.check_consistency();

        // Update exclusive zones before relayout
        self.update_tiling_area_from_output();

        // Apply exclusive zone to workspace if it's visible
        if let Some(vo_id) = self.workspace_manager.find_workspace_location(workspace_id) {
            if let Some(&area) = self.virtual_output_exclusive_zones.get(&vo_id) {
                if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id) {
                    workspace.area = area;
                    workspace.layout.set_area(area);
                }
            }
        }

        // Get the workspace and relayout
        let window_geometry = if let Some(workspace) = self.workspace_manager.get_mut(workspace_id)
        {
            info!(
                "Workspace {} has {} windows",
                workspace_id,
                workspace.windows.len()
            );
            workspace.relayout();
            let geom = workspace.layout.get_window_geometry(window_id);
            info!("Window {} geometry after relayout: {:?}", window_id, geom);
            geom
        } else {
            None
        };

        // Apply layout to space
        self.apply_workspace_layout(workspace_id);

        // Use the window's geometry for bounds
        if let Some(geometry) = window_geometry {
            debug!("Window {} geometry: {:?}", window_id, geometry);

            // Set initial bounds and size
            if let Some(toplevel) = window.0.toplevel() {
                toplevel.with_pending_state(|state| {
                    use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State;
                    state.bounds = Some(geometry.size);
                    state.size = Some(geometry.size);
                    // Set all tiled states for new tiled windows
                    if let Some(managed_window) = self.window_registry().get(window_id) {
                        if managed_window.is_tiled() {
                            state.states.set(State::TiledLeft);
                            state.states.set(State::TiledRight);
                            state.states.set(State::TiledTop);
                            state.states.set(State::TiledBottom);
                        }
                    }
                });
                // Send configure if initial configure was already sent
                if toplevel.is_initial_configure_sent() {
                    toplevel.send_configure();
                }
            }
        } else {
            warn!("No geometry found for window {} after layout", window_id);
        }

        debug!(
            "Added window {} to workspace {} using new system. Space now has {} windows",
            window_id,
            workspace_id,
            self.space().elements().count()
        );

        // Focus the new window if the workspace is visible (i3/sway behavior - new windows steal focus)
        if let Some(_workspace) = self.workspace_manager.get(workspace_id) {
            // Check if this workspace is visible
            let is_visible = self
                .workspace_manager
                .find_workspace_location(workspace_id)
                .and_then(|vo_id| self.virtual_output_manager.get(vo_id))
                .map(|vo| vo.active_workspace() == Some(workspace_id.get() as usize))
                .unwrap_or(false);

            if is_visible {
                debug!(
                    "Focusing new window {} in visible workspace {}",
                    window_id, workspace_id
                );
                self.focus_window(&window);

                // Queue redraw for outputs where the new window is visible
                if let Some(bbox) = self.space().element_bbox(&window) {
                    let _outputs_to_redraw: Vec<_> = self
                        .space()
                        .outputs()
                        .filter(|output| {
                            self.space()
                                .output_geometry(output)
                                .map(|geo| geo.overlaps(bbox))
                                .unwrap_or(false)
                        })
                        .cloned()
                        .collect();
                }
            }
        }

        Some(window_id)
    }

    /// Get the focused window element
    pub fn focused_window(&self) -> Option<crate::shell::WindowElement> {
        if let Some(keyboard) = self.seat().get_keyboard() {
            if let Some(focus) = keyboard.current_focus() {
                match &focus {
                    crate::focus::KeyboardFocusTarget::Window(w) => {
                        self.space().elements().find(|elem| &elem.0 == w).cloned()
                    }
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Get window at a specific location
    pub fn window_at(&self, location: Point<i32, Logical>) -> Option<crate::shell::WindowElement> {
        let loc_f64 = Point::<f64, Logical>::from((location.x as f64, location.y as f64));
        self.space()
            .element_under(loc_f64)
            .map(|(elem, _)| elem)
            .cloned()
    }

    /// Focus a window element
    pub fn focus_window(&mut self, window: &crate::shell::WindowElement) {
        if let Some(keyboard) = self.seat().get_keyboard() {
            keyboard.set_focus(
                self,
                Some(crate::focus::KeyboardFocusTarget::Window(window.0.clone())),
                smithay::utils::SERIAL_COUNTER.next_serial(),
            );
            // Raise to top
            self.space_mut().raise_element(window, true);

            // Queue redraw for outputs affected by focus change
            if let Some(bbox) = self.space().element_bbox(window) {
                let _outputs_to_redraw: Vec<_> = self
                    .space()
                    .outputs()
                    .filter(|output| {
                        self.space()
                            .output_geometry(output)
                            .map(|geo| geo.overlaps(bbox))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect();
            }

            // Update workspace's focused_window tracking
            if let Some(window_id) = self.window_registry().find_by_element(window) {
                if let Some(managed_window) = self.window_registry().get(window_id) {
                    let workspace_id = managed_window.workspace;
                    if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id)
                    {
                        workspace.focused_window = Some(window_id);
                    }
                }
            }
        }
    }

    /// Center pointer on window
    pub fn center_pointer_on_window(&mut self, window: &crate::shell::WindowElement) {
        if let Some(loc) = self.space().element_location(window) {
            let geo = window.geometry();
            let center = Point::<f64, Logical>::from((
                (loc.x + geo.size.w / 2) as f64,
                (loc.y + geo.size.h / 2) as f64,
            ));
            self.pointer().set_location(center);
        }
    }

    /// Get current pointer location as integer coordinates
    pub fn pointer_location(&self) -> Point<i32, Logical> {
        let loc = self.pointer().current_location();
        Point::from((loc.x as i32, loc.y as i32))
    }

    /// Clamp pointer location to screen boundaries
    pub fn clamp_pointer_location(&self, location: Point<f64, Logical>) -> Point<f64, Logical> {
        // Get the maximum X coordinate across all outputs
        let max_x = self.space().outputs().fold(0, |acc, o| {
            acc + self
                .space()
                .output_geometry(o)
                .map(|g| g.size.w)
                .unwrap_or(0)
        });

        // Clamp X to valid range [0, max_x - 1]
        let clamped_x = location.x.clamp(0.0, (max_x - 1).max(0) as f64);

        // Find the output containing the clamped X coordinate to get the correct max_y
        let max_y = self
            .space()
            .outputs()
            .find(|o| {
                self.space()
                    .output_geometry(o)
                    .map(|geo| geo.contains(Point::from((clamped_x as i32, 0))))
                    .unwrap_or(false)
            })
            .and_then(|o| self.space().output_geometry(o).map(|g| g.size.h))
            .unwrap_or(0);

        // Clamp Y to valid range [0, max_y - 1]
        let clamped_y = location.y.clamp(0.0, (max_y - 1).max(0) as f64);

        Point::from((clamped_x, clamped_y))
    }

    /// Get virtual output at pointer location
    pub fn virtual_output_at_pointer(&self) -> Option<crate::virtual_output::VirtualOutputId> {
        self.virtual_output_manager
            .virtual_output_at(self.pointer_location())
    }

    /// Move current workspace to output in direction
    pub fn move_workspace_to_output(&mut self, direction: crate::config::Direction) {
        info!(
            "move_workspace_to_output called with direction: {:?}",
            direction
        );

        // Get current virtual output based on pointer
        let current_vo_id = match self.virtual_output_at_pointer() {
            Some(id) => id,
            None => {
                warn!("No virtual output at pointer location");
                return;
            }
        };

        // Get current workspace on this output
        let workspace_id = match self.workspace_manager.workspace_on_output(current_vo_id) {
            Some(id) => id,
            None => {
                warn!("No workspace on current virtual output");
                return;
            }
        };

        // Find target virtual output in the given direction
        let current_center = {
            let vo = match self.virtual_output_manager.get(current_vo_id) {
                Some(vo) => vo,
                None => {
                    error!("Current virtual output should exist but was not found");
                    return;
                }
            };
            let region = vo.logical_region();
            Point::<i32, Logical>::from((
                region.loc.x + region.size.w / 2,
                region.loc.y + region.size.h / 2,
            ))
        };

        // Find all other virtual outputs
        let mut candidates = Vec::new();
        for vo in self.virtual_output_manager.all_virtual_outputs() {
            if vo.id() == current_vo_id {
                continue;
            }

            let region = vo.logical_region();
            let center = Point::<i32, Logical>::from((
                region.loc.x + region.size.w / 2,
                region.loc.y + region.size.h / 2,
            ));

            // Check if this output is in the target direction
            let is_candidate = match direction {
                crate::config::Direction::Left => {
                    center.x < current_center.x
                        && (center.y - current_center.y).abs() < region.size.h
                }
                crate::config::Direction::Right => {
                    center.x > current_center.x
                        && (center.y - current_center.y).abs() < region.size.h
                }
                crate::config::Direction::Up => {
                    center.y < current_center.y
                        && (center.x - current_center.x).abs() < region.size.w
                }
                crate::config::Direction::Down => {
                    center.y > current_center.y
                        && (center.x - current_center.x).abs() < region.size.w
                }
            };

            if is_candidate {
                let distance = ((center.x - current_center.x).pow(2)
                    + (center.y - current_center.y).pow(2)) as f64;
                candidates.push((vo.id(), distance));
            }
        }

        // Sort by distance and take the closest one
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let target_vo_id = match candidates.first() {
            Some((id, _)) => *id,
            None => {
                info!("No virtual output found in direction {:?}", direction);
                return;
            }
        };

        // Check if target already has this workspace
        if let Some(target_workspace) = self.workspace_manager.workspace_on_output(target_vo_id) {
            if target_workspace == workspace_id {
                info!("Workspace {} already on target output", workspace_id);
                return;
            }
        }

        info!(
            "Moving workspace {} from output {} to output {} (direction: {:?})",
            workspace_id, current_vo_id, target_vo_id, direction
        );

        // Get the area of the target output
        let target_area = self
            .virtual_output_exclusive_zones
            .get(&target_vo_id)
            .copied()
            .unwrap_or_else(|| {
                self.virtual_output_manager
                    .get(target_vo_id)
                    .map(|vo| vo.logical_region())
                    .unwrap_or_else(|| {
                        tracing::error!("No virtual output found for ID {target_vo_id}");
                        Rectangle::from_size((1920, 1080).into())
                    })
            });

        // Hide windows from current workspace on current output
        let windows_to_move: Vec<_> = self
            .workspace_manager
            .get(workspace_id)
            .map(|ws| {
                ws.windows
                    .iter()
                    .filter_map(|window_id| {
                        self.window_registry()
                            .get(*window_id)
                            .map(|mw| mw.element.clone())
                    })
                    .collect()
            })
            .unwrap_or_default();

        for window_elem in &windows_to_move {
            self.space_mut().unmap_elem(window_elem);
        }

        // Hide the workspace from current output
        self.workspace_manager.hide_workspace(workspace_id);

        // Find a workspace to show on the current output (that just lost its workspace)
        // Look for a workspace that's either hidden or already associated with this output
        let replacement_workspace =
            (0..10)
                .map(|i| crate::workspace::WorkspaceId::new(i))
                .find(|&ws_id| {
                    ws_id != workspace_id
                        && self.workspace_manager.workspace_location(ws_id).is_none()
                    // Hidden workspace
                });

        if let Some(replacement_ws) = replacement_workspace {
            info!(
                "Switching current output {} to workspace {}",
                current_vo_id, replacement_ws
            );
            // Get the current output's area
            let current_area = self
                .virtual_output_exclusive_zones
                .get(&current_vo_id)
                .copied()
                .unwrap_or_else(|| {
                    self.virtual_output_manager
                        .get(current_vo_id)
                        .map(|vo| vo.logical_region())
                        .unwrap_or_else(|| {
                            tracing::error!("No virtual output found for ID {current_vo_id}");
                            Rectangle::from_size((1920, 1080).into())
                        })
                });

            // Show the replacement workspace on the current output
            let _ = self.workspace_manager.show_workspace_on_output(
                replacement_ws,
                current_vo_id,
                current_area,
            );
            self.virtual_output_manager
                .set_active_workspace(current_vo_id, replacement_ws.get() as usize);
            self.apply_workspace_layout(replacement_ws);
        }

        // Update workspace association to target output
        self.workspace_manager
            .associate_workspace_with_output(workspace_id, target_vo_id);

        // Show the workspace on the target output
        if let Err(e) =
            self.workspace_manager
                .show_workspace_on_output(workspace_id, target_vo_id, target_area)
        {
            warn!("Failed to move workspace to target output: {:?}", e);
            return;
        }

        // Apply the layout on the new output
        self.apply_workspace_layout(workspace_id);

        // Update the virtual output's active workspace
        self.virtual_output_manager
            .set_active_workspace(target_vo_id, workspace_id.get() as usize);

        // Move pointer to center of target output to follow the workspace
        if let Some(target_vo) = self.virtual_output_manager.get(target_vo_id) {
            let region = target_vo.logical_region();
            let center = Point::<f64, Logical>::from((
                (region.loc.x + region.size.w / 2) as f64,
                (region.loc.y + region.size.h / 2) as f64,
            ));
            self.pointer().set_location(center);
        }

        // Focus the first window in the moved workspace
        if let Some(workspace) = self.workspace_manager.get(workspace_id) {
            if let Some(window_id) = workspace
                .focused_window
                .or_else(|| workspace.windows.first().copied())
            {
                let element = self
                    .window_registry()
                    .get(window_id)
                    .map(|mw| mw.element.clone());
                if let Some(element) = element {
                    self.focus_window(&element);
                }
            }
        }

        // Update IPC state
        self.update_ipc_workspace_state();
    }

    /// Toggle fullscreen mode for focused window
    pub fn toggle_fullscreen(&mut self, mode: crate::window::FullscreenMode) {
        tracing::info!("toggle_fullscreen called with mode: {:?}", mode);
        if let Some(window_element) = self.focused_window() {
            if let Some(window_id) = self.window_registry().find_by_element(&window_element) {
                // Check if window is in the requested fullscreen mode
                let is_in_mode = self
                    .window_registry()
                    .get(window_id)
                    .map(|mw| {
                        matches!(&mw.layout,
                            crate::window::WindowLayout::Fullscreen { mode: m, .. } if *m == mode
                        )
                    })
                    .unwrap_or(false);

                tracing::info!(
                    "Window {} is_in_mode: {}, toggling to: {}",
                    window_id,
                    is_in_mode,
                    !is_in_mode
                );

                // Toggle: if already in this mode, turn off; otherwise switch to this mode
                let enable_fullscreen = !is_in_mode;

                self.set_window_fullscreen(window_id, enable_fullscreen, mode);
            } else {
                tracing::warn!("Focused window not found in registry");
            }
        } else {
            tracing::warn!("No focused window to toggle fullscreen");
        }
    }

    /// Set fullscreen mode for a window
    pub fn set_window_fullscreen(
        &mut self,
        window_id: crate::window::WindowId,
        enable: bool,
        mode: crate::window::FullscreenMode,
    ) {
        let window_info = self
            .window_registry()
            .get(window_id)
            .map(|mw| (mw.element.clone(), mw.workspace));

        if let Some((window_element, workspace_id)) = window_info {
            // Get workspace area before borrowing window registry
            let workspace_area = if mode == crate::window::FullscreenMode::Container {
                self.workspace_manager
                    .get_workspace(workspace_id)
                    .map(|ws| ws.area)
            } else {
                None
            };

            // Update the window registry
            if let Some(managed_window) = self.window_registry_mut().get_mut(window_id) {
                if enable && !managed_window.is_fullscreen() {
                    // Enter fullscreen - save current layout (must not be fullscreen)
                    if let Some(previous_layout) = managed_window.layout.as_non_fullscreen() {
                        // For container fullscreen, use the workspace area
                        // For other modes, the geometry will be updated later
                        let geometry = if let Some(area) = workspace_area {
                            area
                        } else {
                            managed_window.geometry() // For other modes or fallback
                        };

                        managed_window.layout = crate::window::WindowLayout::Fullscreen {
                            mode,
                            geometry,
                            previous: Box::new(previous_layout),
                        };
                    }
                } else if !enable && managed_window.is_fullscreen() {
                    // Exit fullscreen - restore previous layout
                    if let crate::window::WindowLayout::Fullscreen { previous, .. } =
                        &managed_window.layout
                    {
                        managed_window.layout = previous.clone().into_layout();
                    }
                }
            }

            // Handle the fullscreen mode
            if enable {
                match mode {
                    crate::window::FullscreenMode::Container => {
                        self.set_container_fullscreen(window_id, window_element, workspace_id);
                    }
                    crate::window::FullscreenMode::VirtualOutput => {
                        self.set_virtual_output_fullscreen(window_id, window_element, workspace_id);
                    }
                    crate::window::FullscreenMode::PhysicalOutput => {
                        self.set_physical_output_fullscreen(
                            window_id,
                            window_element,
                            workspace_id,
                        );
                    }
                }
            } else {
                self.unset_fullscreen(window_id, window_element, workspace_id);
            }
        }
    }

    /// Set container fullscreen (maximize within current container)
    fn set_container_fullscreen(
        &mut self,
        window_id: crate::window::WindowId,
        window_element: crate::shell::WindowElement,
        workspace_id: crate::workspace::WorkspaceId,
    ) {
        // Find which virtual output this workspace is on
        if let Some(vo_id) = self.workspace_manager.find_workspace_location(workspace_id) {
            // Get workspace area
            let workspace_area =
                if let Some(workspace) = self.workspace_manager.get_workspace(workspace_id) {
                    workspace.area
                } else {
                    return;
                };

            // Configure the window for container fullscreen
            if let Some(toplevel) = window_element.0.toplevel() {
                toplevel.with_pending_state(|state| {
                    state.states.set(xdg_toplevel::State::Fullscreen);
                    // Clear tiled states when fullscreen
                    state.states.unset(xdg_toplevel::State::TiledLeft);
                    state.states.unset(xdg_toplevel::State::TiledRight);
                    state.states.unset(xdg_toplevel::State::TiledTop);
                    state.states.unset(xdg_toplevel::State::TiledBottom);
                    state.size = Some(workspace_area.size);
                    state.bounds = Some(workspace_area.size);
                });
                if toplevel.is_initial_configure_sent() {
                    toplevel.send_configure();
                }
            }

            // Set as fullscreen window in workspace
            if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id) {
                workspace.fullscreen_window = Some(window_id);
            }

            // If this is the active workspace, apply the layout
            if let Some(vo) = self.virtual_output_manager.get(vo_id) {
                if vo.active_workspace() == Some(workspace_id.get() as usize) {
                    self.apply_workspace_layout(workspace_id);
                }
            }
        }
    }

    /// Set virtual output fullscreen
    fn set_virtual_output_fullscreen(
        &mut self,
        window_id: crate::window::WindowId,
        window_element: crate::shell::WindowElement,
        workspace_id: crate::workspace::WorkspaceId,
    ) {
        tracing::info!(
            "set_virtual_output_fullscreen for window {} in workspace {}",
            window_id,
            workspace_id
        );
        // Find which virtual output this workspace is on
        if let Some(vo_id) = self.workspace_manager.find_workspace_location(workspace_id) {
            // Get the virtual output info we need
            let (vo_region, is_active) = if let Some(vo) = self.virtual_output_manager.get(vo_id) {
                (
                    vo.logical_region(),
                    vo.active_workspace() == Some(workspace_id.get() as usize),
                )
            } else {
                tracing::warn!("Virtual output not found for workspace");
                return;
            };

            tracing::info!(
                "Virtual output {} region: {:?}, is_active: {}",
                vo_id,
                vo_region,
                is_active
            );

            // Update the window's geometry in the registry to match the virtual output
            if let Some(managed_window) = self.window_registry_mut().get_mut(window_id) {
                tracing::info!(
                    "Window {} current layout: {:?}",
                    window_id,
                    managed_window.layout
                );
                if let crate::window::WindowLayout::Fullscreen {
                    ref mut geometry, ..
                } = &mut managed_window.layout
                {
                    tracing::info!(
                        "Setting fullscreen geometry from {:?} to {:?}",
                        *geometry,
                        vo_region
                    );
                    *geometry = vo_region;
                    tracing::info!("After update, layout is: {:?}", managed_window.layout);
                } else {
                    tracing::warn!(
                        "Window {} is not in Fullscreen layout, current layout: {:?}",
                        window_id,
                        managed_window.layout
                    );
                }
            } else {
                tracing::warn!("Window {} not found in registry", window_id);
            }

            // Configure the window for fullscreen
            if let Some(toplevel) = window_element.0.toplevel() {
                toplevel.with_pending_state(|state| {
                    state.states.set(xdg_toplevel::State::Fullscreen);
                    // Clear tiled states when fullscreen
                    state.states.unset(xdg_toplevel::State::TiledLeft);
                    state.states.unset(xdg_toplevel::State::TiledRight);
                    state.states.unset(xdg_toplevel::State::TiledTop);
                    state.states.unset(xdg_toplevel::State::TiledBottom);
                    state.size = Some(vo_region.size);
                    state.bounds = Some(vo_region.size);
                });
                toplevel.send_configure();
            }

            // Set as fullscreen window in workspace
            if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id) {
                workspace.fullscreen_window = Some(window_id);
            }

            // Apply layout if active
            if is_active {
                self.apply_workspace_layout(workspace_id);
            }
        }
    }

    /// Set physical output fullscreen
    fn set_physical_output_fullscreen(
        &mut self,
        window_id: crate::window::WindowId,
        window_element: crate::shell::WindowElement,
        workspace_id: crate::workspace::WorkspaceId,
    ) {
        // Find the output containing this window
        let outputs = self.space().outputs_for_element(&window_element);
        if let Some(output) = outputs.first() {
            let output_geo = self.space().output_geometry(output).unwrap_or_default();

            // Update the window's geometry in the registry
            if let Some(managed_window) = self.window_registry_mut().get_mut(window_id) {
                if let crate::window::WindowLayout::Fullscreen {
                    ref mut geometry, ..
                } = &mut managed_window.layout
                {
                    *geometry = output_geo;
                }
            }

            // Configure the window for fullscreen on physical output
            if let Some(toplevel) = window_element.0.toplevel() {
                toplevel.with_pending_state(|state| {
                    state.states.set(xdg_toplevel::State::Fullscreen);
                    // Clear tiled states when fullscreen
                    state.states.unset(xdg_toplevel::State::TiledLeft);
                    state.states.unset(xdg_toplevel::State::TiledRight);
                    state.states.unset(xdg_toplevel::State::TiledTop);
                    state.states.unset(xdg_toplevel::State::TiledBottom);
                    state.size = Some(output_geo.size);
                    state.bounds = Some(output_geo.size);
                    // fullscreen_output expects a WlOutput, not smithay Output
                    // We need to get the WlOutput from the physical output
                    state.fullscreen_output = output
                        .user_data()
                        .get::<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>()
                        .cloned();
                });
                toplevel.send_configure();
            }

            // Set as fullscreen window in workspace (so other windows get hidden)
            if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id) {
                workspace.fullscreen_window = Some(window_id);
            }

            // Use FullscreenSurface to ensure the window is above everything including waybar
            use crate::shell::FullscreenSurface;
            output
                .user_data()
                .insert_if_missing(FullscreenSurface::default);
            if let Some(fs_surface) = output.user_data().get::<FullscreenSurface>() {
                fs_surface.set(window_element.clone());
            }

            // Map window directly to cover entire physical output
            // This bypasses the workspace layout and positions the window directly
            self.space_mut()
                .map_element(window_element, output_geo.loc, true);
        }
    }

    /// Unset fullscreen mode
    fn unset_fullscreen(
        &mut self,
        window_id: crate::window::WindowId,
        window_element: crate::shell::WindowElement,
        workspace_id: crate::workspace::WorkspaceId,
    ) {
        // Clear fullscreen state in workspace
        if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id) {
            if workspace.fullscreen_window == Some(window_id) {
                workspace.fullscreen_window = None;
            }
        }

        // Clear FullscreenSurface if this was physical output fullscreen
        for output in self.space().outputs() {
            use crate::shell::FullscreenSurface;
            if let Some(fs_surface) = output.user_data().get::<FullscreenSurface>() {
                if let Some(fs_window) = fs_surface.get() {
                    // Check if this is our window
                    if self.window_registry().find_by_element(&fs_window) == Some(window_id) {
                        fs_surface.clear();
                        break;
                    }
                }
            }
        }

        // Clear fullscreen state in window
        if let Some(toplevel) = window_element.0.toplevel() {
            toplevel.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Fullscreen);
                state.size = None;
                state.bounds = None;
                state.fullscreen_output.take();
            });
            toplevel.send_configure();
        }

        // Re-apply layout
        if let Some(vo_id) = self.workspace_manager.find_workspace_location(workspace_id) {
            if let Some(vo) = self.virtual_output_manager.get(vo_id) {
                if vo.active_workspace() == Some(workspace_id.get() as usize) {
                    self.apply_workspace_layout(workspace_id);
                }
            }
        }
    }

    /// Find surface under a given position
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(crate::focus::PointerFocusTarget, Point<f64, Logical>)> {
        use crate::shell::FullscreenSurface;
        use smithay::desktop::{layer_map_for_output, WindowSurfaceType};
        use smithay::wayland::shell::wlr_layer::Layer as WlrLayer;

        let output = self.space().outputs().find(|o| {
            self.space()
                .output_geometry(o)
                .map(|geometry| geometry.contains(pos.to_i32_round()))
                .unwrap_or(false)
        })?;
        let output_geo = self.space().output_geometry(output)?;
        let layers = layer_map_for_output(output);
        let mut under = None;
        if let Some((surface, loc)) = output
            .user_data()
            .get::<FullscreenSurface>()
            .and_then(|f| f.get())
            .and_then(|w| w.surface_under(pos - output_geo.loc.to_f64(), WindowSurfaceType::ALL))
        {
            under = Some((surface, loc + output_geo.loc));
        } else if let Some(focus) = layers
            .layer_under(WlrLayer::Overlay, pos - output_geo.loc.to_f64())
            .or_else(|| layers.layer_under(WlrLayer::Top, pos - output_geo.loc.to_f64()))
            .and_then(|layer| {
                let layer_loc = layers.layer_geometry(layer)?.loc;
                layer
                    .surface_under(
                        pos - output_geo.loc.to_f64() - layer_loc.to_f64(),
                        WindowSurfaceType::ALL,
                    )
                    .map(|(surface, loc)| {
                        (
                            crate::focus::PointerFocusTarget::from(surface),
                            loc + layer_loc + output_geo.loc,
                        )
                    })
            })
        {
            under = Some(focus)
        } else if let Some(focus) = self.space().element_under(pos).and_then(|(window, loc)| {
            window
                .surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL)
                .map(|(surface, surf_loc)| (surface, surf_loc + loc))
        }) {
            under = Some(focus);
        } else if let Some(focus) = layers
            .layer_under(WlrLayer::Bottom, pos - output_geo.loc.to_f64())
            .or_else(|| layers.layer_under(WlrLayer::Background, pos - output_geo.loc.to_f64()))
            .and_then(|layer| {
                let layer_loc = layers.layer_geometry(layer)?.loc;
                layer
                    .surface_under(
                        pos - output_geo.loc.to_f64() - layer_loc.to_f64(),
                        WindowSurfaceType::ALL,
                    )
                    .map(|(surface, loc)| {
                        (
                            crate::focus::PointerFocusTarget::from(surface),
                            loc + layer_loc + output_geo.loc,
                        )
                    })
            })
        {
            under = Some(focus)
        };
        under.map(|(s, l)| (s, l.to_f64()))
    }

    /// Release all currently pressed keys - useful when losing focus
    pub fn release_all_keys(&mut self) {
        use smithay::backend::input::KeyState;

        let Some(keyboard) = self.seat().get_keyboard() else {
            tracing::warn!("No keyboard available");
            return;
        };
        let time = self.clock.now().as_millis() as u32;

        // Get all currently pressed keycodes
        let pressed_keys: Vec<u32> = keyboard
            .with_pressed_keysyms(|keysyms| keysyms.iter().map(|k| k.raw_code().raw()).collect());

        // Release each key
        for keycode in pressed_keys {
            keyboard.input::<(), _>(
                self,
                keycode.into(),
                KeyState::Released,
                SCOUNTER.next_serial(),
                time,
                |_, _, _| smithay::input::keyboard::FilterResult::Forward,
            );
        }
    }
}

pub fn update_primary_scanout_output(
    space: &Space<WindowElement>,
    output: &Output,
    dnd_icon: &Option<DndIcon>,
    cursor_status: &CursorImageStatus,
    render_element_states: &RenderElementStates,
) {
    space.elements().for_each(|window| {
        window.with_surfaces(|surface, states| {
            update_surface_primary_scanout_output(
                surface,
                output,
                states,
                render_element_states,
                default_primary_scanout_output_compare,
            );
        });
    });
    let map = smithay::desktop::layer_map_for_output(output);
    for layer_surface in map.layers() {
        layer_surface.with_surfaces(|surface, states| {
            update_surface_primary_scanout_output(
                surface,
                output,
                states,
                render_element_states,
                default_primary_scanout_output_compare,
            );
        });
    }

    if let CursorImageStatus::Surface(ref surface) = cursor_status {
        with_surfaces_surface_tree(surface, |surface, states| {
            update_surface_primary_scanout_output(
                surface,
                output,
                states,
                render_element_states,
                default_primary_scanout_output_compare,
            );
        });
    }

    if let Some(surface) = dnd_icon.as_ref().map(|icon| &icon.surface) {
        with_surfaces_surface_tree(surface, |surface, states| {
            update_surface_primary_scanout_output(
                surface,
                output,
                states,
                render_element_states,
                default_primary_scanout_output_compare,
            );
        });
    }
}

#[derive(Debug, Clone)]
pub struct SurfaceDmabufFeedback {
    pub render_feedback: DmabufFeedback,
    pub scanout_feedback: DmabufFeedback,
}

#[profiling::function]
pub fn take_presentation_feedback(
    output: &Output,
    space: &Space<WindowElement>,
    render_element_states: &RenderElementStates,
) -> OutputPresentationFeedback {
    let mut output_presentation_feedback = OutputPresentationFeedback::new(output);

    space.elements().for_each(|window| {
        if space.outputs_for_element(window).contains(output) {
            window.take_presentation_feedback(
                &mut output_presentation_feedback,
                surface_primary_scanout_output,
                |surface, _| {
                    surface_presentation_feedback_flags_from_states(surface, render_element_states)
                },
            );
        }
    });
    let map = smithay::desktop::layer_map_for_output(output);
    for layer_surface in map.layers() {
        layer_surface.take_presentation_feedback(
            &mut output_presentation_feedback,
            surface_primary_scanout_output,
            |surface, _| {
                surface_presentation_feedback_flags_from_states(surface, render_element_states)
            },
        );
    }

    output_presentation_feedback
}

impl<BackendData: Backend> StilchState<BackendData> {
    /// Get a reference to the space (temporary delegation method)
    #[inline]
    pub fn space(&self) -> &Space<WindowElement> {
        self.window_manager.space()
    }

    /// Validate internal state consistency
    /// Returns Ok(()) if state is consistent, or Err with a list of validation errors
    #[cfg(debug_assertions)]
    pub fn validate_consistency(&self) -> super::validation::ValidationResult {
        super::validation::validate_full_state(self)
    }

    /// Debug helper to check and log validation errors
    #[cfg(debug_assertions)]
    pub fn check_consistency(&self) {
        if let Err(errors) = self.validate_consistency() {
            tracing::error!(
                "State consistency check failed with {} errors:",
                errors.len()
            );
            super::validation::log_validation_errors(&errors);
        }
    }

    /// Close a window by sending the appropriate close request
    pub fn close_window(&mut self, window: &WindowElement) {
        info!(
            "close_window called for window {:?}",
            window.0.toplevel().map(|t| t.wl_surface().id())
        );
        if let Some(toplevel) = window.0.toplevel() {
            info!("Sending close request to toplevel");
            toplevel.send_close();
        } else {
            warn!("Window has no toplevel surface, cannot send close request");
            // For X11 windows without toplevel, we might need to handle differently
            #[cfg(feature = "xwayland")]
            if let Some(x11_surface) = window.0.x11_surface() {
                info!("Attempting to close X11 window");
                let _ = x11_surface.close();
            }
        }
    }

    /// Close the currently focused window
    pub fn close_focused_window(&mut self) {
        if let Some(window) = self.focused_window() {
            // Find the window ID before closing
            let window_id = self.window_registry().find_by_element(&window);

            // Get workspace and find next window to focus BEFORE sending close
            let (workspace_id, next_focus) = if let Some(wid) = window_id {
                if let Some(mw) = self.window_registry().get(wid) {
                    let ws_id = mw.workspace;
                    // Get all windows in the workspace except the one being closed
                    let other_windows: Vec<_> = self
                        .workspace_manager
                        .get(ws_id)
                        .map(|ws| {
                            ws.windows
                                .iter()
                                .filter(|&&id| id != wid)
                                .copied()
                                .collect()
                        })
                        .unwrap_or_default();
                    (Some(ws_id), other_windows.first().copied())
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };

            info!(
                "Closing window {:?}, next focus candidate: {:?}",
                window_id, next_focus
            );

            // Send close request
            self.close_window(&window);

            // Immediately focus the next window (don't wait for toplevel_destroyed)
            if let Some(next_window_id) = next_focus {
                if let Some(mw) = self.window_registry().get(next_window_id) {
                    let next_window = mw.element.clone();
                    info!("Immediately focusing next window {next_window_id}");
                    self.focus_window(&next_window);

                    // Move pointer to the newly focused window
                    if let Some(loc) = self.space().element_location(&next_window) {
                        let geo = next_window.geometry();
                        let center = Point::<f64, Logical>::from((
                            (loc.x + geo.size.w / 2) as f64,
                            (loc.y + geo.size.h / 2) as f64,
                        ));
                        self.pointer().set_location(center);
                    }
                }
            } else if workspace_id.is_some() {
                // No other windows, clear focus
                info!("No other windows in workspace, clearing focus");
                if let Some(keyboard) = self.seat().get_keyboard() {
                    keyboard.set_focus(self, None, smithay::utils::SERIAL_COUNTER.next_serial());
                }
            }
        }
    }

    /// Get a mutable reference to the space (temporary delegation method)
    #[inline]
    pub fn space_mut(&mut self) -> &mut Space<WindowElement> {
        self.window_manager.space_mut()
    }

    /// Get a reference to the window registry (temporary delegation method)
    #[inline]
    pub fn window_registry(&self) -> &crate::window::WindowRegistry {
        self.window_manager.registry()
    }

    /// Get a mutable reference to the window registry (temporary delegation method)
    #[inline]
    pub fn window_registry_mut(&mut self) -> &mut crate::window::WindowRegistry {
        self.window_manager.registry_mut()
    }

    /// Get a reference to the popups (temporary delegation method)
    #[inline]
    pub fn popups(&self) -> &smithay::desktop::PopupManager {
        self.window_manager.popups()
    }

    /// Get a mutable reference to the popups (temporary delegation method)
    #[inline]
    pub fn popups_mut(&mut self) -> &mut smithay::desktop::PopupManager {
        self.window_manager.popups_mut()
    }

    // Input delegation methods (temporary until direct access is removed)

    /// Get a reference to the seat
    #[inline]
    pub fn seat(&self) -> &Seat<StilchState<BackendData>> {
        self.input_manager.seat()
    }

    /// Get a mutable reference to the seat
    #[inline]
    pub fn seat_mut(&mut self) -> &mut Seat<StilchState<BackendData>> {
        self.input_manager.seat_mut()
    }

    /// Get a reference to the pointer
    #[inline]
    pub fn pointer(&self) -> &PointerHandle<StilchState<BackendData>> {
        self.input_manager.pointer()
    }

    /// Get the cursor status
    #[inline]
    pub fn cursor_status(&self) -> &CursorImageStatus {
        self.input_manager.cursor_status()
    }

    /// Get mutable cursor status
    #[inline]
    pub fn cursor_status_mut(&mut self) -> &mut CursorImageStatus {
        &mut self.input_manager.cursor_status
    }

    /// Get the DnD icon
    #[inline]
    pub fn dnd_icon(&self) -> Option<&DndIcon> {
        self.input_manager.dnd_icon()
    }

    /// Get mutable DnD icon
    #[inline]
    pub fn dnd_icon_mut(&mut self) -> &mut Option<DndIcon> {
        &mut self.input_manager.dnd_icon
    }

    /// Get suppressed keys
    #[inline]
    pub fn suppressed_keys(&self) -> &[Keysym] {
        &self.input_manager.suppressed_keys
    }

    /// Apply workspace layout to space - optimized to only update changed windows
    pub fn apply_workspace_layout(&mut self, workspace_id: crate::workspace::WorkspaceId) {
        tracing::debug!(
            "apply_workspace_layout: Applying layout for workspace {}",
            workspace_id
        );

        // First, call relayout if needed
        if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id) {
            // Just ensure layout is recalculated, don't apply yet
            workspace.relayout();
        }

        // Collect position updates to batch them
        let mut position_updates = Vec::new();

        // Now apply the layout using the window manager's methods
        if let Some(workspace) = self.workspace_manager.get_workspace(workspace_id) {
            // Check if we have a fullscreen window
            if let Some(fullscreen_id) = workspace.fullscreen_window {
                // Handle fullscreen window
                if let Some(managed_window) = self.window_registry().get(fullscreen_id) {
                    if let crate::window::WindowLayout::Fullscreen { mode, .. } =
                        &managed_window.layout
                    {
                        let window_element = managed_window.element.clone();

                        match mode {
                            crate::window::FullscreenMode::Container => {
                                // For container fullscreen, window should fill the workspace
                                // (since we typically have one root container per workspace)
                                tracing::info!(
                                    "Applying container fullscreen for window {} at {:?}",
                                    fullscreen_id,
                                    workspace.area
                                );
                                self.window_manager.space_mut().map_element(
                                    window_element,
                                    workspace.area.loc,
                                    true,
                                );
                                position_updates.push((fullscreen_id, workspace.area.loc));
                                self.window_manager
                                    .resize_window(fullscreen_id, workspace.area);
                            }
                            crate::window::FullscreenMode::VirtualOutput => {
                                // For virtual output fullscreen, use the workspace area
                                tracing::info!(
                                    "Applying virtual output fullscreen for window {} at {:?}",
                                    fullscreen_id,
                                    workspace.area
                                );
                                self.window_manager.space_mut().map_element(
                                    window_element,
                                    workspace.area.loc,
                                    true,
                                );
                                position_updates.push((fullscreen_id, workspace.area.loc));
                                self.window_manager
                                    .resize_window(fullscreen_id, workspace.area);
                            }
                            crate::window::FullscreenMode::PhysicalOutput => {
                                // Physical output fullscreen is handled elsewhere
                                // The window is mapped directly to the physical output
                            }
                        }

                        // Hide other windows when fullscreen
                        let other_windows: Vec<_> = workspace
                            .windows
                            .iter()
                            .filter(|&&id| id != fullscreen_id)
                            .filter_map(|&id| {
                                self.window_registry().get(id).map(|mw| mw.element.clone())
                            })
                            .collect();

                        for element in other_windows {
                            self.window_manager.space_mut().unmap_elem(&element);
                        }
                    }
                }
            } else {
                // Normal layout - no fullscreen
                // Get window positions from the workspace layout
                // Use get_visible_geometries() to only get windows that should be visible
                // (for tabbed/stacked containers, this returns only the active child)
                let geometries = workspace.layout.get_visible_geometries();

                for (window_id, geometry) in geometries {
                    if let Some(managed_window) = self.window_registry().get(window_id) {
                        // Ensure window is mapped to space
                        let window_element = managed_window.element.clone();
                        self.window_manager.space_mut().map_element(
                            window_element,
                            geometry.loc,
                            true,
                        );

                        // Collect position updates for batch processing
                        position_updates.push((window_id, geometry.loc));

                        // TODO: Only resize if size actually changed
                        self.window_manager.resize_window(window_id, geometry);
                    }
                }
            }
        }

        // Batch update positions - this will skip windows that haven't moved
        let events = self.window_manager.batch_update_positions(position_updates);
        for event in events {
            self.event_bus.emit_window(event);
        }
    }
}

pub trait Backend {
    const HAS_RELATIVE_MOTION: bool = false;
    const HAS_GESTURES: bool = false;
    fn seat_name(&self) -> String;
    fn reset_buffers(&mut self, output: &Output);
    fn early_import(&mut self, surface: &WlSurface);
    fn update_led_state(&mut self, led_state: LedState);
    fn request_render(&mut self) {
        // Default implementation does nothing
        // Backends that need explicit render requests should override
    }

    fn request_render_for_output(&mut self, _output: &Output) {
        // Default implementation requests render for all outputs
        // Backends can override to be more selective
        self.request_render();
    }

    fn should_schedule_render(&self) -> bool {
        // Default implementation always returns true
        // Backends can override to prevent duplicate idle callbacks
        true
    }
}
