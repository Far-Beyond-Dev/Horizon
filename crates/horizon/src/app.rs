//! Main application logic and lifecycle management.
//!
//! This module contains the core `Application` struct that orchestrates
//! server startup, monitoring, and shutdown with enhanced error handling
//! and performance monitoring.

use crate::{cli::CliArgs, config::AppConfig, logging::display_banner, signals::{setup_signal_handlers, setup_signal_handlers_silent}};
use horizon_event_system::ShutdownState;
use game_server::GameServer;
use tracing::{error, info, warn};

/// Main application struct with enhanced monitoring capabilities.
/// 
/// The `Application` struct manages the complete lifecycle of the Horizon server,
/// including configuration loading, server initialization, health monitoring,
/// and graceful shutdown handling.
/// 
/// # Architecture
/// 
/// * **Configuration Management**: Loads and validates configuration from files and CLI
/// * **Server Orchestration**: Initializes and manages the game server instance
/// * **Health Monitoring**: Provides real-time statistics and performance monitoring
/// * **Graceful Shutdown**: Handles termination signals and cleanup procedures
pub struct Application {
    /// Loaded application configuration
    config: AppConfig,
    /// Game server instance
    server: GameServer,
}

impl Application {
    /// Creates a new application instance with the refactored architecture.
    /// 
    /// Loads configuration, applies CLI overrides, validates settings, and
    /// initializes the game server with proper error handling.
    /// 
    /// # Arguments
    /// 
    /// * `args` - Parsed command-line arguments
    /// 
    /// # Returns
    /// 
    /// A configured `Application` instance ready to run, or an error if
    /// initialization failed.
    /// 
    /// # Process
    /// 
    /// 1. Load configuration from file (creating default if missing)
    /// 2. Apply command-line argument overrides
    /// 3. Validate merged configuration
    /// 4. Display startup banner
    /// 5. Initialize game server with configuration
    /// 6. Log startup information and feature summary
    pub async fn new(args: CliArgs) -> Result<Self, Box<dyn std::error::Error>> {
        // Load configuration first (before logging setup)
        info!("🔧 Loading configuration from: {}", args.config_path.display());
        let mut config = AppConfig::load_from_file(&args.config_path).await?;
        
        info!("✅ Configuration loaded successfully from {}", args.config_path.display());

        // Extract plugin safety config before consuming args
        let plugin_safety_config = args.to_plugin_safety_config();

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
            return Err(format!("Configuration validation failed: {e}").into());
        } else {
            info!("✅ Configuration loaded and validated successfully");
        }

        // Display banner after logging is setup
        display_banner();

        // Create server with new architecture
        let server_config = config.to_server_config(plugin_safety_config)?;
        
        // Parse log level from config
        let log_level = match config.logging.level.as_str() {
            "error" => horizon_event_system::LogLevel::Error,
            "warn" => horizon_event_system::LogLevel::Warn,
            "info" => horizon_event_system::LogLevel::Info,
            "debug" => horizon_event_system::LogLevel::Debug,
            "trace" => horizon_event_system::LogLevel::Trace,
            _ => horizon_event_system::LogLevel::Info,
        };
        
        let server = GameServer::new(server_config, log_level);

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

    /// Runs the application with enhanced monitoring and error handling.
    /// 
    /// Starts the server, sets up monitoring tasks, waits for shutdown signals,
    /// and performs graceful cleanup with comprehensive statistics reporting.
    /// 
    /// # Returns
    /// 
    /// `Ok(())` if the application ran and shut down successfully, or an error
    /// if there was a critical failure during execution.
    /// 
    /// # Monitoring Features
    /// 
    /// * **Configuration Summary**: Displays key settings at startup
    /// * **Initial Statistics**: Shows system state before accepting connections
    /// * **Periodic Health Reports**: Real-time statistics every 60 seconds
    /// * **High Activity Detection**: Alerts for unusual event volumes
    /// * **Final Statistics**: Summary report during shutdown
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        info!("🌟 Starting Horizon Game Server Application");
        
        // Display configuration summary
        self.log_configuration_summary();

        // Get references for monitoring before moving the server
        let horizon_event_system = self.server.get_horizon_event_system();

        // Display initial statistics
        let initial_stats = horizon_event_system.get_stats().await;
        info!("📊 Initial Event System State:");
        info!("  - Handlers registered: {}", initial_stats.total_handlers);
        info!("  - Events emitted: {}", initial_stats.events_emitted);

        // Clone the config for final statistics display
        let config = self.config.clone();

        // Create shutdown state for coordinated shutdown  
        let shutdown_state = ShutdownState::new();
        let shutdown_state_for_server = shutdown_state.clone();

        // Get plugin manager reference before moving server
        let plugin_manager = self.server.get_plugin_manager();

        // Start server in background with enhanced error handling
        let server_handle = {
            let server = self.server;
            tokio::spawn(async move {
                match server.start_with_shutdown_state(shutdown_state_for_server).await {
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

            tokio::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
                let mut last_events_emitted = 0u64;

                loop {
                    interval.tick().await;

                    // Display periodic statistics
                    let stats = horizon_event_system.get_stats().await;
                    let events_this_period = stats.events_emitted - last_events_emitted;
                    last_events_emitted = stats.events_emitted;

                    info!(
                        "📊 System Health - {} events/min | {} handlers | {} plugins active",
                        events_this_period, stats.total_handlers, ""
                    );

                    if events_this_period > 10000 {
                        info!(
                            "🔥 High activity detected - {} events processed this minute",
                            events_this_period
                        );
                    }
                }
            })
        };

        // Display ready message
        info!("✅ Horizon Server is now running!");
        info!(
            "🎮 Ready to accept connections on {}",
            config.server.bind_address
        );
        info!("🔍 Health monitoring active - stats every 60 seconds");
        info!("🛑 Press Ctrl+C to gracefully shutdown");

        // Wait for shutdown signal - this will update the shared shutdown state
        let signal_shutdown_state = setup_signal_handlers().await?;

        // merciless shutdown
        tokio::spawn(async move {
            if let Err(e) = setup_signal_handlers_silent().await {
                error!("Failed to set up merciless shutdown signal handler: {e}");
                return;
            }

            warn!("Shutdown handler received again! I'll make this quick.");
            std::process::exit(1);
        });
        
        // Transfer shutdown state to our server's shutdown state
        if signal_shutdown_state.is_shutdown_initiated() {
            shutdown_state.initiate_shutdown();
        }

        info!("🛑 Shutdown signal received, beginning graceful shutdown...");

        // Phase 1: Stop accepting new connections and events
        info!("📡 Phase 1: Stopping new event processing...");
        
        // Cancel monitoring first
        monitoring_handle.abort();

        // Wait for existing events to be processed by the event system
        info!("⏳ Phase 2: Processing remaining events in the system...");
        
        // Give the event system time to process any pending events
        let mut wait_cycles = 0;
        const MAX_WAIT_CYCLES: u32 = 30; // Wait up to 3 seconds (30 * 100ms)
        
        while wait_cycles < MAX_WAIT_CYCLES {
            let stats = horizon_event_system.get_stats().await;
            
            // Check if there are any pending events or active handlers processing
            if stats.events_emitted == 0 && stats.total_handlers == 0 {
                break;
            }
            
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            wait_cycles += 1;
        }
        
        if wait_cycles >= MAX_WAIT_CYCLES {
            info!("⏰ Timeout reached, proceeding with shutdown (some events may not have completed)");
        } else {
            info!("✅ All events processed successfully");
        }
        
        // Mark shutdown as complete for the event system
        shutdown_state.complete_shutdown();

        // Phase 3: Final cleanup - shutdown server accept loops first
        info!("🧹 Phase 3: Final cleanup - stopping server accept loops...");
        
        // Wait for server accept loops to stop gracefully
        server_handle.abort();
        info!("⏳ Waiting for server task to complete gracefully...");
        if let Err(e) = tokio::time::timeout(
            tokio::time::Duration::from_secs(8), 
            server_handle
        ).await {
            warn!("⏰ Server task did not complete within timeout, proceeding with cleanup: {:?}", e);
            // Server task will be cancelled when it goes out of scope, but continue with plugin shutdown
        } else {
            info!("✅ Server task completed gracefully");
        }

        // Phase 4: Plugin shutdown (separate from server task to prevent timeout issues)
        info!("🔌 Phase 4: Shutting down plugins...");
        if let Err(e) = plugin_manager.shutdown().await {
            error!("❌ Plugin shutdown failed: {}", e);
        } else {
            info!("✅ Plugin shutdown completed successfully");
        }

        // Give time for connection cleanup
        info!("⏳ Waiting for connections to close...");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Display final statistics
        log_final_statistics(&horizon_event_system).await;

        info!("✅ Horizon Game Server shutdown complete");
        info!("👋 Thank you for using Horizon Game Server!");

        Ok(())
    }

    /// Logs the configuration summary at startup.
    fn log_configuration_summary(&self) {
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
    }
}

/// Logs final statistics during shutdown.
async fn log_final_statistics(horizon_event_system: &std::sync::Arc<horizon_event_system::EventSystem>) {
    info!("📊 Final Statistics:");
    let final_stats = horizon_event_system.get_stats().await;
    info!("  - Total events processed: {}", final_stats.events_emitted);
    info!("  - Peak handlers: {}", final_stats.total_handlers);
}
