//! Logging system setup and configuration
//! 
//! This module handles the initialization of the tracing-based logging system
//! used throughout the server for debugging, monitoring, and diagnostic output.

use anyhow::Result;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::config::Args;

/// Initialize the logging system
/// 
/// Sets up structured logging using the tracing crate with configurable
/// output format and filtering levels. The logging level can be controlled
/// through command-line arguments or environment variables.
/// 
/// # Arguments
/// * `args` - Command line arguments containing debug flag
/// 
/// # Returns
/// * `Result<()>` - Success or error during logging setup
/// 
/// # Environment Variables
/// * `RUST_LOG` - Override the default logging filter (e.g., "debug", "my_crate=trace")
pub fn setup_logging(args: &Args) -> Result<()> {
    // Determine the base logging level from arguments
    let level = if args.debug { "debug" } else { "info" };
    
    // Create a filter that respects RUST_LOG environment variable,
    // falling back to the level determined from command-line args
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));
    
    // Initialize the global tracing subscriber with:
    // - Environment-aware filtering
    // - Formatted output without target module names (cleaner output)
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .init();
    
    Ok(())
}

/// Initialize logging with JSON format
/// 
/// Alternative logging setup that outputs structured JSON logs,
/// useful for log aggregation systems and machine parsing.
/// 
/// # Arguments
/// * `args` - Command line arguments containing debug flag
/// * `json_format` - Whether to use JSON formatting
/// 
/// # Returns
/// * `Result<()>` - Success or error during logging setup
pub fn setup_logging_with_format(args: &Args, json_format: bool) -> Result<()> {
    let level = if args.debug { "debug" } else { "info" };
    
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));
    
    if json_format {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json().with_target(false))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_target(false))
            .init();
    }
    
    Ok(())
}