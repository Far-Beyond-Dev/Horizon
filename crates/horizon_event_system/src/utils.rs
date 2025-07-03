//! # Utility Functions
//!
//! This module provides utility functions and convenience methods for the
//! Horizon Event System. These functions simplify common operations and
//! provide consistent interfaces across the entire system.
//!
//! ## Key Functions
//!
//! - [`current_timestamp()`] - Consistent timestamp generation
//! - [`create_horizon_event_system()`] - Event system factory function
//!
//! ## Design Goals
//!
//! - **Consistency**: All timestamps use the same generation method
//! - **Convenience**: Simple factory functions for common operations
//! - **Safety**: All functions handle edge cases gracefully
//! - **Performance**: Optimized implementations for frequent operations

use crate::system::EventSystem;
use std::sync::Arc;

// ============================================================================
// Utility Functions
// ============================================================================

/// Returns the current Unix timestamp in seconds.
/// 
/// This function provides a consistent way to get timestamps across the
/// entire system. All events should use this function for timestamp
/// generation to ensure consistency.
/// 
/// # Panics
/// 
/// Panics if the system clock is set to a time before the Unix epoch
/// (January 1, 1970). This should never happen in practice on modern systems.
/// 
/// # Returns
/// 
/// Current time as seconds since Unix epoch (1970-01-01 00:00:00 UTC).
pub fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

/// Creates a new Horizon event system instance.
/// 
/// This is the primary factory function for creating event system instances.
/// It returns an `Arc<EventSystem>` that can be safely shared across multiple
/// threads and stored in various contexts.
/// 
/// The returned event system is fully initialized and ready to accept
/// handler registrations and event emissions.
/// 
/// # Returns
/// 
/// A new `Arc<EventSystem>` ready for use.
pub fn create_horizon_event_system() -> Arc<EventSystem> {
    Arc::new(EventSystem::new())
}