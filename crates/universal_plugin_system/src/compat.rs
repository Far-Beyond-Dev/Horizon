//! ABI Compatibility layer for legacy plugins
//!
//! This module provides compatibility between the old horizon_event_system plugin ABI
//! and the new universal plugin system. It allows existing plugins to work without
//! recompilation while providing a migration path to the new system.

use crate::context::PluginContext;
use crate::error::PluginSystemError;
use crate::event::EventBus;
use crate::plugin::Plugin;
use crate::propagation::EventPropagator;
use async_trait::async_trait;
use libloading::{Library, Symbol};
use std::sync::Arc;
use tracing::{info, warn};

/// Legacy plugin error type from horizon_event_system
#[derive(Debug)]
pub enum LegacyPluginError {
    InitializationFailed(String),
    ExecutionError(String),
    NotFound(String),
    Runtime(String),
}

impl std::fmt::Display for LegacyPluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LegacyPluginError::InitializationFailed(msg) => write!(f, "Plugin initialization failed: {}", msg),
            LegacyPluginError::ExecutionError(msg) => write!(f, "Plugin execution error: {}", msg),
            LegacyPluginError::NotFound(msg) => write!(f, "Plugin not found: {}", msg),
            LegacyPluginError::Runtime(msg) => write!(f, "Plugin runtime error: {}", msg),
        }
    }
}

impl std::error::Error for LegacyPluginError {}

/// Convert legacy plugin error to universal plugin system error
impl From<LegacyPluginError> for PluginSystemError {
    fn from(err: LegacyPluginError) -> Self {
        match err {
            LegacyPluginError::InitializationFailed(msg) => PluginSystemError::InitializationFailed(msg),
            LegacyPluginError::ExecutionError(msg) => PluginSystemError::RuntimeError(msg),
            LegacyPluginError::NotFound(msg) => PluginSystemError::PluginNotFound(msg),
            LegacyPluginError::Runtime(msg) => PluginSystemError::RuntimeError(msg),
        }
    }
}

/// Legacy plugin trait from horizon_event_system
#[async_trait]
pub trait LegacyPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    async fn pre_init(&mut self, context: Arc<dyn LegacyServerContext>) -> Result<(), LegacyPluginError>;
    async fn init(&mut self, context: Arc<dyn LegacyServerContext>) -> Result<(), LegacyPluginError>;
    async fn shutdown(&mut self, context: Arc<dyn LegacyServerContext>) -> Result<(), LegacyPluginError>;
}

/// Legacy server context trait from horizon_event_system
#[async_trait]
pub trait LegacyServerContext: Send + Sync {
    fn events(&self) -> Arc<dyn LegacyEventSystem>;
    fn log(&self, level: LegacyLogLevel, message: &str);
    fn region_id(&self) -> u64; // Simplified for compatibility
    fn tokio_handle(&self) -> Option<tokio::runtime::Handle>;
    async fn send_to_player(&self, player_id: u64, data: &[u8]) -> Result<(), LegacyServerError>;
    async fn broadcast(&self, data: &[u8]) -> Result<(), LegacyServerError>;
}

/// Legacy event system trait (simplified)
#[allow(unused)]
pub trait LegacyEventSystem: Send + Sync {
    // Simplified interface for compatibility
}

/// Legacy log level enum
#[derive(Debug, Clone, Copy)]
pub enum LegacyLogLevel {
    Trace,
    Debug,  
    Info,
    Warn,
    Error,
}

/// Legacy server error type
#[derive(Debug)]
pub enum LegacyServerError {
    Internal(String),
}

impl std::fmt::Display for LegacyServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LegacyServerError::Internal(msg) => write!(f, "Server error: {}", msg),
        }
    }
}

impl std::error::Error for LegacyServerError {}

/// Plugin ABI version detector
pub struct AbiVersionDetector;

impl AbiVersionDetector {
    /// Detect which ABI version a plugin library uses
    pub fn detect_abi_version(library: &Library) -> Result<PluginAbiVersion, PluginSystemError> {
        // Try to get version function
        let get_version_result: Result<Symbol<unsafe extern "C" fn() -> *const std::os::raw::c_char>, _> = 
            unsafe { library.get(b"get_plugin_version") };
            
        match get_version_result {
            Ok(get_version) => {
                let version_ptr = unsafe { get_version() };
                if version_ptr.is_null() {
                    return Ok(PluginAbiVersion::Unknown);
                }
                
                let version_str = unsafe {
                    std::ffi::CStr::from_ptr(version_ptr)
                        .to_string_lossy()
                        .to_string()
                };
                
                // Check if it's the universal system version format
                if version_str.starts_with(crate::UNIVERSAL_PLUGIN_SYSTEM_VERSION) {
                    return Ok(PluginAbiVersion::Universal);
                }
                
                // Check if it's the horizon system version format (contains ":")
                if version_str.contains(':') {
                    return Ok(PluginAbiVersion::Horizon);
                }
                
                Ok(PluginAbiVersion::Unknown)
            }
            Err(_) => {
                // No version function, might be very old plugin
                Ok(PluginAbiVersion::Unknown)
            }
        }
    }
}

/// Plugin ABI version enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum PluginAbiVersion {
    /// Universal plugin system ABI
    Universal,
    /// Horizon event system ABI  
    Horizon,
    /// Unknown or unsupported ABI
    Unknown,
}

/// Adapter that wraps legacy plugins to work with universal system
pub struct LegacyPluginAdapter<K: crate::event::EventKeyType, P: EventPropagator<K>> {
    /// The legacy plugin instance
    legacy_plugin: Box<dyn LegacyPlugin>,
    /// Context adapter for converting between context types
    context_adapter: Option<Arc<LegacyContextAdapter<K, P>>>,
    /// Marker for generic types
    _phantom: std::marker::PhantomData<(K, P)>,
}

impl<K: crate::event::EventKeyType, P: EventPropagator<K>> LegacyPluginAdapter<K, P> {
    /// Create a new legacy plugin adapter
    pub fn new(legacy_plugin: Box<dyn LegacyPlugin>) -> Self {
        Self {
            legacy_plugin,
            context_adapter: None,
            _phantom: std::marker::PhantomData,
        }
    }
    
    /// Set the context adapter
    pub fn with_context_adapter(mut self, adapter: Arc<LegacyContextAdapter<K, P>>) -> Self {
        self.context_adapter = Some(adapter);
        self
    }
}

#[async_trait]
impl<K: crate::event::EventKeyType, P: EventPropagator<K>> Plugin<K, P> for LegacyPluginAdapter<K, P> {
    fn name(&self) -> &str {
        self.legacy_plugin.name()
    }
    
    fn version(&self) -> &str {
        self.legacy_plugin.version()
    }
    
    async fn pre_init(&mut self, context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError> {
        info!("🔄 Adapting legacy plugin '{}' pre_init call", self.name());
        
        // Create adapter if not set
        if self.context_adapter.is_none() {
            self.context_adapter = Some(Arc::new(LegacyContextAdapter::new(context.clone())));
        }
        
        let adapter = self.context_adapter.as_ref().unwrap().clone();
        
        match self.legacy_plugin.pre_init(adapter).await {
            Ok(_) => {
                info!("✅ Legacy plugin '{}' pre_init successful", self.name());
                Ok(())
            }
            Err(e) => {
                warn!("❌ Legacy plugin '{}' pre_init failed: {}", self.name(), e);
                Err(e.into())
            }
        }
    }
    
    async fn init(&mut self, context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError> {
        info!("🔄 Adapting legacy plugin '{}' init call", self.name());
        
        // Create adapter if not set
        if self.context_adapter.is_none() {
            self.context_adapter = Some(Arc::new(LegacyContextAdapter::new(context.clone())));
        }
        
        let adapter = self.context_adapter.as_ref().unwrap().clone();
        
        match self.legacy_plugin.init(adapter).await {
            Ok(_) => {
                info!("✅ Legacy plugin '{}' init successful", self.name());
                Ok(())
            }
            Err(e) => {
                warn!("❌ Legacy plugin '{}' init failed: {}", self.name(), e);
                Err(e.into())
            }
        }
    }
    
    async fn shutdown(&mut self, context: Arc<PluginContext<K, P>>) -> Result<(), PluginSystemError> {
        info!("🔄 Adapting legacy plugin '{}' shutdown call", self.name());
        
        // Create adapter if not set
        if self.context_adapter.is_none() {
            self.context_adapter = Some(Arc::new(LegacyContextAdapter::new(context.clone())));
        }
        
        let adapter = self.context_adapter.as_ref().unwrap().clone();
        
        match self.legacy_plugin.shutdown(adapter).await {
            Ok(_) => {
                info!("✅ Legacy plugin '{}' shutdown successful", self.name());
                Ok(())
            }
            Err(e) => {
                warn!("❌ Legacy plugin '{}' shutdown failed: {}", self.name(), e);
                Err(e.into())
            }
        }
    }
}

/// Context adapter to convert universal context to legacy context
pub struct LegacyContextAdapter<K: crate::event::EventKeyType, P: EventPropagator<K>> {
    /// Universal plugin context
    universal_context: Arc<PluginContext<K, P>>,
    /// Marker for generic types
    _phantom: std::marker::PhantomData<(K, P)>,
}

impl<K: crate::event::EventKeyType, P: EventPropagator<K>> LegacyContextAdapter<K, P> {
    /// Create a new context adapter
    pub fn new(universal_context: Arc<PluginContext<K, P>>) -> Self {
        Self {
            universal_context,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<K: crate::event::EventKeyType, P: EventPropagator<K>> LegacyServerContext for LegacyContextAdapter<K, P> {
    fn events(&self) -> Arc<dyn LegacyEventSystem> {
        // Create a dummy event system adapter
        Arc::new(LegacyEventSystemAdapter::new(self.universal_context.event_bus()))
    }
    
    fn log(&self, level: LegacyLogLevel, message: &str) {
        // Convert legacy log level to tracing level
        match level {
            LegacyLogLevel::Trace => tracing::trace!("{}", message),
            LegacyLogLevel::Debug => tracing::debug!("{}", message),
            LegacyLogLevel::Info => tracing::info!("{}", message),
            LegacyLogLevel::Warn => tracing::warn!("{}", message),
            LegacyLogLevel::Error => tracing::error!("{}", message),
        }
    }
    
    fn region_id(&self) -> u64 {
        // Return a default region ID for compatibility
        1
    }
    
    fn tokio_handle(&self) -> Option<tokio::runtime::Handle> {
        tokio::runtime::Handle::try_current().ok()
    }
    
    async fn send_to_player(&self, _player_id: u64, _data: &[u8]) -> Result<(), LegacyServerError> {
        // For now, just log that this was called
        tracing::warn!("Legacy plugin called send_to_player - not implemented in compatibility layer");
        Err(LegacyServerError::Internal("send_to_player not implemented in legacy adapter".to_string()))
    }
    
    async fn broadcast(&self, _data: &[u8]) -> Result<(), LegacyServerError> {
        // For now, just log that this was called
        tracing::warn!("Legacy plugin called broadcast - not implemented in compatibility layer");
        Err(LegacyServerError::Internal("broadcast not implemented in legacy adapter".to_string()))
    }
}

/// Event system adapter for legacy plugins
pub struct LegacyEventSystemAdapter<K: crate::event::EventKeyType, P: EventPropagator<K>> {
    /// Universal event bus
    _event_bus: Arc<EventBus<K, P>>,
    /// Marker for generic types
    _phantom: std::marker::PhantomData<(K, P)>,
}

impl<K: crate::event::EventKeyType, P: EventPropagator<K>> LegacyEventSystemAdapter<K, P> {
    /// Create a new event system adapter
    pub fn new(event_bus: Arc<EventBus<K, P>>) -> Self {
        Self {
            _event_bus: event_bus,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<K: crate::event::EventKeyType, P: EventPropagator<K>> LegacyEventSystem for LegacyEventSystemAdapter<K, P> {
    // Empty implementation for now - legacy plugins will use their own event registration
}

/// Legacy plugin loader that can handle both old and new ABI versions
pub struct CompatibilityPluginLoader;

impl CompatibilityPluginLoader {
    /// Load a plugin with ABI compatibility detection
    pub unsafe fn load_plugin_with_compat<K: crate::event::EventKeyType, P: EventPropagator<K>>(
        library: &Library,
        plugin_path: &std::path::Path,
    ) -> Result<Box<dyn Plugin<K, P>>, PluginSystemError> {
        // Detect ABI version
        let abi_version = AbiVersionDetector::detect_abi_version(library)?;
        
        match abi_version {
            PluginAbiVersion::Universal => {
                info!("🔄 Loading universal plugin from: {}", plugin_path.display());
                Self::load_universal_plugin(library)
            }
            PluginAbiVersion::Horizon => {  
                info!("🔄 Loading legacy horizon plugin from: {}", plugin_path.display());
                Self::load_legacy_plugin(library)
            }
            PluginAbiVersion::Unknown => {
                warn!("⚠️ Unknown plugin ABI version from: {}, attempting legacy load", plugin_path.display());
                Self::load_legacy_plugin(library)
            }
        }
    }
    
    /// Load a universal plugin (new ABI)
    unsafe fn load_universal_plugin<K: crate::event::EventKeyType, P: EventPropagator<K>>(
        library: &Library,
    ) -> Result<Box<dyn Plugin<K, P>>, PluginSystemError> {
        // Try to load as universal plugin
        let create_plugin: Symbol<unsafe extern "C" fn() -> *mut dyn Plugin<K, P>> = library
            .get(b"create_plugin")
            .map_err(|e| PluginSystemError::LoadingFailed(format!("Universal plugin missing create_plugin: {}", e)))?;
            
        let plugin_ptr = create_plugin();
        if plugin_ptr.is_null() {
            return Err(PluginSystemError::LoadingFailed("Universal plugin creation returned null".to_string()));
        }
        
        Ok(Box::from_raw(plugin_ptr))
    }
    
    /// Load a legacy plugin (old ABI) with adapter
    unsafe fn load_legacy_plugin<K: crate::event::EventKeyType, P: EventPropagator<K>>(
        _library: &Library,
    ) -> Result<Box<dyn Plugin<K, P>>, PluginSystemError> {
        // Try to load as legacy plugin - this is a simplified approach
        // In reality, we'd need to handle the exact legacy ABI
        
        // For now, return an error indicating legacy plugins need the bridge
        Err(PluginSystemError::LoadingFailed(
            "Legacy plugin loading requires the HorizonUniversalBridge. Use the bridge's plugin loading methods instead.".to_string()
        ))
    }
}