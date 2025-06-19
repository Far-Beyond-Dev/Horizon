//! Plugin system with safe DLL loading and management
//! 
//! Provides dynamic plugin loading, lifecycle management, and safe
//! cross-DLL communication through the event system.

use event_system::EventSystemImpl;
use types::*;
use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use async_trait::async_trait;

// ============================================================================
// Plugin Manager
// ============================================================================

/// Manages loaded plugins and their lifecycles
pub struct PluginManager {
    /// Event system shared across all plugins
    event_system: Arc<dyn EventSystem>,
    /// Server context shared with plugins
    server_context: Arc<ServerContextImpl>,
    /// Loaded plugins
    plugins: RwLock<HashMap<String, LoadedPlugin>>,
    /// Plugin directory to scan
    plugin_directory: PathBuf,
}

/// A loaded plugin with its library and instance
struct LoadedPlugin {
    /// The plugin instance
    plugin: Box<dyn Plugin>,
    /// The loaded library (kept alive to prevent unloading)
    _library: Library,
    /// Plugin metadata
    metadata: PluginMetadata,
}

/// Plugin metadata
#[derive(Debug, Clone)]
struct PluginMetadata {
    name: String,
    version: String,
    path: PathBuf,
    loaded_at: std::time::SystemTime,
}

impl PluginManager {
    /// Create a new plugin manager
    pub fn new(
        event_system: Arc<dyn EventSystem>,
        plugin_directory: impl AsRef<Path>,
        region_id: RegionId,
    ) -> Self {
        let server_context = Arc::new(ServerContextImpl::new(
            event_system.clone(),
            region_id,
        ));
        
        Self {
            event_system,
            server_context,
            plugins: RwLock::new(HashMap::new()),
            plugin_directory: plugin_directory.as_ref().to_path_buf(),
        }
    }
    
    /// Load a single plugin from a DLL file
    pub async fn load_plugin(&self, plugin_path: impl AsRef<Path>) -> Result<(), PluginError> {
        let plugin_path = plugin_path.as_ref();
        
        info!("Loading plugin from: {}", plugin_path.display());
        
        // Load the dynamic library
        let library = unsafe {
            Library::new(plugin_path).map_err(|e| {
                PluginError::InitializationFailed(format!("Failed to load library: {}", e))
            })?
        };
        
        // Get the plugin creation function
        let create_plugin: Symbol<unsafe extern "C" fn() -> *mut dyn Plugin> = unsafe {
            library.get(b"create_plugin").map_err(|e| {
                PluginError::InitializationFailed(format!("Failed to find create_plugin function: {}", e))
            })?
        };
        
        // Create the plugin instance
        let plugin_ptr = unsafe { create_plugin() };
        if plugin_ptr.is_null() {
            return Err(PluginError::InitializationFailed(
                "create_plugin returned null pointer".to_string(),
            ));
        }
        
        let mut plugin = unsafe { Box::from_raw(plugin_ptr) };
        
        let plugin_name = plugin.name().to_string();
        let plugin_version = plugin.version().to_string();
        
        // Check if plugin is already loaded
        {
            let plugins = self.plugins.read().await;
            if plugins.contains_key(&plugin_name) {
                return Err(PluginError::AlreadyLoaded(plugin_name));
            }
        }
        
        info!("Created plugin: {} v{}", plugin_name, plugin_version);
        
        // Pre-initialize the plugin (register event handlers)
        plugin.pre_init(self.server_context.clone()).await.map_err(|e| {
            error!("Plugin {} pre-initialization failed: {}", plugin_name, e);
            e
        })?;
        
        info!("Plugin {} pre-initialized successfully", plugin_name);
        
        // Initialize the plugin (startup tasks, emit events)
        plugin.init(self.server_context.clone()).await.map_err(|e| {
            error!("Plugin {} initialization failed: {}", plugin_name, e);
            e
        })?;
        
        info!("Plugin {} initialized successfully", plugin_name);
        
        // Store the loaded plugin
        let metadata = PluginMetadata {
            name: plugin_name.clone(),
            version: plugin_version,
            path: plugin_path.to_path_buf(),
            loaded_at: std::time::SystemTime::now(),
        };
        
        let loaded_plugin = LoadedPlugin {
            plugin,
            _library: library,
            metadata,
        };
        
        {
            let mut plugins = self.plugins.write().await;
            plugins.insert(plugin_name.clone(), loaded_plugin);
        }
        
        info!("Plugin {} loaded and initialized successfully", plugin_name);
        
        // Emit plugin loaded event
        self.event_system.emit_namespaced(
            EventId::core("plugin_loaded"),
            &PluginLoadedEvent {
                plugin_name,
                timestamp: event_system::current_timestamp(),
            }
        ).await.map_err(|e| {
            warn!("Failed to emit plugin loaded event: {}", e);
            // Don't fail plugin loading for this
        }).ok();
        
        Ok(())
    }
    
    /// Load all plugins from the plugin directory
    pub async fn load_all_plugins(&self) -> Result<Vec<String>, PluginError> {
        if !self.plugin_directory.exists() {
            warn!("Plugin directory does not exist: {}", self.plugin_directory.display());
            return Ok(Vec::new());
        }
        
        let mut loaded_plugins = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.plugin_directory).await.map_err(|e| {
            PluginError::InitializationFailed(format!("Failed to read plugin directory: {}", e))
        })?;
        
        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            PluginError::InitializationFailed(format!("Failed to read directory entry: {}", e))
        })? {
            let path = entry.path();
            
            // Check if this is a dynamic library
            if let Some(extension) = path.extension() {
                let ext_str = extension.to_string_lossy();
                if ext_str == "so" || ext_str == "dll" || ext_str == "dylib" {
                    match self.load_plugin(&path).await {
                        Ok(()) => {
                            if let Some(file_name) = path.file_stem() {
                                loaded_plugins.push(file_name.to_string_lossy().to_string());
                            }
                        }
                        Err(e) => {
                            error!("Failed to load plugin from {}: {}", path.display(), e);
                            // Continue loading other plugins
                        }
                    }
                }
            }
        }
        
        info!("Loaded {} plugins from {}", loaded_plugins.len(), self.plugin_directory.display());
        Ok(loaded_plugins)
    }
    
    /// Unload a specific plugin
    pub async fn unload_plugin(&self, plugin_name: &str) -> Result<(), PluginError> {
        let mut plugins = self.plugins.write().await;
        
        if let Some(mut loaded_plugin) = plugins.remove(plugin_name) {
            info!("Unloading plugin: {}", plugin_name);
            
            // Shutdown the plugin
            if let Err(e) = loaded_plugin.plugin.shutdown(self.server_context.clone()).await {
                error!("Error shutting down plugin {}: {}", plugin_name, e);
            }
            
            // Emit plugin unloaded event
            self.event_system.emit_namespaced(
                EventId::core("plugin_unloaded"),
                &PluginUnloadedEvent {
                    plugin_name: plugin_name.to_string(),
                    timestamp: event_system::current_timestamp(),
                }
            ).await.map_err(|e| {
                warn!("Failed to emit plugin unloaded event: {}", e);
            }).ok();
            
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
                if let Err(e) = loaded_plugin.plugin.shutdown(self.server_context.clone()).await {
                    error!("Error shutting down plugin {}: {}", plugin_name, e);
                }
            }
        }
        
        info!("All plugins shut down");
        Ok(())
    }
    
    /// Get list of loaded plugins
    pub async fn get_loaded_plugins(&self) -> Vec<String> {
        let plugins = self.plugins.read().await;
        plugins.keys().cloned().collect()
    }
    
    /// Get plugin information
    pub async fn get_plugin_info(&self, plugin_name: &str) -> Option<PluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.get(plugin_name).map(|loaded_plugin| {
            PluginInfo {
                name: loaded_plugin.metadata.name.clone(),
                version: loaded_plugin.metadata.version.clone(),
                path: loaded_plugin.metadata.path.clone(),
                loaded_at: loaded_plugin.metadata.loaded_at,
            }
        })
    }
    
    /// Get the server context (for use by other components)
    pub fn get_server_context(&self) -> Arc<ServerContextImpl> {
        self.server_context.clone()
    }
}

// ============================================================================
// Server Context Implementation
// ============================================================================

/// Implementation of ServerContext for plugins
pub struct ServerContextImpl {
    event_system: Arc<dyn EventSystem>,
    region_id: RegionId,
    players: Arc<RwLock<HashMap<PlayerId, Player>>>,
}

impl ServerContextImpl {
    pub fn new(event_system: Arc<dyn EventSystem>, region_id: RegionId) -> Self {
        Self {
            event_system,
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
    pub async fn update_player_position(&self, player_id: PlayerId, position: Position) -> Result<(), ServerError> {
        let mut players = self.players.write().await;
        if let Some(player) = players.get_mut(&player_id) {
            player.position = position;
            Ok(())
        } else {
            Err(ServerError::Player(format!("Player {} not found", player_id)))
        }
    }
}

#[async_trait]
impl ServerContext for ServerContextImpl {
    fn events(&self) -> Arc<dyn EventSystem> {
        self.event_system.clone()
    }
    
    fn region_id(&self) -> RegionId {
        self.region_id
    }
    
    async fn get_players(&self) -> Result<Vec<Player>, ServerError> {
        let players = self.players.read().await;
        Ok(players.values().cloned().collect())
    }
    
    async fn get_player(&self, id: PlayerId) -> Result<Option<Player>, ServerError> {
        let players = self.players.read().await;
        Ok(players.get(&id).cloned())
    }
    
    async fn send_to_player(&self, player_id: PlayerId, message: &[u8]) -> Result<(), ServerError> {
        // TODO: Implement actual message sending through WebSocket connections
        debug!("Sending message to player {}: {} bytes", player_id, message.len());
        Ok(())
    }
    
    async fn broadcast(&self, message: &[u8]) -> Result<(), ServerError> {
        // TODO: Implement actual broadcasting through WebSocket connections
        debug!("Broadcasting message: {} bytes", message.len());
        Ok(())
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
}

// ============================================================================
// Plugin Information
// ============================================================================

/// Information about a loaded plugin
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub loaded_at: std::time::SystemTime,
}

// ============================================================================
// Plugin Events
// ============================================================================

/// Event emitted when a plugin is loaded
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginLoadedEvent {
    pub plugin_name: String,
    pub timestamp: u64,
}

/// Event emitted when a plugin is unloaded
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginUnloadedEvent {
    pub plugin_name: String,
    pub timestamp: u64,
}

/// Event for inter-plugin communication
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginMessageEvent {
    pub from_plugin: String,
    pub to_plugin: Option<String>, // None for broadcast
    pub message_type: String,
    pub data: serde_json::Value,
}

// ============================================================================
// Plugin Development Utilities
// ============================================================================

/// Helper macro for creating plugins with reduced boilerplate
#[macro_export]
macro_rules! create_plugin {
    ($plugin_type:ty) => {
        /// Plugin creation function - required export
        #[no_mangle]
        pub unsafe extern "C" fn create_plugin() -> *mut dyn $types::Plugin {
            let plugin = Box::new(<$plugin_type>::new());
            Box::into_raw(plugin)
        }
        
        /// Plugin destruction function - required export
        #[no_mangle]
        pub unsafe extern "C" fn destroy_plugin(plugin: *mut dyn $types::Plugin) {
            if !plugin.is_null() {
                let _ = Box::from_raw(plugin);
            }
        }
    };
}

/// Helper trait for simplified plugin development
#[async_trait]
pub trait SimplePlugin: Send + Sync {
    fn name() -> &'static str where Self: Sized;
    fn version() -> &'static str where Self: Sized;
    
    async fn on_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        // Default implementation does nothing
        Ok(())
    }
    
    async fn on_shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        // Default implementation does nothing
        Ok(())
    }
    
    /// Register event handlers - override this method
    async fn register_handlers(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        // Default implementation does nothing
        Ok(())
    }
}

/// Wrapper type to allow implementing the foreign Plugin trait for SimplePlugin types
pub struct SimplePluginWrapper<T: SimplePlugin + 'static> {
    inner: T,
}

impl<T: SimplePlugin + 'static> SimplePluginWrapper<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<T> Plugin for SimplePluginWrapper<T>
where
    T: SimplePlugin + 'static,
{
    fn name(&self) -> &str {
        T::name()
    }
    
    fn version(&self) -> &str {
        T::version()
    }
    
    async fn pre_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        self.inner.register_handlers(context).await
    }
    
    async fn init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        self.inner.on_init(context).await
    }
    
    async fn shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        self.inner.on_shutdown(context).await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use event_system::create_event_system;
    
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
            // Register a test event handler
            context.events().on("test_event", |event: PluginLoadedEvent| {
                println!("Test plugin received event: {:?}", event);
                Ok(())
            }).await.map_err(|e| PluginError::InitializationFailed(e.to_string()))?;
            
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
        let event_system = create_event_system();
        let plugin_manager = PluginManager::new(
            event_system,
            "./test_plugins",
            RegionId::new(),
        );
        
        let loaded_plugins = plugin_manager.get_loaded_plugins().await;
        assert!(loaded_plugins.is_empty());
    }
    
    #[tokio::test]
    async fn test_server_context() {
        let event_system = create_event_system();
        let context = ServerContextImpl::new(event_system.clone(), RegionId::new());
        
        // Test adding and retrieving players
        let player = Player::new("test_player".to_string(), Position::new(0.0, 0.0, 0.0));
        let player_id = player.id;
        
        context.add_player(player.clone()).await;
        
        let retrieved = context.get_player(player_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test_player");
        
        let all_players = context.get_players().await.unwrap();
        assert_eq!(all_players.len(), 1);
    }
}