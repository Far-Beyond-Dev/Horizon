//! Logging system setup and configuration.
//!
//! This module handles the initialization and configuration of the tracing-based
//! logging system with support for both human-readable and JSON output formats.

use crate::config::LoggingSettings;
use tracing::info;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initializes the logging system with the specified configuration.
/// 
/// Sets up tracing-subscriber with appropriate formatting, filtering, and output
/// based on the provided logging settings and CLI overrides.
/// 
/// # Arguments
/// 
/// * `config` - Logging configuration from the config file
/// * `json_format` - Whether to force JSON output format (CLI override)
/// 
/// # Returns
/// 
/// `Ok(())` if logging was set up successfully, or an error if initialization failed.
/// 
/// # Features
/// 
/// * **Environment variable support** - Respects `RUST_LOG` if set
/// * **Flexible formatting** - Human-readable or JSON output
/// * **Thread information** - Includes thread IDs and names for debugging
/// * **Performance optimized** - Minimal overhead when logging is disabled
pub fn setup_logging(
    config: &LoggingSettings,
    json_format: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let log_level = config.level.as_str();
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));

    let registry = tracing_subscriber::registry().with(filter);

    if json_format || config.json_format {
        // JSON formatting with thread info for structured logging
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
        // Human-readable formatting with thread info for development
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

/// Displays the startup banner using proper logging.
/// 
/// Shows the Horizon server logo and version information using structured
/// logging instead of direct console output.
pub fn display_banner() {
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