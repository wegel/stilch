//! stilch - A Tiling Wayland Compositor with Virtual Outputs
//!
//! stilch is a modern Wayland compositor that introduces innovative features
//! for multi-monitor productivity:
//!
//! - **Virtual Outputs**: Split physical monitors into multiple logical displays
//!   or merge multiple monitors into one unified workspace
//! - **Advanced Fullscreen Modes**: Three-tier fullscreen system (container,
//!   workspace, global) for flexible content viewing
//! - **Intelligent Cursor Transitions**: Seamless cursor movement across monitors
//!   with different DPIs
//!
//! # Architecture
//!
//! The compositor is built on the [Smithay](https://github.com/Smithay/smithay)
//! compositor library and follows a modular architecture:
//!
//! - [`state`]: Core compositor state management
//! - [`shell`]: Wayland protocol implementations (XDG, Layer Shell, etc.)
//! - [`window`]: Window management and tiling logic
//! - [`workspace`]: Workspace organization and switching
//! - [`virtual_output`]: Virtual output management
//! - [`input`]: Input device and event handling
//! - [`render`]: Rendering pipeline and damage tracking
//! - [`backend`]: Platform backends (DRM/KMS, Winit, X11)

#![warn(rust_2018_idioms)]
// If no backend is enabled, a large portion of the codebase is unused.
// So silence this useless warning for the CI.
#![cfg_attr(
    not(any(feature = "winit", feature = "x11", feature = "udev")),
    allow(dead_code, unused_imports)
)]

pub mod backend;
pub mod command;
pub mod config;
#[cfg(any(feature = "udev", feature = "xwayland"))]
pub mod cursor;
pub mod drawing;
pub mod error;
pub mod event;
pub mod focus;
pub mod handlers;
pub mod input;
pub mod ipc;
pub mod keybindings;
pub mod render;
pub mod shell;
pub mod state;
pub mod tab_bar;
pub mod test_ipc;
pub mod test_ipc_server;
pub mod test_mode;
#[cfg(feature = "udev")]
pub mod udev;
pub mod virtual_output;
pub mod window;
#[cfg(feature = "winit")]
pub mod winit;
pub mod workspace;
#[cfg(feature = "x11")]
pub mod x11;

pub use state::{ClientState, StilchState};
