//! Dynamic plugin loading and management system
//! 
//! Handles loading plugins from dynamic libraries, initializing them,
//! and managing their lifecycle.

use crate::context::ServerContextImpl;
use crate::server::{ConnectionManager, EventProcessor};
use dashmap::DashMap;
use libloading::{Library, Symbol};
use shared_types::*;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Manages dynamic plugin loading and lifecycle
/// 
/// The PluginLoader handles:
/// - Loading plugins from dynamic libraries
/// - Plugin initialization and shutdown
/// - Event subscription management
/// - Plugin error handling and recovery
pub struct PluginLoader {
    /// Server region identifier
    region_id: RegionId,
    /// Active players registry
    players: Arc<DashMap<PlayerId, Player>>,
    /// Connection management system
    connection_manager: Arc<ConnectionManager>,
    /// Event processing system
    event_processor: Arc<EventProcessor>,
    /// Loaded plugin instances
    plugins: Arc<RwLock<Vec<Arc<RwLock<Box<dyn Plugin>>>>>>,
    /// Dynamic libraries for loaded plugins
    plugin_libraries: Vec<Library>,
    /// Event subscription mappings (event_id -> plugin indices)
    event_subscriptions: Arc<DashMap<EventId, Vec<usize>>>,
}

impl PluginLoader {
    /// Create a new plugin loader
    /// 
    /// # Arguments
    /// * `region_id` - Server region identifier
    /// * `players` - Shared player registry
    /// * `connection_manager` - Connection management system
    /// * `event_processor` - Event processing system
    pub fn new(
        region_id: RegionId,
        players: Arc<DashMap<PlayerId, Player>>,
        connection_manager: Arc<ConnectionManager>,
        event_processor: Arc<EventProcessor>,
    ) -> Self {
        Self {
            region_id,
            players,
            connection_manager,
            event_processor,
            plugins: Arc::new(RwLock::new(Vec::new())),
            plugin_libraries: Vec::new(),
            event_subscriptions: Arc::new(DashMap::new()),
        }
    }
    
    /// Load a plugin from a dynamic library
    /// 
    /// This method:
    /// 1. Loads the dynamic library
    /// 2. Finds and calls the plugin creation function
    /// 3. **Starts event processing for the plugin FIRST**
    /// 4. Initializes the plugin with server context
    /// 5. Registers event subscriptions
    /// 
    /// # Arguments
    /// * `library_path` - Path to the plugin dynamic library
    /// 
    /// # Returns
    /// Result indicating success or failure of plugin loading
    /// 
    /// # Safety
    /// This function uses unsafe code to load dynamic libraries and call foreign functions.
    /// The plugin must implement the required interface correctly.
    /// 
    /// # Errors
    /// Returns `ServerError::Plugin` if:
    /// - The library cannot be loaded
    /// - The required symbols are not found
    /// - Plugin initialization fails
    pub async fn load_plugin(&mut self, library_path: impl AsRef<Path>) -> Result<(), ServerError> {
        let lib_path = library_path.as_ref();
        info!("Loading plugin from: {}", lib_path.display());
        
        unsafe {
            // Load the dynamic library
            let lib = Library::new(lib_path)
                .map_err(|e| ServerError::Plugin(PluginError::InitializationFailed(
                    format!("Failed to load library: {}", e)
                )))?;
            
            // Find the plugin creation function
            let create_plugin: Symbol<unsafe extern "C" fn() -> Box<dyn Plugin>> = lib.get(b"create_plugin")
                .map_err(|e| ServerError::Plugin(PluginError::InitializationFailed(
                    format!("Failed to find create_plugin function: {}", e)
                )))?;
            
            // Create the plugin instance
            let plugin = create_plugin();
            let plugin_name = plugin.name();
            let plugin_version = plugin.version();
            
            info!("Created plugin: {} v{}", plugin_name, plugin_version);
            
            // Get subscribed events before we move the plugin
            let subscribed_events = plugin.subscribed_events();
            
            // Store the plugin and get its index
            let plugin_index = {
                let mut plugins = self.plugins.write().await;
                let index = plugins.len();
                plugins.push(Arc::new(RwLock::new(plugin)));
                index
            };
            
            // Register event subscriptions
            for event_id in subscribed_events {
                self.event_subscriptions
                    .entry(event_id.clone())
                    .or_insert_with(Vec::new)
                    .push(plugin_index);
                info!("Plugin {} subscribed to event: {}", plugin_name, event_id);
            }
            
            // **CRITICAL FIX: Start event processing BEFORE initializing the plugin**
            // This ensures receivers are active when the plugin tries to emit events during init
            self.start_plugin_event_processing(plugin_index).await;
            
            // Now initialize the plugin (it can safely emit events)
            {
                let plugins = self.plugins.read().await;
                let plugin_arc = plugins[plugin_index].clone();
                let mut plugin = plugin_arc.write().await;
                let context = ServerContextImpl::new(
                    self.region_id,
                    self.players.clone(),
                    self.connection_manager.clone(),
                    self.event_processor.clone()
                );
                
                plugin.initialize(&context).await.map_err(|e| {
                    error!("Plugin {} initialization failed: {}", plugin_name, e);
                    e
                })?;
            }
            
            // Store the library to prevent it from being unloaded
            self.plugin_libraries.push(lib);
            
            info!("Successfully loaded and initialized plugin: {} v{}", plugin_name, plugin_version);
        }
        
        Ok(())
    }
    
    /// Start event processing for a specific plugin
    /// 
    /// # Arguments
    /// * `plugin_index` - Index of the plugin in the plugins vector
    async fn start_plugin_event_processing(&self, plugin_index: usize) {
        let mut event_receiver = self.event_processor.subscribe();
        let plugins = self.plugins.clone();
        let event_subscriptions = self.event_subscriptions.clone();
        let players = self.players.clone();
        let connection_manager = self.connection_manager.clone();
        let event_processor = self.event_processor.clone();
        let region_id = self.region_id;
        
        tokio::spawn(async move {
            info!("Started event processing for plugin index {}", plugin_index);
            
            while let Ok((event_id, event)) = event_receiver.recv().await {
                // Check if this plugin is subscribed to this event
                if let Some(subscriber_indices) = event_subscriptions.get(&event_id) {
                    if subscriber_indices.contains(&plugin_index) {
                        let plugins_guard = plugins.read().await;
                        
                        if let Some(plugin_arc) = plugins_guard.get(plugin_index) {
                            let plugin_arc = plugin_arc.clone();
                            let event_id = event_id.clone();
                            let event = event.clone();
                            let players = players.clone();
                            let connection_manager = connection_manager.clone();
                            let event_processor = event_processor.clone();
                            
                            // Process the event in a separate task to avoid blocking
                            tokio::spawn(async move {
                                let mut plugin = plugin_arc.write().await;
                                let context = ServerContextImpl::new(
                                    region_id,
                                    players,
                                    connection_manager,
                                    event_processor
                                );
                                
                                if let Err(e) = plugin.handle_event(&event_id, event.as_ref(), &context).await {
                                    error!("Plugin error handling event {}: {}", event_id, e);
                                }
                            });
                        }
                    }
                }
            }
        });
    }
    
    /// Load all plugins from a directory
    /// 
    /// Scans the specified directory for dynamic library files and attempts
    /// to load each one as a plugin.
    /// 
    /// # Arguments
    /// * `plugin_dir` - Directory path containing plugin libraries
    /// 
    /// # Returns
    /// Result with the number of successfully loaded plugins
    pub async fn load_plugins_from_directory(&mut self, plugin_dir: impl AsRef<Path>) -> Result<usize, ServerError> {
        let plugin_dir = plugin_dir.as_ref();
        
        if !plugin_dir.exists() {
            warn!("Plugin directory does not exist: {}", plugin_dir.display());
            return Ok(0);
        }
        
        let mut loaded_count = 0;
        let mut entries = tokio::fs::read_dir(plugin_dir).await
            .map_err(|e| ServerError::Plugin(PluginError::InitializationFailed(
                format!("Failed to read plugin directory: {}", e)
            )))?;
        
        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            ServerError::Plugin(PluginError::InitializationFailed(
                format!("Failed to read directory entry: {}", e)
            ))
        })? {
            let path = entry.path();
            
            // Check if this looks like a dynamic library
            if let Some(extension) = path.extension() {
                let ext_str = extension.to_string_lossy();
                if ext_str == "so" || ext_str == "dll" || ext_str == "dylib" {
                    match self.load_plugin(&path).await {
                        Ok(()) => {
                            loaded_count += 1;
                        }
                        Err(e) => {
                            error!("Failed to load plugin from {}: {}", path.display(), e);
                            // Continue loading other plugins
                        }
                    }
                }
            }
        }
        
        info!("Loaded {} plugins from {}", loaded_count, plugin_dir.display());
        Ok(loaded_count)
    }
    
    /// Get information about all loaded plugins
    /// 
    /// # Returns
    /// Vector of plugin information including name, version, and description
    pub async fn get_plugin_info(&self) -> Vec<PluginInfo> {
        let plugins = self.plugins.read().await;
        let mut plugin_info = Vec::new();
        
        for plugin_arc in plugins.iter() {
            let plugin = plugin_arc.read().await;
            plugin_info.push(PluginInfo {
                name: plugin.name().to_owned(),
                version: plugin.version().to_owned(),
            });
        }
        
        plugin_info
    }
    
    /// Shutdown all loaded plugins gracefully
    /// 
    /// Calls the shutdown method on each plugin and handles any errors.
    /// 
    /// # Returns
    /// Result indicating overall success or any critical failures
    pub async fn shutdown_all(&self) -> Result<(), ServerError> {
        info!("Shutting down all plugins...");
        
        let plugins = self.plugins.read().await;
        let mut shutdown_errors = Vec::new();
        
        for (index, plugin_arc) in plugins.iter().enumerate() {
            let mut plugin = plugin_arc.write().await;
            let context = ServerContextImpl::new(
                self.region_id,
                self.players.clone(),
                self.connection_manager.clone(),
                self.event_processor.clone()
            );
            
            if let Err(e) = plugin.shutdown(&context).await {
                error!("Error shutting down plugin {}: {}", index, e);
                shutdown_errors.push(e);
            } else {
                info!("Plugin {} shut down successfully", plugin.name());
            }
        }
        
        if !shutdown_errors.is_empty() {
            return Err(ServerError::Plugin(PluginError::ExecutionError(
                format!("Failed to shutdown {} plugins", shutdown_errors.len())
            )));
        }
        
        info!("All plugins shut down successfully");
        Ok(())
    }
    
    /// Get the number of loaded plugins
    pub async fn plugin_count(&self) -> usize {
        self.plugins.read().await.len()
    }
}

/// Information about a loaded plugin
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
}