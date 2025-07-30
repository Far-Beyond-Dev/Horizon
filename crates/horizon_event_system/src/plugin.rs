//! # Plugin System Interface
//!
//! This module defines the plugin system interfaces and error types for the
//! Horizon Event System. It provides both high-level and low-level plugin
//! development interfaces to support different use cases.
//!
//! ## Plugin Development Approaches
//!
//! ### High-Level: SimplePlugin Trait
//! - Safe, easy-to-use interface for most plugin development
//! - Automatic panic handling and FFI safety
//! - Focus on game logic rather than systems programming
//! - Use with the `create_simple_plugin!` macro
//!
//! ### Low-Level: Plugin Trait
//! - Direct FFI interface for maximum control
//! - Required for plugins with special requirements
//! - Manual panic handling and safety management
//! - Direct dynamic library interface
//!
//! ## Plugin Lifecycle
//!
//! 1. **Creation** - Plugin instance created via `new()`
//! 2. **Pre-initialization** - Handler registration phase
//! 3. **Initialization** - Setup with server context
//! 4. **Operation** - Normal event processing
//! 5. **Shutdown** - Cleanup and resource deallocation
//!
//! ## Error Handling
//!
//! The plugin system provides comprehensive error handling for all lifecycle
//! phases, with automatic panic isolation to prevent plugin failures from
//! crashing the host server.

use crate::context::ServerContext;
use crate::system::EventSystem;
use async_trait::async_trait;
use std::sync::Arc;

// ============================================================================
// Plugin Development Interfaces
// ============================================================================

/// Simplified plugin trait that doesn't require unsafe code.
/// 
/// This trait provides a safe, high-level interface for plugin development.
/// It handles all the complex FFI and lifecycle management internally,
/// allowing plugin developers to focus on game logic rather than
/// low-level systems programming.
/// 
/// # Lifecycle
/// 
/// 1. **Creation**: Plugin instance is created via `new()`
/// 2. **Handler Registration**: `register_handlers()` is called to set up event handlers
/// 3. **Initialization**: `on_init()` is called with server context
/// 4. **Operation**: Plugin receives and processes events
/// 5. **Shutdown**: `on_shutdown()` is called for cleanup
/// 
/// # Examples
/// 
/// ```rust,no_run
/// use horizon_event_system::*;
/// use std::sync::Arc;
/// 
/// struct ChatPlugin {
///     message_count: u64,
/// }
/// 
/// impl ChatPlugin {
///     fn new() -> Self {
///         Self { message_count: 0 }
///     }
/// }
/// 
/// #[async_trait::async_trait]
/// impl plugin::SimplePlugin for ChatPlugin {
///     fn name(&self) -> &str { "chat_system" }
///     fn version(&self) -> &str { "1.0.0" }
///     
///     async fn register_handlers(&mut self, events: Arc<EventSystem>, context: Arc<dyn ServerContext>) -> Result<(), plugin::PluginError> {
///         events.on_client("chat", "message", |event: RawClientMessageEvent| {
///             // Process chat message
///             Ok(())
///         }).await?;
///         Ok(())
///     }
/// }
/// 
/// // create_simple_plugin!(ChatPlugin);
/// ```
#[async_trait]
pub trait SimplePlugin: Send + Sync + 'static {
    /// Returns the name of this plugin.
    /// 
    /// The name should be unique and stable across versions. It's used for
    /// event routing, logging, and plugin management.
    fn name(&self) -> &str;

    /// Returns the version string of this plugin.
    /// 
    /// Should follow semantic versioning (e.g., "1.2.3") for compatibility checking.
    fn version(&self) -> &str;

    /// Registers event handlers during pre-initialization.
    /// 
    /// This method is called before `on_init()` and should set up all event
    /// handlers that the plugin needs. Handler registration must be completed
    /// before the plugin is considered fully loaded.
    /// 
    /// # Arguments
    /// 
    /// * `events` - Reference to the event system for handler registration
    /// * `context` - Server context providing access to core services
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if all handlers were registered successfully, or
    /// `Err(PluginError)` if registration failed.
    async fn register_handlers(&mut self, events: Arc<EventSystem>, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;

    /// Initialize the plugin with server context.
    /// 
    /// This method is called after handler registration and provides access
    /// to server resources. Use this for:
    /// - Loading configuration
    /// - Initializing data structures
    /// - Setting up timers or background tasks
    /// - Validating dependencies
    /// 
    /// # Arguments
    /// 
    /// * `context` - Server context providing access to core services
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if initialization succeeds, or `Err(PluginError)` if it fails.
    /// Failed initialization will prevent the plugin from loading.
    async fn on_init(&mut self, _context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        Ok(()) // Default implementation does nothing
    }

    /// Shutdown the plugin gracefully.
    /// 
    /// This method is called when the plugin is being unloaded or the server
    /// is shutting down. Use this for:
    /// - Saving persistent state
    /// - Cleaning up resources
    /// - Canceling background tasks
    /// - Notifying external services
    /// 
    /// # Arguments
    /// 
    /// * `context` - Server context for accessing core services during shutdown
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if shutdown completes successfully, or `Err(PluginError)`
    /// if cleanup failed. Shutdown errors are logged but don't prevent unloading.
    async fn on_shutdown(&mut self, _context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        Ok(()) // Default implementation does nothing
    }
}

/// Low-level plugin trait for FFI compatibility.
/// 
/// This trait defines the interface that plugin dynamic libraries must implement
/// for compatibility with the plugin loader. Most plugin developers should use
/// the `SimplePlugin` trait instead, which provides a higher-level interface.
/// 
/// # Plugin Lifecycle
/// 
/// 1. **Pre-initialization**: `pre_init()` for handler registration
/// 2. **Initialization**: `init()` for setup with server context  
/// 3. **Operation**: Plugin receives and processes events
/// 4. **Shutdown**: `shutdown()` for cleanup
/// 
/// # FFI Safety
/// 
/// This trait is designed to be safe across FFI boundaries when used with
/// the `create_simple_plugin!` macro, which handles all the necessary
/// panic catching and error conversion.
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Returns the plugin name.
    /// 
    /// Must be stable across plugin versions and unique among all plugins.
    fn name(&self) -> &str;
    
    /// Returns the plugin version string.
    /// 
    /// Should follow semantic versioning for compatibility checking.
    fn version(&self) -> &str;

    /// Pre-initialization phase for registering event handlers.
    /// 
    /// This method is called before `init()` and should register all event
    /// handlers that the plugin needs. The plugin will not receive events
    /// until this method completes successfully.
    /// 
    /// # Arguments
    /// 
    /// * `context` - Server context for accessing core services
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if pre-initialization succeeds, or `Err(PluginError)`
    /// if it fails. Failure will prevent the plugin from loading.
    async fn pre_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;
    
    /// Main initialization phase with full server context access.
    /// 
    /// This method is called after successful pre-initialization and provides
    /// full access to server resources. Use this for loading configuration,
    /// initializing data structures, and setting up any background tasks.
    /// 
    /// # Arguments
    /// 
    /// * `context` - Server context for accessing core services
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if initialization succeeds, or `Err(PluginError)`
    /// if it fails. Failure will prevent the plugin from becoming active.
    async fn init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;
    
    /// Shutdown phase for cleanup and resource deallocation.
    /// 
    /// This method is called when the plugin is being unloaded or the server
    /// is shutting down. It should perform all necessary cleanup including
    /// saving state, stopping background tasks, and releasing resources.
    /// 
    /// # Arguments
    /// 
    /// * `context` - Server context for accessing core services during shutdown
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if shutdown completes successfully, or `Err(PluginError)`
    /// if cleanup failed. Shutdown errors are logged but don't prevent unloading.
    async fn shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError>;
}

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during plugin operations.
/// 
/// This enum covers all error conditions that can arise during plugin
/// lifecycle management, from initialization failures to runtime errors.
/// 
/// # Error Categories
/// 
/// - **InitializationFailed**: Plugin failed to initialize properly
/// - **ExecutionError**: Runtime error during normal operation
/// - **NotFound**: Requested plugin or resource doesn't exist
/// - **Runtime**: Panic or other unexpected runtime condition
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// Plugin initialization failed during startup
    #[error("Plugin initialization failed: {0}")]
    InitializationFailed(String),
    /// Error occurred during plugin execution
    #[error("Plugin execution error: {0}")]
    ExecutionError(String),
    /// Requested plugin was not found
    #[error("Plugin not found: {0}")]
    NotFound(String),
    /// Runtime error such as panic or system failure
    #[error("Plugin runtime error: {0}")]
    Runtime(String),
}