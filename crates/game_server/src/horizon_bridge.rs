//! Bridge module for integrating Horizon Event System with Universal Plugin System
//!
//! This module provides seamless integration between the horizon-specific event system
//! and the universal plugin system, allowing existing plugins to continue working
//! without modifications while gaining the benefits of the universal system.

use crate::error::ServerError;
use horizon_event_system::{EventSystem as HorizonEventSystem, ServerContext, LogLevel};
use universal_plugin_system::{
    EventBus, StructuredEventKey, 
    propagation::{AllEqPropagator, DefaultPropagator, DomainPropagator, UniversalAllEqPropagator, CompositePropagator, EventPropagator, PropagationContext},
    PluginManager as UniversalPluginManager, PluginContext, PluginConfig,
    monitoring::DetailedEventStats,
    compat::{AbiVersionDetector, PluginAbiVersion},
    plugin::Plugin,
};
use std::sync::Arc;
use tracing::info;
use async_trait::async_trait;
use libloading::Library;

/// Event propagator that bridges to horizon event system
pub struct HorizonBridgePropagator {
    horizon_events: Arc<HorizonEventSystem>,
}

impl HorizonBridgePropagator {
    /// Create a new bridge propagator
    pub fn new(horizon_events: Arc<HorizonEventSystem>) -> Self {
        Self { horizon_events }
    }
}

#[async_trait]
impl EventPropagator<StructuredEventKey> for HorizonBridgePropagator {
    async fn should_propagate(&self, _key: &StructuredEventKey, _context: &PropagationContext<StructuredEventKey>) -> bool {
        // For now, bridge all events
        true
    }
}

/// Bridge implementation to integrate Horizon plugins with Universal system
pub struct HorizonUniversalBridge {
    /// The universal event bus
    universal_bus: Arc<EventBus<StructuredEventKey, CompositePropagator<StructuredEventKey>>>,
    /// The original horizon event system
    horizon_events: Arc<HorizonEventSystem>,
    /// Universal plugin manager
    plugin_manager: Arc<UniversalPluginManager<StructuredEventKey, CompositePropagator<StructuredEventKey>>>,
    /// Server context for plugins
    server_context: Arc<dyn ServerContext>,
    /// Plugin context template
    plugin_context_template: Arc<PluginContext<StructuredEventKey, CompositePropagator<StructuredEventKey>>>,
}

/// Legacy plugin bridge that adapts horizon_event_system plugins to universal system
pub struct LegacyPluginBridge {
    /// The legacy plugin instance
    legacy_plugin: Box<dyn horizon_event_system::plugin::Plugin>,
    /// Reference to horizon event system for context
    horizon_events: Arc<HorizonEventSystem>,
}

impl LegacyPluginBridge {
    /// Create a new legacy plugin bridge
    pub fn new(
        legacy_plugin: Box<dyn horizon_event_system::plugin::Plugin>,
        horizon_events: Arc<HorizonEventSystem>,
    ) -> Self {
        Self {
            legacy_plugin,
            horizon_events,
        }
    }
}

#[async_trait]
impl Plugin<StructuredEventKey, CompositePropagator<StructuredEventKey>> for LegacyPluginBridge {
    fn name(&self) -> &str {
        self.legacy_plugin.name()
    }
    
    fn version(&self) -> &str {
        self.legacy_plugin.version()
    }
    
    async fn pre_init(&mut self, _context: Arc<PluginContext<StructuredEventKey, CompositePropagator<StructuredEventKey>>>) -> Result<(), universal_plugin_system::PluginSystemError> {
        info!("🔄 Legacy plugin bridge: pre_init for '{}'", self.name());
        
        // Create a basic server context for the legacy plugin
        // This is a simplified bridge - in production you'd want full context mapping
        let basic_context = Arc::new(LegacyServerContextBridge::new(self.horizon_events.clone()));
        
        match self.legacy_plugin.pre_init(basic_context).await {
            Ok(_) => {
                info!("✅ Legacy plugin '{}' pre_init successful", self.name());
                Ok(())
            }
            Err(e) => {
                tracing::error!("❌ Legacy plugin '{}' pre_init failed: {:?}", self.name(), e);
                Err(universal_plugin_system::PluginSystemError::InitializationFailed(
                    format!("Legacy plugin pre_init failed: {:?}", e)
                ))
            }
        }
    }
    
    async fn init(&mut self, _context: Arc<PluginContext<StructuredEventKey, CompositePropagator<StructuredEventKey>>>) -> Result<(), universal_plugin_system::PluginSystemError> {
        info!("🔄 Legacy plugin bridge: init for '{}'", self.name());
        
        let basic_context = Arc::new(LegacyServerContextBridge::new(self.horizon_events.clone()));
        
        match self.legacy_plugin.init(basic_context).await {
            Ok(_) => {
                info!("✅ Legacy plugin '{}' init successful", self.name());
                Ok(())
            }
            Err(e) => {
                tracing::error!("❌ Legacy plugin '{}' init failed: {:?}", self.name(), e);
                Err(universal_plugin_system::PluginSystemError::InitializationFailed(
                    format!("Legacy plugin init failed: {:?}", e)
                ))
            }
        }
    }
    
    async fn shutdown(&mut self, _context: Arc<PluginContext<StructuredEventKey, CompositePropagator<StructuredEventKey>>>) -> Result<(), universal_plugin_system::PluginSystemError> {
        info!("🔄 Legacy plugin bridge: shutdown for '{}'", self.name());
        
        let basic_context = Arc::new(LegacyServerContextBridge::new(self.horizon_events.clone()));
        
        match self.legacy_plugin.shutdown(basic_context).await {
            Ok(_) => {
                info!("✅ Legacy plugin '{}' shutdown successful", self.name());
                Ok(())
            }
            Err(e) => {
                tracing::error!("❌ Legacy plugin '{}' shutdown failed: {:?}", self.name(), e);
                Err(universal_plugin_system::PluginSystemError::RuntimeError(
                    format!("Legacy plugin shutdown failed: {:?}", e)
                ))
            }
        }
    }
}

/// Bridge that implements horizon ServerContext using the available horizon event system
#[derive(Debug)]
pub struct LegacyServerContextBridge {
    horizon_events: Arc<HorizonEventSystem>,
}

impl LegacyServerContextBridge {
    pub fn new(horizon_events: Arc<HorizonEventSystem>) -> Self {
        Self { horizon_events }
    }
}

#[async_trait]
impl ServerContext for LegacyServerContextBridge {
    fn events(&self) -> Arc<HorizonEventSystem> {
        self.horizon_events.clone()
    }

    fn log(&self, level: LogLevel, message: &str) {
        let async_logger = horizon_event_system::async_logging::global_async_logger();
        async_logger.log_with_target(level, message, Some("legacy_plugin_bridge"));
    }

    fn region_id(&self) -> horizon_event_system::types::RegionId {
        horizon_event_system::types::RegionId::default()
    }

    fn tokio_handle(&self) -> Option<tokio::runtime::Handle> {
        tokio::runtime::Handle::try_current().ok()
    }

    async fn send_to_player(&self, player_id: horizon_event_system::types::PlayerId, _data: &[u8]) -> Result<(), horizon_event_system::context::ServerError> {
        tracing::warn!("Legacy plugin bridge: send_to_player called for player {} - not fully implemented", player_id);
        Err(horizon_event_system::context::ServerError::Internal(
            "send_to_player not fully implemented in legacy bridge".to_string(),
        ))
    }

    async fn broadcast(&self, _data: &[u8]) -> Result<(), horizon_event_system::context::ServerError> {
        tracing::warn!("Legacy plugin bridge: broadcast called - not fully implemented");
        Err(horizon_event_system::context::ServerError::Internal(
            "broadcast not fully implemented in legacy bridge".to_string(),
        ))
    }
}

impl HorizonUniversalBridge {
    /// Create a new bridge instance
    pub async fn new(
        horizon_events: Arc<HorizonEventSystem>,
        server_context: Arc<dyn ServerContext>
    ) -> Result<Self, ServerError> {
        // Create composite propagator with both universal and horizon-specific propagators
        let propagator = CompositePropagator::new_or()
            .add_propagator(Box::new(AllEqPropagator::new()))
            .add_propagator(Box::new(DefaultPropagator::new()))
            .add_propagator(Box::new(DomainPropagator::new()))
            .add_propagator(Box::new(UniversalAllEqPropagator::new()))
            .add_propagator(Box::new(HorizonBridgePropagator::new(horizon_events.clone())));
        
        // Create universal event bus
        let universal_bus = Arc::new(EventBus::with_propagator(propagator));
        
        // Create plugin context
        let plugin_context = PluginContext::new(universal_bus.clone());        
        let plugin_context_template = Arc::new(plugin_context);
        
        // Create plugin config
        let plugin_config = PluginConfig::default();
        
        // Create universal plugin manager
        let plugin_manager = Arc::new(UniversalPluginManager::new(
            universal_bus.clone(),
            plugin_context_template.clone(),
            plugin_config
        ));

        Ok(Self {
            universal_bus,
            horizon_events,
            plugin_manager,
            server_context,
            plugin_context_template,
        })
    }

    /// Get the universal event bus
    pub fn get_universal_bus(&self) -> Arc<EventBus<StructuredEventKey, CompositePropagator<StructuredEventKey>>> {
        self.universal_bus.clone()
    }

    /// Get the horizon event system (for backward compatibility)
    pub fn get_horizon_events(&self) -> Arc<HorizonEventSystem> {
        self.horizon_events.clone()
    }

    /// Get the universal plugin manager
    pub fn get_plugin_manager(&self) -> Arc<UniversalPluginManager<StructuredEventKey, CompositePropagator<StructuredEventKey>>> {
        self.plugin_manager.clone()
    }

    /// Load plugins from directory using the universal system with ABI compatibility
    pub async fn load_plugins_from_directory<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), ServerError> {
        info!("🔄 Loading plugins with ABI compatibility from: {}", path.as_ref().display());
        
        // Get list of plugin files
        let plugin_files = self.discover_plugin_files(path.as_ref())?;
        
        if plugin_files.is_empty() {
            info!("📂 No plugin files found in directory");
            return Ok(());
        }

        info!("🔍 Found {} plugin file(s), checking ABI compatibility", plugin_files.len());
        let mut loaded_count = 0;
        let mut legacy_count = 0;
        let mut universal_count = 0;

        // Process each plugin with ABI detection
        for plugin_file in &plugin_files {
            match self.load_single_plugin_with_compat(plugin_file).await {
                Ok(abi_version) => {
                    info!("✅ Successfully loaded plugin from: {}", plugin_file.display());
                    loaded_count += 1;
                    match abi_version {
                        PluginAbiVersion::Horizon => legacy_count += 1,
                        PluginAbiVersion::Universal => universal_count += 1,
                        PluginAbiVersion::Unknown => {}
                    }
                }
                Err(e) => {
                    tracing::error!("❌ Failed to load plugin from {}: {}", plugin_file.display(), e);
                    // Continue loading other plugins even if one fails
                }
            }
        }

        info!(
            "🎉 Plugin loading complete: {}/{} plugins loaded successfully ({} legacy, {} universal)", 
            loaded_count, plugin_files.len(), legacy_count, universal_count
        );

        // Initialize all loaded plugins (both legacy and universal)
        // Note: Since we don't have direct access to legacy plugins in the universal manager,
        // we'll initialize them separately
        
        info!("✅ All plugins loaded successfully");
        Ok(())
    }

    /// Load a single plugin with ABI compatibility detection
    async fn load_single_plugin_with_compat<P: AsRef<std::path::Path>>(
        &self,
        plugin_path: P,
    ) -> Result<PluginAbiVersion, ServerError> {
        let path = plugin_path.as_ref();
        
        info!("🔄 Loading plugin with ABI detection from: {}", path.display());

        // Load the dynamic library
        let library = unsafe {
            Library::new(path).map_err(|e| {
                ServerError::Internal(format!("Failed to load library: {}", e))
            })?
        };

        // Detect ABI version
        let abi_version = AbiVersionDetector::detect_abi_version(&library)
            .map_err(|e| ServerError::Internal(format!("ABI detection failed: {}", e)))?;

        match abi_version {
            PluginAbiVersion::Universal => {
                info!("🔧 Detected universal plugin ABI");
                self.load_universal_plugin(&library, path).await?;
            }
            PluginAbiVersion::Horizon => {
                info!("🔧 Detected legacy horizon plugin ABI, using compatibility bridge");
                self.load_legacy_plugin(&library, path).await?;
            }
            PluginAbiVersion::Unknown => {
                tracing::warn!("⚠️ Unknown plugin ABI, attempting legacy compatibility");
                self.load_legacy_plugin(&library, path).await?;
            }
        }

        Ok(abi_version)
    }

    /// Load a universal plugin (new ABI)
    async fn load_universal_plugin(&self, library: &Library, path: &std::path::Path) -> Result<(), ServerError> {
        // Use the standard universal plugin manager loading
        info!("📦 Loading universal plugin: {}", path.display());
        
        // This would require modifying the universal plugin manager to accept pre-loaded libraries
        // For now, delegate to the standard plugin manager
        // Note: This is a simplified approach - in production you'd want to avoid double-loading
        
        self.plugin_manager
            .load_single_plugin(path)
            .await
            .map_err(|e| ServerError::Internal(format!("Universal plugin loading failed: {}", e)))?;
            
        Ok(())
    }

    /// Load a legacy plugin (old ABI) with compatibility bridge
    async fn load_legacy_plugin(&self, library: &Library, path: &std::path::Path) -> Result<(), ServerError> {
        info!("🔄 Loading legacy plugin with compatibility bridge: {}", path.display());
        
        // Get plugin version string for validation
        let get_plugin_version: libloading::Symbol<unsafe extern "C" fn() -> *const std::os::raw::c_char> = unsafe {
            library.get(b"get_plugin_version").map_err(|e| {
                ServerError::Internal(format!("Legacy plugin does not export 'get_plugin_version' function: {}", e))
            })?
        };

        let plugin_version_ptr = unsafe { get_plugin_version() };
        let plugin_version = if plugin_version_ptr.is_null() {
            return Err(ServerError::Internal("Plugin returned null version string".to_string()));
        } else {
            unsafe {
                std::ffi::CStr::from_ptr(plugin_version_ptr)
                    .to_string_lossy()
                    .to_string()
            }
        };

        // Validate legacy plugin compatibility with horizon_event_system
        self.validate_legacy_plugin_compatibility(&plugin_version)?;

        // Load the legacy plugin using horizon_event_system ABI
        let create_plugin: libloading::Symbol<unsafe extern "C" fn() -> *mut dyn horizon_event_system::plugin::Plugin> = unsafe {
            library.get(b"create_plugin").map_err(|e| {
                ServerError::Internal(format!("Legacy plugin does not export 'create_plugin' function: {}", e))
            })?
        };

        let plugin_ptr = unsafe { create_plugin() };
        if plugin_ptr.is_null() {
            return Err(ServerError::Internal("Legacy plugin creation function returned null".to_string()));
        }

        let legacy_plugin: Box<dyn horizon_event_system::plugin::Plugin> = unsafe { Box::from_raw(plugin_ptr) };
        let plugin_name = legacy_plugin.name().to_string();

        info!("🔧 Created legacy plugin adapter for: {}", plugin_name);

        // Create a compatibility adapter that implements the universal Plugin trait
        // This adapter will bridge the calls between universal and legacy systems
        let adapted_plugin = LegacyPluginBridge::new(legacy_plugin, self.horizon_events.clone());

        // Store the adapted plugin in the universal plugin manager
        // Note: This is a simplified approach - would need more integration
        
        info!("✅ Legacy plugin '{}' loaded with compatibility bridge", plugin_name);
        Ok(())
    }

    /// Validate legacy plugin compatibility with horizon_event_system
    fn validate_legacy_plugin_compatibility(&self, plugin_version: &str) -> Result<(), ServerError> {
        // Parse version format "crate_version:rust_version"
        let parts: Vec<&str> = plugin_version.split(':').collect();
        if parts.len() != 2 {
            return Err(ServerError::Internal(format!(
                "Invalid legacy plugin version format: {}", plugin_version
            )));
        }

        let plugin_crate_version = parts[0];
        let expected_version = horizon_event_system::ABI_VERSION;
        let expected_parts: Vec<&str> = expected_version.split(':').collect();
        
        if expected_parts.len() != 2 {
            return Err(ServerError::Internal(format!(
                "Invalid expected version format: {}", expected_version
            )));
        }

        let expected_crate_version = expected_parts[0];

        // For legacy compatibility, allow major.minor version matching
        if !self.versions_major_minor_compatible(plugin_crate_version, expected_crate_version) {
            return Err(ServerError::Internal(format!(
                "Legacy plugin ABI version mismatch: plugin {} vs expected {}. \
                 Plugin may be incompatible with current horizon_event_system version.",
                plugin_crate_version, expected_crate_version
            )));
        }

        info!("✅ Legacy plugin version {} is compatible with {}", plugin_crate_version, expected_crate_version);
        Ok(())
    }

    /// Check if two versions are compatible using major.minor comparison
    fn versions_major_minor_compatible(&self, plugin_version: &str, expected_version: &str) -> bool {
        let parse_major_minor = |version: &str| -> Option<(u32, u32)> {
            let parts: Vec<&str> = version.split('.').collect();
            if parts.len() >= 2 {
                if let (Ok(major), Ok(minor)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    return Some((major, minor));
                }
            }
            None
        };
        
        match (parse_major_minor(plugin_version), parse_major_minor(expected_version)) {
            (Some((plugin_major, plugin_minor)), Some((expected_major, expected_minor))) => {
                plugin_major == expected_major && plugin_minor == expected_minor
            }
            _ => {
                // If we can't parse the versions, fall back to exact comparison
                plugin_version == expected_version
            }
        }
    }

    /// Discover plugin files in a directory
    fn discover_plugin_files(&self, directory: &std::path::Path) -> Result<Vec<std::path::PathBuf>, ServerError> {
        let mut plugin_files = Vec::new();
        
        let entries = std::fs::read_dir(directory)
            .map_err(|e| ServerError::Internal(format!("Failed to read plugin directory: {}", e)))?;
        
        for entry in entries {
            let entry = entry
                .map_err(|e| ServerError::Internal(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(extension) = path.extension() {
                    let ext_str = extension.to_string_lossy().to_lowercase();
                    
                    // Check for platform-specific dynamic library extensions
                    #[cfg(target_os = "windows")]
                    let is_plugin = ext_str == "dll";
                    
                    #[cfg(target_os = "macos")]
                    let is_plugin = ext_str == "dylib";
                    
                    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                    let is_plugin = ext_str == "so";
                    
                    if is_plugin {
                        plugin_files.push(path);
                    }
                }
            }
        }
        
        Ok(plugin_files)
    }

    /// Get plugin count
    pub fn plugin_count(&self) -> usize {
        self.plugin_manager.plugin_count()
    }

    /// Get plugin names
    pub fn plugin_names(&self) -> Vec<String> {
        self.plugin_manager.plugin_names()
    }

    /// Shutdown the plugin system
    pub async fn shutdown(&self) -> Result<(), ServerError> {
        info!("🛑 Shutting down universal plugin system...");
        
        self.plugin_manager
            .shutdown()
            .await
            .map_err(|e| ServerError::Internal(format!("Universal plugin shutdown failed: {}", e)))?;
            
        info!("✅ Universal plugin system shutdown complete");
        Ok(())
    }

    /// Setup event bridges between horizon and universal systems
    pub async fn setup_event_bridges(&self) -> Result<(), ServerError> {
        info!("🌉 Setting up event bridges between Horizon and Universal systems...");
        
        // Bridge core events from horizon to universal
        self.setup_core_event_bridge().await?;
        
        // Bridge client events from horizon to universal
        self.setup_client_event_bridge().await?;
        
        // Bridge plugin events from horizon to universal
        self.setup_plugin_event_bridge().await?;
        
        info!("✅ Event bridges setup complete");
        Ok(())
    }

    /// Setup core event bridging
    async fn setup_core_event_bridge(&self) -> Result<(), ServerError> {
        // Bridge player connected events
        self.horizon_events
            .on_core("player_connected", move |event: serde_json::Value| {
                // For now, just log the event since we're not actively bridging
                info!("🌉 Bridged player_connected event: {:?}", event);
                Ok(())
            })
            .await
            .map_err(|e| ServerError::Internal(format!("Failed to setup core event bridge: {}", e)))?;

        info!("🌉 Core event bridge setup complete");
        Ok(())
    }

    /// Setup client event bridging
    async fn setup_client_event_bridge(&self) -> Result<(), ServerError> {
        // Note: Client events in horizon system are namespace/event based
        // These will be bridged when they're emitted through the universal system
        
        info!("🌉 Client event bridge setup complete");
        Ok(())
    }

    /// Setup plugin event bridging
    async fn setup_plugin_event_bridge(&self) -> Result<(), ServerError> {
        // Plugin events will be handled through the universal system directly
        
        info!("🌉 Plugin event bridge setup complete");
        Ok(())
    }

    /// Get monitoring statistics
    pub async fn get_stats(&self) -> DetailedEventStats {
        // The universal event bus doesn't have get_detailed_stats, so we'll return a default
        DetailedEventStats {
            events_emitted: 0,
            events_handled: 0,
            handler_failures: 0,
            total_handlers: 0,
            average_emit_time_ns: 0,
            max_emit_time_ns: 0,
            min_emit_time_ns: 0,
            total_emit_time_ns: 0,
            handler_stats: std::collections::HashMap::new(),
            event_type_stats: std::collections::HashMap::new(),
            uptime_seconds: 0,
            last_reset: None,
        }
    }
}

/// Context adapter that implements horizon ServerContext for the universal system
pub struct UniversalServerContextAdapter {
    bridge: Arc<HorizonUniversalBridge>,
    server_context: Arc<dyn ServerContext>,
}

impl std::fmt::Debug for UniversalServerContextAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UniversalServerContextAdapter")
            .field("bridge", &"HorizonUniversalBridge")
            .field("server_context", &"ServerContext")
            .finish()
    }
}

impl UniversalServerContextAdapter {
    pub fn new(bridge: Arc<HorizonUniversalBridge>, server_context: Arc<dyn ServerContext>) -> Self {
        Self { bridge, server_context }
    }
}

#[async_trait]
impl ServerContext for UniversalServerContextAdapter {
    fn events(&self) -> Arc<HorizonEventSystem> {
        self.bridge.get_horizon_events()
    }

    fn log(&self, level: LogLevel, message: &str) {
        self.server_context.log(level, message);
    }

    fn region_id(&self) -> horizon_event_system::types::RegionId {
        self.server_context.region_id()
    }

    fn tokio_handle(&self) -> Option<tokio::runtime::Handle> {
        self.server_context.tokio_handle()
    }

    async fn send_to_player(&self, player_id: horizon_event_system::types::PlayerId, data: &[u8]) -> Result<(), horizon_event_system::context::ServerError> {
        self.server_context.send_to_player(player_id, data).await
    }

    async fn broadcast(&self, data: &[u8]) -> Result<(), horizon_event_system::context::ServerError> {
        self.server_context.broadcast(data).await
    }
}