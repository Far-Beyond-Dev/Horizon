//! Plugin system with safe DLL loading compatible with the new clean event system
//!
//! Provides dynamic plugin loading, lifecycle management, and integration
//! with the simplified event system API.

use async_trait::async_trait;
use horizon_event_system::{
    create_horizon_event_system, current_timestamp, EventError, EventSystem, LogLevel, PlayerId, Plugin,
    PluginError, Position, RegionId, ServerContext, ServerError, SimplePlugin,
};
use libloading::{Library, Symbol};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// ============================================================================
// Plugin Manager - Updated for New Event System
// ============================================================================

/// Manages loaded plugins and their lifecycles with the new clean event system
pub struct PluginManager {
    /// Event system shared across all plugins
    horizon_event_system: Arc<EventSystem>,
    /// Server context shared with plugins
    server_context: Arc<ServerContextImpl>,
    /// Loaded plugins
    plugins: RwLock<HashMap<String, LoadedPlugin>>,
    /// Plugin directory to scan
    plugin_directory: PathBuf,
    /// Event routing statistics
    event_stats: Arc<RwLock<EventRoutingStats>>,
}

#[derive(Debug, Default)]
struct EventRoutingStats {
    client_events_routed: u64,
    core_events_routed: u64,
    plugin_events_routed: u64,
    total_handlers_registered: u64,
}

/// A loaded plugin with its library and instance
struct LoadedPlugin {
    /// The plugin instance
    plugin: Box<dyn Plugin>,
    /// The loaded library (kept alive to prevent unloading)
    _library: Library,
    /// Plugin metadata
    metadata: PluginMetadata,
    /// Registered event handlers count
    handler_count: usize,
}

/// A plugin that has been loaded from disk but not yet initialized
struct PartiallyLoadedPlugin {
    /// The plugin instance
    plugin: Box<dyn Plugin>,
    /// The loaded library (kept alive to prevent unloading)
    library: Library,
    /// Plugin metadata
    metadata: PluginMetadata,
    /// Plugin name (cached for convenience)
    name: String,
}

/// Plugin metadata
#[derive(Debug, Clone)]
struct PluginMetadata {
    name: String,
    version: String,
    path: PathBuf,
    loaded_at: std::time::SystemTime,
    capabilities: Vec<String>,
}

impl PluginManager {
    /// Create a new plugin manager with the new event system
    pub fn new(
        horizon_event_system: Arc<EventSystem>,
        plugin_directory: impl AsRef<Path>,
        region_id: RegionId,
    ) -> Self {
        let server_context = Arc::new(ServerContextImpl::new(horizon_event_system.clone(), region_id));

        Self {
            horizon_event_system,
            server_context,
            plugins: RwLock::new(HashMap::new()),
            plugin_directory: plugin_directory.as_ref().to_path_buf(),
            event_stats: Arc::new(RwLock::new(EventRoutingStats::default())),
        }
    }

    /// Load a plugin instance from disk but don't initialize it
    async fn load_plugin_instance(
        &self,
        plugin_path: impl AsRef<Path>,
    ) -> Result<PartiallyLoadedPlugin, PluginError> {
        let plugin_path = plugin_path.as_ref();

        debug!("Loading plugin instance from: {}", plugin_path.display());

        // Load the dynamic library
        let library = unsafe {
            Library::new(plugin_path).map_err(|e| {
                PluginError::InitializationFailed(format!("Failed to load library: {}", e))
            })?
        };

        // Get the plugin creation function
        let create_plugin: Symbol<unsafe extern "C" fn() -> *mut dyn Plugin> = unsafe {
            library.get(b"create_plugin").map_err(|e| {
                PluginError::InitializationFailed(format!(
                    "Failed to find create_plugin function: {}",
                    e
                ))
            })?
        };

        // Create the plugin instance
        let plugin_ptr = unsafe { create_plugin() };
        if plugin_ptr.is_null() {
            return Err(PluginError::InitializationFailed(
                "create_plugin returned null pointer".to_string(),
            ));
        }

        let plugin = unsafe { Box::from_raw(plugin_ptr) };

        let plugin_name = plugin.name().to_string();
        let plugin_version = plugin.version().to_string();

        // Check if plugin is already loaded
        {
            let plugins = self.plugins.read().await;
            if plugins.contains_key(&plugin_name) {
                return Err(PluginError::ExecutionError(format!(
                    "Plugin {} is already loaded",
                    plugin_name
                )));
            }
        }

        debug!(
            "Created plugin instance: {} v{}",
            plugin_name, plugin_version
        );

        // Create metadata
        let metadata = PluginMetadata {
            name: plugin_name.clone(),
            version: plugin_version,
            path: plugin_path.to_path_buf(),
            loaded_at: std::time::SystemTime::now(),
            capabilities: vec![
                "client_events".to_string(),
                "inter_plugin".to_string(),
                "type_safe".to_string(),
            ],
        };

        Ok(PartiallyLoadedPlugin {
            plugin,
            library,
            metadata,
            name: plugin_name,
        })
    }

    /// Load a single plugin with immediate initialization
    ///
    /// WARNING: When loading multiple plugins, use load_all_plugins() instead
    /// to ensure proper two-phase initialization where all plugins register
    /// their handlers before any plugin's init() method is called.
    pub async fn load_plugin(&self, plugin_path: impl AsRef<Path>) -> Result<(), PluginError> {
        let plugin_path = plugin_path.as_ref();

        info!("Loading single plugin from: {}", plugin_path.display());
        warn!("Single plugin loading bypasses two-phase initialization. Consider using load_all_plugins() for proper ordering.");

        // Load the plugin instance
        let mut partial = self.load_plugin_instance(plugin_path).await?;

        // Check if plugin is already loaded (double-check since load_plugin_instance also checks)
        {
            let plugins = self.plugins.read().await;
            if plugins.contains_key(&partial.name) {
                return Err(PluginError::ExecutionError(format!(
                    "Plugin {} is already loaded",
                    partial.name
                )));
            }
        }

        info!(
            "Created plugin: {} v{}",
            partial.name, partial.metadata.version
        );

        // Pre-initialization with handler counting
        let handler_count_before = self.get_total_handlers().await;

        partial
            .plugin
            .pre_init(self.server_context.clone())
            .await
            .map_err(|e| {
                error!("Plugin {} pre-initialization failed: {}", partial.name, e);
                e
            })?;

        let handler_count_after = self.get_total_handlers().await;
        let handlers_registered = handler_count_after - handler_count_before;

        info!(
            "Plugin {} pre-initialized successfully, registered {} handlers",
            partial.name, handlers_registered
        );

        // Initialize the plugin
        partial
            .plugin
            .init(self.server_context.clone())
            .await
            .map_err(|e| {
                error!("Plugin {} initialization failed: {}", partial.name, e);
                e
            })?;

        info!("Plugin {} initialized successfully", partial.name);

        // Store the loaded plugin
        let loaded_plugin = LoadedPlugin {
            plugin: partial.plugin,
            _library: partial.library,
            metadata: partial.metadata,
            handler_count: handlers_registered,
        };

        {
            let mut plugins = self.plugins.write().await;
            plugins.insert(partial.name.clone(), loaded_plugin);
        }

        // Update statistics
        {
            let mut stats = self.event_stats.write().await;
            stats.total_handlers_registered += handlers_registered as u64;
        }

        info!(
            "Plugin {} loaded and initialized successfully",
            partial.name
        );

        // Emit plugin loaded event
        self.horizon_event_system
            .emit_core(
                "plugin_loaded",
                &PluginLoadedEvent {
                    plugin_name: partial.name.clone(),
                    capabilities: vec!["type_safe_events".to_string()],
                    timestamp: current_timestamp(),
                },
            )
            .await
            .map_err(|e| {
                warn!("Failed to emit plugin loaded event: {}", e);
            })
            .ok();

        Ok(())
    }

    /// Enhanced plugin discovery with capability detection
    pub async fn discover_plugins(&self) -> Result<Vec<PluginDiscovery>, PluginError> {
        if !self.plugin_directory.exists() {
            warn!(
                "Plugin directory does not exist: {}",
                self.plugin_directory.display()
            );
            return Ok(Vec::new());
        }

        let mut discoveries = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.plugin_directory)
            .await
            .map_err(|e| {
                PluginError::InitializationFailed(format!("Failed to read plugin directory: {}", e))
            })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            PluginError::InitializationFailed(format!("Failed to read directory entry: {}", e))
        })? {
            let path = entry.path();

            if let Some(extension) = path.extension() {
                let ext_str = extension.to_string_lossy();
                if ext_str == "so" || ext_str == "dll" || ext_str == "dylib" {
                    let discovery = PluginDiscovery {
                        name: path
                            .file_stem()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "unknown".to_string()),
                        path: path.clone(),
                        is_loaded: {
                            let plugins = self.plugins.read().await;
                            plugins.contains_key(
                                &path
                                    .file_stem()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "unknown".to_string()),
                            )
                        },
                        estimated_capabilities: self.estimate_plugin_capabilities(&path).await,
                    };
                    discoveries.push(discovery);
                }
            }
        }

        Ok(discoveries)
    }

    /// Load all plugins with two-phase initialization: all pre_init first, then all init
    pub async fn load_all_plugins(&self) -> Result<Vec<String>, PluginError> {
        let discoveries = self.discover_plugins().await?;
        let mut partially_loaded = Vec::new();
        let mut failed_plugins = Vec::new();

        info!(
            "Starting two-phase plugin loading for {} discovered plugins",
            discoveries.len()
        );

        // Phase 1: Load libraries and create plugin instances
        info!("Phase 1: Loading plugin libraries and creating instances");
        for discovery in discoveries {
            if !discovery.is_loaded {
                match self.load_plugin_instance(&discovery.path).await {
                    Ok(partial) => {
                        info!("Loaded plugin instance: {}", partial.name);
                        partially_loaded.push(partial);
                    }
                    Err(e) => {
                        error!("Failed to load plugin instance {}: {}", discovery.name, e);
                        failed_plugins.push((discovery.name, e));
                    }
                }
            }
        }

        info!(
            "Phase 1 complete: {} plugin instances loaded",
            partially_loaded.len()
        );

        // Phase 2: Call pre_init on all plugins (register all handlers)
        info!("Phase 2: Registering event handlers for all plugins");
        let mut pre_init_failed = Vec::new();
        let mut handler_counts = Vec::new();

        for partial in &mut partially_loaded {
            let handler_count_before = self.get_total_handlers().await;

            match partial.plugin.pre_init(self.server_context.clone()).await {
                Ok(()) => {
                    let handler_count_after = self.get_total_handlers().await;
                    let handlers_registered = handler_count_after - handler_count_before;
                    handler_counts.push(handlers_registered);

                    info!(
                        "Plugin {} pre-initialized successfully, registered {} handlers",
                        partial.name, handlers_registered
                    );
                }
                Err(e) => {
                    error!("Plugin {} pre-initialization failed: {}", partial.name, e);
                    handler_counts.push(0);
                    pre_init_failed.push((partial.name.clone(), e));
                }
            }
        }

        // Remove failed plugins from partially_loaded
        partially_loaded.retain(|p| !pre_init_failed.iter().any(|(name, _)| name == &p.name));
        failed_plugins.extend(pre_init_failed);

        info!(
            "Phase 2 complete: {} plugins pre-initialized",
            partially_loaded.len()
        );

        // Phase 3: Call init on all successfully pre-initialized plugins
        info!("Phase 3: Initializing all plugins");
        let mut loaded_plugins = Vec::new();
        let mut init_failed = Vec::new();

        for (mut partial, handler_count) in
            partially_loaded.into_iter().zip(handler_counts.into_iter())
        {
            match partial.plugin.init(self.server_context.clone()).await {
                Ok(()) => {
                    info!("Plugin {} initialized successfully", partial.name);

                    // Store the fully loaded plugin
                    let loaded_plugin = LoadedPlugin {
                        plugin: partial.plugin,
                        _library: partial.library,
                        metadata: partial.metadata,
                        handler_count,
                    };

                    {
                        let mut plugins = self.plugins.write().await;
                        plugins.insert(partial.name.clone(), loaded_plugin);
                    }

                    // Update statistics
                    {
                        let mut stats = self.event_stats.write().await;
                        stats.total_handlers_registered += handler_count as u64;
                    }

                    // Emit plugin loaded event
                    self.horizon_event_system
                        .emit_core(
                            "plugin_loaded",
                            &PluginLoadedEvent {
                                plugin_name: partial.name.clone(),
                                capabilities: vec!["type_safe_events".to_string()],
                                timestamp: current_timestamp(),
                            },
                        )
                        .await
                        .map_err(|e| {
                            warn!("Failed to emit plugin loaded event: {}", e);
                        })
                        .ok();

                    loaded_plugins.push(partial.name);
                }
                Err(e) => {
                    error!("Plugin {} initialization failed: {}", partial.name, e);
                    init_failed.push((partial.name, e));
                }
            }
        }

        failed_plugins.extend(init_failed);

        if !failed_plugins.is_empty() {
            warn!("Failed to load {} plugins", failed_plugins.len());
            for (name, error) in failed_plugins {
                warn!("  {}: {}", name, error);
            }
        }

        info!(
            "Two-phase loading complete: {} plugins loaded successfully from {}",
            loaded_plugins.len(),
            self.plugin_directory.display()
        );

        Ok(loaded_plugins)
    }

    /// Unload a specific plugin
    pub async fn unload_plugin(&self, plugin_name: &str) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write().await;

        if let Some(mut loaded_plugin) = plugins.remove(plugin_name) {
            info!("Unloading plugin: {}", plugin_name);

            // Shutdown the plugin
            if let Err(e) = loaded_plugin
                .plugin
                .shutdown(self.server_context.clone())
                .await
            {
                error!("Error shutting down plugin {}: {}", plugin_name, e);
            }

            // Emit plugin unloaded event using new API
            self.horizon_event_system
                .emit_core(
                    "plugin_unloaded",
                    &PluginUnloadedEvent {
                        plugin_name: plugin_name.to_string(),
                        timestamp: current_timestamp(),
                    },
                )
                .await
                .map_err(|e| {
                    warn!("Failed to emit plugin unloaded event: {}", e);
                })
                .ok();

            info!("Plugin {} unloaded successfully", plugin_name);
            Ok(())
        } else {
            Err(PluginError::NotFound(plugin_name.to_string()))
        }
    }

    /// Shutdown all plugins
    pub async fn shutdown_all(&self) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write().await;
        let plugin_names: Vec<String> = plugins.keys().cloned().collect();

        info!("Shutting down {} plugins", plugin_names.len());

        for plugin_name in plugin_names {
            if let Some(mut loaded_plugin) = plugins.remove(&plugin_name) {
                if let Err(e) = loaded_plugin
                    .plugin
                    .shutdown(self.server_context.clone())
                    .await
                {
                    error!("Error shutting down plugin {}: {}", plugin_name, e);
                }
            }
        }

        info!("All plugins shut down");
        Ok(())
    }

    /// Get enhanced plugin statistics
    pub async fn get_plugin_stats(&self) -> PluginSystemStats {
        let plugins = self.plugins.read().await;
        let event_stats = self.event_stats.read().await;
        let horizon_event_system_stats = self.horizon_event_system.get_stats().await;

        PluginSystemStats {
            total_plugins: plugins.len(),
            total_handlers: horizon_event_system_stats.total_handlers,
            client_events_routed: event_stats.client_events_routed,
            core_events_routed: event_stats.core_events_routed,
            plugin_events_routed: event_stats.plugin_events_routed,
            plugins: plugins
                .iter()
                .map(|(name, plugin)| PluginStats {
                    name: name.clone(),
                    version: plugin.metadata.version.clone(),
                    handler_count: plugin.handler_count,
                    capabilities: plugin.metadata.capabilities.clone(),
                    loaded_at: plugin.metadata.loaded_at,
                })
                .collect(),
        }
    }

    /// Get list of loaded plugins
    pub async fn get_loaded_plugins(&self) -> Vec<String> {
        let plugins = self.plugins.read().await;
        plugins.keys().cloned().collect()
    }

    /// Get plugin information
    pub async fn get_plugin_info(&self, plugin_name: &str) -> Option<PluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.get(plugin_name).map(|loaded_plugin| PluginInfo {
            name: loaded_plugin.metadata.name.clone(),
            version: loaded_plugin.metadata.version.clone(),
            path: loaded_plugin.metadata.path.clone(),
            loaded_at: loaded_plugin.metadata.loaded_at,
        })
    }

    /// Get total number of registered handlers
    async fn get_total_handlers(&self) -> usize {
        self.horizon_event_system.get_stats().await.total_handlers
    }

    /// Estimate plugin capabilities from file analysis
    async fn estimate_plugin_capabilities(&self, _path: &Path) -> Vec<String> {
        // In a real implementation, this could analyze the binary for symbols
        // or read metadata from embedded resources
        vec![
            "client_events".to_string(),
            "type_safe".to_string(),
            "unknown".to_string(),
        ]
    }

    /// Register core event system handlers for routing statistics
    pub async fn setup_event_routing(&self) -> Result<(), PluginError> {
        let stats = self.event_stats.clone();

        // Monitor client event routing using new API
        self.horizon_event_system
            .on_core(
                "__internal_client_event_routed",
                move |_: serde_json::Value| {
                    let stats = stats.clone();
                    tokio::spawn(async move {
                        let mut stats = stats.write().await;
                        stats.client_events_routed += 1;
                    });
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        Ok(())
    }

    /// Get the enhanced server context
    pub fn get_server_context(&self) -> Arc<ServerContextImpl> {
        self.server_context.clone()
    }
}

// ============================================================================
// Server Context Implementation - Updated for New Event System
// ============================================================================

/// Server context with the new clean event system
pub struct ServerContextImpl {
    horizon_event_system: Arc<EventSystem>,
    region_id: RegionId,
    players: Arc<RwLock<HashMap<PlayerId, Player>>>,
}

impl ServerContextImpl {
    pub fn new(horizon_event_system: Arc<EventSystem>, region_id: RegionId) -> Self {
        Self {
            horizon_event_system,
            region_id,
            players: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a player (called by the main server)
    pub async fn add_player(&self, player: Player) {
        let mut players = self.players.write().await;
        players.insert(player.id, player);
    }

    /// Remove a player (called by the main server)
    pub async fn remove_player(&self, player_id: PlayerId) -> Option<Player> {
        let mut players = self.players.write().await;
        players.remove(&player_id)
    }

    /// Update player position (called by the main server)
    pub async fn update_player_position(
        &self,
        player_id: PlayerId,
        position: Position,
    ) -> Result<(), ServerError> {
        let mut players = self.players.write().await;
        if let Some(player) = players.get_mut(&player_id) {
            player.position = position;
            Ok(())
        } else {
            Err(ServerError::Internal(format!(
                "Player {} not found",
                player_id
            )))
        }
    }
}

#[async_trait]
impl ServerContext for ServerContextImpl {
    fn events(&self) -> Arc<EventSystem> {
        self.horizon_event_system.clone()
    }

    fn region_id(&self) -> RegionId {
        self.region_id
    }

    fn log(&self, level: LogLevel, message: &str) {
        match level {
            LogLevel::Error => error!("{}", message),
            LogLevel::Warn => warn!("{}", message),
            LogLevel::Info => info!("{}", message),
            LogLevel::Debug => debug!("{}", message),
            LogLevel::Trace => tracing::trace!("{}", message),
        }
    }

    async fn send_to_player(&self, player_id: PlayerId, data: &[u8]) -> Result<(), ServerError> {
        debug!(
            "Sending message to player {}: {} bytes",
            player_id,
            data.len()
        );
        // TODO: Implement actual WebSocket message sending
        Ok(())
    }

    async fn broadcast(&self, data: &[u8]) -> Result<(), ServerError> {
        debug!("Broadcasting message: {} bytes", data.len());
        // TODO: Implement actual WebSocket broadcasting
        Ok(())
    }
}

// ============================================================================
// Game Types (Moved from Event System)
// ============================================================================

/// Player information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub position: Position,
    pub metadata: HashMap<String, String>,
}

impl Player {
    pub fn new(name: String, position: Position) -> Self {
        Self {
            id: PlayerId::new(),
            name,
            position,
            metadata: HashMap::new(),
        }
    }
}

// ============================================================================
// Enhanced Statistics and Discovery
// ============================================================================

#[derive(Debug, Clone)]
pub struct PluginDiscovery {
    pub name: String,
    pub path: PathBuf,
    pub is_loaded: bool,
    pub estimated_capabilities: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PluginSystemStats {
    pub total_plugins: usize,
    pub total_handlers: usize,
    pub client_events_routed: u64,
    pub core_events_routed: u64,
    pub plugin_events_routed: u64,
    pub plugins: Vec<PluginStats>,
}

#[derive(Debug, Clone)]
pub struct PluginStats {
    pub name: String,
    pub version: String,
    pub handler_count: usize,
    pub capabilities: Vec<String>,
    pub loaded_at: std::time::SystemTime,
}

/// Information about a loaded plugin
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub loaded_at: std::time::SystemTime,
}

/// Enhanced plugin loaded event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLoadedEvent {
    pub plugin_name: String,
    pub capabilities: Vec<String>,
    pub timestamp: u64,
}

/// Plugin unloaded event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUnloadedEvent {
    pub plugin_name: String,
    pub timestamp: u64,
}

/// Event for inter-plugin communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMessageEvent {
    pub from_plugin: String,
    pub to_plugin: Option<String>, // None for broadcast
    pub message_type: String,
    pub data: serde_json::Value,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Create a new plugin manager with default event system
pub fn create_plugin_manager(
    plugin_directory: impl AsRef<Path>,
    region_id: RegionId,
) -> PluginManager {
    let horizon_event_system = create_horizon_event_system();
    PluginManager::new(horizon_event_system, plugin_directory, region_id)
}

/// Create a plugin manager with a specific event system
pub fn create_plugin_manager_with_events(
    horizon_event_system: Arc<EventSystem>,
    plugin_directory: impl AsRef<Path>,
    region_id: RegionId,
) -> PluginManager {
    PluginManager::new(horizon_event_system, plugin_directory, region_id)
}

// ============================================================================
// Tests - Updated for New API
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_event_system::create_horizon_event_system;

    // Mock plugin for testing
    struct TestPlugin {
        name: String,
        initialized: bool,
    }

    impl TestPlugin {
        fn new() -> Self {
            Self {
                name: "test_plugin".to_string(),
                initialized: false,
            }
        }
    }

    #[async_trait]
    impl Plugin for TestPlugin {
        fn name(&self) -> &str {
            &self.name
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        async fn pre_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
            // Register a test event handler using the new clean API
            context
                .events()
                .on_core("test_event", |event: PluginLoadedEvent| {
                    println!("Test plugin received event: {:?}", event);
                    Ok(())
                })
                .await
                .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

            Ok(())
        }

        async fn init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
            self.initialized = true;
            context.log(LogLevel::Info, "Test plugin initialized");
            Ok(())
        }

        async fn shutdown(&mut self, _context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
            self.initialized = false;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_plugin_manager_creation() {
        let horizon_event_system = create_horizon_event_system();
        let plugin_manager = PluginManager::new(horizon_event_system, "./test_plugins", RegionId::new());

        let loaded_plugins = plugin_manager.get_loaded_plugins().await;
        assert!(loaded_plugins.is_empty());
    }

    #[tokio::test]
    async fn test_server_context() {
        let horizon_event_system = create_horizon_event_system();
        let context = ServerContextImpl::new(horizon_event_system.clone(), RegionId::new());

        // Test adding and retrieving players
        let player = Player::new("test_player".to_string(), Position::new(0.0, 0.0, 0.0));
        let player_id = player.id;

        context.add_player(player.clone()).await;

        // Since we don't have get_player in the new ServerContext trait,
        // we'll test through the internal implementation
        let players = context.players.read().await;
        assert!(players.contains_key(&player_id));
        assert_eq!(players.get(&player_id).unwrap().name, "test_player");
        assert_eq!(players.len(), 1);
    }

    #[tokio::test]
    async fn test_plugin_discovery() {
        let horizon_event_system = create_horizon_event_system();
        let plugin_manager = PluginManager::new(horizon_event_system, "./test_plugins", RegionId::new());

        // Test plugin discovery (will be empty if no plugins exist)
        let discoveries = plugin_manager.discover_plugins().await.unwrap();
        assert!(discoveries.len() >= 0); // May be empty in test environment
    }

    #[tokio::test]
    async fn test_plugin_stats() {
        let horizon_event_system = create_horizon_event_system();
        let plugin_manager = PluginManager::new(horizon_event_system, "./test_plugins", RegionId::new());

        let stats = plugin_manager.get_plugin_stats().await;
        assert_eq!(stats.total_plugins, 0); // No plugins loaded yet
        assert_eq!(stats.plugins.len(), 0);
    }

    #[tokio::test]
    async fn test_new_event_api_integration() {
        let horizon_event_system = create_horizon_event_system();
        let context = ServerContextImpl::new(horizon_event_system.clone(), RegionId::new());

        // Test the new clean API
        context
            .events()
            .on_core("plugin_test", |event: serde_json::Value| {
                println!("Plugin manager test event: {:?}", event);
                Ok(())
            })
            .await
            .unwrap();

        // Test emission
        context
            .events()
            .emit_core(
                "plugin_test",
                &serde_json::json!({
                    "test": "plugin_system_integration"
                }),
            )
            .await
            .unwrap();

        // Test client event handling for plugins
        context
            .events()
            .on_client("test_plugin", "test_action", |event: serde_json::Value| {
                println!("Plugin handling client event: {:?}", event);
                Ok(())
            })
            .await
            .unwrap();

        context
            .events()
            .emit_client(
                "test_plugin",
                "test_action",
                &serde_json::json!({
                    "action": "test_action_data"
                }),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_two_phase_loading() {
        let horizon_event_system = create_horizon_event_system();
        let plugin_manager = PluginManager::new(horizon_event_system, "./test_plugins", RegionId::new());

        // Test the two-phase loading process
        let loaded_plugins = plugin_manager.load_all_plugins().await.unwrap();

        // Should return empty list since no actual plugin files exist in test environment
        assert!(loaded_plugins.len() >= 0);

        let stats = plugin_manager.get_plugin_stats().await;
        assert_eq!(stats.total_plugins, loaded_plugins.len());
    }
}
