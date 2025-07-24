//! Minimal Wayland client that creates a simple colored window for testing
//!
//! This creates a window with a solid color background and keeps it alive
//! until killed. Perfect for integration testing.

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    output::{OutputHandler, OutputState},
    reexports::client::{
        globals::registry_queue_init,
        protocol::{wl_output, wl_shm, wl_surface},
        Connection, QueueHandle,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        xdg::{
            window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
            XdgShell,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};

fn main() {
    // Get window title from args or use default
    let args: Vec<String> = std::env::args().collect();
    let title = args.get(1).unwrap_or(&"Test Window".to_string()).clone();
    let color = args
        .get(2)
        .and_then(|s| match s.as_str() {
            "red" => Some(0xFFFF0000),
            "green" => Some(0xFF00FF00),
            "blue" => Some(0xFF0000FF),
            "yellow" => Some(0xFFFFFF00),
            _ => None,
        })
        .unwrap_or(0xFF808080); // Default gray

    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to connect to Wayland: {e:?}");
            std::process::exit(1);
        }
    };
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell = XdgShell::bind(&globals, &qh).unwrap();
    let shm = Shm::bind(&globals, &qh).unwrap();

    let surface = compositor.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::None, &qh);
    window.set_title(title);
    window.set_app_id("simple-window".to_string());

    // Configure window size
    window.set_min_size(Some((256, 256)));

    // Request initial configure
    window.commit();

    let pool = SlotPool::new(800 * 600 * 4, &shm).unwrap();

    let mut simple_window = SimpleWindow {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,

        window,
        pool,
        color,
        configured: false,
        width: 800,
        height: 600,
    };

    // Initial draw
    simple_window.draw(&qh);

    loop {
        match event_queue.blocking_dispatch(&mut simple_window) {
            Ok(_) => {},
            Err(e) => {
                // Check if it's a broken pipe error (compositor gone)
                let err_str = format!("{e:?}");
                if err_str.contains("Broken pipe") || err_str.contains("broken pipe") {
                    eprintln!("Compositor connection lost (broken pipe), exiting cleanly");
                    break;
                }
                // For other errors, panic as before
                panic!("Event dispatch failed: {e}");
            }
        }
    }
}

struct SimpleWindow {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,

    window: Window,
    pool: SlotPool,
    color: u32,
    configured: bool,
    width: u32,
    height: u32,
}

impl SimpleWindow {
    fn draw(&mut self, _qh: &QueueHandle<Self>) {
        let (buffer, canvas) = self
            .pool
            .create_buffer(
                self.width as i32,
                self.height as i32,
                (self.width * 4) as i32,
                wl_shm::Format::Argb8888,
            )
            .expect("create buffer");

        // Fill with solid color
        for pixel in canvas.chunks_exact_mut(4) {
            pixel[0] = (self.color & 0xFF) as u8; // B
            pixel[1] = ((self.color >> 8) & 0xFF) as u8; // G
            pixel[2] = ((self.color >> 16) & 0xFF) as u8; // R
            pixel[3] = ((self.color >> 24) & 0xFF) as u8; // A
        }

        self.window
            .wl_surface()
            .attach(Some(buffer.wl_buffer()), 0, 0);
        self.window
            .wl_surface()
            .damage_buffer(0, 0, self.width as i32, self.height as i32);
        self.window.wl_surface().commit();

        // Buffer will be released when it goes out of scope
    }
}

impl CompositorHandler for SimpleWindow {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.draw(qh);
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for SimpleWindow {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl WindowHandler for SimpleWindow {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        std::process::exit(0);
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        if let (Some(w), Some(h)) = configure.new_size {
            self.width = w.get();
            self.height = h.get();
        }

        self.configured = true;
        self.draw(qh);
    }
}

impl ShmHandler for SimpleWindow {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for SimpleWindow {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers!(OutputState);
}

smithay_client_toolkit::delegate_compositor!(SimpleWindow);
smithay_client_toolkit::delegate_output!(SimpleWindow);
smithay_client_toolkit::delegate_shm!(SimpleWindow);
smithay_client_toolkit::delegate_xdg_shell!(SimpleWindow);
smithay_client_toolkit::delegate_xdg_window!(SimpleWindow);
smithay_client_toolkit::delegate_registry!(SimpleWindow);
