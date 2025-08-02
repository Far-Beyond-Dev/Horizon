//! Asynchronous logging system with dedicated thread.
//!
//! This module provides a non-blocking logging system that offloads log processing
//! to a dedicated thread, preventing main/hot threads from being blocked by stdout speed.

use crate::context::LogLevel;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};

/// Log message sent to the dedicated logging thread.
#[derive(Debug, Clone)]
pub struct LogMessage {
    pub level: LogLevel,
    pub message: String,
    pub target: Option<String>,
}

/// Asynchronous logging handle for non-blocking log operations.
#[derive(Debug, Clone)]
pub struct AsyncLogger {
    sender: mpsc::UnboundedSender<LogMessage>,
}

impl AsyncLogger {
    /// Creates a new async logger with a dedicated background thread.
    /// 
    /// Returns the logger handle and spawns a background task that processes
    /// log messages without blocking the caller.
    pub fn new() -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<LogMessage>();
        
        // Spawn dedicated logging task
        tokio::spawn(async move {
            while let Some(log_msg) = receiver.recv().await {
                Self::write_log(log_msg);
            }
            
            // Process any remaining messages before shutdown
            while let Ok(log_msg) = receiver.try_recv() {
                Self::write_log(log_msg);
            }
        });
        
        Self { sender }
    }
    
    /// Log a message asynchronously without blocking the caller.
    /// 
    /// This method immediately returns after queuing the message for processing
    /// by the dedicated logging thread.
    pub fn log(&self, level: LogLevel, message: &str) {
        self.log_with_target(level, message, None);
    }
    
    /// Log a message with a specific target asynchronously.
    /// 
    /// The target can be used to categorize log messages (e.g., "plugin", "network").
    pub fn log_with_target(&self, level: LogLevel, message: &str, target: Option<&str>) {
        let log_msg = LogMessage {
            level,
            message: message.to_string(),
            target: target.map(|t| t.to_string()),
        };
        
        // Use try_send to avoid blocking if the channel is full
        // In high-load scenarios, we prefer to drop log messages rather than block
        if let Err(_) = self.sender.send(log_msg) {
            // Logger has been dropped or channel is closed
            // In production, we might want to fall back to synchronous logging
            eprintln!("Warning: Async logger unavailable, log message dropped");
        }
    }
    
    /// Internal method to write log messages using tracing.
    /// 
    /// This runs on the dedicated logging thread and performs the actual
    /// I/O operations without blocking other threads.
    fn write_log(log_msg: LogMessage) {
        let message = if let Some(target) = &log_msg.target {
            format!("{}: {}", target, log_msg.message)
        } else {
            log_msg.message
        };
        
        match log_msg.level {
            LogLevel::Error => error!("{}", message),
            LogLevel::Warn => warn!("{}", message),
            LogLevel::Info => info!("{}", message),
            LogLevel::Debug => debug!("{}", message),
            LogLevel::Trace => trace!("{}", message),
        }
    }
    
    /// Creates a logger handle that can be safely shared across threads.
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

/// Global async logger instance for use throughout the application.
/// 
/// This provides a singleton pattern for the async logger while maintaining
/// thread safety and avoiding the overhead of multiple logging threads.
static GLOBAL_LOGGER: std::sync::OnceLock<Arc<AsyncLogger>> = std::sync::OnceLock::new();

/// Initialize the global async logger.
/// 
/// This should be called once during application startup to set up the
/// dedicated logging thread.
pub fn init_global_async_logger() {
    GLOBAL_LOGGER.get_or_init(|| AsyncLogger::shared());
}

/// Get the global async logger instance.
/// 
/// Returns the shared logger instance, initializing it if not already done.
/// This is safe to call from multiple threads concurrently.
pub fn global_async_logger() -> Arc<AsyncLogger> {
    GLOBAL_LOGGER
        .get_or_init(|| AsyncLogger::shared())
        .clone()
}