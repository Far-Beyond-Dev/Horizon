//! Plugin manager implementation for loading and managing dynamic plugins.

use crate::error::PluginSystemError;
use dashmap::DashMap;
use horizon_event_system::plugin::Plugin;
use horizon_event_system::{EventSystem, context::ServerContext, LogLevel};
use libloading::{Library, Symbol};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Configuration for plugin loading safety checks.
/// 
/// These flags allow users to override safety validations when they understand the risks.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PluginSafetyConfig {
    /// Ignore Rust compiler version differences between plugin and server.
    /// WARNING: This may cause crashes due to ABI incompatibilities.
    pub allow_unsafe_plugins: bool,
    
    /// Ignore crate version differences between plugin and server.
    /// WARNING: This may cause crashes or undefined behavior.
    pub allow_abi_mismatch: bool,
    
    /// Require exact version matching including patch digits.
    /// When false, only major.minor must match (ignoring patch).
    pub strict_versioning: bool,
}


//TODO: provide real region and player communication.
/// Minimal server context for plugin initialization and testing.
#[derive(Clone)]
struct BasicServerContext {
    event_system: Arc<EventSystem>,
    region_id: horizon_event_system::types::RegionId,
    luminal_handle: luminal::Handle,
    gorc_instance_manager: Option<Arc<horizon_event_system::gorc::GorcInstanceManager>>,
}

impl std::fmt::Debug for BasicServerContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BasicServerContext")
            .field("region_id", &self.region_id)
            .finish()
    }
}

impl BasicServerContext {
    /// Create a new basic context with a specific region.
    fn new(event_system: Arc<EventSystem>) -> Self {
        let luminal_rt = luminal::Runtime::new().expect("Failed to create luminal runtime");
        Self {
            event_system,
            region_id: horizon_event_system::types::RegionId::default(),
            luminal_handle: luminal_rt.handle().clone(),
            gorc_instance_manager: None,
        }
    }

    /// Create a context with a custom region id.
    #[allow(dead_code)]
    fn with_region(event_system: Arc<EventSystem>, region_id: horizon_event_system::types::RegionId) -> Self {
        let luminal_rt = luminal::Runtime::new().expect("Failed to create luminal runtime");
        Self { 
            event_system, 
            region_id,
            luminal_handle: luminal_rt.handle().clone(),
            gorc_instance_manager: None,
        }
    }

    /// Create a context with an explicit luminal handle.
    #[allow(dead_code)]
    fn with_luminal_handle(event_system: Arc<EventSystem>, luminal_handle: luminal::Handle) -> Self {
        Self {
            event_system,
            region_id: horizon_event_system::types::RegionId::default(),
            luminal_handle: luminal_handle,
            gorc_instance_manager: None,
        }
    }

    /// Create a context with a GORC instance manager.
    #[allow(dead_code)]
    fn with_gorc(event_system: Arc<EventSystem>, gorc_instance_manager: Arc<horizon_event_system::gorc::GorcInstanceManager>) -> Self {
        let luminal_rt = luminal::Runtime::new().expect("Failed to create luminal runtime");
        Self {
            event_system,
            region_id: horizon_event_system::types::RegionId::default(),
            luminal_handle: luminal_rt.handle().clone(),
            gorc_instance_manager: Some(gorc_instance_manager),
        }
    }
}

#[async_trait::async_trait]
impl ServerContext for BasicServerContext {
    fn events(&self) -> Arc<EventSystem> {
        self.event_system.clone()
    }

    fn log(&self, level: LogLevel, message: &str) {
        // Use async logger to prevent blocking hot threads
        let async_logger = horizon_event_system::async_logging::global_async_logger();
        async_logger.log_with_target(level, message, Some("plugin_system"));
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

    fn luminal_handle(&self) -> luminal::Handle {
        self.luminal_handle.clone()
    }

    fn gorc_instance_manager(&self) -> Option<Arc<horizon_event_system::gorc::GorcInstanceManager>> {
        self.gorc_instance_manager.clone()
    }
}

/// Information about a loaded plugin
pub struct LoadedPlugin {
    /// The name of the plugin
    #[allow(dead_code)]
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
    /// Safety configuration for plugin loading
    safety_config: PluginSafetyConfig,
    /// Optional GORC instance manager for object replication
    gorc_instance_manager: Option<Arc<horizon_event_system::gorc::GorcInstanceManager>>,
}

impl PluginManager {
    /// Creates a new plugin manager with the given event system and safety configuration.
    ///
    /// # Arguments
    ///
    /// * `event_system` - The event system that plugins will use for communication
    /// * `safety_config` - Configuration for plugin loading safety checks
    ///
    /// # Returns
    ///
    /// A new `PluginManager` instance ready to load plugins.
    pub fn new(event_system: Arc<EventSystem>, safety_config: PluginSafetyConfig) -> Self {
        Self {
            event_system,
            loaded_plugins: DashMap::new(),
            safety_config,
            gorc_instance_manager: None,
        }
    }

    /// Creates a new plugin manager with GORC instance manager support.
    ///
    /// # Arguments
    ///
    /// * `event_system` - The event system that plugins will use for communication
    /// * `safety_config` - Configuration for plugin loading safety checks
    /// * `gorc_instance_manager` - GORC instance manager for object replication
    ///
    /// # Returns
    ///
    /// A new `PluginManager` instance ready to load plugins with GORC support.
    pub fn with_gorc(
        event_system: Arc<EventSystem>, 
        safety_config: PluginSafetyConfig,
        gorc_instance_manager: Arc<horizon_event_system::gorc::GorcInstanceManager>
    ) -> Self {
        Self {
            event_system,
            loaded_plugins: DashMap::new(),
            safety_config,
            gorc_instance_manager: Some(gorc_instance_manager),
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

        // Look for the plugin version function
        let get_plugin_version: Symbol<unsafe extern "C" fn() -> *const std::os::raw::c_char> = unsafe {
            library.get(b"get_plugin_version").map_err(|e| {
                PluginSystemError::LoadingError(format!(
                    "Plugin does not export 'get_plugin_version' function: {}", e
                ))
            })?
        };

        // Get plugin version string
        let plugin_version_ptr = unsafe { get_plugin_version() };
        let plugin_version = if plugin_version_ptr.is_null() {
            return Err(PluginSystemError::LoadingError(
                "Plugin returned null version string".to_string()
            ));
        } else {
            {
                // Validate the pointer and ensure it is null-terminated
                const MAX_PLUGIN_VERSION_LENGTH: usize = 1024; // Define a reasonable maximum length
                let plugin_version = unsafe {
                    let slice = std::slice::from_raw_parts(plugin_version_ptr as *const u8, MAX_PLUGIN_VERSION_LENGTH);
                    if let Some(_null_pos) = slice.iter().position(|&c| c == 0) {
                        std::ffi::CStr::from_ptr(plugin_version_ptr)
                            .to_string_lossy()
                            .to_string()
                    } else {
                        return Err(PluginSystemError::LoadingError(
                            "Plugin version string is not null-terminated".to_string(),
                        ));
                    }
                };
                plugin_version
            }
        };

        // Parse versions and validate compatibility
        let expected_version = horizon_event_system::ABI_VERSION;
        self.validate_plugin_compatibility(&plugin_version, expected_version)?;

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

        let context = if let Some(gorc_manager) = &self.gorc_instance_manager {
            Arc::new(BasicServerContext::with_gorc(self.event_system.clone(), gorc_manager.clone()))
        } else {
            Arc::new(BasicServerContext::new(self.event_system.clone()))
        };

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

        let context = if let Some(gorc_manager) = &self.gorc_instance_manager {
            Arc::new(BasicServerContext::with_gorc(self.event_system.clone(), gorc_manager.clone()))
        } else {
            Arc::new(BasicServerContext::new(self.event_system.clone()))
        };

        // Call shutdown on all plugins and collect libraries for controlled cleanup
        let plugin_names: Vec<String> = self.loaded_plugins.iter().map(|entry| entry.key().clone()).collect();
        let mut libraries_to_unload = Vec::new();
        
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

        // Remove plugins from map and collect libraries for controlled unloading
        for plugin_name in &plugin_names {
            if let Some((_, loaded_plugin)) = self.loaded_plugins.remove(plugin_name) {
                info!("🔌 Dropping plugin instance for: {}", plugin_name);
                // Drop the plugin instance first (this drops the Box<dyn Plugin>)
                drop(loaded_plugin.plugin);
                
                // Keep the library for later controlled unloading
                libraries_to_unload.push((plugin_name.clone(), loaded_plugin.library));
                info!("📚 Library queued for cleanup: {}", plugin_name);
            }
        }

        // Give some time for any remaining references to be cleaned up
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Now unload libraries in reverse order (LIFO)
        libraries_to_unload.reverse();
        
        info!("📚 Unloading {} plugin libraries...", libraries_to_unload.len());
        
        // On Windows, aggressive library unloading can sometimes cause access violations
        // if there are still references in the system. We can disable unloading for safety.
        #[cfg(windows)]
        let should_unload_libraries = std::env::var("HORIZON_UNLOAD_PLUGIN_LIBRARIES")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        
        #[cfg(not(windows))]
        let should_unload_libraries = true;
        
        if should_unload_libraries {
            for (plugin_name, library) in libraries_to_unload {
                info!("📚 Unloading library for plugin: {}", plugin_name);
                // The library will be dropped automatically here, but we're doing it
                // in a controlled manner after ensuring plugin instances are dropped
                drop(library);
            }
        } else {
            info!("📚 Skipping library unloading for safety (set HORIZON_UNLOAD_PLUGIN_LIBRARIES=true to enable)");
            // Let the libraries leak - the OS will clean them up on process exit
            // This is safer than risking access violations during shutdown
            std::mem::forget(libraries_to_unload);
        }

        info!("🧹 Plugin cleanup completed");

        Ok(())
    }

    /// Gets the number of currently loaded plugins.
    pub fn plugin_count(&self) -> usize {
        self.loaded_plugins.len()
    }

        /// Returns a reference to the event system used by the plugin manager.
        pub fn event_system_ref(&self) -> &EventSystem {
            &self.event_system
        }

        /// Returns the Arc<EventSystem> used by the plugin manager (cloned).
        pub fn event_system_arc(&self) -> Arc<EventSystem> {
            self.event_system.clone()
        }

    /// Gets a list of loaded plugin names.
    pub fn plugin_names(&self) -> Vec<String> {
        self.loaded_plugins.iter().map(|entry| entry.key().clone()).collect()
    }

    /// Checks if a plugin with the given name is loaded.
    pub fn is_plugin_loaded(&self, plugin_name: &str) -> bool {
        self.loaded_plugins.contains_key(plugin_name)
    }

    /// Validates plugin compatibility based on ABI version string.
    /// 
    /// ABI version format: "crate_version:rust_version" (e.g., "0.10.0:1.75.0")
    /// 
    /// This function checks:
    /// 1. Crate version compatibility (exact match required)
    /// 2. Rust compiler version compatibility (can be bypassed with --danger-allow-unsafe-plugins)
    /// 
    /// # Safety Flags
    /// - `--danger-allow-unsafe-plugins`: Ignore Rust compiler version differences (already implemented in CLI)
    /// - `--danger-allow-abi-mismatch`: Ignore crate version differences (already implemented in CLI)
    /// - Both flags can be combined to disable all version checking
    /// Validates plugin compatibility using ABI version strings.
    /// 
    /// Checks both crate version and Rust compiler version for safety.
    /// Can be overridden with CLI safety flags.
    fn validate_plugin_compatibility(&self, plugin_version: &str, expected_version: &str) -> Result<(), PluginSystemError> {
        // Parse both versions
        let plugin_parts: Vec<&str> = plugin_version.split(':').collect();
        let expected_parts: Vec<&str> = expected_version.split(':').collect();
        
        if plugin_parts.len() != 2 || expected_parts.len() != 2 {
            return Err(PluginSystemError::VersionMismatch(format!(
                "Invalid version format. Expected 'crate:rust', got plugin='{}', expected='{}'",
                plugin_version, expected_version
            )));
        }
        
        let plugin_crate_version = plugin_parts[0];
        let plugin_rust_version = plugin_parts[1];
        let expected_crate_version = expected_parts[0];
        let expected_rust_version = expected_parts[1];
        
        // Check crate version compatibility (can be overridden with --danger-allow-abi-mismatch)
        let versions_compatible = if self.safety_config.strict_versioning {
            // Strict: exact version match required
            plugin_crate_version == expected_crate_version
        } else {
            // Relaxed: only major.minor must match (ignore patch)
            self.versions_major_minor_compatible(plugin_crate_version, expected_crate_version)
        };
        
        if !versions_compatible && !self.safety_config.allow_abi_mismatch {
            let comparison_type = if self.safety_config.strict_versioning { "exact" } else { "major.minor" };
            return Err(PluginSystemError::VersionMismatch(format!(
                "ABI version mismatch: plugin compiled against horizon_event_system v{}, but server uses v{} ({} matching required when flag --strict-versioning is {}). \
                This plugin is incompatible and may cause crashes or undefined behavior. \
                Recompile the plugin against the correct version, use --strict-versioning=false for relaxed matching, or use --danger-allow-abi-mismatch to override (NOT RECOMMENDED).",
                plugin_crate_version, expected_crate_version, comparison_type, self.safety_config.strict_versioning
            )));
        }
        
        // Check Rust compiler version compatibility (can be overridden with --danger-allow-unsafe-plugins)
        if plugin_rust_version != expected_rust_version && 
           plugin_rust_version != "unknown" && 
           expected_rust_version != "unknown" && 
           !self.safety_config.allow_unsafe_plugins {
            return Err(PluginSystemError::VersionMismatch(format!(
                "Rust compiler version mismatch: plugin compiled with Rust {}, but server compiled with Rust {}. \
                This may cause ABI incompatibilities due to different trait object layouts or calling conventions. \
                Recompile with the same Rust version, or use --danger-allow-unsafe-plugins to override (MAY CAUSE CRASHES).",
                plugin_rust_version, expected_rust_version
            )));
        }
        
        // Log warnings if safety overrides are in use
        if self.safety_config.allow_abi_mismatch && plugin_crate_version != expected_crate_version {
            warn!("Loading plugin with ABI version mismatch (override enabled): plugin v{} != server v{}", 
                  plugin_crate_version, expected_crate_version);
        }
        
        if self.safety_config.allow_unsafe_plugins && 
           plugin_rust_version != expected_rust_version && 
           plugin_rust_version != "unknown" && 
           expected_rust_version != "unknown" {
            warn!("Loading plugin with Rust compiler version mismatch (override enabled): plugin {} != server {}", 
                  plugin_rust_version, expected_rust_version);
        }
        
        Ok(())
    }
    
    /// Checks if two version strings are compatible using major.minor comparison.
    /// Ignores patch versions (e.g., "0.11.2" is compatible with "0.11.0").
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_version_mismatch_error() {
        // Test that VersionMismatch error can be created and formatted correctly
        let error = PluginSystemError::VersionMismatch("expected 1, got 2".to_string());
        let error_message = format!("{}", error);
        assert!(error_message.contains("Plugin version mismatch"));
        assert!(error_message.contains("expected 1, got 2"));
    }

    #[test]
    fn test_plugin_discovery() {
        // Create a temporary directory with plugin files
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create mock plugin files
        #[cfg(target_os = "windows")]
        let plugin_extension = "dll";
        #[cfg(target_os = "macos")]
        let plugin_extension = "dylib";
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let plugin_extension = "so";

        let plugin_file = temp_path.join(format!("test_plugin.{}", plugin_extension));
        fs::write(&plugin_file, "dummy content").unwrap();

        let non_plugin_file = temp_path.join("not_a_plugin.txt");
        fs::write(&non_plugin_file, "dummy content").unwrap();

        // Create plugin manager
        let event_system = Arc::new(EventSystem::new());
        let manager = PluginManager::new(event_system, PluginSafetyConfig::default());

        // Test plugin discovery
        let discovered = manager.discover_plugin_files(temp_path).unwrap();
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0], plugin_file);
    }

    #[test]
    fn test_expected_plugin_version_constant() {
        // Verify that the expected plugin version is using the ABI_VERSION from horizon_event_system
        // This test ensures that we're using the same version that plugins will report
        let expected_version = horizon_event_system::ABI_VERSION;
        
        // The current version should be in format "crate_version:rust_version"
        assert!(expected_version.contains(':'), "ABI version should contain ':' separator");
        
        let parts: Vec<&str> = expected_version.split(':').collect();
        assert_eq!(parts.len(), 2, "ABI version should have exactly 2 parts separated by ':'");
        
        let crate_version = parts[0];
        let rust_version = parts[1];
        
        // Verify the crate version is not empty and looks like a semantic version
        assert!(!crate_version.is_empty(), "Crate version should not be empty");
        assert!(crate_version.contains('.'), "Crate version should contain '.' separators");
        
        // Verify the rust version is not empty
        assert!(!rust_version.is_empty(), "Rust version should not be empty");
        
        // Verify the version makes sense (not the old hardcoded format)
        assert_ne!(expected_version, "1", "ABI version should not be the old hardcoded value of '1'");
        
        info!("✅ ABI version format is correct: {}", expected_version);
    }

    #[test]
    fn test_plugin_compatibility_validation() {
        let event_system = Arc::new(EventSystem::new());
        
        // Test exact match - should pass
        let manager_strict = PluginManager::new(event_system.clone(), PluginSafetyConfig::default());
        assert!(manager_strict.validate_plugin_compatibility("0.10.0:1.75.0", "0.10.0:1.75.0").is_ok());
        
        // Test crate version mismatch - should fail with strict config
        let result = manager_strict.validate_plugin_compatibility("0.9.0:1.75.0", "0.10.0:1.75.0");
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, PluginSystemError::VersionMismatch(_)));
        
        // Test Rust version mismatch - should fail with strict config
        let result = manager_strict.validate_plugin_compatibility("0.10.0:1.74.0", "0.10.0:1.75.0");
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, PluginSystemError::VersionMismatch(_)));
        
        // Test with safety overrides enabled
        let manager_unsafe = PluginManager::new(event_system.clone(), PluginSafetyConfig {
            allow_unsafe_plugins: true,
            allow_abi_mismatch: true,
            strict_versioning: false,
        });
        
        // Should pass with overrides
        assert!(manager_unsafe.validate_plugin_compatibility("0.9.0:1.74.0", "0.10.0:1.75.0").is_ok());
        
        // Test unknown Rust version - should pass (one side unknown)
        assert!(manager_strict.validate_plugin_compatibility("0.10.0:unknown", "0.10.0:1.75.0").is_ok());
        assert!(manager_strict.validate_plugin_compatibility("0.10.0:1.75.0", "0.10.0:unknown").is_ok());
        
        // Test both unknown Rust versions - should pass
        assert!(manager_strict.validate_plugin_compatibility("0.10.0:unknown", "0.10.0:unknown").is_ok());
        
        // Test invalid format - should fail
        let result = manager_strict.validate_plugin_compatibility("invalid", "0.10.0:1.75.0");
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, PluginSystemError::VersionMismatch(_)));
    }
    
    #[test]
    fn test_relaxed_versioning() {
        let event_system = Arc::new(EventSystem::new());
        
        // Test relaxed versioning (strict_versioning = false)
        let manager_relaxed = PluginManager::new(event_system.clone(), PluginSafetyConfig {
            allow_unsafe_plugins: false,
            allow_abi_mismatch: false,
            strict_versioning: false, // Relaxed versioning
        });
        
        // Same major.minor, different patch - should pass with relaxed versioning
        assert!(manager_relaxed.validate_plugin_compatibility("0.11.2:1.75.0", "0.11.0:1.75.0").is_ok());
        assert!(manager_relaxed.validate_plugin_compatibility("0.11.0:1.75.0", "0.11.5:1.75.0").is_ok());
        
        // Different major version - should fail even with relaxed versioning
        let result = manager_relaxed.validate_plugin_compatibility("1.11.0:1.75.0", "0.11.0:1.75.0");
        assert!(result.is_err());
        
        // Different minor version - should fail even with relaxed versioning
        let result = manager_relaxed.validate_plugin_compatibility("0.10.0:1.75.0", "0.11.0:1.75.0");
        assert!(result.is_err());
        
        // Test strict versioning (strict_versioning = true)
        let manager_strict = PluginManager::new(event_system.clone(), PluginSafetyConfig {
            allow_unsafe_plugins: false,
            allow_abi_mismatch: false,
            strict_versioning: true, // Strict versioning
        });
        
        // Same major.minor, different patch - should fail with strict versioning
        let result = manager_strict.validate_plugin_compatibility("0.11.2:1.75.0", "0.11.0:1.75.0");
        assert!(result.is_err());
        
        // Exact match - should pass with strict versioning
        assert!(manager_strict.validate_plugin_compatibility("0.11.0:1.75.0", "0.11.0:1.75.0").is_ok());
    }
    
    #[test]
    fn test_major_minor_version_parsing() {
        let event_system = Arc::new(EventSystem::new());
        let manager = PluginManager::new(event_system, PluginSafetyConfig::default());
        
        // Test valid version parsing
        assert!(manager.versions_major_minor_compatible("1.2.3", "1.2.0"));
        assert!(manager.versions_major_minor_compatible("1.2.0", "1.2.999"));
        assert!(!manager.versions_major_minor_compatible("1.2.0", "1.3.0"));
        assert!(!manager.versions_major_minor_compatible("1.2.0", "2.2.0"));
        
        // Test invalid versions - should fall back to exact comparison
        assert!(manager.versions_major_minor_compatible("invalid", "invalid"));
        assert!(!manager.versions_major_minor_compatible("invalid", "1.2.0"));
        assert!(!manager.versions_major_minor_compatible("1.2.0", "invalid"));
    }
}