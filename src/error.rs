//! Error types for stilch
//!
//! This module defines the error types used throughout the compositor.
//! We use thiserror for convenient error derivation and avoid panics
//! in production code by properly propagating errors.

use std::fmt;

/// Main error type for stilch operations
#[derive(Debug, thiserror::Error)]
pub enum StilchError {
    /// Window not found in registry
    #[error("Window {0:?} not found")]
    WindowNotFound(crate::window::WindowId),

    /// Surface has no associated window
    #[error("Surface has no associated window")]
    SurfaceNotMapped,

    /// Client is missing required compositor state
    #[error("Client is missing compositor state")]
    ClientStateMissing,

    /// Invalid window operation
    #[error("Invalid window operation: {0}")]
    InvalidOperation(String),

    /// Workspace not found
    #[error("Workspace {0} not found")]
    WorkspaceNotFound(u8),

    /// Virtual output not found
    #[error("Virtual output {0:?} not found")]
    VirtualOutputNotFound(crate::virtual_output::VirtualOutputId),

    /// Container not found
    #[error("Container {0:?} not found")]
    ContainerNotFound(crate::window::ContainerId),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Rendering error
    #[error("Rendering error: {0}")]
    Render(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Wayland protocol error
    #[error("Wayland error: {0}")]
    Wayland(String),

    /// Backend-specific error
    #[error("Backend error: {0}")]
    Backend(String),
}

/// Result type alias for stilch operations
pub type StilchResult<T> = Result<T, StilchError>;

/// Extension trait for Option to convert to Result with error context
pub trait OptionExt<T> {
    /// Convert None to an error with context
    fn ok_or_log<F>(self, error_fn: F) -> StilchResult<T>
    where
        F: FnOnce() -> StilchError;
}

impl<T> OptionExt<T> for Option<T> {
    fn ok_or_log<F>(self, error_fn: F) -> StilchResult<T>
    where
        F: FnOnce() -> StilchError,
    {
        match self {
            Some(val) => Ok(val),
            None => {
                let err = error_fn();
                tracing::error!("{err}");
                Err(err)
            }
        }
    }
}

/// Helper for operations that should log errors but not propagate them
pub fn log_error<T, E: fmt::Display>(result: Result<T, E>) -> Option<T> {
    match result {
        Ok(val) => Some(val),
        Err(err) => {
            tracing::error!("Operation failed: {err}");
            None
        }
    }
}

/// Helper for operations that should log errors and provide a default value
pub fn log_error_default<T: Default, E: fmt::Display>(result: Result<T, E>) -> T {
    match result {
        Ok(val) => val,
        Err(err) => {
            tracing::error!("Operation failed, using default: {err}");
            T::default()
        }
    }
}
