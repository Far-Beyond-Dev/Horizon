//! Event processing and distribution system with callback-based routing
//! 
//! Manages game events and directly dispatches them to registered plugin callbacks.

use shared_types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, debug, warn};

/// Plugin reference for callback dispatch
#[derive(Clone)]
pub struct PluginRef {
    pub plugin: Arc<RwLock<Box<dyn Plugin>>>,
    pub context: Arc<dyn ServerContext>,
}

/// Processes and distributes game events directly to plugin callbacks
/// 
/// The EventProcessor manages:
/// - Event callback registration from plugins
/// - Direct event dispatch to interested plugins
/// - Event routing and filtering
pub struct EventProcessor {
    /// Map of event IDs to plugin callbacks
    event_callbacks: Arc<RwLock<HashMap<EventId, Vec<PluginRef>>>>,
    /// Flag to track if processor is running
    is_running: Arc<tokio::sync::RwLock<bool>>,
}

impl EventProcessor {
    /// Create a new event processor
    pub fn new() -> Self {
        Self {
            event_callbacks: Arc::new(RwLock::new(HashMap::new())),
            is_running: Arc::new(tokio::sync::RwLock::new(false)),
        }
    }
    
    /// Start the event processing system
    /// 
    /// This method marks the processor as active and ready to dispatch events.
    pub async fn start(&self) {
        let mut running = self.is_running.write().await;
        if !*running {
            *running = true;
            debug!("Event processor started with callback-based dispatch");
        }
    }
    
    /// Register a plugin for specific events
    /// 
    /// # Arguments
    /// * `plugin` - Plugin instance to register
    /// * `context` - Server context for the plugin
    /// * `event_ids` - List of event IDs the plugin wants to receive
    pub async fn register_plugin_callbacks(
        &self,
        plugin: Arc<RwLock<Box<dyn Plugin>>>,
        context: Arc<dyn ServerContext>,
        event_ids: Vec<EventId>,
    ) -> Result<(), ServerError> {
        let mut callbacks = self.event_callbacks.write().await;
        let plugin_ref = PluginRef { plugin, context };
        
        let plugin_name = {
            let p = plugin_ref.plugin.read().await;
            p.name().to_string()
        };
        
        for event_id in event_ids {
            callbacks
                .entry(event_id.clone())
                .or_insert_with(Vec::new)
                .push(plugin_ref.clone());
            
            debug!("Registered plugin '{}' for event: {}", plugin_name, event_id);
        }
        
        debug!("Plugin '{}' registered for {} events", plugin_name, callbacks.len());
        Ok(())
    }
    
    /// Emit an event and directly dispatch to registered callbacks
    /// 
    /// # Arguments
    /// * `event_id` - Unique identifier for the event type
    /// * `event` - The event data to distribute
    /// 
    /// # Returns
    /// Result indicating success or failure of event emission and dispatch
    pub async fn emit_event(
        &self,
        event_id: EventId,
        event: Arc<dyn GameEvent + Send + Sync>
    ) -> Result<(), ServerError> {
        // Check if processor is running
        let running = self.is_running.read().await;
        if !*running {
            return Err(ServerError::Internal("Event processor not started".to_string()));
        }
        drop(running);
        
        debug!("Emitting event: {}", event_id);
        
        // Get registered callbacks for this event
        let callbacks = self.event_callbacks.read().await;
        let interested_plugins = callbacks.get(&event_id).cloned().unwrap_or_default();
        drop(callbacks);
        
        if interested_plugins.is_empty() {
            debug!("No plugins registered for event: {}", event_id);
            return Ok(());
        }
        
        debug!("Dispatching event {} to {} plugins", event_id, interested_plugins.len());
        
        // Dispatch event to all registered plugins concurrently
        let dispatch_tasks: Vec<_> = interested_plugins
            .into_iter()
            .map(|plugin_ref| {
                let event_id = event_id.clone();
                let event = event.clone();
                tokio::spawn(async move {
                    let plugin_name = {
                        let p = plugin_ref.plugin.read().await;
                        p.name().to_string()
                    };
                    
                    // Call the plugin's handle_event method
                    let mut plugin = plugin_ref.plugin.write().await;
                    match plugin.handle_event(&event_id, event.as_ref(), plugin_ref.context.as_ref()).await {
                        Ok(()) => {
                            debug!("Plugin '{}' successfully handled event: {}", plugin_name, event_id);
                        }
                        Err(e) => {
                            error!("Plugin '{}' failed to handle event {}: {}", plugin_name, event_id, e);
                        }
                    }
                })
            })
            .collect();
        
        // Wait for all dispatches to complete (but don't fail if individual plugins error)
        for task in dispatch_tasks {
            if let Err(e) = task.await {
                error!("Plugin dispatch task failed: {}", e);
            }
        }
        
        debug!("Event {} dispatch completed", event_id);
        Ok(())
    }
    
    /// Get statistics about registered callbacks
    /// 
    /// # Returns
    /// HashMap mapping event IDs to the number of registered plugins
    pub async fn get_callback_stats(&self) -> HashMap<String, usize> {
        let callbacks = self.event_callbacks.read().await;
        callbacks
            .iter()
            .map(|(event_id, plugins)| (event_id.to_string(), plugins.len()))
            .collect()
    }
    
    /// Remove all callbacks for a specific plugin
    /// 
    /// This is useful when a plugin is being unloaded or reloaded.
    /// 
    /// # Arguments
    /// * `plugin_name` - Name of the plugin to remove callbacks for
    pub async fn unregister_plugin(&self, plugin_name: &str) -> Result<usize, ServerError> {
        let mut callbacks = self.event_callbacks.write().await;
        let mut removed_count = 0;
        
        for (event_id, plugin_refs) in callbacks.iter_mut() {
            let initial_len = plugin_refs.len();
            plugin_refs.retain(|plugin_ref| {
                // Check if this plugin matches the name we want to remove
                // Note: This is an async operation in a sync context, so we need to be careful
                // For now, we'll keep all plugins (this could be improved with a different design)
                true // TODO: Implement proper plugin name matching
            });
            removed_count += initial_len - plugin_refs.len();
        }
        
        // Remove empty event entries
        callbacks.retain(|_, plugin_refs| !plugin_refs.is_empty());
        
        if removed_count > 0 {
            debug!("Removed {} callbacks for plugin '{}'", removed_count, plugin_name);
        }
        
        Ok(removed_count)
    }
    
    /// Get the number of events with registered callbacks
    pub async fn registered_event_count(&self) -> usize {
        let callbacks = self.event_callbacks.read().await;
        callbacks.len()
    }
    
    /// Get the total number of registered callbacks across all events
    pub async fn total_callback_count(&self) -> usize {
        let callbacks = self.event_callbacks.read().await;
        callbacks.values().map(|v| v.len()).sum()
    }
    
    /// Shutdown the event processor
    /// 
    /// Stops processing events and clears all registered callbacks.
    pub async fn shutdown(&self) {
        let mut running = self.is_running.write().await;
        *running = false;
        
        let mut callbacks = self.event_callbacks.write().await;
        let total_callbacks = callbacks.values().map(|v| v.len()).sum::<usize>();
        callbacks.clear();
        
        debug!("Event processor shut down. Cleared {} callbacks", total_callbacks);
    }
}

impl Default for EventProcessor {
    fn default() -> Self {
        Self::new()
    }
}