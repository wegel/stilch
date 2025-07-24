//! IPC Event Handler
//!
//! This module handles events and updates the IPC server state accordingly.

use crate::{
    event::{Event, EventHandler, WindowEvent, WorkspaceEvent},
    ipc::{IpcServer, WorkspaceInfo},
    virtual_output::VirtualOutputId,
};
use std::sync::Arc;
use tracing::debug;

/// IPC event handler that updates IPC clients when state changes
pub struct IpcEventHandler {
    ipc_server: Option<Arc<IpcServer>>,
}

impl IpcEventHandler {
    /// Create a new IPC event handler
    pub fn new(ipc_server: Option<Arc<IpcServer>>) -> Self {
        Self { ipc_server }
    }

    /// Send workspace update to IPC clients
    fn send_workspace_update(
        &self,
        virtual_output_id: VirtualOutputId,
        workspaces: Vec<WorkspaceInfo>,
    ) {
        if let Some(ipc_server) = &self.ipc_server {
            ipc_server.send_workspace_update(virtual_output_id, workspaces);
        }
    }
}

impl EventHandler for IpcEventHandler {
    fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Window(window_event) => match window_event {
                WindowEvent::Created { .. }
                | WindowEvent::Destroyed { .. }
                | WindowEvent::WorkspaceChanged { .. } => {
                    // The workspace state will be updated via WorkspaceEvent::LayoutChanged
                    // which should be emitted after window changes
                    debug!("Window event received, waiting for workspace update");
                }
                _ => {}
            },
            Event::Workspace(workspace_event) => match workspace_event {
                WorkspaceEvent::Switched { virtual_output, .. } => {
                    debug!(
                        "Workspace switched on output {}, updating IPC",
                        virtual_output
                    );
                    // The caller should emit a StateUpdate event with the new workspace info
                }
                WorkspaceEvent::LayoutChanged { .. } => {
                    debug!("Workspace layout changed, waiting for state update");
                }
            },
            Event::Ipc(ipc_event) => {
                // Handle IPC-specific events if needed
                debug!("IPC event: {:?}", ipc_event);
            }
            Event::StateUpdate(state_update) => {
                // This is the new event type that carries the actual state
                self.send_workspace_update(
                    state_update.virtual_output,
                    state_update.workspaces.clone(),
                );
            }
            _ => {}
        }
    }
}

/// State update event that carries the actual workspace state
#[derive(Debug, Clone)]
pub struct StateUpdateEvent {
    pub virtual_output: VirtualOutputId,
    pub workspaces: Vec<WorkspaceInfo>,
}
