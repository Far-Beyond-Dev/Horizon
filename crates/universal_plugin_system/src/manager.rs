//! Plugin manager for loading and managing dynamic plugins

use crate::context::PluginContext;
use crate::error::PluginSystemError;
use crate::event::EventBus;
use crate::plugin::{Plugin, PluginFactory, PluginMetadata};
use crate::propagation::EventPropagator;
use dashmap::DashMap;
use libloading::{Library, Symbol};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Configuration for plugin loading safety checks
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PluginConfig {
    /// Ignore version differences between plugin and system
    pub allow_version_mismatch: bool,
    
    /// Allow loading plugins with different ABI versions
    pub allow_abi_mismatch: bool,
    
    /// Require exact version matching including patch digits
    pub strict_versioning: bool,
    
    /// Maximum number of plugins to load
    pub max_plugins: Option<usize>,
    
    /// Plugin search directories
    pub search_directories: Vec<PathBuf>,
}

/// Information about a loaded plugin
pub struct LoadedPlugin<K: crate::event::EventKeyType, P: EventPropagator<K>> {
    /// Plugin metadata
    pub metadata: PluginMetadata,
    /// The loaded library (if loaded from dynamic library)
    pub library: Option<Library>,
    /// The plugin instance
    pub plugin: Box<dyn Plugin<K, P>>,
    /// Plugin factory (if available)
    pub factory: Option<Box<dyn PluginFactory<K, P>>>,
}

/// Plugin manager for loading and managing dynamic plugins
#[allow(unused)]
pub struct PluginManager<K: crate::event::EventKeyType, P: EventPropagator<K>> {
    /// Event bus for plugin communication
    event_bus: Arc<EventBus<K, P>>,
    /// Plugin context template
    context_template: Arc<PluginContext<K, P>>,
    /// Map of loaded plugins by name
    loaded_plugins: DashMap<String, LoadedPlugin<K, P>>,
    /// Safety configuration
    config: PluginConfig,
}

impl<K: crate::event::EventKeyType, P: EventPropagator<K>> PluginManager<K, P> {
    /// Create a new plugin manager
    pub fn new(
        event_bus: Arc<EventBus<K, P>>,
        context_template: Arc<PluginContext<K, P>>,
        config: PluginConfig,
    ) -> Self {
        Self {
            event_bus,
            context_template,
            loaded_plugins: DashMap::new(),
            config,
        }
    }

    /// Load plugins from the specified directory
    pub async fn load_plugins_from_directory<Pt: AsRef<Path>>(
        &self,
        plugin_directory: Pt,
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

        // Discover and load plugin files
        let plugin_files = self.discover_plugin_files(dir_path)?;
        
        if plugin_files.is_empty() {
            info!("📂 No plugin files found in directory");
            return Ok(());
        }

        info!("🔍 Found {} plugin file(s)", plugin_files.len());
        let plugin_count = plugin_files.len();

        // Check max plugins limit
        if let Some(max_plugins) = self.config.max_plugins {
            let current_count = self.loaded_plugins.len();
            if current_count + plugin_count > max_plugins {
                return Err(PluginSystemError::LoadingFailed(format!(
                    "Loading {} plugins would exceed maximum limit of {} (currently loaded: {})",
                    plugin_count, max_plugins, current_count
                )));
            }
        }

        // Load each plugin
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

        // Initialize all loaded plugins
        self.initialize_plugins().await?;

        info!("🎉 Plugin loading complete: {}/{} plugins loaded successfully", 
              loaded_count, plugin_count);

        Ok(())
    }

    /// Load a single plugin from a factory
    pub async fn load_plugin_from_factory(
        &self,
        factory: Box<dyn PluginFactory<K, P>>,
    ) -> Result<String, PluginSystemError> {
        let plugin_name = factory.plugin_name().to_string();
        let plugin_version = factory.plugin_version().to_string();

        // Check if plugin already exists
        if self.loaded_plugins.contains_key(&plugin_name) {
            return Err(PluginSystemError::PluginAlreadyExists(plugin_name));
        }

        // Create plugin instance
        let plugin = factory.create()?;

        // Create metadata
        let metadata = PluginMetadata::new(plugin_name.clone(), plugin_version);

        // Store the loaded plugin
        let loaded_plugin = LoadedPlugin {
            metadata,
            library: None, // No library for factory-created plugins
            plugin,
            factory: Some(factory),
        };

        self.loaded_plugins.insert(plugin_name.clone(), loaded_plugin);
        
        // Initialize the plugin
        self.initialize_single_plugin(&plugin_name).await?;

        Ok(plugin_name)
    }

    /// Discover plugin files in the given directory
    fn discover_plugin_files<Pt: AsRef<Path>>(
        &self,
        directory: Pt,
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

    /// Load a single plugin from the specified file
    async fn load_single_plugin<Pt: AsRef<Path>>(
        &self,
        plugin_path: Pt,
    ) -> Result<String, PluginSystemError> {
        let path = plugin_path.as_ref();
        
        info!("🔄 Loading plugin from: {}", path.display());

        // Load the dynamic library
        let library = unsafe {
            Library::new(path).map_err(|e| {
                PluginSystemError::LibraryError(format!("Failed to load library: {}", e))
            })?
        };

        // Look for the plugin version function
        let get_plugin_version: Symbol<unsafe extern "C" fn() -> *const std::os::raw::c_char> = unsafe {
            library.get(b"get_plugin_version").map_err(|e| {
                PluginSystemError::LoadingFailed(format!(
                    "Plugin does not export 'get_plugin_version' function: {}", e
                ))
            })?
        };

        // Get plugin version string
        let plugin_version_ptr = unsafe { get_plugin_version() };
        let plugin_version = if plugin_version_ptr.is_null() {
            return Err(PluginSystemError::LoadingFailed(
                "Plugin returned null version string".to_string()
            ));
        } else {
            unsafe {
                std::ffi::CStr::from_ptr(plugin_version_ptr)
                    .to_string_lossy()
                    .to_string()
            }
        };

        // Validate plugin compatibility
        self.validate_plugin_compatibility(&plugin_version)?;

        // Look for the plugin creation function
        let create_plugin: Symbol<unsafe extern "C" fn() -> *mut dyn Plugin<K, P>> = unsafe {
            library.get(b"create_plugin").map_err(|e| {
                PluginSystemError::LoadingFailed(format!(
                    "Plugin does not export 'create_plugin' function: {}", e
                ))
            })?
        };

        // Create the plugin instance
        let plugin_ptr = unsafe { create_plugin() };
        if plugin_ptr.is_null() {
            return Err(PluginSystemError::LoadingFailed(
                "Plugin creation function returned null".to_string(),
            ));
        }

        let plugin = unsafe { Box::from_raw(plugin_ptr) };
        
        // Get plugin name for registration
        let plugin_name = plugin.name().to_string();
        let plugin_version = plugin.version().to_string();

        // Check if plugin already exists
        if self.loaded_plugins.contains_key(&plugin_name) {
            return Err(PluginSystemError::PluginAlreadyExists(plugin_name));
        }

        // Create metadata
        let metadata = PluginMetadata::new(plugin_name.clone(), plugin_version);

        // Store the loaded plugin
        let loaded_plugin = LoadedPlugin {
            metadata,
            library: Some(library),
            plugin,
            factory: None,
        };

        self.loaded_plugins.insert(plugin_name.clone(), loaded_plugin);
        
        Ok(plugin_name)
    }

    /// Initialize all loaded plugins
    async fn initialize_plugins(&self) -> Result<(), PluginSystemError> {
        info!("🔧 Initializing {} loaded plugins", self.loaded_plugins.len());

        let plugin_names: Vec<String> = self.loaded_plugins.iter().map(|entry| entry.key().clone()).collect();
        
        // Phase 1: Pre-initialization (register handlers)
        for plugin_name in &plugin_names {
            self.pre_initialize_single_plugin(plugin_name).await?;
        }

        // Phase 2: Full initialization
        for plugin_name in &plugin_names {
            self.initialize_single_plugin(plugin_name).await?;
        }

        Ok(())
    }

    /// Pre-initialize a single plugin
    async fn pre_initialize_single_plugin(&self, plugin_name: &str) -> Result<(), PluginSystemError> {
        info!("🔧 Pre-initializing plugin: {}", plugin_name);

        if let Some(mut loaded_plugin) = self.loaded_plugins.get_mut(plugin_name) {
            let context = self.context_template.clone();
            match loaded_plugin.plugin.pre_init(context).await {
                Ok(_) => {
                    info!("📡 Event handlers registered for plugin: {}", plugin_name);
                }
                Err(e) => {
                    error!("❌ Failed to register handlers for plugin {}: {:?}", plugin_name, e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Initialize a single plugin
    async fn initialize_single_plugin(&self, plugin_name: &str) -> Result<(), PluginSystemError> {
        info!("🔧 Initializing plugin: {}", plugin_name);

        if let Some(mut loaded_plugin) = self.loaded_plugins.get_mut(plugin_name) {
            let context = self.context_template.clone();
            match loaded_plugin.plugin.init(context).await {
                Ok(_) => {
                    info!("✅ Plugin initialized successfully: {}", plugin_name);
                }
                Err(e) => {
                    error!("❌ Plugin initialization failed for {}: {:?}", plugin_name, e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Shutdown all loaded plugins
    pub async fn shutdown(&self) -> Result<(), PluginSystemError> {
        info!("🛑 Shutting down {} plugins", self.loaded_plugins.len());

        let plugin_names: Vec<String> = self.loaded_plugins.iter().map(|entry| entry.key().clone()).collect();
        
        for plugin_name in &plugin_names {
            info!("🛑 Shutting down plugin: {}", plugin_name);

            if let Some(mut loaded_plugin) = self.loaded_plugins.get_mut(plugin_name) {
                let context = self.context_template.clone();
                match loaded_plugin.plugin.shutdown(context).await {
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

        // Remove plugins from map and handle library cleanup
        for plugin_name in &plugin_names {
            if let Some((_, loaded_plugin)) = self.loaded_plugins.remove(plugin_name) {
                info!("🔌 Dropping plugin instance for: {}", plugin_name);
                // Drop the plugin instance first
                drop(loaded_plugin.plugin);
                
                // Handle library cleanup if needed
                if let Some(library) = loaded_plugin.library {
                    info!("📚 Unloading library for plugin: {}", plugin_name);
                    drop(library);
                }
            }
        }

        info!("🧹 Plugin cleanup completed");
        Ok(())
    }

    /// Validate plugin compatibility
    fn validate_plugin_compatibility(&self, plugin_version: &str) -> Result<(), PluginSystemError> {
        if self.config.allow_version_mismatch {
            return Ok(()); // Skip validation if allowed
        }

        let expected_version = crate::UNIVERSAL_PLUGIN_SYSTEM_VERSION;
        
        if self.config.strict_versioning {
            // Exact version match required
            if plugin_version != expected_version {
                return Err(PluginSystemError::VersionMismatch(format!(
                    "Exact version mismatch: plugin v{}, system v{}",
                    plugin_version, expected_version
                )));
            }
        } else {
            // Relaxed version matching (major.minor only)
            if !self.versions_compatible(plugin_version, expected_version) {
                return Err(PluginSystemError::VersionMismatch(format!(
                    "Version mismatch: plugin v{}, system v{}",
                    plugin_version, expected_version
                )));
            }
        }

        Ok(())
    }

    /// Check if two versions are compatible (major.minor matching)
    fn versions_compatible(&self, plugin_version: &str, expected_version: &str) -> bool {
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

    /// Get the number of currently loaded plugins
    pub fn plugin_count(&self) -> usize {
        self.loaded_plugins.len()
    }

    /// Get a list of loaded plugin names
    pub fn plugin_names(&self) -> Vec<String> {
        self.loaded_plugins.iter().map(|entry| entry.key().clone()).collect()
    }

    /// Check if a plugin with the given name is loaded
    pub fn is_plugin_loaded(&self, plugin_name: &str) -> bool {
        self.loaded_plugins.contains_key(plugin_name)
    }

    /// Get plugin metadata
    pub fn get_plugin_metadata(&self, plugin_name: &str) -> Option<PluginMetadata> {
        self.loaded_plugins.get(plugin_name).map(|plugin| plugin.metadata.clone())
    }

    /// Unload a specific plugin
    pub async fn unload_plugin(&self, plugin_name: &str) -> Result<(), PluginSystemError> {
        info!("🛑 Unloading plugin: {}", plugin_name);

        // Shutdown the plugin first
        if let Some(mut loaded_plugin) = self.loaded_plugins.get_mut(plugin_name) {
            let context = self.context_template.clone();
            if let Err(e) = loaded_plugin.plugin.shutdown(context).await {
                error!("❌ Plugin shutdown failed for {}: {:?}", plugin_name, e);
                // Continue with unloading even if shutdown failed
            }
        }

        // Remove and drop the plugin
        if let Some((_, loaded_plugin)) = self.loaded_plugins.remove(plugin_name) {
            info!("🔌 Dropping plugin instance for: {}", plugin_name);
            drop(loaded_plugin.plugin);
            
            if let Some(library) = loaded_plugin.library {
                info!("📚 Unloading library for plugin: {}", plugin_name);
                drop(library);
            }
            
            info!("✅ Plugin unloaded successfully: {}", plugin_name);
            Ok(())
        } else {
            Err(PluginSystemError::PluginNotFound(plugin_name.to_string()))
        }
    }
}