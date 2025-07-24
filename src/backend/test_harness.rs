//! Test harness for ASCII backend testing

use super::ascii::{AsciiBackend, AsciiCommand, AsciiWindow};
use crate::window::WindowId;
use crate::workspace::WorkspaceId;
use smithay::utils::Rectangle;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Test compositor interface
pub struct TestCompositor {
    /// ASCII backend
    backend: Arc<Mutex<AsciiBackend>>,

    /// Command sender
    command_tx: Sender<TestCommand>,

    /// Response receiver
    response_rx: Receiver<String>,
}

/// Commands that can be sent to the test compositor
#[derive(Debug, Clone)]
pub enum TestCommand {
    /// ASCII backend command
    Ascii(AsciiCommand),

    /// Wait for a condition
    WaitFor(WaitCondition),

    /// Shutdown the compositor
    Shutdown,
}

/// Conditions to wait for
#[derive(Debug, Clone)]
pub enum WaitCondition {
    /// Wait for a specific number of windows
    WindowCount(usize),

    /// Wait for a window to be focused
    WindowFocused(WindowId),

    /// Wait for a specific workspace to be active
    WorkspaceActive(WorkspaceId),
}

impl TestCompositor {
    /// Create a new test compositor
    pub fn new() -> (Self, Sender<String>) {
        let backend = Arc::new(Mutex::new(AsciiBackend::default()));
        let (command_tx, _command_rx) = channel();
        let (response_tx, response_rx) = channel();

        (
            Self {
                backend,
                command_tx,
                response_rx,
            },
            response_tx,
        )
    }

    /// Send a command string
    pub fn send(&self, command: &str) -> Result<(), String> {
        if let Some(cmd) = AsciiCommand::parse(command) {
            self.command_tx
                .send(TestCommand::Ascii(cmd))
                .map_err(|e| format!("Failed to send command: {e}"))
        } else {
            Err(format!("Failed to parse command: {command}"))
        }
    }

    /// Get the current ASCII state
    pub fn get_ascii_state(&self) -> String {
        self.backend.lock().unwrap().render()
    }

    /// Wait for a response with timeout
    pub fn wait_response(&self, timeout: Duration) -> Result<String, String> {
        self.response_rx
            .recv_timeout(timeout)
            .map_err(|e| format!("Timeout waiting for response: {e}"))
    }

    /// Create a window for testing
    pub fn create_test_window(&self, id: u32, x: i32, y: i32, w: i32, h: i32) {
        let window = AsciiWindow {
            id: WindowId::new(id),
            bounds: Rectangle::new((x, y).into(), (w, h).into()),
            focused: false,
            floating: false,
            fullscreen: false,
            urgent: false,
            tab_info: None,
        };

        self.backend.lock().unwrap().update_window(window);
    }

    /// Focus a window
    pub fn focus_test_window(&self, id: u32) {
        self.backend
            .lock()
            .unwrap()
            .set_focus(Some(WindowId::new(id)));
    }

    /// Remove a window
    pub fn remove_test_window(&self, id: u32) {
        self.backend
            .lock()
            .unwrap()
            .remove_window(WindowId::new(id));
    }

    /// Shutdown the test compositor
    pub fn shutdown(&self) {
        let _ = self.command_tx.send(TestCommand::Shutdown);
    }
}

/// Example test implementations
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_compositor() {
        let (compositor, _response_tx) = TestCompositor::new();

        let ascii = compositor.get_ascii_state();

        // Should be all spaces (empty grid)
        assert!(ascii.chars().all(|c| c == ' ' || c == '\n'));

        compositor.shutdown();
    }

    #[test]
    fn test_single_window() {
        let (compositor, _response_tx) = TestCompositor::new();

        // Create a window that fills the left half
        compositor.create_test_window(1, 0, 0, 1920, 2160);

        let ascii = compositor.get_ascii_state();
        println!("Single window:\n{ascii}");

        // Should contain box drawing characters
        assert!(ascii.contains('┌'));
        assert!(ascii.contains('┐'));
        assert!(ascii.contains('└'));
        assert!(ascii.contains('┘'));

        compositor.shutdown();
    }

    #[test]
    fn test_vertical_tiling() {
        let (compositor, _response_tx) = TestCompositor::new();

        // Create two windows tiled vertically
        compositor.create_test_window(1, 0, 0, 3840, 1080);
        compositor.create_test_window(2, 0, 1080, 3840, 1080);

        let ascii = compositor.get_ascii_state();
        println!("Vertical tiling:\n{ascii}");

        // Both windows should be visible
        assert!(ascii.contains("1"));
        assert!(ascii.contains("2"));

        compositor.shutdown();
    }

    #[test]
    fn test_focused_window() {
        let (compositor, _response_tx) = TestCompositor::new();

        // Create two windows
        compositor.create_test_window(1, 0, 0, 1920, 2160);
        compositor.create_test_window(2, 1920, 0, 1920, 2160);

        // Focus the first window
        compositor.focus_test_window(1);

        let ascii = compositor.get_ascii_state();
        println!("Focused window:\n{ascii}");

        // Should show focus indicator
        assert!(ascii.contains("[F]"));
        // Should have double-line borders for focused window (left window)
        assert!(ascii.contains('╔'));
        assert!(ascii.contains('╚'));
        assert!(ascii.contains('║'));
        // The boundary between windows uses single lines
        assert!(ascii.contains('│'));

        compositor.shutdown();
    }

    #[test]
    fn test_three_windows_with_focus() {
        let (compositor, _response_tx) = TestCompositor::new();

        // Create three windows tiled vertically
        compositor.create_test_window(1, 0, 0, 3840, 720);
        compositor.create_test_window(2, 0, 720, 3840, 720);
        compositor.create_test_window(3, 0, 1440, 3840, 720);

        // Focus the middle window
        compositor.focus_test_window(2);

        let ascii = compositor.get_ascii_state();
        println!("Three windows with middle focused:\n{ascii}");

        // All three windows should be visible
        assert!(ascii.contains("1"));
        assert!(ascii.contains("2"));
        assert!(ascii.contains("3"));

        // Window 2 should have focus indicator
        assert!(ascii.contains("2 [F]"));

        compositor.shutdown();
    }

    #[test]
    fn test_fullscreen_window() {
        let (compositor, _response_tx) = TestCompositor::new();

        // Create a fullscreen window
        let window = AsciiWindow {
            id: WindowId::new(1),
            bounds: Rectangle::new((0, 0).into(), (3840, 2160).into()),
            focused: true,
            floating: false,
            fullscreen: true,
            urgent: false,
            tab_info: None,
        };

        {
            let mut backend = compositor.backend.lock().unwrap();
            backend.update_window(window.clone());
            backend.set_focus(Some(window.id));
        }

        let ascii = compositor.get_ascii_state();
        println!("Fullscreen window:\n{ascii}");

        // Should have heavy borders
        assert!(ascii.contains('┏'));
        assert!(ascii.contains('┓'));
        assert!(ascii.contains('┗'));
        assert!(ascii.contains('┛'));
        assert!(ascii.contains('━'));
        assert!(ascii.contains('┃'));

        // Should show fullscreen indicator
        assert!(ascii.contains("[FS]"));

        compositor.shutdown();
    }

    #[test]
    fn test_floating_window() {
        let (compositor, _response_tx) = TestCompositor::new();

        // Create a tiled window
        compositor.create_test_window(1, 0, 0, 3840, 2160);

        // Create a floating window on top
        let floating = AsciiWindow {
            id: WindowId::new(2),
            bounds: Rectangle::new((960, 540).into(), (1920, 1080).into()),
            focused: false,
            floating: true,
            fullscreen: false,
            urgent: false,
            tab_info: None,
        };

        compositor.backend.lock().unwrap().update_window(floating);

        let ascii = compositor.get_ascii_state();
        println!("Floating window over tiled:\n{ascii}");

        // Should have rounded corners for floating window
        assert!(ascii.contains('╭'));
        assert!(ascii.contains('╮'));
        assert!(ascii.contains('╰'));
        assert!(ascii.contains('╯'));

        compositor.shutdown();
    }
}
