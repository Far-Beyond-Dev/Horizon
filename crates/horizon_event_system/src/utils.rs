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
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::{PlayerConnectedEvent, PlayerId, current_timestamp};
/// 
/// let player_id = PlayerId::new();
/// let conn_id = "conn_123".to_string();
/// let addr = "127.0.0.1:8080".to_string();
/// 
/// let event = PlayerConnectedEvent {
///     player_id: player_id,
///     connection_id: conn_id,
///     remote_addr: addr,
///     timestamp: current_timestamp(),
/// };
/// ```
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
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::*;
/// 
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let events = create_horizon_event_system();
/// 
///     // Register some handlers
///     events.on_core("player_connected", |event: PlayerConnectedEvent| {
///         println!("Server online!");
///         Ok(())
///     }).await?;
/// 
///     Ok(())
/// }
/// ```
/// 
/// # Returns
/// 
/// A new `Arc<EventSystem>` ready for use.
pub fn create_horizon_event_system() -> Arc<EventSystem> {
    Arc::new(EventSystem::new())
}