//! Safe Rust plugin loading using trait objects directly
//! 
//! Since plugins are written in Rust, we can safely pass trait objects across FFI.

use crate::context::ServerContextImpl;
use crate::server::{ConnectionManager, EventProcessor};
use dashmap::DashMap;
use libloading::{Library, Symbol};
use shared_types::*;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Plugin reference for callback dispatch
#[derive(Clone)]
pub struct PluginRef {
    pub plugin: Arc<RwLock<Box<dyn Plugin>>>,
    pub context: Arc<dyn ServerContext>,
}

/// Safe FFI signatures for Rust plugins that export trait objects directly
type CreatePluginFn = unsafe extern "C" fn() -> *mut dyn Plugin;
type DestroyPluginFn = unsafe extern "C" fn(*mut dyn Plugin);

/// Plugin instance with associated context
#[derive(Clone)]
struct PluginInstance {
    plugin: Arc<RwLock<Box<dyn Plugin>>>,
    context: Arc<ServerContextImpl>,
    name: String,
    version: String,
}

/// Manages plugin loading and lifecycle with callback-based events
pub struct PluginLoader {
    /// Server region identifier
    region_id: RegionId,
    /// Active players registry
    players: Arc<DashMap<PlayerId, Player>>,
    /// Connection management system
    connection_manager: Arc<ConnectionManager>,
    /// Event processing system with callback dispatch
    event_processor: Arc<EventProcessor>,
    /// Loaded plugin instances with their contexts
    plugins: Arc<RwLock<Vec<PluginInstance>>>,
    /// Dynamic libraries for loaded plugins (kept alive to prevent unloading)
    plugin_libraries: Vec<Library>,
}

impl PluginLoader {
    /// Create a new plugin loader
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
        }
    }
    
    /// Load a plugin from a dynamic library using safe trait object FFI
    pub async fn load_plugin(&mut self, library_path: impl AsRef<Path>) -> Result<(), ServerError> {
        let lib_path = library_path.as_ref();
        info!("Loading Rust plugin from: {}", lib_path.display());
        
        unsafe {
            // Load the dynamic library
            let lib = Library::new(lib_path)
                .map_err(|e| ServerError::Plugin(PluginError::InitializationFailed(
                    format!("Failed to load library: {}", e)
                )))?;
            
            // Find the plugin creation function that returns *mut dyn Plugin directly
            let create_plugin: Symbol<CreatePluginFn> = lib.get(b"create_plugin")
                .map_err(|e| ServerError::Plugin(PluginError::InitializationFailed(
                    format!("Failed to find create_plugin function: {}", e)
                )))?;
            
            // Call the creation function - this returns *mut dyn Plugin
            let plugin_ptr = create_plugin();
            if plugin_ptr.is_null() {
                return Err(ServerError::Plugin(PluginError::InitializationFailed(
                    "create_plugin returned null pointer".to_string()
                )));
            }
            
            // SAFE: Convert the trait object pointer back to a Box
            // This is safe because:
            // 1. The plugin was created as Box<ConcretePlugin> and converted to Box<dyn Plugin>
            // 2. Both sides use the same Rust trait object layout
            // 3. The pointer is guaranteed to be valid until we call destroy_plugin
            let plugin: Box<dyn Plugin> = Box::from_raw(plugin_ptr);
            
            let plugin_name = plugin.name().to_string();
            let plugin_version = plugin.version().to_string();
            
            info!("Created Rust plugin: {} v{}", plugin_name, plugin_version);
            
            // Get subscribed events before we move the plugin
            let subscribed_events = plugin.subscribed_events();
            
            // Wrap plugin in Arc<RwLock<>>
            let plugin_arc = Arc::new(RwLock::new(plugin));
            
            // Create server context for this plugin
            let context = Arc::new(ServerContextImpl::new(
                self.region_id,
                self.players.clone(),
                self.connection_manager.clone(),
                self.event_processor.clone(),
            ));
            
            // Initialize the plugin
            {
                let mut plugin_guard = plugin_arc.write().await;
                plugin_guard.initialize(context.as_ref()).await.map_err(|e| {
                    error!("Plugin {} initialization failed: {}", plugin_name, e);
                    e
                })?;
            }
            
            // Register plugin callbacks with the event processor
            info!("Registering {} event callbacks for plugin '{}'", subscribed_events.len(), plugin_name);
            self.event_processor
                .register_plugin_callbacks(
                    plugin_arc.clone(),
                    context.clone(),
                    subscribed_events,
                )
                .await
                .map_err(|e| {
                    error!("Failed to register callbacks for plugin {}: {}", plugin_name, e);
                    e
                })?;
            
            // Store plugin instance
            let plugin_instance = PluginInstance {
                plugin: plugin_arc,
                context,
                name: plugin_name.clone(),
                version: plugin_version.clone(),
            };
            
            {
                let mut plugins = self.plugins.write().await;
                plugins.push(plugin_instance);
            }
            
            // Store the library to prevent it from being unloaded
            self.plugin_libraries.push(lib);
            
            info!("Successfully loaded and initialized Rust plugin: {} v{}", plugin_name, plugin_version);
            info!("Plugin '{}' is now registered for event callbacks", plugin_name);
        }
        
        Ok(())

    }

    /// List all loaded plugins
    pub async fn get_loaded_plugins(&self) -> Vec<PluginRef> {
        let plugins = self.plugins.read().await;
        plugins.iter().map(|p| PluginRef {
            plugin: p.plugin.clone(),
            context: p.context.clone(),
        }).collect()
    }

    /// Load all plugins from a directory
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
    pub async fn get_plugin_info(&self) -> Vec<PluginInfo> {
        let plugins = self.plugins.read().await;
        let mut plugin_info = Vec::new();
        
        for plugin_instance in plugins.iter() {
            plugin_info.push(PluginInfo {
                name: plugin_instance.name.clone(),
                version: plugin_instance.version.clone(),
            });
        }
        
        plugin_info
    }
    
    /// Get event processor statistics
    pub async fn get_event_stats(&self) -> EventStats {
        EventStats {
            registered_events: self.event_processor.registered_event_count().await,
            total_callbacks: self.event_processor.total_callback_count().await,
            callback_details: self.event_processor.get_callback_stats().await,
        }
    }
    
    /// Shutdown all loaded plugins gracefully
    pub async fn shutdown_all(&self) -> Result<(), ServerError> {
        info!("Shutting down all plugins...");
        
        let plugins = self.plugins.read().await;
        let mut shutdown_errors = Vec::new();
        
        for plugin_instance in plugins.iter() {
            // Shutdown the plugin
            {
                let mut plugin = plugin_instance.plugin.write().await;
                if let Err(e) = plugin.shutdown(plugin_instance.context.as_ref()).await {
                    error!("Error shutting down plugin {}: {}", plugin_instance.name, e);
                    shutdown_errors.push(e);
                } else {
                    info!("Plugin '{}' shut down successfully", plugin_instance.name);
                }
            }
            
            // Unregister callbacks (best effort)
            if let Err(e) = self.event_processor.unregister_plugin(&plugin_instance.name).await {
                warn!("Failed to unregister callbacks for plugin '{}': {}", plugin_instance.name, e);
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
    
    /// Check if a specific plugin is loaded
    pub async fn is_plugin_loaded(&self, plugin_name: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins.iter().any(|p| p.name == plugin_name)
    }
}

/// Information about a loaded plugin
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
}

/// Statistics about the event callback system
#[derive(Debug, Clone)]
pub struct EventStats {
    pub registered_events: usize,
    pub total_callbacks: usize,
    pub callback_details: std::collections::HashMap<String, usize>,
}





pub struct Car {
    pub make: String,
    pub model: String,
    pub year: u16,
}

pub fn get_car_year(car: &Car) -> u16 {
    car.year
}

fn get_car_make(car: &Car) -> &str {
    &car.make
}