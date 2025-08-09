//! State management module
//!
//! This module contains the main compositor state and its components.

mod main;
mod protocols;
pub mod validation;

pub use main::{
    take_presentation_feedback, update_primary_scanout_output, Backend, ClientState, DndIcon,
    StilchState, SurfaceDmabufFeedback,
};
pub use protocols::ProtocolState;
