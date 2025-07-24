//! Protocol handler implementations for stilch
//!
//! This module contains implementations of various Wayland protocol handlers.
//! These are separated from the main state module for better organization.

pub mod data_device;
pub mod seat;
pub mod misc;

// Re-export handler implementations
