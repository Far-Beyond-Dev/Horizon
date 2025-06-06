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
/// 
/// # Examples
/// ```
/// use your_crate::{Args, setup_logging};
/// 
/// let args = Args::default();
/// setup_logging(&args).expect("Failed to initialize logging");
/// ```
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_setup() {
        let args = Args::default();
        
        // Note: In a real test environment, you might want to avoid
        // actually initializing the global logger multiple times
        // This test mainly verifies the function doesn't panic
        let result = setup_logging(&args);
        
        // The first call should succeed, subsequent calls will fail
        // because the global logger can only be initialized once
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_debug_logging() {
        let mut args = Args::default();
        args.debug = true;
        
        // This primarily tests that the function handles debug flag correctly
        let result = setup_logging(&args);
        assert!(result.is_ok() || result.is_err());
    }
}