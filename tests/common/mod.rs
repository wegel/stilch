//! Common testing utilities for stilch integration tests

use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

/// Test environment configuration
pub struct TestEnv {
    pub test_name: String,
    pub test_socket: String,
    pub wayland_display: String,
    pub compositor_process: Option<Child>,
}

impl TestEnv {
    /// Create a new test environment with unique names
    pub fn new(test_name: &str) -> Self {
        Self {
            test_name: test_name.to_string(),
            test_socket: format!("/tmp/stilch-test-{test_name}.sock"),
            wayland_display: format!("wayland-test-{test_name}"),
            compositor_process: None,
        }
    }

    /// Clean up any existing processes and sockets
    pub fn cleanup(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Clean up old sockets (don't kill processes - let Drop handle that)
        let _ = fs::remove_file(&self.test_socket);
        let _ = fs::remove_file(format!("/tmp/stilch-ipc-{}.sock", self.test_name));
        let _ = fs::remove_file(format!("/run/user/1000/{}", self.wayland_display));

        Ok(())
    }

    /// Start the compositor with given arguments
    pub fn start_compositor(&mut self, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        // Create a unique IPC socket path for this test
        let ipc_socket = format!("/tmp/stilch-ipc-{}.sock", self.test_name);

        let env_vars = [("STILCH_TEST_SOCKET", self.test_socket.as_str()),
            ("STILCH_IPC_SOCKET", ipc_socket.as_str()),
            ("STILCH_WAYLAND_SOCKET", self.wayland_display.as_str()),
            ("WAYLAND_DISPLAY", self.wayland_display.as_str()),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ("RUST_LOG", "warn")];

        println!("Starting compositor for test '{}'...", self.test_name);
        let child = Command::new("target/debug/stilch")
            .args(args)
            .envs(env_vars.iter().cloned())
            .spawn()?;

        self.compositor_process = Some(child);

        // Wait for compositor to create sockets
        self.wait_for_sockets()?;

        Ok(())
    }

    /// Start the compositor with multiple outputs
    pub fn start_compositor_multi_output(
        &mut self,
        output_count: usize,
        output_width: u32,
        output_height: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Build args for multiple outputs
        let mut args = vec!["--test"];

        // Set the logical size for the default output
        args.push("--logical-size");
        let logical_size = format!("{output_width}x{output_height}");
        args.push(Box::leak(logical_size.into_boxed_str()));

        // Add ASCII output definitions for additional outputs (start from 1 since we have a default)
        for i in 1..output_count {
            args.push("--ascii-output");
            let output_def = format!(
                "{}x{}+{}+0",
                output_width,
                output_height,
                i as u32 * output_width
            );
            args.push(Box::leak(output_def.into_boxed_str()));
        }

        self.start_compositor(&args)
    }

    /// Wait for compositor sockets to be created
    fn wait_for_sockets(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Waiting for compositor sockets...");
        let wayland_socket_path = format!("/run/user/1000/{}", self.wayland_display);

        for i in 0..50 {
            // 5 seconds max
            if std::path::Path::new(&self.test_socket).exists()
                && std::path::Path::new(&wayland_socket_path).exists()
            {
                println!("✓ Sockets created");
                thread::sleep(Duration::from_millis(100));
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));

            if i == 49 {
                return Err("Compositor failed to create sockets".into());
            }
        }
        Ok(())
    }

    /// Start a simple window
    pub fn start_window(
        &self,
        title: &str,
        color: Option<&str>,
    ) -> Result<Child, Box<dyn std::error::Error>> {
        let mut cmd = Command::new("target/debug/simple_window");
        cmd.arg(title);
        if let Some(c) = color {
            cmd.arg(c);
        }
        cmd.env("WAYLAND_DISPLAY", &self.wayland_display)
            .env("XDG_RUNTIME_DIR", "/run/user/1000")
            .spawn()
            .map_err(|e| e.into())
    }

    /// Get environment variables for running clients
    pub fn client_env(&self) -> Vec<(&str, &str)> {
        vec![
            ("WAYLAND_DISPLAY", &self.wayland_display),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
        ]
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        // Kill compositor process if running
        if let Some(mut child) = self.compositor_process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        // Clean up sockets
        let _ = self.cleanup();
    }
}

/// IPC client for test communication
pub struct TestClient {
    socket_path: String,
}

impl TestClient {
    pub fn new(socket_path: &str) -> Self {
        Self {
            socket_path: socket_path.to_string(),
        }
    }

    /// Send a command and get response
    pub fn send_command(&self, command: &Value) -> Result<Value, Box<dyn std::error::Error>> {
        let mut sock = UnixStream::connect(&self.socket_path)?;
        let command_str = serde_json::to_string(command)?;
        sock.write_all(command_str.as_bytes())?;
        sock.write_all(b"\n")?;

        let mut buffer = vec![0u8; 65536];
        let n = sock.read(&mut buffer)?;
        let response = String::from_utf8_lossy(&buffer[..n]);

        serde_json::from_str(&response)
            .map_err(|e| format!("Failed to parse response: {e} (response: {response})").into())
    }

    /// Get list of windows
    pub fn get_windows(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let response = self.send_command(&serde_json::json!({"type": "GetWindows"}))?;
        Ok(response
            .get("windows")
            .and_then(|w| w.as_array())
            .cloned()
            .unwrap_or_default())
    }

    /// Get focused window ID
    pub fn get_focused_window(&self) -> Result<Option<u64>, Box<dyn std::error::Error>> {
        let response = self.send_command(&serde_json::json!({"type": "GetFocusedWindow"}))?;
        Ok(response.get("id").and_then(|id| id.as_u64()))
    }

    /// Focus a window by ID
    pub fn focus_window(&self, id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let response = self.send_command(&serde_json::json!({
            "type": "FocusWindow",
            "id": id
        }))?;

        if response.get("type").and_then(|t| t.as_str()) == Some("Error") {
            return Err(response
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error")
                .into());
        }

        Ok(())
    }

    /// Get ASCII snapshot
    pub fn get_ascii_snapshot(
        &self,
        show_ids: bool,
        show_focus: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let response = self.send_command(&serde_json::json!({
            "type": "GetAsciiSnapshot",
            "show_ids": show_ids,
            "show_focus": show_focus
        }))?;

        Ok(response
            .get("snapshot")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string())
    }

    /// Wait for a specific window count
    pub fn wait_for_window_count(
        &self,
        expected: usize,
        context: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for i in 0..50 {
            let windows = self.get_windows()?;
            if windows.len() == expected {
                println!("✓ {expected} window(s) {context}");
                return Ok(());
            }

            if i == 49 {
                return Err(format!("Failed to get {expected} windows {context}").into());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    /// Wait for focus on a specific window
    pub fn wait_for_focus(
        &self,
        window_id: u64,
        context: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for i in 0..50 {
            if let Some(focused) = self.get_focused_window()? {
                if focused == window_id {
                    println!("✓ Window {window_id} focused {context}");
                    return Ok(());
                }
            }

            if i == 49 {
                return Err(
                    format!("Failed to get focus on window {window_id} {context}").into(),
                );
            }
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    /// Get outputs from compositor
    pub fn get_outputs(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let response = self.send_command(&serde_json::json!({"type": "GetOutputs"}))?;
        Ok(response
            .get("outputs")
            .and_then(|o| o.as_array())
            .cloned()
            .unwrap_or_default())
    }

    /// Get workspaces from compositor
    pub fn get_workspaces(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let response = self.send_command(&serde_json::json!({"type": "GetWorkspaces"}))?;
        Ok(response
            .get("workspaces")
            .and_then(|w| w.as_array())
            .cloned()
            .unwrap_or_default())
    }

    /// Move workspace to output in direction
    pub fn move_workspace_to_output(
        &self,
        direction: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let response = self.send_command(&serde_json::json!({
            "type": "MoveWorkspaceToOutput",
            "direction": direction
        }))?;

        if response.get("type").and_then(|t| t.as_str()) == Some("Error") {
            return Err(response
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error")
                .into());
        }

        Ok(())
    }

    /// Switch to workspace by index
    pub fn switch_workspace(&self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
        let response = self.send_command(&serde_json::json!({
            "type": "SwitchWorkspace",
            "index": index
        }))?;

        if response.get("type").and_then(|t| t.as_str()) == Some("Error") {
            return Err(response
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error")
                .into());
        }

        Ok(())
    }

    /// Send a simple string command (convenience method)
    pub fn send_simple_command(&self, cmd: &str) -> Result<(), Box<dyn std::error::Error>> {
        let json_cmd = match cmd {
            "LayoutTabbed" => serde_json::json!({"type": "SetLayout", "mode": "tabbed"}),
            "FocusLeft" => serde_json::json!({"type": "MoveFocus", "direction": "left"}),
            "FocusRight" => serde_json::json!({"type": "MoveFocus", "direction": "right"}),
            "Fullscreen" => serde_json::json!({"type": "Fullscreen"}),
            "FullscreenContainer" => serde_json::json!({"type": "FullscreenContainer"}),
            "FullscreenVirtualOutput" => serde_json::json!({"type": "FullscreenVirtualOutput"}),
            "FullscreenPhysicalOutput" => serde_json::json!({"type": "FullscreenPhysicalOutput"}),
            _ => return Err(format!("Unknown command: {cmd}").into()),
        };

        let response = self.send_command(&json_cmd)?;

        if response.get("type").and_then(|t| t.as_str()) == Some("Error") {
            return Err(response
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error")
                .into());
        }

        Ok(())
    }
}

/// Helper to verify window geometry
pub fn verify_window_geometry(
    window: &Value,
    expected_x: i32,
    expected_y: i32,
    expected_width: i32,
    expected_height: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    let x = window.get("x").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
    let y = window.get("y").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
    let width = window.get("width").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
    let height = window.get("height").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;

    if x != expected_x || y != expected_y || width != expected_width || height != expected_height {
        return Err(format!(
            "Window geometry mismatch: got ({x}, {y}, {width}x{height}), expected ({expected_x}, {expected_y}, {expected_width}x{expected_height})"
        )
        .into());
    }

    Ok(())
}

/// Helper to run a test with timeout
pub fn run_with_timeout<F>(
    test_fn: F,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    F: FnOnce() -> Result<(), Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();

    let handle = thread::spawn(move || {
        let result = test_fn();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(timeout) {
        Ok(result) => {
            let _ = handle.join();
            result
        }
        Err(_) => {
            // Test timed out
            Err("Test timed out".into())
        }
    }
}
