use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug, instrument};
use uuid::Uuid;

use crate::config::PluginConfig;

/// Plugin manager handles dynamic loading and management of plugins
pub struct PluginManager {
    config: PluginConfig,
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
    plugin_metadata: Arc<RwLock<HashMap<String, PluginMetadata>>>,
    event_handlers: Arc<RwLock<HashMap<String, Vec<PluginEventHandler>>>>,
    
    // Service providers
    storage_provider: Arc<RwLock<Option<Arc<dyn StorageService>>>>,
    auth_provider: Arc<RwLock<Option<Arc<dyn AuthService>>>>,
    
    watcher_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Loaded plugin instance
pub struct LoadedPlugin {
    pub library: Library,
    pub instance: *mut c_void,
    pub metadata: PluginMetadata,
    pub api: PluginApi,
    pub enabled: bool,
    pub load_time: chrono::DateTime<chrono::Utc>,
}

/// Plugin metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub min_horizon_version: String,
    pub max_horizon_version: Option<String>,
    pub dependencies: Vec<PluginDependency>,
    pub capabilities: Vec<PluginCapability>,
    pub checksum: String,
    pub provides_services: Vec<ServiceType>,
}

/// Plugin dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    pub name: String,
    pub version_requirement: String,
    pub optional: bool,
}

/// Plugin capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginCapability {
    NetworkHandler,
    EventProcessor,
    WorldSimulation,
    StorageProvider,
    AuthProvider,
    Monitoring,
    Custom(String),
}

/// Service types that plugins can provide
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceType {
    Storage,
    Authentication,
    Cache,
    Monitoring,
    Custom(String),
}

/// Plugin API interface
pub struct PluginApi {
    pub init: extern "C" fn() -> *mut c_void,
    pub deinit: extern "C" fn(*mut c_void),
    pub get_metadata: extern "C" fn() -> *const PluginMetadata,
    pub on_load: Option<extern "C" fn(*mut c_void) -> i32>,
    pub on_unload: Option<extern "C" fn(*mut c_void)>,
    pub on_enable: Option<extern "C" fn(*mut c_void) -> i32>,
    pub on_disable: Option<extern "C" fn(*mut c_void)>,
    pub on_event: Option<extern "C" fn(*mut c_void, *const c_char, *const c_void, usize) -> i32>,
    pub get_version: Option<extern "C" fn() -> *const c_char>,
    pub health_check: Option<extern "C" fn(*mut c_void) -> i32>,
    
    // Service provider functions
    pub get_storage_service: Option<extern "C" fn(*mut c_void) -> *mut c_void>,
    pub get_auth_service: Option<extern "C" fn(*mut c_void) -> *mut c_void>,
}

/// Plugin event handler
pub struct PluginEventHandler {
    pub plugin_name: String,
    pub handler: extern "C" fn(*mut c_void, *const c_char, *const c_void, usize) -> i32,
    pub instance: *mut c_void,
}

/// Plugin events that can be handled
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginEvent {
    PlayerConnected { player_id: Uuid, data: serde_json::Value },
    PlayerDisconnected { player_id: Uuid, reason: String },
    MessageReceived { player_id: Uuid, message: serde_json::Value },
    WorldUpdate { region_id: Uuid, update: serde_json::Value },
    Custom { event_type: String, data: serde_json::Value },
}

/// Plugin information for listing
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
    pub load_time: chrono::DateTime<chrono::Utc>,
    pub health_status: PluginHealthStatus,
    pub provides_services: Vec<ServiceType>,
}

/// Plugin health status
#[derive(Debug, Clone)]
pub enum PluginHealthStatus {
    Healthy,
    Warning(String),
    Error(String),
    Unknown,
}

/// Storage service trait for plugins to implement
#[async_trait::async_trait]
pub trait StorageService: Send + Sync {
    async fn save_data(&self, key: &str, data: &[u8]) -> Result<()>;
    async fn load_data(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn delete_data(&self, key: &str) -> Result<()>;
    async fn list_keys(&self, prefix: &str) -> Result<Vec<String>>;
    async fn health_check(&self) -> Result<()>;
}

/// Authentication service trait for plugins to implement
#[async_trait::async_trait]
pub trait AuthService: Send + Sync {
    async fn authenticate(&self, username: &str, credentials: &AuthCredentials) -> Result<Option<AuthUser>>;
    async fn create_session(&self, user: &AuthUser) -> Result<String>;
    async fn validate_session(&self, token: &str) -> Result<Option<AuthUser>>;
    async fn logout(&self, token: &str) -> Result<()>;
    async fn register_user(&self, registration: &UserRegistration) -> Result<AuthUser>;
    async fn health_check(&self) -> Result<()>;
}

/// Authentication credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthCredentials {
    Password(String),
    Token(String),
    OAuth { provider: String, token: String },
    Custom(serde_json::Value),
}

/// Authenticated user information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub permissions: PlayerPermissions,
    pub metadata: serde_json::Value,
    pub last_login: Option<chrono::DateTime<chrono::Utc>>,
}

/// User registration request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRegistration {
    pub username: String,
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Player permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerPermissions {
    pub admin: bool,
    pub moderator: bool,
    pub can_build: bool,
    pub can_chat: bool,
    pub custom_permissions: Vec<String>,
}

impl Default for PlayerPermissions {
    fn default() -> Self {
        Self {
            admin: false,
            moderator: false,
            can_build: true,
            can_chat: true,
            custom_permissions: Vec::new(),
        }
    }
}

impl PluginManager {
    /// Create new plugin manager
    #[instrument(skip(config))]
    pub async fn new(config: &PluginConfig) -> Result<Self> {
        info!("🔌 Initializing plugin manager");
        
        // Create plugin directory if it doesn't exist
        if !config.directory.exists() {
            tokio::fs::create_dir_all(&config.directory).await
                .context("Failed to create plugin directory")?;
        }
        
        let manager = Self {
            config: config.clone(),
            plugins: Arc::new(RwLock::new(HashMap::new())),
            plugin_metadata: Arc::new(RwLock::new(HashMap::new())),
            event_handlers: Arc::new(RwLock::new(HashMap::new())),
            storage_provider: Arc::new(RwLock::new(None)),
            auth_provider: Arc::new(RwLock::new(None)),
            watcher_handle: None,
        };
        
        info!("✅ Plugin manager initialized");
        Ok(manager)
    }
    
    /// Load all plugins from the plugin directory
    #[instrument(skip(self))]
    pub async fn load_all_plugins(&self) -> Result<()> {
        info!("📦 Loading all plugins from {}", self.config.directory.display());
        
        let mut dir = tokio::fs::read_dir(&self.config.directory).await
            .context("Failed to read plugin directory")?;
        
        let mut loaded_count = 0;
        let mut failed_count = 0;
        
        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            
            // Only load .dll/.so/.dylib files
            if let Some(extension) = path.extension() {
                let is_plugin = match extension.to_str() {
                    Some("dll") | Some("so") | Some("dylib") => true,
                    _ => false,
                };
                
                if is_plugin {
                    match self.load_plugin_from_path(&path).await {
                        Ok(_) => {
                            loaded_count += 1;
                            info!("✅ Loaded plugin: {}", path.display());
                        }
                        Err(e) => {
                            failed_count += 1;
                            warn!("❌ Failed to load plugin {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }
        
        info!("📊 Plugin loading complete: {} loaded, {} failed", loaded_count, failed_count);
        
        // Start file watcher if hot reload is enabled
        if self.config.hot_reload {
            self.start_file_watcher().await?;
        }
        
        Ok(())
    }
    
    /// Load a plugin from a specific path
    #[instrument(skip(self))]
    async fn load_plugin_from_path(&self, path: &Path) -> Result<()> {
        debug!("Loading plugin from: {}", path.display());
        
        // Load the dynamic library
        let library = unsafe {
            Library::new(path)
                .context(format!("Failed to load library: {}", path.display()))?
        };
        
        // Get the plugin API functions
        let api = self.extract_plugin_api(&library)
            .context("Failed to extract plugin API")?;
        
        // Get plugin metadata
        let metadata_ptr = (api.get_metadata)();
        if metadata_ptr.is_null() {
            return Err(anyhow::anyhow!("Plugin returned null metadata"));
        }
        
        let metadata = unsafe { (*metadata_ptr).clone() };
        
        // Validate plugin compatibility
        self.validate_plugin_compatibility(&metadata)
            .context("Plugin compatibility check failed")?;
        
        // Check dependencies
        self.check_plugin_dependencies(&metadata).await
            .context("Plugin dependency check failed")?;
        
        // Initialize the plugin
        let instance = (api.init)();
        if instance.is_null() {
            return Err(anyhow::anyhow!("Plugin initialization failed"));
        }
        
        // Call on_load if available
        if let Some(on_load) = api.on_load {
            let result = (on_load)(instance);
            if result != 0 {
                // Cleanup on failure
                (api.deinit)(instance);
                return Err(anyhow::anyhow!("Plugin on_load failed with code: {}", result));
            }
        }
        
        // Register event handlers if available
        if api.on_event.is_some() {
            self.register_event_handler(&metadata.name, &api, instance).await;
        }
        
        // Register service providers
        self.register_service_providers(&metadata, &api, instance).await?;
        
        let loaded_plugin = LoadedPlugin {
            library,
            instance,
            api,
            enabled: true,
            load_time: chrono::Utc::now(),
            metadata: metadata.clone(),
        };
        
        // Store the plugin
        self.plugins.write().await.insert(metadata.name.clone(), loaded_plugin);
        self.plugin_metadata.write().await.insert(metadata.name.clone(), metadata);
        
        info!("🎉 Plugin '{}' v{} loaded successfully", 
              self.plugin_metadata.read().await.get(path.file_stem().unwrap().to_str().unwrap()).unwrap().name,
              self.plugin_metadata.read().await.get(path.file_stem().unwrap().to_str().unwrap()).unwrap().version);
        
        Ok(())
    }
    
    /// Register service providers from a plugin
    async fn register_service_providers(&self, metadata: &PluginMetadata, api: &PluginApi, instance: *mut c_void) -> Result<()> {
        for service_type in &metadata.provides_services {
            match service_type {
                ServiceType::Storage => {
                    if let Some(get_storage) = api.get_storage_service {
                        let service_ptr = (get_storage)(instance);
                        if !service_ptr.is_null() {
                            info!("📦 Registered storage service from plugin: {}", metadata.name);
                            // In a real implementation, you'd create a wrapper around the C API
                            // and store it in self.storage_provider
                        }
                    }
                }
                ServiceType::Authentication => {
                    if let Some(get_auth) = api.get_auth_service {
                        let service_ptr = (get_auth)(instance);
                        if !service_ptr.is_null() {
                            info!("🔐 Registered auth service from plugin: {}", metadata.name);
                            // In a real implementation, you'd create a wrapper around the C API
                            // and store it in self.auth_provider
                        }
                    }
                }
                _ => {
                    debug!("Service type {:?} not implemented yet", service_type);
                }
            }
        }
        Ok(())
    }
    
    /// Extract plugin API from loaded library
    fn extract_plugin_api(&self, library: &Library) -> Result<PluginApi> {
        unsafe {
            let init: Symbol<extern "C" fn() -> *mut c_void> = library
                .get(b"plugin_init")
                .context("Missing plugin_init function")?;
            
            let deinit: Symbol<extern "C" fn(*mut c_void)> = library
                .get(b"plugin_deinit")
                .context("Missing plugin_deinit function")?;
            
            let get_metadata: Symbol<extern "C" fn() -> *const PluginMetadata> = library
                .get(b"plugin_get_metadata")
                .context("Missing plugin_get_metadata function")?;
            
            // Optional functions
            let on_load = library.get(b"plugin_on_load").ok()
                .map(|s: Symbol<extern "C" fn(*mut c_void) -> i32>| *s);
            
            let on_unload = library.get(b"plugin_on_unload").ok()
                .map(|s: Symbol<extern "C" fn(*mut c_void)>| *s);
            
            let on_enable = library.get(b"plugin_on_enable").ok()
                .map(|s: Symbol<extern "C" fn(*mut c_void) -> i32>| *s);
            
            let on_disable = library.get(b"plugin_on_disable").ok()
                .map(|s: Symbol<extern "C" fn(*mut c_void)>| *s);
            
            let on_event = library.get(b"plugin_on_event").ok()
                .map(|s: Symbol<extern "C" fn(*mut c_void, *const c_char, *const c_void, usize) -> i32>| *s);
            
            let get_version = library.get(b"plugin_get_version").ok()
                .map(|s: Symbol<extern "C" fn() -> *const c_char>| *s);
            
            let health_check = library.get(b"plugin_health_check").ok()
                .map(|s: Symbol<extern "C" fn(*mut c_void) -> i32>| *s);
            
            // Service provider functions
            let get_storage_service = library.get(b"plugin_get_storage_service").ok()
                .map(|s: Symbol<extern "C" fn(*mut c_void) -> *mut c_void>| *s);
            
            let get_auth_service = library.get(b"plugin_get_auth_service").ok()
                .map(|s: Symbol<extern "C" fn(*mut c_void) -> *mut c_void>| *s);
            
            Ok(PluginApi {
                init: *init,
                deinit: *deinit,
                get_metadata: *get_metadata,
                on_load,
                on_unload,
                on_enable,
                on_disable,
                on_event,
                get_version,
                health_check,
                get_storage_service,
                get_auth_service,
            })
        }
    }
    
    /// Validate plugin compatibility with current Horizon version
    fn validate_plugin_compatibility(&self, metadata: &PluginMetadata) -> Result<()> {
        let current_version = env!("CARGO_PKG_VERSION");
        
        // Check minimum version requirement
        if !self.version_meets_requirement(current_version, &metadata.min_horizon_version) {
            return Err(anyhow::anyhow!(
                "Plugin requires Horizon version {} or higher, current version is {}",
                metadata.min_horizon_version,
                current_version
            ));
        }
        
        // Check maximum version requirement if specified
        if let Some(max_version) = &metadata.max_horizon_version {
            if !self.version_meets_requirement(max_version, current_version) {
                return Err(anyhow::anyhow!(
                    "Plugin requires Horizon version {} or lower, current version is {}",
                    max_version,
                    current_version
                ));
            }
        }
        
        Ok(())
    }
    
    /// Check if version meets requirement (simplified semantic versioning)
    fn version_meets_requirement(&self, version: &str, requirement: &str) -> bool {
        // This is a simplified version comparison
        // In production, you'd want to use a proper semver library
        version >= requirement
    }
    
    /// Check plugin dependencies
    async fn check_plugin_dependencies(&self, metadata: &PluginMetadata) -> Result<()> {
        let plugins = self.plugins.read().await;
        
        for dependency in &metadata.dependencies {
            if !dependency.optional {
                if !plugins.contains_key(&dependency.name) {
                    return Err(anyhow::anyhow!(
                        "Required dependency '{}' not found",
                        dependency.name
                    ));
                }
                
                // Check version requirement
                if let Some(dep_plugin) = plugins.get(&dependency.name) {
                    if !self.version_meets_requirement(
                        &dep_plugin.metadata.version,
                        &dependency.version_requirement
                    ) {
                        return Err(anyhow::anyhow!(
                            "Dependency '{}' version {} does not meet requirement {}",
                            dependency.name,
                            dep_plugin.metadata.version,
                            dependency.version_requirement
                        ));
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Register event handler for a plugin
    async fn register_event_handler(&self, plugin_name: &str, api: &PluginApi, instance: *mut c_void) {
        if let Some(on_event) = api.on_event {
            let handler = PluginEventHandler {
                plugin_name: plugin_name.to_string(),
                handler: on_event,
                instance,
            };
            
            // Register for all event types (could be more selective)
            let event_types = vec![
                "player_connected".to_string(),
                "player_disconnected".to_string(),
                "message_received".to_string(),
                "world_update".to_string(),
            ];
            
            let mut handlers = self.event_handlers.write().await;
            for event_type in event_types {
                handlers.entry(event_type).or_default().push(PluginEventHandler {
                    plugin_name: handler.plugin_name.clone(),
                    handler: handler.handler,
                    instance: handler.instance,
                });
            }
        }
    }
    
    /// Install a plugin from a file
    #[instrument(skip(self))]
    pub async fn install_plugin(&self, source_path: &Path) -> Result<()> {
        info!("📥 Installing plugin from: {}", source_path.display());
        
        if !source_path.exists() {
            return Err(anyhow::anyhow!("Plugin file does not exist"));
        }
        
        let filename = source_path.file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid plugin filename"))?;
        
        let target_path = self.config.directory.join(filename);
        
        // Copy the plugin file
        tokio::fs::copy(source_path, &target_path).await
            .context("Failed to copy plugin file")?;
        
        // Load the plugin
        self.load_plugin_from_path(&target_path).await
            .context("Failed to load installed plugin")?;
        
        info!("✅ Plugin installed successfully");
        Ok(())
    }
    
    /// Uninstall a plugin
    #[instrument(skip(self))]
    pub async fn uninstall_plugin(&self, name: &str) -> Result<()> {
        info!("🗑️  Uninstalling plugin: {}", name);
        
        // Unload the plugin first
        self.unload_plugin(name).await?;
        
        // Remove the plugin file
        let plugin_files = self.find_plugin_files(name).await?;
        for file in plugin_files {
            tokio::fs::remove_file(&file).await
                .context(format!("Failed to remove plugin file: {}", file.display()))?;
        }
        
        info!("✅ Plugin uninstalled successfully");
        Ok(())
    }
    
    /// Reload a plugin
    #[instrument(skip(self))]
    pub async fn reload_plugin(&self, name: &str) -> Result<()> {
        info!("🔄 Reloading plugin: {}", name);
        
        // Find the plugin file
        let plugin_files = self.find_plugin_files(name).await?;
        if plugin_files.is_empty() {
            return Err(anyhow::anyhow!("Plugin file not found"));
        }
        
        // Unload the plugin
        self.unload_plugin(name).await?;
        
        // Reload the plugin
        for file in plugin_files {
            self.load_plugin_from_path(&file).await?;
        }
        
        info!("✅ Plugin reloaded successfully");
        Ok(())
    }
    
    /// Unload a plugin
    async fn unload_plugin(&self, name: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        
        if let Some(plugin) = plugins.remove(name) {
            // Unregister services if this plugin provides them
            if plugin.metadata.provides_services.contains(&ServiceType::Storage) {
                *self.storage_provider.write().await = None;
                info!("📦 Unregistered storage service from plugin: {}", name);
            }
            
            if plugin.metadata.provides_services.contains(&ServiceType::Authentication) {
                *self.auth_provider.write().await = None;
                info!("🔐 Unregistered auth service from plugin: {}", name);
            }
            
            // Call on_unload if available
            if let Some(on_unload) = plugin.api.on_unload {
                (on_unload)(plugin.instance);
            }
            
            // Deinitialize the plugin
            (plugin.api.deinit)(plugin.instance);
            
            // Remove event handlers
            let mut handlers = self.event_handlers.write().await;
            for (_, event_handlers) in handlers.iter_mut() {
                event_handlers.retain(|h| h.plugin_name != name);
            }
            
            // Remove metadata
            self.plugin_metadata.write().await.remove(name);
            
            info!("🔌 Plugin '{}' unloaded", name);
        }
        
        Ok(())
    }
    
    /// Find plugin files by name
    async fn find_plugin_files(&self, name: &str) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        let mut dir = tokio::fs::read_dir(&self.config.directory).await?;
        
        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if let Some(file_stem) = path.file_stem() {
                if file_stem.to_string_lossy().contains(name) {
                    if let Some(extension) = path.extension() {
                        if matches!(extension.to_str(), Some("dll") | Some("so") | Some("dylib")) {
                            files.push(path);
                        }
                    }
                }
            }
        }
        
        Ok(files)
    }
    
    /// Start file watcher for hot reload
    async fn start_file_watcher(&self) -> Result<()> {
        info!("👁️  Starting plugin file watcher for hot reload");
        
        // Implementation would use a file system watcher
        // For now, we'll use a simple polling approach
        let plugins_dir = self.config.directory.clone();
        let scan_interval = std::time::Duration::from_millis(self.config.scan_interval as u64);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(scan_interval);
            
            loop {
                interval.tick().await;
                
                // Scan for changes (simplified implementation)
                // In production, you'd use a proper file watcher like notify
                debug!("Scanning for plugin changes...");
            }
        });
        
        Ok(())
    }
    
    /// Dispatch event to all registered plugin handlers
    #[instrument(skip(self, event))]
    pub async fn dispatch_event(&self, event: PluginEvent) -> Result<()> {
        let event_type = match &event {
            PluginEvent::PlayerConnected { .. } => "player_connected",
            PluginEvent::PlayerDisconnected { .. } => "player_disconnected",
            PluginEvent::MessageReceived { .. } => "message_received",
            PluginEvent::WorldUpdate { .. } => "world_update",
            PluginEvent::Custom { event_type, .. } => event_type,
        };
        
        let handlers = self.event_handlers.read().await;
        if let Some(event_handlers) = handlers.get(event_type) {
            let event_data = serde_json::to_vec(&event)?;
            let event_type_cstr = CString::new(event_type)?;
            
            for handler in event_handlers {
                let result = (handler.handler)(
                    handler.instance,
                    event_type_cstr.as_ptr(),
                    event_data.as_ptr() as *const c_void,
                    event_data.len(),
                );
                
                if result != 0 {
                    warn!("Plugin '{}' event handler failed with code: {}", 
                          handler.plugin_name, result);
                }
            }
        }
        
        Ok(())
    }
    
    /// Get storage service provider
    pub async fn get_storage_service(&self) -> Option<Arc<dyn StorageService>> {
        self.storage_provider.read().await.clone()
    }
    
    /// Get auth service provider
    pub async fn get_auth_service(&self) -> Option<Arc<dyn AuthService>> {
        self.auth_provider.read().await.clone()
    }
    
    /// List all plugins
    pub async fn list_plugins(&self) -> Result<Vec<PluginInfo>> {
        let plugins = self.plugins.read().await;
        let mut plugin_list = Vec::new();
        
        for (name, plugin) in plugins.iter() {
            let health_status = self.check_plugin_health(name, plugin).await;
            
            plugin_list.push(PluginInfo {
                name: plugin.metadata.name.clone(),
                version: plugin.metadata.version.clone(),
                description: plugin.metadata.description.clone(),
                author: plugin.metadata.author.clone(),
                enabled: plugin.enabled,
                load_time: plugin.load_time,
                health_status,
                provides_services: plugin.metadata.provides_services.clone(),
            });
        }
        
        Ok(plugin_list)
    }
    
    /// Get plugin information
    pub async fn get_plugin_info(&self, name: &str) -> Result<Option<PluginInfo>> {
        let plugins = self.plugins.read().await;
        
        if let Some(plugin) = plugins.get(name) {
            let health_status = self.check_plugin_health(name, plugin).await;
            
            Ok(Some(PluginInfo {
                name: plugin.metadata.name.clone(),
                version: plugin.metadata.version.clone(),
                description: plugin.metadata.description.clone(),
                author: plugin.metadata.author.clone(),
                enabled: plugin.enabled,
                load_time: plugin.load_time,
                health_status,
                provides_services: plugin.metadata.provides_services.clone(),
            }))
        } else {
            Ok(None)
        }
    }
    
    /// Check plugin health
    async fn check_plugin_health(&self, name: &str, plugin: &LoadedPlugin) -> PluginHealthStatus {
        if let Some(health_check) = plugin.api.health_check {
            match (health_check)(plugin.instance) {
                0 => PluginHealthStatus::Healthy,
                1 => PluginHealthStatus::Warning("Plugin reported warning".to_string()),
                _ => PluginHealthStatus::Error("Plugin health check failed".to_string()),
            }
        } else {
            PluginHealthStatus::Unknown
        }
    }
    
    /// Get plugin count
    pub async fn get_plugin_count(&self) -> usize {
        self.plugins.read().await.len()
    }
    
    /// Shutdown plugin manager
    #[instrument(skip(self))]
    pub async fn shutdown(&self) -> Result<()> {
        info!("🛑 Shutting down plugin manager");
        
        // Stop file watcher
        if let Some(handle) = &self.watcher_handle {
            handle.abort();
        }
        
        // Unload all plugins
        let plugin_names: Vec<String> = self.plugins.read().await.keys().cloned().collect();
        for name in plugin_names {
            if let Err(e) = self.unload_plugin(&name).await {
                error!("Failed to unload plugin '{}': {}", name, e);
            }
        }
        
        info!("✅ Plugin manager shutdown complete");
        Ok(())
    }
}

// Safety: These are safe as long as the plugin implements the C API correctly
unsafe impl Send for LoadedPlugin {}
unsafe impl Sync for LoadedPlugin {}
unsafe impl Send for PluginEventHandler {}
unsafe impl Sync for PluginEventHandler {}