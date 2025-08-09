//! Command pattern implementation for stilch
//!
//! This module provides a command pattern implementation for all actions
//! in the compositor, enabling undo/redo and better action composition.

use crate::{
    state::{Backend, StilchState},
    virtual_output::VirtualOutputId,
    window::WindowId,
    workspace::WorkspaceId,
};
use smithay::utils::{Logical, Point};
use std::fmt::Debug;

/// Result of executing a command
pub type CommandResult = Result<(), CommandError>;

/// Error that can occur during command execution
#[derive(Debug, Clone)]
pub enum CommandError {
    /// Window not found
    WindowNotFound(WindowId),
    /// Workspace not found
    WorkspaceNotFound(WorkspaceId),
    /// Virtual output not found
    VirtualOutputNotFound(VirtualOutputId),
    /// Invalid operation
    InvalidOperation(String),
    /// Other error
    Other(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::WindowNotFound(id) => write!(f, "Window {} not found", id),
            CommandError::WorkspaceNotFound(id) => write!(f, "Workspace {} not found", id),
            CommandError::VirtualOutputNotFound(id) => write!(f, "Virtual output {} not found", id),
            CommandError::InvalidOperation(msg) => write!(f, "Invalid operation: {msg}"),
            CommandError::Other(msg) => write!(f, "Error: {msg}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// Command trait for all actions
pub trait Command<BackendData: Backend>: Debug + Send {
    /// Execute the command
    fn execute(&mut self, state: &mut StilchState<BackendData>) -> CommandResult;

    /// Undo the command
    fn undo(&mut self, state: &mut StilchState<BackendData>) -> CommandResult;

    /// Check if the command can be undone
    fn can_undo(&self) -> bool {
        true
    }

    /// Get a description of the command
    fn description(&self) -> String;
}

/// Move window command
#[derive(Debug)]
pub struct MoveWindowCommand {
    window_id: WindowId,
    target_position: Point<i32, Logical>,
    previous_position: Option<Point<i32, Logical>>,
}

impl MoveWindowCommand {
    pub fn new(window_id: WindowId, target_position: Point<i32, Logical>) -> Self {
        Self {
            window_id,
            target_position,
            previous_position: None,
        }
    }
}

impl<BackendData: Backend> Command<BackendData> for MoveWindowCommand {
    fn execute(&mut self, state: &mut StilchState<BackendData>) -> CommandResult {
        // Store previous position for undo
        self.previous_position = state.window_manager.window_position(self.window_id);

        if self.previous_position.is_none() {
            return Err(CommandError::WindowNotFound(self.window_id));
        }

        // Move the window
        if let Some(event) = state
            .window_manager
            .move_window(self.window_id, self.target_position)
        {
            state.event_bus.emit_window(event);
        }

        Ok(())
    }

    fn undo(&mut self, state: &mut StilchState<BackendData>) -> CommandResult {
        if let Some(prev_pos) = self.previous_position {
            if let Some(event) = state.window_manager.move_window(self.window_id, prev_pos) {
                state.event_bus.emit_window(event);
            }
            Ok(())
        } else {
            Err(CommandError::InvalidOperation(
                "No previous position stored".to_string(),
            ))
        }
    }

    fn description(&self) -> String {
        format!(
            "Move window {} to {:?}",
            self.window_id, self.target_position
        )
    }
}

/// Switch workspace command
#[derive(Debug)]
pub struct SwitchWorkspaceCommand {
    virtual_output: VirtualOutputId,
    target_workspace: WorkspaceId,
    previous_workspace: Option<WorkspaceId>,
}

impl SwitchWorkspaceCommand {
    pub fn new(virtual_output: VirtualOutputId, target_workspace: WorkspaceId) -> Self {
        Self {
            virtual_output,
            target_workspace,
            previous_workspace: None,
        }
    }
}

impl<BackendData: Backend> Command<BackendData> for SwitchWorkspaceCommand {
    fn execute(&mut self, state: &mut StilchState<BackendData>) -> CommandResult {
        // Store previous workspace for undo
        self.previous_workspace = state
            .workspace_manager
            .workspace_on_output(self.virtual_output);

        // Switch workspace
        state.switch_workspace(self.virtual_output, self.target_workspace);

        // Emit event
        if let Some(prev) = self.previous_workspace {
            state
                .event_bus
                .emit_workspace(crate::event::WorkspaceEvent::Switched {
                    old_workspace: prev,
                    new_workspace: self.target_workspace,
                    virtual_output: self.virtual_output,
                    timestamp: std::time::Instant::now(),
                });
        }

        Ok(())
    }

    fn undo(&mut self, state: &mut StilchState<BackendData>) -> CommandResult {
        if let Some(prev_ws) = self.previous_workspace {
            state.switch_workspace(self.virtual_output, prev_ws);

            // Emit event
            state
                .event_bus
                .emit_workspace(crate::event::WorkspaceEvent::Switched {
                    old_workspace: self.target_workspace,
                    new_workspace: prev_ws,
                    virtual_output: self.virtual_output,
                    timestamp: std::time::Instant::now(),
                });

            Ok(())
        } else {
            Err(CommandError::InvalidOperation(
                "No previous workspace stored".to_string(),
            ))
        }
    }

    fn description(&self) -> String {
        format!(
            "Switch output {} to workspace {}",
            self.virtual_output, self.target_workspace
        )
    }
}

/// Move window to workspace command
#[derive(Debug)]
pub struct MoveWindowToWorkspaceCommand {
    window_id: WindowId,
    target_workspace: WorkspaceId,
    previous_workspace: Option<WorkspaceId>,
}

impl MoveWindowToWorkspaceCommand {
    pub fn new(window_id: WindowId, target_workspace: WorkspaceId) -> Self {
        Self {
            window_id,
            target_workspace,
            previous_workspace: None,
        }
    }
}

impl<BackendData: Backend> Command<BackendData> for MoveWindowToWorkspaceCommand {
    fn execute(&mut self, state: &mut StilchState<BackendData>) -> CommandResult {
        // Get the window's current workspace
        let managed_window = state
            .window_registry()
            .get(self.window_id)
            .ok_or(CommandError::WindowNotFound(self.window_id))?;

        self.previous_workspace = Some(managed_window.workspace);

        // Don't move if already on target workspace
        if managed_window.workspace == self.target_workspace {
            return Ok(());
        }

        let old_workspace = managed_window.workspace;

        // Remove from old workspace
        if let Some(workspace) = state.workspace_manager.get_workspace_mut(old_workspace) {
            workspace.remove_window(self.window_id);
        }

        // Add to new workspace
        if let Some(workspace) = state
            .workspace_manager
            .get_workspace_mut(self.target_workspace)
        {
            workspace.add_window(self.window_id);
        } else {
            return Err(CommandError::WorkspaceNotFound(self.target_workspace));
        }

        // Update window registry
        state
            .window_manager
            .set_window_workspace(self.window_id, self.target_workspace);

        // Emit event
        state
            .event_bus
            .emit_window(crate::event::WindowEvent::WorkspaceChanged {
                window_id: self.window_id,
                old_workspace,
                new_workspace: self.target_workspace,
                timestamp: std::time::Instant::now(),
            });

        // Update layouts if needed
        state.apply_workspace_layout(old_workspace);
        state.apply_workspace_layout(self.target_workspace);

        Ok(())
    }

    fn undo(&mut self, state: &mut StilchState<BackendData>) -> CommandResult {
        if let Some(prev_ws) = self.previous_workspace {
            // Move back to previous workspace
            let mut move_back = MoveWindowToWorkspaceCommand::new(self.window_id, prev_ws);
            move_back.execute(state)
        } else {
            Err(CommandError::InvalidOperation(
                "No previous workspace stored".to_string(),
            ))
        }
    }

    fn description(&self) -> String {
        format!(
            "Move window {} to workspace {}",
            self.window_id, self.target_workspace
        )
    }
}

/// Command executor with undo/redo support
pub struct CommandExecutor<BackendData: Backend> {
    /// History of executed commands
    history: Vec<Box<dyn Command<BackendData>>>,
    /// Current position in history
    current: usize,
}

impl<BackendData: Backend> std::fmt::Debug for CommandExecutor<BackendData> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandExecutor")
            .field("history_len", &self.history.len())
            .field("current", &self.current)
            .finish()
    }
}

impl<BackendData: Backend> CommandExecutor<BackendData> {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            current: 0,
        }
    }

    /// Execute a command
    pub fn execute(
        &mut self,
        mut command: Box<dyn Command<BackendData>>,
        state: &mut StilchState<BackendData>,
    ) -> CommandResult {
        // Execute the command
        command.execute(state)?;

        // Remove any commands after current position
        self.history.truncate(self.current);

        // Add to history
        self.history.push(command);
        self.current += 1;

        Ok(())
    }

    /// Undo the last command
    pub fn undo(&mut self, state: &mut StilchState<BackendData>) -> CommandResult {
        if self.current == 0 {
            return Err(CommandError::InvalidOperation(
                "Nothing to undo".to_string(),
            ));
        }

        self.current -= 1;
        let command = &mut self.history[self.current];

        if !command.can_undo() {
            self.current += 1;
            return Err(CommandError::InvalidOperation(
                "Command cannot be undone".to_string(),
            ));
        }

        command.undo(state)
    }

    /// Redo the next command
    pub fn redo(&mut self, state: &mut StilchState<BackendData>) -> CommandResult {
        if self.current >= self.history.len() {
            return Err(CommandError::InvalidOperation(
                "Nothing to redo".to_string(),
            ));
        }

        let command = &mut self.history[self.current];
        command.execute(state)?;
        self.current += 1;

        Ok(())
    }

    /// Clear command history
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.current = 0;
    }

    /// Get the number of commands that can be undone
    pub fn undo_count(&self) -> usize {
        self.current
    }

    /// Get the number of commands that can be redone
    pub fn redo_count(&self) -> usize {
        self.history.len() - self.current
    }
}

impl<BackendData: Backend> Default for CommandExecutor<BackendData> {
    fn default() -> Self {
        Self::new()
    }
}
