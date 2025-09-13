//! # Server Context Interface
//!
//! This module defines the server context interface that provides plugins with
//! access to core server services. The context serves as the primary bridge
//! between plugin code and the host server infrastructure.
//!
//! ## Core Services
//!
//! The [`ServerContext`] provides access to:
//! - **Event System** - For emitting events and registering additional handlers
//! - **Logging** - Structured logging integrated with server infrastructure
//! - **Player Communication** - Direct messaging and broadcasting capabilities
//! - **Region Information** - Context about the current game region
//!
//! ## Design Principles
//!
//! - **Minimal Interface**: Only essential services are exposed to plugins
//! - **Type Safety**: All operations use strongly typed interfaces
//! - **Async Support**: Network operations are non-blocking and async
//! - **Error Handling**: Comprehensive error types for all fallible operations
//!
//! ## Thread Safety
//!
//! All context operations are thread-safe and can be called from multiple
//! threads concurrently. The context uses appropriate synchronization
//! internally to ensure data consistency.

use crate::system::EventSystem;
use crate::types::{PlayerId, RegionId};
use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;
use luminal;

// ============================================================================
// Server Context Interface (Minimal)
// ============================================================================

/// Server context interface providing access to core server services.
/// 
/// This trait defines the interface that plugins use to interact with the
/// core server. It provides access to essential services like the event
/// system, logging, and player communication while maintaining a clean
/// separation between plugin code and server internals.
/// 
/// # Design Principles
/// 
/// - **Minimal Interface**: Only essential services are exposed
/// - **Type Safety**: All operations are strongly typed
/// - **Async Support**: All potentially blocking operations are async
/// - **Error Handling**: Proper error types for all fallible operations
/// 
/// # Examples
/// 
/// ```rust,no_run
/// use horizon_event_system::{ServerContext, LogLevel, PluginError};
/// use std::sync::Arc;
/// 
/// async fn example_plugin_init(context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
/// 
///     // Access event system
///     let events = context.events();
/// 
///     // Log plugin initialization
///     context.log(LogLevel::Info, "Combat plugin initialized");
/// 
///     // Get current region
///     let region_id = context.region_id();
/// 
///     Ok(())
/// }
/// 
/// # struct TestServerContext { events: std::sync::Arc<horizon_event_system::EventSystem> }
/// # impl TestServerContext {
/// #     fn new() -> Self { Self { events: horizon_event_system::create_horizon_event_system() } }
/// # }
/// # #[horizon_event_system::async_trait]
/// # impl ServerContext for TestServerContext {
/// #     fn events(&self) -> std::sync::Arc<horizon_event_system::EventSystem> { self.events.clone() }
/// #     fn log(&self, _level: LogLevel, _msg: &str) {}
/// #     fn region_id(&self) -> horizon_event_system::RegionId { horizon_event_system::RegionId::new() }
/// # }
/// ```
#[async_trait]
pub trait ServerContext: Send + Sync + Debug {
    /// Returns a reference to the event system.
    /// 
    /// This provides access to the same event system used by the core server,
    /// allowing plugins to emit events and register additional handlers.
    fn events(&self) -> Arc<EventSystem>;
    
    /// Returns the ID of the region this context is associated with.
    /// 
    /// Plugins can use this to understand which region they're operating in
    /// and to emit region-specific events.
    fn region_id(&self) -> RegionId;
    
    /// Logs a message with the specified level.
    /// 
    /// This integrates with the server's logging system and should be used
    /// for all plugin logging to ensure consistent log formatting and routing.
    /// 
    /// # Arguments
    /// 
    /// * `level` - Severity level of the log message
    /// * `message` - The message to log
    fn log(&self, level: LogLevel, message: &str);

    /// Sends raw data to a specific player.
    /// 
    /// This method bypasses the event system and sends data directly to a
    /// player's connection. It should be used for high-frequency or
    /// latency-sensitive communications.
    /// 
    /// # Arguments
    /// 
    /// * `player_id` - Target player identifier
    /// * `data` - Raw bytes to send
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the data was queued for sending, or `Err(ServerError)`
    /// if the send failed (e.g., player not connected).
    async fn send_to_player(&self, player_id: PlayerId, data: &[u8]) -> Result<(), ServerError>;

    /// Broadcasts raw data to all connected players.
    /// 
    /// This method sends data to all players currently connected to the server.
    /// Use with caution as it can generate significant network traffic.
    /// 
    /// # Arguments
    /// 
    /// * `data` - Raw bytes to broadcast
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if the broadcast was initiated, or `Err(ServerError)`
    /// if the broadcast failed.
    async fn broadcast(&self, data: &[u8]) -> Result<(), ServerError>;

    /// Returns the luminal runtime handle for cross-DLL compatibility.
    /// 
    /// This provides plugins with access to a luminal runtime for async operations
    /// that need to be executed within the proper runtime context. This is essential
    /// for plugins loaded as DLLs where the runtime context may not be automatically
    /// available and tokio doesn't support cross-DLL runtime bounds.
    /// 
    /// # Returns
    /// 
    /// Returns the luminal runtime handle for cross-DLL async execution.
    fn luminal_handle(&self) -> luminal::Handle;

    /// Returns access to the GORC instance manager for object replication.
    /// 
    /// This provides plugins with direct access to the GORC (Game Object Replication
    /// Channels) system for managing object instances, zones, and replication. This
    /// allows plugins to register objects, access instance data, and work with the
    /// replication system without needing to use create_complete_horizon_system.
    /// 
    /// # Returns
    /// 
    /// Returns an Arc to the GorcInstanceManager if available, or None if GORC
    /// is not enabled for this server context.
    fn gorc_instance_manager(&self) -> Option<Arc<crate::gorc::GorcInstanceManager>>;
}

// ============================================================================
// Supporting Types
// ============================================================================

/// Enumeration of log levels for structured logging.
/// 
/// These levels follow standard logging conventions and integrate with
/// the server's logging infrastructure. Higher levels indicate more
/// severe or important messages.
/// 
/// # Level Guidelines
/// 
/// - **Error**: System errors, plugin failures, critical issues
/// - **Warn**: Recoverable errors, deprecated usage, performance issues  
/// - **Info**: General information, plugin lifecycle, major events
/// - **Debug**: Detailed debugging information, development diagnostics
/// - **Trace**: Very detailed execution traces, performance profiling
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::LogLevel;
/// 
/// # struct TestContext;
/// # impl TestContext {
/// #     fn log(&self, level: LogLevel, msg: &str) {}
/// # }
/// # let context = TestContext;
/// context.log(LogLevel::Info, "Combat plugin initialized successfully");
/// context.log(LogLevel::Warn, "Player inventory is nearly full");
/// context.log(LogLevel::Error, "Failed to load combat configuration");
/// ```
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    /// Critical errors that may affect system stability
    Error,
    /// Warning conditions that should be investigated
    Warn,
    /// General informational messages
    Info,
    /// Detailed information for debugging
    Debug,
    /// Very detailed trace information
    Trace,
}

/// Errors that can occur during server operations.
/// 
/// This enum covers error conditions that can arise when plugins interact
/// with core server functionality, particularly networking and internal
/// service operations.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// Network-related error (connection issues, send failures, etc.)
    #[error("Network error: {0}")]
    Network(String),
    /// Internal server error (resource exhaustion, invalid state, etc.)
    #[error("Internal error: {0}")]
    Internal(String),
}