//! Plugin trait definitions and wrapper implementations

use crate::context::PluginContext;
use crate::error::PluginSystemError;
use crate::event::EventBus;
use crate::propagation::EventPropagator;
use async_trait::async_trait;
use std::sync::Arc;

/// Simplified plugin trait for easy plugin development
#[async_trait]
pub trait SimplePlugin<K: crate::event::EventKeyType, P: EventPropagator<K>>: Send + Sync + 'static {
    /// Returns the name of this plugin
    fn name(&self) -> &str;

    /// Returns the version string of this plugin
    fn version(&self) -> &str;

    /// Register event handlers during pre-initialization
    async fn register_handlers(
        &mut self,
        event_bus: Arc<EventBus<K, P>>,
        context: Arc<PluginContext<K, P>>,
    ) -> Result<(), PluginSystemError>;

    /// Initialize the plugin with context
    async fn on_init(&mut self, _context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError> {
        Ok(()) // Default implementation does nothing
    }

    /// Shutdown the plugin gracefully
    async fn on_shutdown(&mut self, _context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError> {
        Ok(()) // Default implementation does nothing
    }
}

/// Low-level plugin trait for FFI compatibility
#[async_trait]
pub trait Plugin<K: crate::event::EventKeyType, P: EventPropagator<K>>: Send + Sync {
    /// Returns the plugin name
    fn name(&self) -> &str;
    
    /// Returns the plugin version string
    fn version(&self) -> &str;

    /// Pre-initialization phase for registering event handlers
    async fn pre_init(&mut self, context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError>;
    
    /// Main initialization phase with full context access
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