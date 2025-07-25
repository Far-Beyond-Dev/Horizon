//! Plugin manager implementation for loading and managing dynamic plugins.

use crate::error::PluginSystemError;
use dashmap::DashMap;
use horizon_event_system::plugin::Plugin;
use horizon_event_system::{EventSystem, context::ServerContext, LogLevel};
use libloading::{Library, Symbol};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info, warn};


/// Minimal server context for plugin initialization and testing.
///
/// In production, use a context that provides real region and player communication.
#[derive(Debug, Clone)]
struct BasicServerContext {
    event_system: Arc<EventSystem>,
    region_id: horizon_event_system::types::RegionId,
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl BasicServerContext {
    /// Create a new basic context with a specific region.
    fn new(event_system: Arc<EventSystem>) -> Self {
        Self {
            event_system,
            region_id: horizon_event_system::types::RegionId::default(),
            tokio_handle: tokio::runtime::Handle::try_current().ok(),
        }
    }

    /// Create a context with a custom region id.
    fn with_region(event_system: Arc<EventSystem>, region_id: horizon_event_system::types::RegionId) -> Self {
        Self { 
            event_system, 
            region_id,
            tokio_handle: tokio::runtime::Handle::try_current().ok(),
        }
    }

    /// Create a context with an explicit tokio handle.
    fn with_tokio_handle(event_system: Arc<EventSystem>, tokio_handle: tokio::runtime::Handle) -> Self {
        Self {
            event_system,
            region_id: horizon_event_system::types::RegionId::default(),
            tokio_handle: Some(tokio_handle),
        }
    }
}

#[async_trait::async_trait]
impl ServerContext for BasicServerContext {
    fn events(&self) -> Arc<EventSystem> {
        self.event_system.clone()
    }

    fn log(&self, level: LogLevel, message: &str) {
        match level {
            LogLevel::Error => error!("{}", message),
            LogLevel::Warn => warn!("{}", message),
            LogLevel::Info => info!("{}", message),
            LogLevel::Debug => tracing::debug!("{}", message),
            LogLevel::Trace => tracing::trace!("{}", message),
        }
    }


    fn region_id(&self) -> horizon_event_system::types::RegionId {
        self.region_id
    }

    async fn send_to_player(&self, player_id: horizon_event_system::types::PlayerId, _data: &[u8]) -> Result<(), horizon_event_system::context::ServerError> {
        warn!("send_to_player called in BasicServerContext (player_id: {player_id}) - not implemented");
        Err(horizon_event_system::context::ServerError::Internal(
            "Player communication is not available in BasicServerContext".to_string(),
        ))
    }

    async fn broadcast(&self, _data: &[u8]) -> Result<(), horizon_event_system::context::ServerError> {
        warn!("broadcast called in BasicServerContext - not implemented");
        Err(horizon_event_system::context::ServerError::Internal(
            "Broadcast is not available in BasicServerContext".to_string(),
        ))
    }

    fn tokio_handle(&self) -> Option<tokio::runtime::Handle> {
        self.tokio_handle.clone()
    }
}

/// Information about a loaded plugin
pub struct LoadedPlugin {
    /// The name of the plugin
    pub name: String,
    /// The loaded library
    pub library: Library,
    /// The plugin instance (boxed for dynamic dispatch)
    pub plugin: Box<dyn Plugin + Send + Sync>,
}

/// Plugin manager for loading and managing dynamic plugins.
///
/// The `PluginManager` handles the complete lifecycle of plugins including:
/// - Discovery of plugin files in specified directories
/// - Dynamic loading of plugin libraries
/// - Plugin initialization and registration with the event system
/// - Plugin cleanup and shutdown
/// - Error handling and isolation between plugins
pub struct PluginManager {
    /// Event system for plugin communication
    event_system: Arc<EventSystem>,
    /// Map of loaded plugins by name
    loaded_plugins: DashMap<String, LoadedPlugin>,
}

impl PluginManager {
    /// Creates a new plugin manager with the given event system.
    ///
    /// # Arguments
    ///
    /// * `event_system` - The event system that plugins will use for communication
    ///
    /// # Returns
    ///
    /// A new `PluginManager` instance ready to load plugins.
    pub fn new(event_system: Arc<EventSystem>) -> Self {
        Self {
            event_system,
            loaded_plugins: DashMap::new(),
        }
    }

    /// Loads all plugins from the specified directory.
    ///
    /// This method performs a two-phase initialization:
    /// 1. Pre-initialization phase: Load libraries and create plugin instances
    /// 2. Initialization phase: Register event handlers and complete setup
    ///
    /// # Arguments
    ///
    /// * `plugin_directory` - Path to the directory containing plugin files
    ///
    /// # Returns
    ///
    /// `Ok(())` if all plugins were loaded successfully, or a `PluginSystemError`
    /// if any plugin failed to load.
    pub async fn load_plugins_from_directory<P: AsRef<Path>>(
        &self,
        plugin_directory: P,
    ) -> Result<(), PluginSystemError> {
        let dir_path = plugin_directory.as_ref();
        
        if !dir_path.exists() {
            warn!("Plugin directory does not exist: {}", dir_path.display());
            return Ok(());
        }

        if !dir_path.is_dir() {
            return Err(PluginSystemError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                format!("Plugin path is not a directory: {}", dir_path.display()),
            )));
        }

        info!("🔌 Loading plugins from: {}", dir_path.display());

        // Phase 1: Discover and load plugin files
        let plugin_files = self.discover_plugin_files(dir_path)?;
        
        if plugin_files.is_empty() {
            info!("📂 No plugin files found in directory");
            return Ok(());
        }

        info!("🔍 Found {} plugin file(s)", plugin_files.len());
        let plugin_count = plugin_files.len();

        // Phase 2: Load each plugin
        let mut loaded_count = 0;
        for plugin_file in &plugin_files {
            match self.load_single_plugin(plugin_file).await {
                Ok(plugin_name) => {
                    info!("✅ Successfully loaded plugin: {}", plugin_name);
                    loaded_count += 1;
                }
                Err(e) => {
                    error!("❌ Failed to load plugin from {}: {}", plugin_file.display(), e);
                    // Continue loading other plugins even if one fails
                }
            }
        }

        // Phase 3: Initialize all loaded plugins
        self.initialize_plugins().await?;

        info!("🎉 Plugin loading complete: {}/{} plugins loaded successfully", 
              loaded_count, plugin_count);

        Ok(())
    }

    /// Discovers plugin files in the given directory.
    ///
    /// Looks for files with platform-specific dynamic library extensions
    /// (.dll on Windows, .so on Unix-like systems, .dylib on macOS).
    ///
    /// # Arguments
    ///
    /// * `directory` - Path to search for plugin files
    ///
    /// # Returns
    ///
    /// A vector of paths to potential plugin files.
    fn discover_plugin_files<P: AsRef<Path>>(
        &self,
        directory: P,
    ) -> Result<Vec<PathBuf>, PluginSystemError> {
        let mut plugin_files = Vec::new();
        
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
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

    /// Loads a single plugin from the specified file.
    ///
    /// # Arguments
    ///
    /// * `plugin_path` - Path to the plugin library file
    ///
    /// # Returns
    ///
    /// The name of the loaded plugin, or a `PluginSystemError` if loading failed.
    async fn load_single_plugin<P: AsRef<Path>>(
        &self,
        plugin_path: P,
    ) -> Result<String, PluginSystemError> {
        let path = plugin_path.as_ref();
        
        info!("🔄 Loading plugin from: {}", path.display());

        // Load the dynamic library
        let library = unsafe {
            Library::new(path).map_err(|e| {
                PluginSystemError::LibraryError(format!("Failed to load library: {}", e))
            })?
        };

        // Look for the plugin creation function
        let create_plugin: Symbol<unsafe extern "C" fn() -> *mut dyn Plugin> = unsafe {
            library.get(b"create_plugin").map_err(|e| {
                PluginSystemError::LoadingError(format!(
                    "Plugin does not export 'create_plugin' function: {}", e
                ))
            })?
        };

        // Create the plugin instance
        let plugin_ptr = unsafe { create_plugin() };
        if plugin_ptr.is_null() {
            return Err(PluginSystemError::LoadingError(
                "Plugin creation function returned null".to_string(),
            ));
        }

        let plugin = unsafe { Box::from_raw(plugin_ptr) };
        
        // Get plugin name for registration
        let plugin_name = plugin.name().to_string();

        // Check if plugin already exists
        if self.loaded_plugins.contains_key(&plugin_name) {
            return Err(PluginSystemError::PluginAlreadyExists(plugin_name));
        }

        // Store the loaded plugin
        let loaded_plugin = LoadedPlugin {
            name: plugin_name.clone(),
            library,
            plugin,
        };

        self.loaded_plugins.insert(plugin_name.clone(), loaded_plugin);
        
        Ok(plugin_name)
    }

    /// Initializes all loaded plugins.
    ///
    /// This method calls the initialization methods on all loaded plugins
    /// in a safe manner, isolating any panics or errors to individual plugins.
    async fn initialize_plugins(&self) -> Result<(), PluginSystemError> {
        info!("🔧 Initializing {} loaded plugins", self.loaded_plugins.len());

        let context = Arc::new(BasicServerContext::new(self.event_system.clone()));

        // Phase 1: Pre-initialization (register handlers)
        let plugin_names: Vec<String> = self.loaded_plugins.iter().map(|entry| entry.key().clone()).collect();
        
        for plugin_name in &plugin_names {
            info!("🔧 Pre-initializing plugin: {}", plugin_name);

            if let Some(mut loaded_plugin) = self.loaded_plugins.get_mut(plugin_name) {
                match loaded_plugin.plugin.pre_init(context.clone()).await {
                    Ok(_) => {
                        info!("📡 Event handlers registered for plugin: {}", plugin_name);
                    }
                    Err(e) => {
                        error!("❌ Failed to register handlers for plugin {}: {:?}", plugin_name, e);
                        continue;
                    }
                }
            }
        }

        // Phase 2: Full initialization
        for plugin_name in &plugin_names {
            info!("🔧 Initializing plugin: {}", plugin_name);

            if let Some(mut loaded_plugin) = self.loaded_plugins.get_mut(plugin_name) {
                match loaded_plugin.plugin.init(context.clone()).await {
                    Ok(_) => {
                        info!("✅ Plugin initialized successfully: {}", plugin_name);
                    }
                    Err(e) => {
                        error!("❌ Plugin initialization failed for {}: {:?}", plugin_name, e);
                        continue;
                    }
                }
            }
        }

        Ok(())
    }

    /// Shuts down all loaded plugins and cleans up resources.
    ///
    /// This method should be called when the server is shutting down to ensure
    /// all plugins have a chance to clean up their resources properly.
    pub async fn shutdown(&self) -> Result<(), PluginSystemError> {
        info!("🛑 Shutting down {} plugins", self.loaded_plugins.len());

        let context = Arc::new(BasicServerContext::new(self.event_system.clone()));

        // Call shutdown on all plugins
        let plugin_names: Vec<String> = self.loaded_plugins.iter().map(|entry| entry.key().clone()).collect();
        
        for plugin_name in &plugin_names {
            info!("🛑 Shutting down plugin: {}", plugin_name);

            if let Some(mut loaded_plugin) = self.loaded_plugins.get_mut(plugin_name) {
                match loaded_plugin.plugin.shutdown(context.clone()).await {
                    Ok(_) => {
                        info!("✅ Plugin shutdown completed: {}", plugin_name);
                    }
                    Err(e) => {
                        error!("❌ Plugin shutdown failed for {}: {:?}", plugin_name, e);
                        // Continue shutting down other plugins
                    }
                }
            }
        }

        // Clear all loaded plugins
        self.loaded_plugins.clear();
        info!("🧹 Plugin cleanup completed");

        Ok(())
    }

    /// Gets the number of currently loaded plugins.
    pub fn plugin_count(&self) -> usize {
        self.loaded_plugins.len()
    }

    /// Gets a list of loaded plugin names.
    pub fn plugin_names(&self) -> Vec<String> {
        self.loaded_plugins.iter().map(|entry| entry.key().clone()).collect()
    }

    /// Checks if a plugin with the given name is loaded.
    pub fn is_plugin_loaded(&self, plugin_name: &str) -> bool {
        self.loaded_plugins.contains_key(plugin_name)
    }
}