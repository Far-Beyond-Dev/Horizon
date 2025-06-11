//! Graceful shutdown handling
//! 
//! This module provides cross-platform signal handling for graceful server shutdown.
//! It listens for termination signals and provides a clean way to shut down the server.

use tokio::sync::oneshot;
use tracing::info;
use horizon_server::shutdown::setup_shutdown_handler;

/// Set up a shutdown signal handler
/// 
/// Creates a signal handler that listens for termination signals and provides
/// a channel receiver that will be triggered when shutdown is requested.
/// 
/// # Platform Support
/// * Unix/Linux: Handles SIGINT (Ctrl+C) and SIGTERM signals
/// * Windows: Handles Ctrl+C events
/// 
/// # Returns
/// * `oneshot::Receiver<()>` - Receiver that will be triggered on shutdown signal
pub async fn setup_shutdown_handler() -> oneshot::Receiver<()> {
    let (tx, rx) = oneshot::channel();
    
    tokio::spawn(async move {
        let mut tx = Some(tx);
        
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            
            // Set up handlers for common Unix termination signals
            let mut sigint = signal(SignalKind::interrupt())
                .expect("Failed to create SIGINT handler");
            let mut sigterm = signal(SignalKind::terminate())
                .expect("Failed to create SIGTERM handler");
            
            tokio::select! {
                _ = sigint.recv() => {
                    info!("SIGINT received - initiating graceful shutdown");
                }
                _ = sigterm.recv() => {
                    info!("SIGTERM received - initiating graceful shutdown");
                }
            }
        }
        
        #[cfg(windows)]
        {
            use tokio::signal::windows::ctrl_c;
            
            let mut ctrl_c = ctrl_c()
                .expect("Failed to create Ctrl+C handler");
            
            ctrl_c.recv().await;
            info!("Ctrl+C received - initiating graceful shutdown");
        }
        
        // Send shutdown signal if sender is still available
        if let Some(tx) = tx.take() {
            let _ = tx.send(());
        }
    });
    
    rx
}

/// Setup shutdown handler with custom timeout
/// 
/// Similar to `setup_shutdown_handler` but allows specifying a timeout
/// after which the shutdown will be forced.
/// 
/// # Arguments
/// * `timeout_secs` - Number of seconds to wait before forcing shutdown
/// 
/// # Returns
/// * `oneshot::Receiver<()>` - Receiver that will be triggered on shutdown signal
pub async fn setup_shutdown_handler_with_timeout(timeout_secs: u64) -> oneshot::Receiver<()> {
    let (tx, rx) = oneshot::channel();
    
    tokio::spawn(async move {
        let mut tx = Some(tx);
        
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            use tokio::time::{sleep, Duration};
            
            let mut sigint = signal(SignalKind::interrupt())
                .expect("Failed to create SIGINT handler");
            let mut sigterm = signal(SignalKind::terminate())
                .expect("Failed to create SIGTERM handler");
            
            tokio::select! {
                _ = sigint.recv() => {
                    info!("SIGINT received - initiating graceful shutdown");
                }
                _ = sigterm.recv() => {
                    info!("SIGTERM received - initiating graceful shutdown");
                }
            }
            
            // Start timeout for forced shutdown
            let tx_clone = tx.take();
            tokio::spawn(async move {
                sleep(Duration::from_secs(timeout_secs)).await;
                info!("Shutdown timeout reached - forcing exit");
                std::process::exit(1);
            });
            
            if let Some(tx) = tx_clone {
                let _ = tx.send(());
            }
        }
        
        #[cfg(windows)]
        {
            use tokio::signal::windows::ctrl_c;
            use tokio::time::{sleep, Duration};
            
            let mut ctrl_c = ctrl_c()
                .expect("Failed to create Ctrl+C handler");
            
            ctrl_c.recv().await;
            info!("Ctrl+C received - initiating graceful shutdown");
            
            // Start timeout for forced shutdown
            let tx_clone = tx.take();
            tokio::spawn(async move {
                sleep(Duration::from_secs(timeout_secs)).await;
                info!("Shutdown timeout reached - forcing exit");
                std::process::exit(1);
            });
            
            if let Some(tx) = tx_clone {
                let _ = tx.send(());
            }
        }
    });
    
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_shutdown_handler_creation() {
        // Test that the shutdown handler can be created without panicking
        let shutdown_rx = setup_shutdown_handler().await;
        
        // The receiver should be ready but not yet triggered
        let result = timeout(Duration::from_millis(10), shutdown_rx).await;
        assert!(result.is_err()); // Should timeout since no signal was sent
    }

    #[tokio::test]
    async fn test_shutdown_handler_with_timeout_creation() {
        // Test that the timeout version can be created
        let shutdown_rx = setup_shutdown_handler_with_timeout(1).await;
        
        // Should not be immediately ready
        let result = timeout(Duration::from_millis(10), shutdown_rx).await;
        assert!(result.is_err()); // Should timeout since no signal was sent
    }
}