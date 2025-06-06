//! Configuration module for the Distributed Games Server
//! 
//! This module handles command-line arguments, configuration file parsing,
//! and provides default settings for the server.

pub mod args;
pub mod settings;

pub use args::Args;
pub use settings::{Config, ServerSettings, RegionSettings, PluginSettings, LoggingSettings};

use anyhow::Result;
use tracing::{info, warn};

/// Load configuration from file or create default configuration
/// 
/// This function attempts to load configuration from the specified file.
/// If the file doesn't exist, it creates a default configuration file
/// and returns the default settings.
/// 
/// # Arguments
/// * `args` - Command line arguments containing the config file path
/// 
/// # Returns
/// * `Result<Config>` - The loaded or default configuration
/// 
/// # Errors
/// * Returns error if file I/O operations fail
/// * Returns error if TOML parsing fails
pub async fn load_config(args: &Args) -> Result<Config> {
    if args.config.exists() {
        let config_str = tokio::fs::read_to_string(&args.config).await?;
        match toml::de::from_str::<Config>(&config_str) {
            Ok(config) => Ok(config),
            Err(e) => {
                warn!("Failed to parse config file {}: {}", args.config.display(), e);
                Err(e.into())
            }
        }
    } else {
        warn!("Configuration file not found: {}, using defaults", args.config.display());

        // Create default config file
        let default_config = Config::default();
        let config_str = toml::to_string_pretty(&default_config)?;
        tokio::fs::write(&args.config, config_str).await?;
        info!("Created default configuration file: {}", args.config.display());

        Ok(default_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[tokio::test]
    async fn test_load_config_default() {
        let temp_file = NamedTempFile::new().unwrap();
        let args = Args {
            config: temp_file.path().to_path_buf(),
            ..Default::default()
        };
        
        // Delete the file to test default creation
        drop(temp_file);
        
        let config = load_config(&args).await.unwrap();
        assert_eq!(config.server.listen_addr, "127.0.0.1:8080");
    }

    #[tokio::test]
    async fn test_load_config_existing() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"
[server]
listen_addr = "0.0.0.0:9090"
max_players = 500
tick_rate = 60

[region]
min_x = -500.0
max_x = 500.0
min_y = -500.0
max_y = 500.0
min_z = -50.0
max_z = 50.0

[plugins]
directory = "test_plugins"
auto_load = ["test_plugin"]
        "#;
        
        temp_file.write_all(config_content.as_bytes()).unwrap();
        
        let args = Args {
            config: temp_file.path().to_path_buf(),
            ..Default::default()
        };
        
        let config = load_config(&args).await.unwrap();
        assert_eq!(config.server.listen_addr, "0.0.0.0:9090");
        assert_eq!(config.server.max_players, 500);
    }
}