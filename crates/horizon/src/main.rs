//! # Horizon Game Server - Main Entry Point
//!
//! High-performance game server with modern plugin architecture, type-safe events,
//! and clean separation of concerns. This entry point handles CLI parsing,
//! configuration loading, and application lifecycle management.
//!
//! ## Quick Start
//!
//! ```bash
//! # Run with default configuration
//! horizon
//!
//! # Specify custom configuration
//! horizon --config production.toml
//!
//! # Override specific settings
//! horizon --bind 0.0.0.0:8080 --plugins /opt/horizon/plugins --log-level debug
//!
//! # JSON logging for production
//! horizon --json-logs
//! ```
//!
//! ## Configuration
//!
//! The server loads configuration from a TOML file (default: `config.toml`).
//! If the file doesn't exist, a default configuration will be created.
//!
//! ## Signal Handling
//!
//! The server handles graceful shutdown on:
//! - SIGINT (Ctrl+C)
//! - SIGTERM (Unix systems)
//!
//! ## Architecture
//!
//! * **Modular Design**: Separated concerns across focused modules
//! * **Event-Driven**: Plugin communication through type-safe events
//! * **Memory Safe**: Zero unsafe code in core infrastructure
//! * **High Performance**: Multi-threaded networking with efficient routing

use tracing::error;

mod app;
mod cli;
mod config;
mod logging;
mod signals;

use app::Application;
use cli::CliArgs;
use config::AppConfig;

/// Main entry point for the Horizon Game Server.
/// 
/// Handles the complete application lifecycle including:
/// 1. Command-line argument parsing
/// 2. Configuration loading and validation
/// 3. Logging system initialization
/// 4. Application creation and execution
/// 5. Error handling and cleanup
/// 
/// # Exit Codes
/// 
/// * **0**: Successful execution and shutdown
/// * **1**: Error during startup, configuration, or runtime
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI arguments first
    let args = CliArgs::parse();

    // Load configuration to get logging settings
    let config = AppConfig::load_from_file(&args.config_path)
        .await
        .unwrap_or_default();

    // Setup logging before anything else
    if let Err(e) = logging::setup_logging(&config.logging, args.json_logs) {
        eprintln!("❌ Failed to setup logging: {e}");
        std::process::exit(1);
    }

    // Create and run application
    match Application::new(args).await {
        Ok(app) => {
            if let Err(e) = app.run().await {
                error!("❌ Application error: {:?}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            error!("❌ Failed to start application: {e:?}");
            std::process::exit(1);
        }
    }

    Ok(())
}

// Re-export main types for potential library usage
pub use config::{LoggingSettings, PluginSettings, RegionSettings, ServerSettings};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_default_config() {
        let config = AppConfig::default();
        assert!(config.validate().is_ok());

        // Test conversion to ServerConfig
        let plugin_safety_config = game_server::config::PluginSafetyConfig::default();
        let server_config = config.to_server_config(plugin_safety_config)
            .expect("Default config should convert to ServerConfig");
        assert_eq!(server_config.max_connections, 1000);
        assert_eq!(server_config.connection_timeout, 60);
    }

    #[tokio::test]
    async fn test_config_validation() {
        let mut config = AppConfig::default();

        // Test invalid bind address
        config.server.bind_address = "invalid".to_string();
        assert!(config.validate().is_err());

        // Test invalid region bounds
        config.server.bind_address = "127.0.0.1:8080".to_string();
        config.server.region.min_x = 100.0;
        config.server.region.max_x = 50.0; // Invalid: min > max
        assert!(config.validate().is_err());

        // Test invalid log level
        config.server.region.max_x = 200.0; // Fix region
        config.logging.level = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_cli_parsing() {
        // Test CLI argument structure
        let args = CliArgs {
            config_path: PathBuf::from("test.toml"),
            plugin_dir: Some(PathBuf::from("test_plugins")),
            bind_address: Some("127.0.0.1:9000".to_string()),
            log_level: Some("debug".to_string()),
            json_logs: true,
            danger_allow_unsafe_plugins: false,
            danger_allow_abi_mismatch: false,
            strict_versioning: false,
        };

        assert_eq!(args.config_path, PathBuf::from("test.toml"));
        assert_eq!(args.plugin_dir, Some(PathBuf::from("test_plugins")));
        assert_eq!(args.bind_address, Some("127.0.0.1:9000".to_string()));
        assert_eq!(args.log_level, Some("debug".to_string()));
        assert!(args.json_logs);
    }

    #[tokio::test]
    async fn test_application_creation() {
        let args = CliArgs {
            config_path: PathBuf::from("test_config.toml"),
            plugin_dir: None,
            bind_address: None,
            log_level: Some("debug".to_string()),
            json_logs: false,
            danger_allow_unsafe_plugins: false,
            danger_allow_abi_mismatch: false,
            strict_versioning: false,
        };

        // Create a test config file
        let test_config = AppConfig::default();
        let toml_content = toml::to_string_pretty(&test_config)
            .expect("Failed to serialize default config to TOML");
        tokio::fs::write(&args.config_path, toml_content)
            .await
            .expect("Failed to write test config file");

        // Verify file was created
        assert!(args.config_path.exists());

        // Cleanup
        tokio::fs::remove_file(&args.config_path).await.ok();
    }
}