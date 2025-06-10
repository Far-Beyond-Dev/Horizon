//! Plugin management system
//! 
//! This module handles the loading and management of dynamic plugins
//! for the Distributed Games Server. Plugins extend server functionality
//! and can be loaded at runtime.

use anyhow::Result;
use server_core::GameServer;
use shared_types::ServerError;
use std::path::Path;
use tracing::{error, info, warn};

use crate::config::PluginSettings;

pub struct PluginLoadStats {
    pub plugin_count: usize,
    pub registered_events: usize,
    pub total_callbacks: usize,
    pub plugins: Vec<PluginCallbackStats>,
}

pub struct PluginCallbackStats {
    pub path: String,
    pub callbacks_by_event: Vec<(String, usize)>,
}

/// Load plugins based on configuration
/// 
/// This function loads plugins specified in the server configuration.
/// It handles plugin directory creation, platform-specific library naming,
/// and provides detailed error reporting for troubleshooting.
/// 
/// # Arguments
/// * `server` - Mutable reference to the game server instance
/// * `plugin_config` - Plugin configuration settings
/// * `plugin_dir` - Directory path where plugins are stored
/// 
/// # Returns
/// * `Result<(), ServerError>` - Success or detailed error information
/// 
/// # Platform Support
/// * Windows: Loads `.dll` files
/// * macOS: Loads `.dylib` files  
/// * Linux/Unix: Loads `.so` files
/// 
/// # Plugin Naming Convention
/// * Plugin name "example" becomes:
///   - Windows: `example.dll`
///   - macOS: `libexample.dylib`
///   - Linux: `libexample.so`
pub async fn load_plugins(
    server: &mut GameServer,
    plugin_config: &PluginSettings,
    plugin_dir: &str,
) -> Result<(), ServerError> {
    let plugin_path = Path::new(plugin_dir);

    // Ensure plugin directory exists
    if !plugin_path.exists() {
        warn!("Plugin directory does not exist: {}", plugin_dir);
        if let Err(e) = tokio::fs::create_dir_all(plugin_path).await {
            error!("Failed to create plugin directory: {}", e);
            return Err(ServerError::Internal(format!(
                "Failed to create plugin directory: {}", e
            )));
        }
        info!("Created plugin directory: {}", plugin_dir);
    }

    // Load each plugin specified in auto_load configuration
    for plugin_name in &plugin_config.auto_load {
        let plugin_file = get_platform_plugin_filename(plugin_name);
        let plugin_full_path = plugin_path.join(&plugin_file);

        if plugin_full_path.exists() {
            info!("Loading plugin: {}", plugin_name);
            match server.load_plugin(&plugin_full_path).await {
                Ok(_) => info!("Successfully loaded plugin: {}", plugin_name),
                Err(e) => {
                    error!("Failed to load plugin {}: {}", plugin_name, e);
                    return Err(e);
                }
            }
        } else {
            warn!("Plugin file not found: {}", plugin_full_path.display());
            info!("Expected plugin file: {}", plugin_file);
            provide_build_instructions(plugin_name);
        }
    }

    Ok(())
}

/// Get the platform-specific filename for a plugin
/// 
/// Converts a plugin name to the appropriate dynamic library filename
/// for the current platform.
/// 
/// # Arguments
/// * `plugin_name` - Base name of the plugin
/// 
/// # Returns
/// * `String` - Platform-specific filename
/// 
/// # Examples
/// ```
/// // On Linux: get_platform_plugin_filename("horizon") returns "libhorizon.so"
/// // On Windows: get_platform_plugin_filename("horizon") returns "horizon.dll"
/// // On macOS: get_platform_plugin_filename("horizon") returns "libhorizon.dylib"
/// ```
fn get_platform_plugin_filename(plugin_name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{}.dll", plugin_name)
    } else if cfg!(target_os = "macos") {
        format!("lib{}.dylib", plugin_name)
    } else {
        format!("lib{}.so", plugin_name)
    }
}

/// Provide helpful build instructions for missing plugins
/// 
/// When a plugin file is not found, this function outputs helpful
/// information about how to build the plugin.
/// 
/// # Arguments
/// * `plugin_name` - Name of the missing plugin
fn provide_build_instructions(plugin_name: &str) {
    match plugin_name {
        "horizon" => {
            info!("To build the horizon plugin, run:");
            info!("  cargo build --release --package horizon-plugin");
        }
        _ => {
            info!("To build the {} plugin, run:", plugin_name);
            info!("  cargo build --release --package {}-plugin", plugin_name);
        }
    }
    
    info!("Make sure the plugin crate exists and is properly configured in Cargo.toml");
}

/// Load a single plugin by path
/// 
/// Utility function to load a single plugin from a specific file path.
/// This can be used for manual plugin loading or hot-reloading scenarios.
/// 
/// # Arguments
/// * `server` - Mutable reference to the game server
/// * `plugin_path` - Full path to the plugin file
/// 
/// # Returns
/// * `Result<(), ServerError>` - Success or error details
pub async fn load_single_plugin(
    server: &mut GameServer,
    plugin_path: &Path,
) -> Result<(), ServerError> {
    if !plugin_path.exists() {
        return Err(ServerError::Internal(format!(
            "Plugin file does not exist: {}",
            plugin_path.display()
        )));
    }

    let plugin_name = plugin_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    info!("Loading plugin from path: {}", plugin_path.display());
    
    match server.load_plugin(plugin_path).await {
        Ok(_) => {
            info!("Successfully loaded plugin: {}", plugin_name);
            Ok(())
        }
        Err(e) => {
            error!("Failed to load plugin {}: {}", plugin_name, e);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_plugin_filename() {
        let plugin_name = "test_plugin";
        let filename = get_platform_plugin_filename(plugin_name);
        
        if cfg!(target_os = "windows") {
            assert_eq!(filename, "test_plugin.dll");
        } else if cfg!(target_os = "macos") {
            assert_eq!(filename, "libtest_plugin.dylib");
        } else {
            assert_eq!(filename, "libtest_plugin.so");
        }
    }

    #[test]
    fn test_provide_build_instructions() {
        // This test mainly ensures the function doesn't panic
        provide_build_instructions("horizon");
        provide_build_instructions("custom_plugin");
    }

    #[tokio::test]
    async fn test_load_single_plugin_nonexistent() {
        use shared_types::RegionBounds;
        
        let region_bounds = RegionBounds {
            min_x: -100.0,
            max_x: 100.0,
            min_y: -100.0,
            max_y: 100.0,
            min_z: -10.0,
            max_z: 10.0,
        };
        
        let mut server = GameServer::new(region_bounds);
        let nonexistent_path = Path::new("/nonexistent/plugin.so");
        
        let result = load_single_plugin(&mut server, nonexistent_path).await;
        assert!(result.is_err());
    }
}