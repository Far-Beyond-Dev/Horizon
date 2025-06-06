//! Event processing and distribution system
//! 
//! Manages game events and distributes them to subscribed plugins.

use shared_types::*;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, debug};

/// Processes and distributes game events to plugins
/// 
/// The EventProcessor manages:
/// - Event emission from various system components
/// - Event distribution to subscribed plugins
/// - Event queue management and processing
pub struct EventProcessor {
    /// Event broadcast channel sender
    event_sender: broadcast::Sender<(EventId, Arc<dyn GameEvent + Send + Sync>)>,
}

impl EventProcessor {
    /// Create a new event processor
    pub fn new() -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        
        Self {
            event_sender,
        }
    }
    
    /// Start the event processing system
    /// 
    /// This method should be called once to begin processing events.
    /// It's safe to call multiple times - subsequent calls are no-ops.
    pub async fn start(&self) {
        debug!("Event processor started");
        // Event processing is handled by the broadcast channel automatically
        // Plugins will subscribe to the event_sender when they're loaded
    }
    
    /// Emit an event to all subscribed listeners
    /// 
    /// # Arguments
    /// * `event_id` - Unique identifier for the event type
    /// * `event` - The event data to distribute
    /// 
    /// # Returns
    /// Result indicating success or failure of event emission
    /// 
    /// # Errors
    /// Returns `ServerError::Internal` if the event cannot be sent
    pub async fn emit_event(
        &self,
        event_id: EventId,
        event: Arc<dyn GameEvent + Send + Sync>
    ) -> Result<(), ServerError> {
        self.event_sender.send((event_id.clone(), event))
            .map_err(|e| ServerError::Internal(format!("Failed to emit event {}: {}", event_id, e)))?;
        
        debug!("Emitted event: {}", event_id);
        Ok(())
    }
    
    /// Get a receiver for subscribing to events
    /// 
    /// This is used by plugins to subscribe to the event stream.
    /// 
    /// # Returns
    /// A broadcast receiver for event notifications
    pub fn subscribe(&self) -> broadcast::Receiver<(EventId, Arc<dyn GameEvent + Send + Sync>)> {
        self.event_sender.subscribe()
    }
    
    /// Shutdown the event processor
    /// 
    /// Stops processing events and cleans up resources.
    pub async fn shutdown(&self) {
        debug!("Event processor shutting down");
        // The broadcast channel will automatically clean up when all senders are dropped
    }
}

impl Default for EventProcessor {
    fn default() -> Self {
        Self::new()
    }
}