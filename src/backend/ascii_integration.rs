//! Integration of ASCII backend with the compositor state

use super::ascii::{AsciiBackend, AsciiCommand, AsciiWindow};
use super::test_harness::TestCommand;
use crate::state::StilchState;
use crate::window::{WindowId, WindowLayout};
use crate::workspace::WorkspaceId;
use smithay::utils::Rectangle;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// ASCII backend integration for StilchState
pub struct AsciiIntegration {
    backend: Arc<Mutex<AsciiBackend>>,
    command_rx: Receiver<TestCommand>,
    response_tx: Sender<String>,
}

impl AsciiIntegration {
    /// Create a new ASCII integration
    pub fn new(backend: Arc<Mutex<AsciiBackend>>) -> (Self, Sender<TestCommand>, Receiver<String>) {
        let (command_tx, command_rx) = channel();
        let (response_tx, response_rx) = channel();

        (
            Self {
                backend,
                command_rx,
                response_tx,
            },
            command_tx,
            response_rx,
        )
    }

    /// Process commands from the test harness
    pub fn process_commands<BackendData>(&mut self, state: &mut StilchState<BackendData>)
    where
        BackendData: crate::state::Backend + 'static,
    {
        while let Ok(command) = self.command_rx.try_recv() {
            match command {
                TestCommand::Ascii(cmd) => {
                    self.handle_ascii_command(state, cmd);
                }
                TestCommand::WaitFor(_condition) => {
                    // TODO: Implement wait conditions
                }
                TestCommand::Shutdown => {
                    // TODO: Trigger compositor shutdown
                }
            }
        }
    }

    /// Handle an ASCII command
    fn handle_ascii_command<BackendData>(
        &mut self,
        _state: &mut StilchState<BackendData>,
        cmd: AsciiCommand,
    ) where
        BackendData: crate::state::Backend + 'static,
    {
        match cmd {
            AsciiCommand::CreateWindow { id, size } => {
                // This would need to integrate with your actual window creation
                // For now, just update the ASCII backend
                let window = AsciiWindow {
                    id,
                    bounds: Rectangle::new((0, 0).into(), size),
                    focused: false,
                    floating: false,
                    fullscreen: false,
                    urgent: false,
                    tab_info: None,
                };
                self.backend.lock().unwrap().update_window(window);
            }
            AsciiCommand::DestroyWindow { id } => {
                self.backend.lock().unwrap().remove_window(id);
            }
            AsciiCommand::FocusWindow { id } => {
                self.backend.lock().unwrap().set_focus(Some(id));
            }
            AsciiCommand::GetState => {
                let ascii = self.backend.lock().unwrap().render();
                let _ = self.response_tx.send(ascii);
            }
            AsciiCommand::GetWindows => {
                let windows = self.backend.lock().unwrap().get_windows();
                let response = format!("{:?}", windows);
                let _ = self.response_tx.send(response);
            }
            _ => {}
        }
    }

    /// Update the ASCII backend with current compositor state
    pub fn sync_state<BackendData>(&mut self, state: &StilchState<BackendData>)
    where
        BackendData: crate::state::Backend + 'static,
    {
        let mut backend = self.backend.lock().unwrap();

        // Clear existing windows
        match backend.windows.lock() {
            Ok(mut windows) => windows.clear(),
            Err(e) => tracing::error!("Windows lock poisoned: {e}"),
        }

        // Get current workspace
        if let Some(virtual_output) = state
            .virtual_output_manager
            .list_virtual_outputs()
            .first()
            .and_then(|id| state.virtual_output_manager.get(*id))
        {
            let workspace_idx = virtual_output.active_workspace().unwrap_or(0);
            let workspace_id = WorkspaceId::new(workspace_idx as u8);

            if let Some(workspace) = state.workspace_manager.get(workspace_id) {
                // Get all windows in the workspace
                for &window_id in &workspace.windows {
                    if let Some(managed_window) = state.window_registry().get(window_id) {
                        // Get window geometry from layout
                        let bounds = match &managed_window.layout {
                            WindowLayout::Tiled { geometry, .. } => *geometry,
                            WindowLayout::Floating { geometry } => *geometry,
                            WindowLayout::Fullscreen { geometry, .. } => *geometry,
                        };

                        let ascii_window = AsciiWindow {
                            id: window_id,
                            bounds,
                            focused: workspace.focused_window == Some(window_id),
                            floating: matches!(
                                managed_window.layout,
                                WindowLayout::Floating { .. }
                            ),
                            fullscreen: matches!(
                                managed_window.layout,
                                WindowLayout::Fullscreen { .. }
                            ),
                            urgent: false,
                            tab_info: None,
                        };

                        backend.update_window(ascii_window);
                    }
                }

                // Set focus
                backend.set_focus(workspace.focused_window);
            }
        }
    }
}

/// Run the ASCII backend in test mode
pub fn run_ascii_test_mode() {
    println!("Starting ASCII test mode...");

    // Create the backend
    let backend = Arc::new(Mutex::new(AsciiBackend::default()));

    // Example: Create some test windows
    let test_windows = vec![
        AsciiWindow {
            id: WindowId::new(1),
            bounds: Rectangle::new((0, 0).into(), (1280, 2160).into()),
            focused: true,
            floating: false,
            fullscreen: false,
            urgent: false,
            tab_info: None,
        },
        AsciiWindow {
            id: WindowId::new(2),
            bounds: Rectangle::new((1280, 0).into(), (1280, 2160).into()),
            focused: false,
            floating: false,
            fullscreen: false,
            urgent: false,
            tab_info: None,
        },
        AsciiWindow {
            id: WindowId::new(3),
            bounds: Rectangle::new((2560, 0).into(), (1280, 2160).into()),
            focused: false,
            floating: false,
            fullscreen: false,
            urgent: false,
            tab_info: None,
        },
    ];

    // Add windows to backend
    {
        let mut backend = backend
            .lock()
            .expect("Test backend lock should not be poisoned");
        for window in test_windows {
            backend.update_window(window);
        }
        backend.set_focus(Some(WindowId::new(1)));
    }

    // Render and display
    println!("\n=== 3 Windows Tiled Vertically (Window 1 Focused) ===\n");
    println!(
        "{}",
        backend
            .lock()
            .expect("Test backend lock should not be poisoned")
            .render()
    );

    // Change focus
    {
        let mut backend = backend
            .lock()
            .expect("Test backend lock should not be poisoned");
        backend.set_focus(Some(WindowId::new(2)));
    }

    println!("\n=== Same Layout with Window 2 Focused ===\n");
    println!(
        "{}",
        backend
            .lock()
            .expect("Test backend lock should not be poisoned")
            .render()
    );

    // Change to horizontal layout
    {
        let mut backend = backend
            .lock()
            .expect("Test backend lock should not be poisoned");
        match backend.windows.lock() {
            Ok(mut windows) => windows.clear(),
            Err(e) => tracing::error!("Windows lock poisoned: {e}"),
        }

        let horizontal_windows = vec![
            AsciiWindow {
                id: WindowId::new(1),
                bounds: Rectangle::new((0, 0).into(), (3840, 720).into()),
                focused: false,
                floating: false,
                fullscreen: false,
                urgent: false,
                tab_info: None,
            },
            AsciiWindow {
                id: WindowId::new(2),
                bounds: Rectangle::new((0, 720).into(), (3840, 720).into()),
                focused: true,
                floating: false,
                fullscreen: false,
                urgent: false,
                tab_info: None,
            },
            AsciiWindow {
                id: WindowId::new(3),
                bounds: Rectangle::new((0, 1440).into(), (3840, 720).into()),
                focused: false,
                floating: false,
                fullscreen: false,
                urgent: false,
                tab_info: None,
            },
        ];

        for window in horizontal_windows {
            backend.update_window(window);
        }
        backend.set_focus(Some(WindowId::new(2)));
    }

    println!("\n=== Horizontal Layout (Window 2 Focused) ===\n");
    println!(
        "{}",
        backend
            .lock()
            .expect("Test backend lock should not be poisoned")
            .render()
    );

    // Fullscreen example
    {
        let mut backend = backend
            .lock()
            .expect("Test backend lock should not be poisoned");
        match backend.windows.lock() {
            Ok(mut windows) => windows.clear(),
            Err(e) => tracing::error!("Windows lock poisoned: {e}"),
        }

        let fullscreen_window = AsciiWindow {
            id: WindowId::new(1),
            bounds: Rectangle::new((0, 0).into(), (3840, 2160).into()),
            focused: true,
            floating: false,
            fullscreen: true,
            urgent: false,
            tab_info: None,
        };

        backend.update_window(fullscreen_window);
        backend.set_focus(Some(WindowId::new(1)));
    }

    println!("\n=== Fullscreen Window ===\n");
    println!(
        "{}",
        backend
            .lock()
            .expect("Test backend lock should not be poisoned")
            .render()
    );

    // Floating window example
    {
        let mut backend = backend
            .lock()
            .expect("Test backend lock should not be poisoned");
        match backend.windows.lock() {
            Ok(mut windows) => windows.clear(),
            Err(e) => tracing::error!("Windows lock poisoned: {e}"),
        }

        // Tiled background windows
        backend.update_window(AsciiWindow {
            id: WindowId::new(1),
            bounds: Rectangle::new((0, 0).into(), (1920, 2160).into()),
            focused: false,
            floating: false,
            fullscreen: false,
            urgent: false,
            tab_info: None,
        });

        backend.update_window(AsciiWindow {
            id: WindowId::new(2),
            bounds: Rectangle::new((1920, 0).into(), (1920, 2160).into()),
            focused: false,
            floating: false,
            fullscreen: false,
            urgent: false,
            tab_info: None,
        });

        // Floating window on top
        backend.update_window(AsciiWindow {
            id: WindowId::new(3),
            bounds: Rectangle::new((960, 540).into(), (1920, 1080).into()),
            focused: true,
            floating: true,
            fullscreen: false,
            urgent: false,
            tab_info: None,
        });

        backend.set_focus(Some(WindowId::new(3)));
    }

    println!("\n=== Floating Window Over Tiled Windows ===\n");
    println!(
        "{}",
        backend
            .lock()
            .expect("Test backend lock should not be poisoned")
            .render()
    );
}
