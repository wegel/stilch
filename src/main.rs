//! stilch - Tiling Wayland Compositor
//!
//! Entry point for the stilch compositor. Supports multiple backends:
//! - **udev**: Native DRM/KMS backend for running on bare metal (TTY)
//! - **winit**: Nested compositor running as a Wayland or X11 client
//! - **x11**: X11 client backend for testing
//! - **test**: ASCII backend for automated testing
//!
//! Run with `--help` to see available backends and options.

static POSSIBLE_BACKENDS: &[&str] = &[
    #[cfg(feature = "winit")]
    "--winit : Run stilch as a X11 or Wayland client using winit.",
    #[cfg(feature = "udev")]
    "--tty-udev : Run stilch as a tty udev client (requires root if without logind).",
    "             --enable-test-ipc : Enable test IPC server for debugging (with udev backend).",
    #[cfg(feature = "x11")]
    "--x11 : Run stilch as an X11 client.",
    "--test : Run stilch with ASCII backend for testing.",
    "         Options: --ascii-size WIDTHxHEIGHT (default: 160x90)",
    "                  --logical-size WIDTHxHEIGHT (default: 3840x2160)",
    "                  --ascii-output WIDTHxHEIGHT+X+Y (add additional outputs)",
];

#[cfg(feature = "profile-with-tracy-mem")]
#[global_allocator]
static GLOBAL: profiling::tracy_client::ProfiledAllocator<std::alloc::System> =
    profiling::tracy_client::ProfiledAllocator::new(std::alloc::System, 10);

// Allow in this function because of existing usage
#[allow(clippy::uninlined_format_args)]
fn main() {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt()
            .compact()
            .with_env_filter(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt().compact().init();
    }

    #[cfg(feature = "profile-with-tracy")]
    profiling::tracy_client::Client::start();

    profiling::register_thread!("Main Thread");

    #[cfg(feature = "profile-with-puffin")]
    let _server =
        puffin_http::Server::new(&format!("0.0.0.0:{}", puffin_http::DEFAULT_PORT))
            .expect("Failed to start puffin profiling server");
    #[cfg(feature = "profile-with-puffin")]
    profiling::puffin::set_scopes_on(true);

    let arg = ::std::env::args().nth(1);
    match arg.as_ref().map(|s| &s[..]) {
        #[cfg(feature = "winit")]
        Some("--winit") => {
            tracing::info!("Starting stilch with winit backend");
            if let Err(e) = stilch::winit::run_winit() {
                tracing::error!("Failed to run Winit backend: {e}");
                std::process::exit(1);
            }
        }
        #[cfg(feature = "udev")]
        Some("--tty-udev") => {
            // Check if --enable-test-ipc is present in args
            let args: Vec<String> = ::std::env::args().collect();
            let enable_test_ipc = args.iter().any(|arg| arg == "--enable-test-ipc");

            if enable_test_ipc {
                tracing::info!("Starting stilch on a tty using udev with test IPC enabled");
            } else {
                tracing::info!("Starting stilch on a tty using udev");
            }
            if let Err(e) = stilch::udev::run_udev(enable_test_ipc) {
                tracing::error!("Fatal error: {e}");
                std::process::exit(1);
            }
        }
        #[cfg(feature = "x11")]
        Some("--x11") => {
            tracing::info!("Starting stilch with x11 backend");
            if let Err(e) = stilch::x11::run_x11() {
                tracing::error!("Failed to run X11 backend: {e}");
                std::process::exit(1);
            }
        }
        Some("--test") => {
            // Parse additional arguments for test mode configuration
            let mut config = stilch::test_mode::TestModeConfig::default();
            let mut config_file = None;

            // Check for arguments
            let args: Vec<String> = ::std::env::args().collect();
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--ascii-size" && i + 1 < args.len() {
                    if let Some((width, height)) = args[i + 1].split_once('x') {
                        if let (Ok(w), Ok(h)) = (width.parse::<usize>(), height.parse::<usize>()) {
                            config.ascii_width = w;
                            config.ascii_height = h;
                            tracing::info!("Using custom ASCII size: {}x{}", w, h);
                        }
                    }
                    i += 2;
                } else if args[i] == "--logical-size" && i + 1 < args.len() {
                    if let Some((width, height)) = args[i + 1].split_once('x') {
                        if let (Ok(w), Ok(h)) = (width.parse::<i32>(), height.parse::<i32>()) {
                            config.logical_width = w;
                            config.logical_height = h;
                            tracing::info!("Using custom logical size: {}x{}", w, h);
                        }
                    }
                    i += 2;
                } else if args[i] == "--ascii-output" && i + 1 < args.len() {
                    // Parse output geometry in format WIDTHxHEIGHT+X+Y
                    if let Some((size_part, pos_part)) = args[i + 1].split_once('+') {
                        if let Some((x_part, y_part)) = pos_part.split_once('+') {
                            if let Some((width, height)) = size_part.split_once('x') {
                                if let (Ok(w), Ok(h), Ok(x), Ok(y)) = (
                                    width.parse::<i32>(),
                                    height.parse::<i32>(),
                                    x_part.parse::<i32>(),
                                    y_part.parse::<i32>(),
                                ) {
                                    use smithay::utils::{Point, Rectangle};
                                    let rect = Rectangle::new(Point::from((x, y)), (w, h).into());
                                    config.additional_outputs.push(rect);
                                    tracing::info!("Added output: {}x{}+{}+{}", w, h, x, y);
                                }
                            }
                        }
                    }
                    i += 2;
                } else if args[i] == "--config" && i + 1 < args.len() {
                    config_file = Some(args[i + 1].clone());
                    tracing::info!("Using config file: {}", args[i + 1]);
                    i += 2;
                } else {
                    i += 1;
                }
            }

            // Set config file path if provided
            if let Some(config_path) = config_file {
                std::env::set_var("STILCH_CONFIG_FILE", config_path);
            }

            tracing::info!("Starting stilch with ASCII backend for testing");
            stilch::test_mode::run_test_mode(config);
        }
        Some(other) => {
            tracing::error!("Unknown backend: {other}");
        }
        None => {
            #[allow(clippy::disallowed_macros)]
            {
                println!("USAGE: stilch --backend");
                println!();
                println!("Possible backends are:");
                for b in POSSIBLE_BACKENDS {
                    println!("\t{b}");
                }
            }
        }
    }
}
