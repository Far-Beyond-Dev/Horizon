//! Plugin trait definitions and wrapper implementations

use crate::context::PluginContext;
use crate::error::PluginSystemError;
use crate::event::EventBus;
use crate::propagation::EventPropagator;
use async_trait::async_trait;
use std::sync::Arc;

/// Simplified plugin trait for easy plugin development
/// 
/// This trait provides a safe, high-level interface for plugin development.
/// It handles all the complex FFI and lifecycle management internally,
/// allowing plugin developers to focus on business logic rather than
/// low-level systems programming.
/// 
/// # Two-Phase Initialization Lifecycle
/// 
/// The plugin system uses a two-phase initialization pattern to prevent race conditions:
/// 
/// 1. **Handler Registration Phase**: `register_handlers()` is called on ALL plugins first
/// 2. **Initialization Phase**: `on_init()` is called only after ALL plugins have registered handlers
/// 3. **Operation Phase**: Plugin receives and processes events normally
/// 4. **Shutdown Phase**: `on_shutdown()` is called for cleanup
/// 
/// This ensures that when any plugin emits an event during initialization, all other
/// plugins have already registered their handlers and can receive those events.
/// 
/// # Critical Rule: Handler Registration vs Initialization
/// 
/// - **`register_handlers()`**: ONLY register event handlers. Do NOT emit events or perform business logic.
/// - **`on_init()`**: Perform initialization logic, emit startup events, access resources.
/// 
/// Following this rule ensures all plugins can communicate properly during startup.
/// 
/// # Examples
/// 
/// ```rust,no_run
/// use universal_plugin_system::*;
/// use std::sync::Arc;
/// 
/// struct MyPlugin {
///     name: String,
/// }
/// 
/// impl MyPlugin {
///     fn new() -> Self {
///         Self { name: "my_plugin".to_string() }
///     }
/// }
/// 
/// #[async_trait::async_trait]
/// impl SimplePlugin<StructuredEventKey, AllEqPropagator> for MyPlugin {
///     fn name(&self) -> &str { &self.name }
///     fn version(&self) -> &str { "1.0.0" }
///     
///     // Phase 1: Register handlers ONLY - no business logic
///     async fn register_handlers(
///         &mut self,
///         event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
///         context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>,
///     ) -> Result<(), PluginSystemError> {
///         // Register handler for player events
///         event_bus.on("player", "joined", |event: PlayerEvent| {
///             println!("Player joined: {:?}", event);
///             Ok(())
///         }).await?;
///         
///         // DO NOT emit events or perform initialization here!
///         Ok(())
///     }
///     
///     // Phase 2: Perform initialization - handlers are guaranteed to be registered
///     async fn on_init(&mut self, context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>) -> Result<(), PluginSystemError> {
///         // Now it's safe to emit events - all handlers are registered
///         context.event_bus().emit("plugin", "initialized", &PluginStatusEvent {
///             plugin_name: self.name().to_string(),
///             status: "ready".to_string(),
///         }).await?;
///         
///         // Perform other initialization logic
///         println!("Plugin {} initialized", self.name());
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait SimplePlugin<K: crate::event::EventKeyType, P: EventPropagator<K>>: Send + Sync + 'static {
    /// Returns the name of this plugin
    fn name(&self) -> &str;

    /// Returns the version string of this plugin
    fn version(&self) -> &str;

    /// Register event handlers during pre-initialization (Phase 1)
    /// 
    /// This method is called FIRST on ALL plugins before any plugin proceeds to `on_init()`.
    /// Use this method ONLY to register event handlers - do NOT emit events, perform business
    /// logic, or access external resources.
    /// 
    /// # Phase 1 Rules
    /// 
    /// - ✅ Register event handlers with `event_bus.on()`, `event_bus.on_key()`, etc.
    /// - ❌ Do NOT emit events - other plugins may not have registered handlers yet
    /// - ❌ Do NOT perform initialization logic - use `on_init()` for that
    /// - ❌ Do NOT access external resources or perform I/O
    /// 
    /// # Arguments
    /// 
    /// * `event_bus` - Reference to the event system for handler registration
    /// * `context` - Plugin context providing access to core services
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if all handlers were registered successfully, or
    /// `Err(PluginSystemError)` if registration failed. Registration failures
    /// will prevent the plugin from loading.
    async fn register_handlers(
        &mut self,
        event_bus: Arc<EventBus<K, P>>,
        context: Arc<PluginContext<K, P>>,
    ) -> Result<(), PluginSystemError>;

    /// Initialize the plugin with context (Phase 2)
    /// 
    /// This method is called ONLY after ALL plugins have completed `register_handlers()`.
    /// At this point, all event handlers are guaranteed to be registered, so it's safe
    /// to emit events, perform business logic, and access resources.
    /// 
    /// # Phase 2 Capabilities
    /// 
    /// - ✅ Emit events - all handlers are now registered
    /// - ✅ Perform initialization logic and setup
    /// - ✅ Access external resources, files, network, etc.
    /// - ✅ Start background tasks or timers
    /// - ✅ Validate plugin dependencies
    /// 
    /// # Arguments
    /// 
    /// * `context` - Plugin context providing access to core services and event bus
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if initialization succeeds, or `Err(PluginSystemError)` if
    /// it fails. Failed initialization will prevent the plugin from becoming active.
    async fn on_init(&mut self, _context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError> {
        Ok(()) // Default implementation does nothing
    }

    /// Shutdown the plugin gracefully
    async fn on_shutdown(&mut self, _context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError> {
        Ok(()) // Default implementation does nothing
    }
}

/// Low-level plugin trait for FFI compatibility
/// 
/// This trait defines the interface that plugin dynamic libraries must implement
/// for compatibility with the plugin loader. Most plugin developers should use
/// the `SimplePlugin` trait instead, which provides a higher-level interface.
/// 
/// # Two-Phase Plugin Lifecycle
/// 
/// 1. **Pre-initialization**: `pre_init()` for handler registration (Phase 1)
/// 2. **Initialization**: `init()` for setup with full context (Phase 2)  
/// 3. **Operation**: Plugin receives and processes events
/// 4. **Shutdown**: `shutdown()` for cleanup
/// 
/// The two-phase initialization ensures all plugins register their event handlers
/// before any plugin begins its main initialization, preventing race conditions.
/// 
/// # FFI Safety
/// 
/// This trait is designed to be safe across FFI boundaries when used with
/// the plugin wrapper system, which handles all the necessary
/// panic catching and error conversion.
#[async_trait]
pub trait Plugin<K: crate::event::EventKeyType, P: EventPropagator<K>>: Send + Sync {
    /// Returns the plugin name
    fn name(&self) -> &str;
    
    /// Returns the plugin version string
    fn version(&self) -> &str;

    /// Pre-initialization phase for registering event handlers (Phase 1)
    /// 
    /// This method is called FIRST on ALL plugins before any plugin proceeds to `init()`.
    /// Use this method to register event handlers with the event bus, but do NOT emit
    /// events or perform business logic.
    /// 
    /// # Phase 1 Rules
    /// 
    /// - ✅ Register event handlers via `context.event_bus().on()`, etc.
    /// - ❌ Do NOT emit events - other plugins may not have registered handlers yet
    /// - ❌ Do NOT perform initialization logic - use `init()` for that
    /// 
    /// # Arguments
    /// 
    /// * `context` - Plugin context providing access to event bus and core services
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if pre-initialization succeeds, or `Err(PluginSystemError)`
    /// if it fails. Failure will prevent the plugin from loading.
    async fn pre_init(&mut self, context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError>;
    
    /// Main initialization phase with full context access (Phase 2)
    /// 
    /// This method is called ONLY after ALL plugins have completed `pre_init()`.
    /// At this point, all event handlers are guaranteed to be registered, so it's safe
    /// to emit events, perform business logic, and access resources.
    /// 
    /// # Phase 2 Capabilities
    /// 
    /// - ✅ Emit events - all handlers are now registered
    /// - ✅ Perform initialization logic and setup
    /// - ✅ Access external resources, files, network, etc.
    /// - ✅ Start background tasks or timers
    /// 
    /// # Arguments
    /// 
    /// * `context` - Plugin context providing access to event bus and core services
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if initialization succeeds, or `Err(PluginSystemError)`
    /// if it fails. Failure will prevent the plugin from becoming active.
    async fn init(&mut self, context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError>;
    
    /// Shutdown phase for cleanup and resource deallocation
    async fn shutdown(&mut self, context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError>;
}

/// Wrapper to bridge SimplePlugin and Plugin traits with panic protection
pub struct PluginWrapper<T, K: crate::event::EventKeyType, P: EventPropagator<K>> {
    inner: T,
    _phantom: std::marker::PhantomData<(K, P)>,
}

impl<T, K: crate::event::EventKeyType, P: EventPropagator<K>> PluginWrapper<T, K, P>
where
    T: SimplePlugin<K, P>,
{
    /// Create a new plugin wrapper
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Helper to convert panics to PluginSystemError
    fn panic_to_error(panic_info: Box<dyn std::any::Any + Send>) -> PluginSystemError {
        let message = if let Some(s) = panic_info.downcast_ref::<&str>() {
            format!("Plugin panicked: {}", s)
        } else if let Some(s) = panic_info.downcast_ref::<String>() {
            format!("Plugin panicked: {}", s)
        } else {
            "Plugin panicked with unknown error".to_string()
        };
        
        PluginSystemError::RuntimeError(message)
    }
}

#[async_trait]
impl<T, K: crate::event::EventKeyType, P: EventPropagator<K>> Plugin<K, P> for PluginWrapper<T, K, P>
where
    T: SimplePlugin<K, P>,
{
    fn name(&self) -> &str {
        // For synchronous methods, we can use catch_unwind directly
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.inner.name())) {
            Ok(name) => name,
            Err(_) => "unknown-plugin-name", // Fallback name if panic occurs
        }
    }

    fn version(&self) -> &str {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.inner.version())) {
            Ok(version) => version,
            Err(_) => "unknown-version", // Fallback version if panic occurs
        }
    }

    async fn pre_init(&mut self, context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError> {
        // Create a future that runs the plugin's register_handlers method
        let event_bus = context.event_bus();
        
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            futures::executor::block_on(self.inner.register_handlers(event_bus, context))
        })) {
            Ok(result) => result,
            Err(panic_info) => Err(Self::panic_to_error(panic_info)),
        }
    }

    async fn init(&mut self, context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            futures::executor::block_on(self.inner.on_init(context))
        })) {
            Ok(result) => result,
            Err(panic_info) => Err(Self::panic_to_error(panic_info)),
        }
    }

    async fn shutdown(&mut self, context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            futures::executor::block_on(self.inner.on_shutdown(context))
        })) {
            Ok(result) => result,
            Err(panic_info) => Err(Self::panic_to_error(panic_info)),
        }
    }
}

/// Trait for plugin factories that can create plugin instances
pub trait PluginFactory<K: crate::event::EventKeyType, P: EventPropagator<K>>: Send + Sync {
    /// Create a new plugin instance
    fn create(&self) -> Result<Box<dyn Plugin<K, P>>, PluginSystemError>;
    
    /// Get the plugin name
    fn plugin_name(&self) -> &str;
    
    /// Get the plugin version
    fn plugin_version(&self) -> &str;
}

/// Simple plugin factory that wraps a constructor function
pub struct SimplePluginFactory<T, K: crate::event::EventKeyType, P: EventPropagator<K>>
where
    T: SimplePlugin<K, P>,
{
    constructor: Box<dyn Fn() -> T + Send + Sync>,
    name: String,
    version: String,
    _phantom: std::marker::PhantomData<(K, P)>,
}

impl<T, K: crate::event::EventKeyType, P: EventPropagator<K>> SimplePluginFactory<T, K, P>
where
    T: SimplePlugin<K, P>,
{
    /// Create a new simple plugin factory
    pub fn new<F>(name: String, version: String, constructor: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            constructor: Box::new(constructor),
            name,
            version,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T, K: crate::event::EventKeyType, P: EventPropagator<K>> PluginFactory<K, P> for SimplePluginFactory<T, K, P>
where
    T: SimplePlugin<K, P>,
{
    fn create(&self) -> Result<Box<dyn Plugin<K, P>>, PluginSystemError> {
        let plugin = (self.constructor)();
        let wrapper = PluginWrapper::new(plugin);
        Ok(Box::new(wrapper))
    }
    
    fn plugin_name(&self) -> &str {
        &self.name
    }
    
    fn plugin_version(&self) -> &str {
        &self.version
    }
}

/// Plugin metadata
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    /// Plugin name
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    pub description: Option<String>,
    /// Plugin author
    pub author: Option<String>,
    /// Plugin dependencies
    pub dependencies: Vec<String>,
}

impl PluginMetadata {
    /// Create new plugin metadata
    pub fn new(name: String, version: String) -> Self {
        Self {
            name,
            version,
            description: None,
            author: None,
            dependencies: Vec::new(),
        }
    }

    /// Set description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Set author
    pub fn with_author(mut self, author: String) -> Self {
        self.author = Some(author);
        self
    }

    /// Add dependency
    pub fn with_dependency(mut self, dependency: String) -> Self {
        self.dependencies.push(dependency);
        self
    }
}