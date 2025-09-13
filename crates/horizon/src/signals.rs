//! Signal handling for graceful server shutdown.
//!
//! This module provides cross-platform signal handling to allow the server
//! to shut down gracefully when receiving termination signals. It supports
//! a two-phase shutdown process: first stopping new events, then processing
//! existing events before final cleanup.

use horizon_event_system::ShutdownState;
use tokio::signal;
use tracing::info;

/// Sets up graceful shutdown signal handling for the application.
/// 
/// Listens for termination signals (SIGINT, SIGTERM on Unix; Ctrl+C on Windows)
/// and returns when one is received, along with a shutdown state for coordinating
/// graceful shutdown across components.
/// 
/// # Platform Support
/// 
/// * **Unix platforms**: Handles SIGINT and SIGTERM signals
/// * **Windows**: Handles Ctrl+C signal
/// 
/// # Returns
/// 
/// `Ok(shutdown_state)` when a shutdown signal is received, containing the
/// shutdown coordination state, or an error if signal handling setup failed.
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
///     let shutdown_state = setup_signal_handlers().await?;
///     
///     // Use shutdown_state to coordinate graceful shutdown...
///     
///     Ok(())
/// }
/// ```
pub async fn setup_signal_handlers() -> Result<ShutdownState, Box<dyn std::error::Error>> {
    let shutdown_state = setup_signal_handlers_silent().await?;
    info!("📡 Received shutdown signal - initiating graceful shutdown");
    Ok(shutdown_state)
}

pub async fn setup_signal_handlers_silent() -> Result<ShutdownState, Box<dyn std::error::Error>> {
    let shutdown_state = ShutdownState::new();

    #[cfg(unix)]
    {
        use signal::unix::{signal, SignalKind};

        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigterm = signal(SignalKind::terminate())?;

        tokio::select! {
            _ = sigint.recv() => (),
            _ = sigterm.recv() => ()
        }
    }

    #[cfg(windows)]
    signal::ctrl_c().await?;

    shutdown_state.initiate_shutdown();
    Ok(shutdown_state)
}
