//! Type-safe ID types for window management
//!
//! This module provides strongly-typed IDs that:
//! - Cannot be zero (using NonZeroU64)
//! - Cannot be accidentally mixed up (different types)
//! - Are efficient (single u64 internally)
//! - Support atomic generation

use std::fmt;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for windows
/// 
/// This ID is guaranteed to be:
/// - Non-zero (can use Option<WindowId> without overhead)
/// - Unique within the compositor lifetime
/// - Type-safe (cannot be confused with other ID types)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct WindowId(NonZeroU64);

/// Atomic counter for generating unique window IDs
/// Starts at 1 to ensure NonZeroU64 is always valid
static WINDOW_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

impl WindowId {
    /// Generate a new unique window ID
    /// 
    /// This is guaranteed to never return the same ID twice
    /// within a single compositor session
    pub fn next() -> Self {
        let id = WINDOW_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        // Safety: We start at 1 and only increment, so this is never zero
        WindowId(NonZeroU64::new(id).expect("Window ID counter overflow"))
    }

    /// Create a WindowId from a raw value
    /// 
    /// Returns None if the value is zero
    pub fn from_raw(id: u64) -> Option<Self> {
        NonZeroU64::new(id).map(WindowId)
    }

    /// Create a WindowId for testing
    /// 
    /// # Panics
    /// Panics if id is zero
    pub fn new(id: u32) -> Self {
        WindowId(NonZeroU64::new(id as u64).expect("WindowId cannot be zero"))
    }

    /// Get the raw ID value
    pub fn get(&self) -> u64 {
        self.0.get()
    }

    /// Get as NonZeroU64 for efficient Option storage
    pub fn as_nonzero(&self) -> NonZeroU64 {
        self.0
    }
}

impl fmt::Display for WindowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Window({})", self.0)
    }
}

/// Container identifier for layout tree
///
/// Containers are internal nodes in the layout tree that
/// group windows together for tiling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ContainerId(NonZeroU64);

/// Atomic counter for generating unique container IDs
static CONTAINER_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

impl ContainerId {
    /// Generate a new unique container ID
    pub fn next() -> Self {
        let id = CONTAINER_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        // Safety: We start at 1 and only increment, so this is never zero
        ContainerId(NonZeroU64::new(id).expect("Container ID counter overflow"))
    }

    /// Create from raw value
    pub fn from_raw(id: u64) -> Option<Self> {
        NonZeroU64::new(id).map(ContainerId)
    }

    /// Get the raw ID value
    pub fn get(&self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for ContainerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Container({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_id_is_unique() {
        let id1 = WindowId::next();
        let id2 = WindowId::next();
        assert_ne!(id1, id2);
    }

    #[test]
    fn window_id_never_zero() {
        for _ in 0..100 {
            let id = WindowId::next();
            assert_ne!(id.get(), 0);
        }
    }

    #[test]
    fn window_id_from_raw_rejects_zero() {
        assert!(WindowId::from_raw(0).is_none());
        assert!(WindowId::from_raw(1).is_some());
    }

    #[test]
    fn container_id_is_unique() {
        let id1 = ContainerId::next();
        let id2 = ContainerId::next();
        assert_ne!(id1, id2);
    }
}