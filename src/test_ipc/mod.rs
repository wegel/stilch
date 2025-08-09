//! IPC mechanism for test mode communication
//!
//! Provides a Unix socket interface for test clients to control
//! the compositor and request ASCII state representations.

pub mod compositor_handler;

pub use compositor_handler::CompositorTestHandler;

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

/// Direction for movement/focus commands
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::Left => write!(f, "left"),
            Direction::Right => write!(f, "right"),
            Direction::Up => write!(f, "up"),
            Direction::Down => write!(f, "down"),
        }
    }
}

impl Direction {
    /// Convert to config::Direction
    pub fn to_config_direction(self) -> crate::config::Direction {
        match self {
            Direction::Left => crate::config::Direction::Left,
            Direction::Right => crate::config::Direction::Right,
            Direction::Up => crate::config::Direction::Up,
            Direction::Down => crate::config::Direction::Down,
        }
    }
}

/// Split direction for layout
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

impl std::fmt::Display for SplitDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SplitDirection::Horizontal => write!(f, "horizontal"),
            SplitDirection::Vertical => write!(f, "vertical"),
        }
    }
}

impl SplitDirection {
    /// Convert to workspace layout SplitDirection
    pub fn to_layout_split(self) -> crate::workspace::layout::SplitDirection {
        match self {
            SplitDirection::Horizontal => crate::workspace::layout::SplitDirection::Horizontal,
            SplitDirection::Vertical => crate::workspace::layout::SplitDirection::Vertical,
        }
    }
}

/// Layout mode for containers
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutMode {
    Tabbed,
    Stacking,
    #[serde(rename = "splith")]
    SplitH,
    #[serde(rename = "splitv")]
    SplitV,
}

impl std::fmt::Display for LayoutMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutMode::Tabbed => write!(f, "tabbed"),
            LayoutMode::Stacking => write!(f, "stacking"),
            LayoutMode::SplitH => write!(f, "splith"),
            LayoutMode::SplitV => write!(f, "splitv"),
        }
    }
}

impl LayoutMode {
    /// Convert to config LayoutCommand
    pub fn to_layout_command(self) -> Option<crate::config::LayoutCommand> {
        match self {
            LayoutMode::Tabbed => Some(crate::config::LayoutCommand::Tabbed),
            LayoutMode::Stacking => Some(crate::config::LayoutCommand::Stacking),
            LayoutMode::SplitH => Some(crate::config::LayoutCommand::SplitH),
            LayoutMode::SplitV => Some(crate::config::LayoutCommand::SplitV),
        }
    }
}

/// Mouse button for click events
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Commands that can be sent from test client to compositor
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TestCommand {
    /// Create a new window with specified size
    CreateWindow { width: i32, height: i32 },

    /// Destroy a window
    DestroyWindow { id: u64 },

    /// Kill the currently focused window (same as Super+Q)
    KillFocusedWindow,

    /// Focus a specific window
    FocusWindow { id: u64 },

    /// Move focus in a direction
    MoveFocus { direction: Direction },

    /// Click at a specific location
    ClickAt { x: i32, y: i32 },

    /// Move a window in a direction (swap positions)
    MoveWindow { id: u64, direction: Direction },

    /// Resize a window
    ResizeWindow { id: u64, width: i32, height: i32 },

    /// Set split direction for next window
    SetSplitDirection { direction: SplitDirection },

    /// Switch to a workspace
    SwitchWorkspace { index: usize },

    /// Move window to workspace
    MoveWindowToWorkspace { window_id: u64, workspace: usize },

    /// Move the focused window to another workspace
    MoveFocusedWindowToWorkspace { workspace: usize },

    /// Set window to fullscreen
    SetFullscreen { id: u64, enabled: bool },

    /// Set window to floating
    SetFloating { id: u64, enabled: bool },

    /// Request the current ASCII state
    GetState,

    /// Request list of windows
    GetWindows,

    /// Get currently focused window
    GetFocusedWindow,

    /// Get list of workspaces and their state
    GetWorkspaces,

    /// Get ASCII snapshot with optional annotations
    GetAsciiSnapshot { show_ids: bool, show_focus: bool },

    /// Get list of outputs
    GetOutputs,

    /// Set layout mode for current container
    SetLayout { mode: LayoutMode },

    /// Move workspace to output in direction
    MoveWorkspaceToOutput { direction: Direction },

    /// Simulate key press
    KeyPress {
        key: String, // e.g., "Super+1", "Super+Return"
    },

    /// Move mouse to position
    MoveMouse { x: i32, y: i32 },

    /// Get current cursor position
    GetCursorPosition,

    /// Click mouse button
    MouseClick {
        button: MouseButton,
        x: Option<i32>,
        y: Option<i32>,
    },

    /// Toggle fullscreen (default: virtual output)
    Fullscreen,

    /// Toggle container fullscreen
    FullscreenContainer,

    /// Toggle virtual output fullscreen
    FullscreenVirtualOutput,

    /// Toggle physical output fullscreen
    FullscreenPhysicalOutput,

    /// Wait for a condition
    WaitFor {
        condition: WaitCondition,
        timeout_ms: u64,
    },
}

/// Conditions to wait for
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WaitCondition {
    WindowCount(usize),
    WindowFocused(u64),
    WorkspaceActive(usize),
}

/// Responses from compositor to test client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TestResponse {
    /// Command executed successfully
    Success { message: String },

    /// Command failed
    Error { message: String },

    /// ASCII state representation
    State { ascii: String },

    /// Window list
    Windows { windows: Vec<WindowInfo> },

    /// Focused window
    FocusedWindow { id: Option<u64> },

    /// Workspace list
    Workspaces { workspaces: Vec<WorkspaceInfo> },

    /// Output list
    Outputs { outputs: Vec<OutputInfo> },

    /// ASCII snapshot
    AsciiSnapshot {
        snapshot: String,
        width: usize,
        height: usize,
    },

    /// Window created
    WindowCreated { id: u64 },

    /// Condition met
    ConditionMet,

    /// Timeout waiting for condition
    Timeout,
}

/// Window information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u64,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub workspace: usize,
    pub focused: bool,
    pub floating: bool,
    pub fullscreen: bool,
    pub title: Option<String>,
    pub visible: bool,
}

/// Workspace information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: usize,
    pub name: String,
    pub visible: bool,
    pub output: Option<String>,
    pub window_count: usize,
    pub focused: bool,
}

/// Output information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputInfo {
    pub id: u64,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub name: String,
}

/// Test IPC server that runs in the compositor
pub struct TestIpcServer {
    socket_path: PathBuf,
    listener: Option<UnixListener>,
    handler: Arc<Mutex<dyn TestCommandHandler + Send>>,
}

/// Trait for handling test commands in the compositor
pub trait TestCommandHandler {
    /// Handle a test command and return a response
    fn handle_command(&mut self, command: TestCommand) -> TestResponse;
}

impl TestIpcServer {
    /// Create a new test IPC server
    pub fn new(socket_path: PathBuf, handler: Arc<Mutex<dyn TestCommandHandler + Send>>) -> Self {
        Self {
            socket_path,
            listener: None,
            handler,
        }
    }

    /// Start listening for connections
    pub fn start(&mut self) -> std::io::Result<()> {
        // Remove existing socket if it exists
        let _ = std::fs::remove_file(&self.socket_path);

        // Create the listener
        let listener = UnixListener::bind(&self.socket_path)?;
        self.listener = Some(listener);

        println!("Test IPC server listening on {:?}", self.socket_path);
        Ok(())
    }

    /// Accept and handle a single connection (for testing in same thread)
    pub fn handle_one_connection(&mut self) -> std::io::Result<()> {
        if let Some(listener) = &self.listener {
            let (stream, _) = listener.accept()?;
            self.handle_client(stream)?;
        }
        Ok(())
    }

    /// Check for and handle any pending connections (non-blocking)
    pub fn handle_connections(&mut self) {
        if let Some(listener) = &self.listener {
            // Set non-blocking mode
            let _ = listener.set_nonblocking(true);

            // Try to accept a connection
            match listener.accept() {
                Ok((stream, _)) => {
                    let handler = self.handler.clone();
                    // Spawn a thread to handle this client
                    std::thread::spawn(move || {
                        if let Err(e) = Self::handle_client_static(stream, handler) {
                            eprintln!("Error handling client: {e}");
                        }
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No connections pending, this is normal
                }
                Err(e) => {
                    eprintln!("Error accepting connection: {e}");
                }
            }
        }
    }

    /// Spawn a thread to handle connections
    pub fn spawn_handler(mut self) -> std::io::Result<thread::JoinHandle<()>> {
        self.start()?;

        let handler = self.handler.clone();
        let listener = self.listener.take().unwrap();

        Ok(thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let handler = handler.clone();
                        thread::spawn(move || {
                            if let Err(e) = Self::handle_client_static(stream, handler) {
                                eprintln!("Error handling client: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("Error accepting connection: {e}");
                    }
                }
            }
        }))
    }

    /// Handle a client connection
    fn handle_client(&self, stream: UnixStream) -> std::io::Result<()> {
        Self::handle_client_static(stream, self.handler.clone())
    }

    fn handle_client_static(
        mut stream: UnixStream,
        handler: Arc<Mutex<dyn TestCommandHandler + Send>>,
    ) -> std::io::Result<()> {
        let reader = BufReader::new(stream.try_clone()?);

        for line in reader.lines() {
            let line = line?;

            // Parse command
            let command: TestCommand = match serde_json::from_str(&line) {
                Ok(cmd) => cmd,
                Err(e) => {
                    let response = TestResponse::Error {
                        message: format!("Failed to parse command: {e}"),
                    };
                    writeln!(stream, "{}", serde_json::to_string(&response)?)?;
                    continue;
                }
            };

            // Handle command
            let response = handler.lock().unwrap().handle_command(command);

            // Send response
            writeln!(stream, "{}", serde_json::to_string(&response)?)?;
            stream.flush()?;
        }

        Ok(())
    }
}

/// Test IPC client for sending commands to the compositor
pub struct TestIpcClient {
    stream: UnixStream,
}

impl TestIpcClient {
    /// Connect to the test IPC server
    pub fn connect(socket_path: &PathBuf) -> std::io::Result<Self> {
        let stream = UnixStream::connect(socket_path)?;
        Ok(Self { stream })
    }

    /// Send a command and wait for response
    pub fn send_command(&mut self, command: TestCommand) -> std::io::Result<TestResponse> {
        // Send command
        writeln!(self.stream, "{}", serde_json::to_string(&command)?)?;
        self.stream.flush()?;

        // Read response
        let mut reader = BufReader::new(self.stream.try_clone()?);
        let mut line = String::new();
        reader.read_line(&mut line)?;

        // Parse response
        serde_json::from_str(&line)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Get the current ASCII state
    pub fn get_state(&mut self) -> std::io::Result<String> {
        match self.send_command(TestCommand::GetState)? {
            TestResponse::State { ascii } => Ok(ascii),
            TestResponse::Error { message } => {
                Err(std::io::Error::new(std::io::ErrorKind::Other, message))
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            )),
        }
    }

    /// Create a window
    pub fn create_window(&mut self, width: i32, height: i32) -> std::io::Result<u64> {
        match self.send_command(TestCommand::CreateWindow { width, height })? {
            TestResponse::WindowCreated { id } => Ok(id),
            TestResponse::Error { message } => {
                Err(std::io::Error::new(std::io::ErrorKind::Other, message))
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            )),
        }
    }

    /// Focus a window
    pub fn focus_window(&mut self, id: u64) -> std::io::Result<()> {
        match self.send_command(TestCommand::FocusWindow { id })? {
            TestResponse::Success { .. } => Ok(()),
            TestResponse::Error { message } => {
                Err(std::io::Error::new(std::io::ErrorKind::Other, message))
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            )),
        }
    }
}
