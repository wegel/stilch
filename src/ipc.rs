use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};

use crate::virtual_output::VirtualOutputId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcMessage {
    WorkspaceUpdate {
        virtual_output: usize,
        workspaces: Vec<WorkspaceInfo>,
    },
    Subscribe {
        events: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: usize,
    pub active: bool,
    pub windows: usize,
    pub urgent: bool,
}

#[derive(Debug)]
pub struct IpcServer {
    socket_path: PathBuf,
    tx: broadcast::Sender<IpcMessage>,
    clients: Arc<RwLock<HashMap<usize, UnixStream>>>,
    next_client_id: Arc<RwLock<usize>>,
}

impl IpcServer {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Allow overriding the socket path via environment variable
        let socket_path = std::env::var("STILCH_IPC_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp/stilch-ipc.sock"));

        // Remove existing socket if it exists
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        let (tx, _) = broadcast::channel(100);

        Ok(Self {
            socket_path,
            tx,
            clients: Arc::new(RwLock::new(HashMap::new())),
            next_client_id: Arc::new(RwLock::new(0)),
        })
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = UnixListener::bind(&self.socket_path)?;
        info!("IPC server listening on {:?}", self.socket_path);

        let tx = self.tx.clone();
        let clients = self.clients.clone();
        let next_client_id = self.next_client_id.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let mut rx = tx.subscribe();
                        let clients = clients.clone();
                        let next_client_id = next_client_id.clone();

                        tokio::spawn(async move {
                            let client_id = {
                                let mut id = next_client_id.write().await;
                                let current = *id;
                                *id += 1;
                                current
                            };

                            info!("New IPC client connected: {client_id}");

                            clients.write().await.insert(client_id, stream);

                            // Handle client messages
                            loop {
                                match rx.recv().await {
                                    Ok(msg) => {
                                        let mut clients = clients.write().await;
                                        if let Some(stream) = clients.get_mut(&client_id) {
                                            let json = match serde_json::to_string(&msg) {
                                                Ok(j) => j,
                                                Err(e) => {
                                                    warn!("Failed to serialize IPC message: {e}");
                                                    continue;
                                                }
                                            };
                                            if let Err(e) = stream.write_all(json.as_bytes()).await
                                            {
                                                warn!(
                                                    "Failed to send to client {}: {}",
                                                    client_id, e
                                                );
                                                clients.remove(&client_id);
                                                break;
                                            }
                                            if let Err(e) = stream.write_all(b"\n").await {
                                                warn!(
                                                    "Failed to send newline to client {}: {}",
                                                    client_id, e
                                                );
                                                clients.remove(&client_id);
                                                break;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Broadcast receive error: {e}");
                                        break;
                                    }
                                }
                            }

                            info!("IPC client disconnected: {client_id}");
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept IPC connection: {e}");
                    }
                }
            }
        });

        Ok(())
    }

    pub fn send_workspace_update(
        &self,
        virtual_output_id: VirtualOutputId,
        workspaces: Vec<WorkspaceInfo>,
    ) {
        let msg = IpcMessage::WorkspaceUpdate {
            virtual_output: virtual_output_id.get() as usize,
            workspaces,
        };

        if let Err(e) = self.tx.send(msg) {
            warn!("Failed to broadcast workspace update: {e}");
        }
    }

    pub fn get_socket_path(&self) -> &PathBuf {
        &self.socket_path
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        if self.socket_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.socket_path) {
                warn!("Failed to remove IPC socket: {e}");
            }
        }
    }
}
