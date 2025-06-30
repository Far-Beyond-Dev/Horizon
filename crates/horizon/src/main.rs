//! Main application entry point for the game server
//!
//! Provides CLI interface, configuration loading, and server startup with
//! the new clean event system and plugin architecture.

use clap::{Arg, Command};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::signal;
use toml;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use horizon_event_system::RegionBounds;
use game_server::{GameServer, ServerConfig};

// ============================================================================
// Configuration
// ============================================================================

/// Application configuration loaded from TOML file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Server configuration
    pub server: ServerSettings,
    /// Plugin configuration
    pub plugins: PluginSettings,
    /// Logging configuration
    pub logging: LoggingSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    /// Bind address
    pub bind_address: String,
    /// Region bounds
    pub region: RegionSettings,
    /// Connection limits
    pub max_connections: usize,
    /// Network timeouts
    pub connection_timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionSettings {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
    pub min_z: f64,
    pub max_z: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSettings {
    /// Plugin directory
    pub directory: String,
    /// Auto-load plugins on startup
    pub auto_load: bool,
    /// Plugin whitelist (empty means load all)
    pub whitelist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingSettings {
    /// Log level filter
    pub level: String,
    /// JSON formatting
    pub json_format: bool,
    /// Log to file
    pub file_path: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerSettings {
                bind_address: "127.0.0.1:8080".to_string(),
                region: RegionSettings {
                    min_x: -1000.0,
                    max_x: 1000.0,
                    min_y: -1000.0,
                    max_y: 1000.0,
                    min_z: -100.0,
                    max_z: 100.0,
                },
                max_connections: 1000,
                connection_timeout: 60,
            },
            plugins: PluginSettings {
                directory: "plugins".to_string(),
                auto_load: true,
                whitelist: vec![],
            },
            logging: LoggingSettings {
                level: "info".to_string(),
                json_format: false,
                file_path: None,
            },
        }
    }
}

impl AppConfig {
    /// Load configuration from file
    pub async fn load_from_file(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        if path.exists() {
            let content = tokio::fs::read_to_string(path).await?;
            let config: AppConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            // Create default config file
            let default_config = AppConfig::default();
            let toml_content = toml::to_string_pretty(&default_config)?;
            tokio::fs::write(path, toml_content).await?;
            info!("Created default configuration file: {}", path.display());
            Ok(default_config)
        }
    }

    /// Convert to ServerConfig (updated for new system)
    pub fn to_server_config(&self) -> Result<ServerConfig, Box<dyn std::error::Error>> {
        Ok(ServerConfig {
            bind_address: self.server.bind_address.parse()?,
            region_bounds: RegionBounds {
                min_x: self.server.region.min_x,
                max_x: self.server.region.max_x,
                min_y: self.server.region.min_y,
                max_y: self.server.region.max_y,
                min_z: self.server.region.min_z,
                max_z: self.server.region.max_z,
            },
            plugin_directory: PathBuf::from(&self.plugins.directory),
            max_connections: self.server.max_connections,
            connection_timeout: self.server.connection_timeout,
        })
    }
}

// ============================================================================
// CLI Interface
// ============================================================================

/// Command line arguments
#[derive(Debug, Clone)]
pub struct CliArgs {
    pub config_path: PathBuf,
    pub plugin_dir: Option<PathBuf>,
    pub bind_address: Option<String>,
    pub log_level: Option<String>,
    pub json_logs: bool,
}

impl CliArgs {
    /// Parse command line arguments
    pub fn parse() -> Self {
        let matches = Command::new("Horizon Game Server")
            .version("1.0.0")
            .author("Horizon Team <team@horizon.dev>")
            .about("High-performance game server with clean plugin architecture")
            .arg(
                Arg::new("config")
                    .short('c')
                    .long("config")
                    .value_name("FILE")
                    .help("Configuration file path")
                    .default_value("config.toml"),
            )
            .arg(
                Arg::new("plugins")
                    .short('p')
                    .long("plugins")
                    .value_name("DIR")
                    .help("Plugin directory path"),
            )
            .arg(
                Arg::new("bind")
                    .short('b')
                    .long("bind")
                    .value_name("ADDRESS")
                    .help("Bind address (e.g., 127.0.0.1:8080)"),
            )
            .arg(
                Arg::new("log-level")
                    .short('l')
                    .long("log-level")
                    .value_name("LEVEL")
                    .help("Log level (trace, debug, info, warn, error)"),
            )
            .arg(
                Arg::new("json-logs")
                    .long("json-logs")
                    .help("Output logs in JSON format")
                    .action(clap::ArgAction::SetTrue),
            )
            .get_matches();

        Self {
            config_path: PathBuf::from(matches.get_one::<String>("config").expect("Default config path should always be set")),
            plugin_dir: matches.get_one::<String>("plugins").map(PathBuf::from),
            bind_address: matches.get_one::<String>("bind").cloned(),
            log_level: matches.get_one::<String>("log-level").cloned(),
            json_logs: matches.get_flag("json-logs"),
        }
    }
}


// ============================================================================
// Logging Setup
// ============================================================================

/// Initialize logging system
fn setup_logging(
    config: &LoggingSettings,
    json_format: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let log_level = config.level.as_str();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));

    let registry = tracing_subscriber::registry().with(filter);

    if json_format || config.json_format {
        // JSON formatting with thread info
        registry
            .with(fmt::layer()
                .json()
                .with_file(false)
                .with_line_number(false)
                .with_thread_ids(true)
                .with_thread_names(true)
            )
            .init();
    } else {
        // Human-readable formatting with thread info
        registry
            .with(fmt::layer()
                .with_ansi(true)
                .with_file(false)
                .with_line_number(false)
                .with_thread_ids(true)
                .with_thread_names(true)
            )
            .init();
    }

    info!("🔧 Logging initialized with level: {}", log_level);
    Ok(())
}


// ============================================================================
// Signal Handling
// ============================================================================

/// Setup graceful shutdown signal handling
async fn setup_signal_handlers() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        use signal::unix::{signal, SignalKind};

        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigterm = signal(SignalKind::terminate())?;

        tokio::select! {
            _ = sigint.recv() => {
                info!("📡 Received SIGINT");
            }
            _ = sigterm.recv() => {
                info!("📡 Received SIGTERM");
            }
        }
    }

    #[cfg(windows)]
    {
        signal::ctrl_c().await?;
        info!("📡 Received Ctrl+C");
    }

    Ok(())
}

// ============================================================================
// Enhanced Application with New Architecture
// ============================================================================

/// Main application struct with enhanced capabilities
pub struct Application {
    config: AppConfig,
    server: GameServer,
}

impl Application {
    /// Create new application with the refactored system
    pub async fn new(args: CliArgs) -> Result<Self, Box<dyn std::error::Error>> {
        // Load configuration first (before logging setup)
        let mut config = AppConfig::load_from_file(&args.config_path).await?;

        // Apply CLI overrides
        if let Some(plugin_dir) = args.plugin_dir {
            config.plugins.directory = plugin_dir.to_string_lossy().to_string();
        }

        if let Some(bind_address) = args.bind_address {
            config.server.bind_address = bind_address;
        }

        if let Some(log_level) = args.log_level {
            config.logging.level = log_level;
        }

        if args.json_logs {
            config.logging.json_format = true;
        }

        // Validate configuration
        if let Err(e) = config.validate() {
            return Err(format!("Configuration validation failed: {}", e).into());
        }

        // Setup logging
        //setup_logging(&config.logging, args.json_logs)?;

        // Display banner after logging is setup
        display_banner();

        // Create server with new architecture
        let server_config: ServerConfig = config.to_server_config()?;
        let server = GameServer::new(server_config);

        // Log startup information
        info!("🚀 Horizon Game Server v1.0.0 - Community Edition");
        info!("🏗️ Architecture: Core Infrastructure + Plugin System");
        info!("🎯 Features: Type-safe events, Clean separation, Zero unsafe plugins");
        info!(
            "📂 Config: {} | Plugins: {}",
            args.config_path.display(),
            config.plugins.directory
        );

        Ok(Self { config, server })
    }

    /// Run the application with enhanced monitoring
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        info!("🌟 Starting Horizon Game Server Application");
        info!("📋 Configuration Summary:");
        info!("  🌐 Bind address: {}", self.config.server.bind_address);
        info!("  🔌 Plugin directory: {}", self.config.plugins.directory);
        info!(
            "  🌍 Region: {:.0}x{:.0}x{:.0} units",
            self.config.server.region.max_x - self.config.server.region.min_x,
            self.config.server.region.max_y - self.config.server.region.min_y,
            self.config.server.region.max_z - self.config.server.region.min_z
        );
        info!(
            "  👥 Max connections: {}",
            self.config.server.max_connections
        );
        info!(
            "  ⏱️ Connection timeout: {}s",
            self.config.server.connection_timeout
        );

        // Get references for monitoring
        let horizon_event_system = self.server.get_horizon_event_system();
        //        let plugin_manager = self.server.();

        // Display initial statistics
        let initial_stats = horizon_event_system.get_stats().await;
        info!("📊 Initial Event System State:");
        info!("  - Handlers registered: {}", initial_stats.total_handlers);
        info!("  - Events emitted: {}", initial_stats.events_emitted);

        //        let plugin_stats = plugin_manager.get_plugin_stats().await;
        //        info!("📊 Initial Plugin System State:");
        //        info!("  - Plugins loaded: {}", plugin_stats.total_plugins);
        //        info!("  - Plugin handlers: {}", plugin_stats.total_handlers);

        // Start server in background with enhanced error handling
        let server_handle = {
            let server = self.server;
            tokio::spawn(async move {
                match server.start().await {
                    Ok(()) => {
                        info!("✅ Server completed successfully");
                    }
                    Err(e) => {
                        error!("❌ Server error: {:?}", e);
                        std::process::exit(1);
                    }
                }
            })
        };

        // Start monitoring task for real-time statistics
        let monitoring_handle = {
            let horizon_event_system = horizon_event_system.clone();
            //            let plugin_manager = plugin_manager.clone();

            tokio::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
                let mut last_events_emitted = 0u64;

                loop {
                    interval.tick().await;

                    // Display periodic statistics
                    let stats = horizon_event_system.get_stats().await;
                    //                    let plugin_stats = plugin_manager.get_plugin_stats().await;
                    let events_this_period = stats.events_emitted - last_events_emitted;
                    last_events_emitted = stats.events_emitted;

                    info!(
                        "📊 System Health - {} events/min | {} handlers | {} plugins active",
                        events_this_period, stats.total_handlers, ""
                    );

                    if events_this_period > 1000 {
                        info!(
                            "🔥 High activity detected - {} events processed this minute",
                            events_this_period
                        );
                    }
                }
            })
        };

        // Wait for shutdown signal
        info!("✅ Horizon Server is now running!");
        info!(
            "🎮 Ready to accept connections on {}",
            self.config.server.bind_address
        );
        info!("🔍 Health monitoring active - stats every 60 seconds");
        info!("🛑 Press Ctrl+C to gracefully shutdown");

        setup_signal_handlers().await?;

        info!("🛑 Shutdown signal received, initiating graceful shutdown...");

        // Cancel monitoring first
        monitoring_handle.abort();

        // The server's Drop implementation will handle plugin shutdown
        server_handle.abort();

        // Give time for graceful cleanup
        info!("⏳ Waiting for connections to close...");
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        info!("✅ Horizon Game Server shutdown complete");
        info!("📊 Final Statistics:");

        // Display final statistics if possible
        let final_stats = horizon_event_system.get_stats().await;
        info!("  - Total events processed: {}", final_stats.events_emitted);
        info!("  - Peak handlers: {}", final_stats.total_handlers);

        info!("👋 Thank you for using Horizon Game Server!");

        Ok(())
    }
}

// ============================================================================
// Entry Point
// ============================================================================

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI arguments
    let args = CliArgs::parse();

    // Create and run application
    match Application::new(args).await {
        Ok(app) => {
            if let Err(e) = app.run().await {
                error!("❌ Application error: {:?}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to start application: {:?}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

// ============================================================================
// Utilities and Helpers
// ============================================================================

/// Display startup banner using proper logging
fn display_banner() {
    // Fetch version from Cargo environment variable if available
    let version = option_env!("CARGO_PKG_VERSION").unwrap_or("UNK");
    info!("╔══════════════════════════════════════════╗");
    info!("║            🌟 HORIZON SERVER 🌟          ║");
    info!("║          Community Edition v{}       ║", version);
    info!("║                                          ║");
    info!("║  High-Performance Game Server            ║");
    info!("║  with Modern Plugin Architecture         ║");
    info!("║                                          ║");
    info!("║  🎯 Type-Safe Events                     ║");
    info!("║  🔌 Zero-Unsafe Plugins                  ║");
    info!("║  🛡️  Memory Safe Architecture             ║");
    info!("║  ⚡ High Performance Core                ║");
    info!("║  🌐 WebSocket + TCP Support              ║");
    info!("║                                          ║");
    info!("╚══════════════════════════════════════════╝");
}

/// Configuration validation
impl AppConfig {
    pub fn validate(&self) -> Result<(), String> {
        // Validate bind address
        if self
            .server
            .bind_address
            .parse::<std::net::SocketAddr>()
            .is_err()
        {
            return Err(format!(
                "Invalid bind address: {}",
                self.server.bind_address
            ));
        }

        // Validate region bounds
        if self.server.region.min_x >= self.server.region.max_x {
            return Err("Region min_x must be less than max_x".to_string());
        }
        if self.server.region.min_y >= self.server.region.max_y {
            return Err("Region min_y must be less than max_y".to_string());
        }
        if self.server.region.min_z >= self.server.region.max_z {
            return Err("Region min_z must be less than max_z".to_string());
        }

        // Validate plugin directory
        if self.plugins.directory.is_empty() {
            return Err("Plugin directory cannot be empty".to_string());
        }

        // Validate log level
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(format!(
                "Invalid log level: {}. Must be one of: {:?}",
                self.logging.level, valid_levels
            ));
        }

        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_config() {
        let config = AppConfig::default();
        assert!(config.validate().is_ok());

        // Test conversion to ServerConfig
        let server_config = config.to_server_config().expect("Default config should convert to ServerConfig test panic!");
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
        // This would require more setup to test properly with clap
        // For now, we'll just test that the structure is correct
        let args = CliArgs {
            config_path: PathBuf::from("test.toml"),
            plugin_dir: Some(PathBuf::from("test_plugins")),
            bind_address: Some("127.0.0.1:9000".to_string()),
            log_level: Some("debug".to_string()),
            json_logs: true,
        };

        assert_eq!(args.config_path, PathBuf::from("test.toml"));
        assert_eq!(args.plugin_dir, Some(PathBuf::from("test_plugins")));
        assert_eq!(args.bind_address, Some("127.0.0.1:9000".to_string()));
        assert_eq!(args.log_level, Some("debug".to_string()));
        assert_eq!(args.json_logs, true);
    }

    #[tokio::test]
    async fn test_application_creation() {
        let args = CliArgs {
            config_path: PathBuf::from("test_config.toml"),
            plugin_dir: None,
            bind_address: None,
            log_level: Some("debug".to_string()),
            json_logs: false,
        };

        // Create a test config file
        let test_config = AppConfig::default();
        let toml_content = toml::to_string_pretty(&test_config).expect("Failed to serialize default config to TOML");
        tokio::fs::write(&args.config_path, toml_content)
            .await
            .expect("Failed to write test config file to disk");

        // This would create the application but we can't easily test the full flow
        // without setting up the entire system
        assert!(args.config_path.exists());

        // Cleanup
        tokio::fs::remove_file(&args.config_path).await.ok();
    }
}
