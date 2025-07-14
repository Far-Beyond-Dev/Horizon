//! Signal handling for graceful server shutdown.
//!
//! This module provides cross-platform signal handling to allow the server
//! to shut down gracefully when receiving termination signals.

use tokio::signal;
use tracing::info;

/// Sets up graceful shutdown signal handling for the application.
/// 
/// Listens for termination signals (SIGINT, SIGTERM on Unix; Ctrl+C on Windows)
/// and returns when one is received, allowing the application to perform
/// cleanup operations before exiting.
/// 
/// # Platform Support
/// 
/// * **Unix platforms**: Handles SIGINT and SIGTERM signals
/// * **Windows**: Handles Ctrl+C signal
/// 
/// # Returns
/// 
/// `Ok(())` when a shutdown signal is received, or an error if signal
/// handling setup failed.
/// 
/// # Example
/// 
/// ```rust
/// use horizon::signals::setup_signal_handlers;
/// 
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Start your server...
///     
///     // Wait for shutdown signal
///     setup_signal_handlers().await?;
///     
///     // Perform cleanup...
///     
///     Ok(())
/// }
/// ```
pub async fn setup_signal_handlers() -> Result<(), Box<dyn std::error::Error>> {
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