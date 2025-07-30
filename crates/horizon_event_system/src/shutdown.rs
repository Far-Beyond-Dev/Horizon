//! Shutdown coordination for graceful server shutdown.
//!
//! This module provides shared shutdown state for coordinating graceful shutdown
//! across all server components, ensuring that existing events are processed
//! before final cleanup.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::info;

/// Shared shutdown state for coordinating graceful shutdown across components.
#[derive(Debug, Clone)]
pub struct ShutdownState {
    /// Flag indicating shutdown has been initiated - no new events should be processed
    shutdown_initiated: Arc<AtomicBool>,
    /// Flag indicating all existing events have been processed and final shutdown can begin
    shutdown_complete: Arc<AtomicBool>,
}

impl ShutdownState {
    /// Creates a new shutdown state with both flags set to false.
    pub fn new() -> Self {
        Self {
            shutdown_initiated: Arc::new(AtomicBool::new(false)),
            shutdown_complete: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns true if shutdown has been initiated - no new events should be processed.
    pub fn is_shutdown_initiated(&self) -> bool {
        self.shutdown_initiated.load(Ordering::Acquire)
    }

    /// Returns true if shutdown is complete and final cleanup can begin.
    pub fn is_shutdown_complete(&self) -> bool {
        self.shutdown_complete.load(Ordering::Acquire)
    }

    /// Initiates shutdown - sets the flag to stop accepting new events.
    pub fn initiate_shutdown(&self) {
        self.shutdown_initiated.store(true, Ordering::Release);
        info!("🛑 Shutdown initiated - no new events will be processed");
    }

    /// Marks shutdown as complete - all existing events have been processed.
    pub fn complete_shutdown(&self) {
        self.shutdown_complete.store(true, Ordering::Release);
        info!("✅ All events processed - ready for final cleanup");
    }
}

impl Default for ShutdownState {
    fn default() -> Self {
        Self::new()
    }
}