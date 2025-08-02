//! Plugin context and dependency injection system

use crate::error::PluginSystemError;
use crate::event::EventBus;
use crate::propagation::EventPropagator;
use async_trait::async_trait;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// Plugin context provides access to services and dependencies
pub struct PluginContext<K: crate::event::EventKeyType, P: EventPropagator<K>> {
    /// Event bus for plugin communication
    event_bus: Arc<EventBus<K, P>>,
    /// Context providers for dependency injection
    providers: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    /// Plugin metadata
    metadata: HashMap<String, String>,
}

impl<K: crate::event::EventKeyType, P: EventPropagator<K>> PluginContext<K, P> {
    /// Create a new plugin context
    pub fn new(event_bus: Arc<EventBus<K, P>>) -> Self {
        Self {
            event_bus,
            providers: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Get the event bus
    pub fn event_bus(&self) -> Arc<EventBus<K, P>> {
        self.event_bus.clone()
    }

    /// Add a context provider
    pub fn add_provider<T: Send + Sync + 'static>(&mut self, provider: T) {
        let type_id = TypeId::of::<T>();
        self.providers.insert(type_id, Box::new(provider));
    }

    /// Get a context provider
    pub fn get_provider<T: Send + Sync + 'static>(&self) -> Option<&T> {
        let type_id = TypeId::of::<T>();
        self.providers.get(&type_id)?.downcast_ref::<T>()
    }

    /// Add metadata
    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }

    /// Get metadata
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }

    /// Get all metadata
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
}

impl<K: crate::event::EventKeyType, P: EventPropagator<K>> Clone for PluginContext<K, P> {
    fn clone(&self) -> Self {
        // Note: We can't easily clone the providers HashMap due to trait object limitations
        // In practice, you'd want to use Arc<> around providers or redesign this
        Self {
            event_bus: self.event_bus.clone(),
            providers: HashMap::new(), // TODO: Implement proper cloning for providers
            metadata: self.metadata.clone(),
        }
    }
}

/// Trait for context providers
#[async_trait]
pub trait ContextProvider: Send + Sync + 'static {
    /// Get the type name of this provider
    fn type_name(&self) -> &'static str;

    /// Initialize the provider
    async fn initialize(&mut self) -> Result<(), PluginSystemError> {
        Ok(())
    }

    /// Shutdown the provider
    async fn shutdown(&mut self) -> Result<(), PluginSystemError> {
        Ok(())
    }
}

/// Database connection provider example
pub struct DatabaseProvider {
    connection_string: String,
    connected: bool,
}

impl DatabaseProvider {
    pub fn new(connection_string: String) -> Self {
        Self {
            connection_string,
            connected: false,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub async fn execute_query(&self, _query: &str) -> Result<Vec<String>, PluginSystemError> {
        if !self.connected {
            return Err(PluginSystemError::ContextError("Database not connected".to_string()));
        }
        // Mock implementation
        Ok(vec!["result1".to_string(), "result2".to_string()])
    }
}

#[async_trait]
impl ContextProvider for DatabaseProvider {
    fn type_name(&self) -> &'static str {
        "DatabaseProvider"
    }

    async fn initialize(&mut self) -> Result<(), PluginSystemError> {
        // Mock connection
        tracing::info!("Connecting to database: {}", self.connection_string);
        self.connected = true;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginSystemError> {
        tracing::info!("Disconnecting from database");
        self.connected = false;
        Ok(())
    }
}

/// Configuration provider
pub struct ConfigProvider {
    config: HashMap<String, String>,
}

impl ConfigProvider {
    pub fn new() -> Self {
        Self {
            config: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: String, value: String) {
        self.config.insert(key, value);
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.config.get(key)
    }

    pub fn get_or_default(&self, key: &str, default: &str) -> String {
        self.config.get(key).cloned().unwrap_or_else(|| default.to_string())
    }
}

impl Default for ConfigProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContextProvider for ConfigProvider {
    fn type_name(&self) -> &'static str {
        "ConfigProvider"
    }
}

/// Logging provider
pub struct LoggingProvider {
    prefix: String,
}

impl LoggingProvider {
    pub fn new(prefix: String) -> Self {
        Self { prefix }
    }

    pub fn log(&self, level: LogLevel, message: &str) {
        let prefixed_message = format!("[{}] {}", self.prefix, message);
        match level {
            LogLevel::Trace => tracing::trace!("{}", prefixed_message),
            LogLevel::Debug => tracing::debug!("{}", prefixed_message),
            LogLevel::Info => tracing::info!("{}", prefixed_message),
            LogLevel::Warn => tracing::warn!("{}", prefixed_message),
            LogLevel::Error => tracing::error!("{}", prefixed_message),
        }
    }

    pub fn trace(&self, message: &str) {
        self.log(LogLevel::Trace, message);
    }

    pub fn debug(&self, message: &str) {
        self.log(LogLevel::Debug, message);
    }

    pub fn info(&self, message: &str) {
        self.log(LogLevel::Info, message);
    }

    pub fn warn(&self, message: &str) {
        self.log(LogLevel::Warn, message);
    }

    pub fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }
}

#[async_trait]
impl ContextProvider for LoggingProvider {
    fn type_name(&self) -> &'static str {
        "LoggingProvider"
    }
}

/// Log levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Network provider for sending messages
pub struct NetworkProvider {
    connected: bool,
}

impl NetworkProvider {
    pub fn new() -> Self {
        Self { connected: false }
    }

    pub async fn send_message(&self, target: &str, data: &[u8]) -> Result<(), PluginSystemError> {
        if !self.connected {
            return Err(PluginSystemError::ContextError("Network not connected".to_string()));
        }
        
        tracing::debug!("Sending {} bytes to {}", data.len(), target);
        // Mock implementation
        Ok(())
    }

    pub async fn broadcast(&self, data: &[u8]) -> Result<(), PluginSystemError> {
        if !self.connected {
            return Err(PluginSystemError::ContextError("Network not connected".to_string()));
        }
        
        tracing::debug!("Broadcasting {} bytes", data.len());
        // Mock implementation
        Ok(())
    }
}

impl Default for NetworkProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContextProvider for NetworkProvider {
    fn type_name(&self) -> &'static str {
        "NetworkProvider"
    }

    async fn initialize(&mut self) -> Result<(), PluginSystemError> {
        tracing::info!("Initializing network provider");
        self.connected = true;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginSystemError> {
        tracing::info!("Shutting down network provider");
        self.connected = false;
        Ok(())
    }
}