//! Test IPC server for debugging and testing

use smithay::reexports::calloop::{
    generic::Generic, EventLoop, Interest, Mode as CallMode, PostAction,
};
use std::collections::HashMap;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use tracing::{error, info};

use crate::{
    state::{StilchState, Backend as BackendTrait},
    test_ipc::{TestCommand, TestResponse, WindowInfo, WorkspaceInfo},
    window::WindowLayout,
    workspace::WorkspaceId,
};

/// Per-connection state for IPC clients
struct ClientConnection {
    stream: UnixStream,
    buffer: String,
}

impl ClientConnection {
    fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            buffer: String::new(),
        }
    }
}

/// Initialize the test IPC server for any backend
pub fn init_test_ipc_server<BackendData: BackendTrait + 'static>(
    _state: &mut StilchState<BackendData>,
    event_loop: &mut EventLoop<'_, StilchState<BackendData>>,
    socket_path: String,
) -> std::io::Result<()> {
    let socket_path_buf = PathBuf::from(&socket_path);

    // Remove old socket if it exists
    let _ = std::fs::remove_file(&socket_path_buf);

    let listener = UnixListener::bind(&socket_path_buf)?;
    listener.set_nonblocking(true)?;

    info!("Test IPC server listening on {:?}", socket_path);

    // Track active connections
    let next_conn_id = Arc::new(StdMutex::new(0usize));
    let active_connections: Arc<StdMutex<HashMap<usize, ClientConnection>>> =
        Arc::new(StdMutex::new(HashMap::new()));

    let next_id_for_listener = next_conn_id.clone();
    let connections_for_listener = active_connections.clone();
    let handle_for_listener = event_loop.handle();

    // Add listener source for accepting new connections
    event_loop
        .handle()
        .insert_source(
            Generic::new(listener, Interest::READ, CallMode::Level),
            move |_, listener, _state: &mut StilchState<BackendData>| {
                // Try to accept new connections
                while let Ok((stream, _)) = listener.accept() {
                    info!("Test client connected");
                    stream.set_nonblocking(true).unwrap();

                    // Get a unique ID for this connection
                    let conn_id = {
                        let mut id = next_id_for_listener.lock().unwrap();
                        let current = *id;
                        *id += 1;
                        current
                    };

                    // Create the connection object
                    let connection = ClientConnection::new(stream.try_clone().unwrap());
                    connections_for_listener
                        .lock()
                        .unwrap()
                        .insert(conn_id, connection);

                    // Register this stream as an event source
                    let conn_for_source = active_connections.clone();
                    handle_for_listener
                        .insert_source(
                            Generic::new(stream, Interest::READ, CallMode::Level),
                            move |event, _stream, state: &mut StilchState<BackendData>| {
                                if event.readable {
                                    let mut connections = conn_for_source.lock().unwrap();
                                    if let Some(connection) = connections.get_mut(&conn_id) {
                                        if let Err(e) = process_client_data(connection, state) {
                                            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                                                info!("Test client {} disconnected", conn_id);
                                            } else {
                                                error!(
                                                    "Error processing client {}: {}",
                                                    conn_id, e
                                                );
                                            }
                                            connections.remove(&conn_id);
                                            return Ok(PostAction::Remove);
                                        }
                                    }
                                }
                                Ok(PostAction::Continue)
                            },
                        )
                        .unwrap();
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to insert listener source: {e}"),
            )
        })?;

    Ok(())
}

/// Process data from a test IPC client
fn process_client_data<BackendData: BackendTrait + 'static>(
    connection: &mut ClientConnection,
    state: &mut StilchState<BackendData>,
) -> std::io::Result<()> {
    use std::io::{Read, Write};

    // Read available data into buffer
    let mut temp_buffer = [0u8; 4096];
    let mut read_something = false;

    loop {
        match connection.stream.read(&mut temp_buffer) {
            Ok(0) if !read_something => {
                // Connection closed
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Connection closed",
                ));
            }
            Ok(0) => {
                // No more data right now, but we already read something
                break;
            }
            Ok(n) => {
                // Add data to buffer
                connection
                    .buffer
                    .push_str(&String::from_utf8_lossy(&temp_buffer[..n]));
                read_something = true;

                // Check if we have complete command(s) to process
                if connection.buffer.contains('\n') {
                    break; // Process what we have
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No more data available right now
                break;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    // Process all complete lines in the buffer
    while let Some(newline_pos) = connection.buffer.find('\n') {
        let line = connection.buffer.drain(..=newline_pos).collect::<String>();
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        // Parse and process command
        let command: TestCommand = match serde_json::from_str(line) {
            Ok(cmd) => cmd,
            Err(e) => {
                eprintln!("Failed to parse command: {e}");
                let error_response = TestResponse::Error {
                    message: format!("Failed to parse command: {e}"),
                };
                let response_json = serde_json::to_string(&error_response).unwrap();
                writeln!(connection.stream, "{}", response_json)?;
                connection.stream.flush()?;
                continue;
            }
        };

        // Process command and generate response
        let response = match command {
            TestCommand::GetWindows => {
                // Get REAL windows from the registry
                info!(
                    "GetWindows: Registry has {} windows",
                    state.window_manager.registry().len()
                );
                let windows: Vec<_> = state
                    .window_manager
                    .registry()
                    .windows()
                    .map(|managed_window| {
                        let window_id = managed_window.id.get();
                        let geometry = match &managed_window.layout {
                            WindowLayout::Tiled { geometry, .. } => geometry,
                            WindowLayout::Floating { geometry } => geometry,
                            WindowLayout::Fullscreen { geometry, .. } => geometry,
                        };

                        let is_focused = state
                            .focused_window()
                            .map(|w| {
                                state.window_manager.registry().find_by_element(&w)
                                    == Some(managed_window.id)
                            })
                            .unwrap_or(false);

                        let is_floating =
                            matches!(&managed_window.layout, WindowLayout::Floating { .. });
                        let is_fullscreen =
                            matches!(&managed_window.layout, WindowLayout::Fullscreen { .. });

                        WindowInfo {
                            id: window_id,
                            x: geometry.loc.x,
                            y: geometry.loc.y,
                            width: geometry.size.w,
                            height: geometry.size.h,
                            workspace: managed_window
                                .workspace
                                .display_name()
                                .parse()
                                .unwrap_or(managed_window.workspace.get() as usize + 1),
                            focused: is_focused,
                            floating: is_floating,
                            fullscreen: is_fullscreen,
                            title: None,   // Title not available yet
                            visible: true, // All returned windows are visible
                        }
                    })
                    .collect();

                TestResponse::Windows { windows }
            }

            TestCommand::GetWorkspaces => {
                let workspaces: Vec<_> = (0..10)
                    .map(|i| {
                        let (visible, output, window_count) = if let Some(workspace) =
                            state.workspace_manager.get(WorkspaceId::new(i))
                        {
                            (
                                workspace.output().is_some(),
                                workspace.output().map(|vo| vo.get() as usize),
                                workspace.windows.len(),
                            )
                        } else {
                            (false, None, 0)
                        };

                        // Check if this workspace is focused
                        let focused = state
                            .virtual_output_manager
                            .all_virtual_outputs()
                            .any(|vo| vo.active_workspace() == Some(i as usize));

                        let workspace_id = WorkspaceId::new(i);
                        WorkspaceInfo {
                            id: workspace_id
                                .display_name()
                                .parse()
                                .unwrap_or(i as usize + 1),
                            name: workspace_id.display_name(),
                            visible,
                            output: output.map(|o| o.to_string()),
                            window_count,
                            focused,
                        }
                    })
                    .collect();

                TestResponse::Workspaces { workspaces }
            }

            TestCommand::SwitchWorkspace { index } => {
                // Switch to the specified workspace
                if index < 10 {
                    // Find the first virtual output
                    let vo_id = state
                        .virtual_output_manager
                        .all_virtual_outputs()
                        .next()
                        .map(|vo| vo.id());

                    if let Some(vo_id) = vo_id {
                        let workspace_id = WorkspaceId::new(index as u8);
                        state.switch_workspace(vo_id, workspace_id);

                        TestResponse::Success {
                            message: format!("Switched to workspace {index}"),
                        }
                    } else {
                        TestResponse::Error {
                            message: "No virtual output available".to_string(),
                        }
                    }
                } else {
                    TestResponse::Error {
                        message: format!("Invalid workspace index: {index}"),
                    }
                }
            }

            _ => {
                info!("Unhandled test command: {:?}", command);
                TestResponse::Error {
                    message: "Command not implemented for this backend".to_string(),
                }
            }
        };

        // Send response
        let response_json = serde_json::to_string(&response).unwrap();
        writeln!(connection.stream, "{}", response_json)?;
        connection.stream.flush()?;
    }

    Ok(())
}
