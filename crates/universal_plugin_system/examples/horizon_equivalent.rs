//! Example showing how to recreate the exact Horizon plugin system using the universal plugin system

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use universal_plugin_system::*;

// ============================================================================
// Horizon-equivalent Event Types
// ============================================================================

/// Horizon's RawClientMessageEvent equivalent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawClientMessageEvent {
    pub player_id: u64,
    pub namespace: String,
    pub event_name: String,
    pub data: Vec<u8>,
    pub timestamp: u64,
}

impl Event for RawClientMessageEvent {
    fn event_type() -> &'static str {
        "raw_client_message"
    }
}

/// Horizon's GorcEvent equivalent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GorcEvent {
    pub object_id: String,
    pub object_type: String,
    pub channel: u8,
    pub data: Vec<u8>,
    pub priority: String,
    pub timestamp: u64,
}

impl Event for GorcEvent {
    fn event_type() -> &'static str {
        "gorc_event"
    }
}

/// Server startup event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStartedEvent {
    pub timestamp: u64,
    pub region_id: String,
}

impl Event for ServerStartedEvent {
    fn event_type() -> &'static str {
        "server_started"
    }
}

// ============================================================================
// Horizon-equivalent Context Providers
// ============================================================================

/// Equivalent to Horizon's ServerContext
pub struct HorizonServerContext {
    pub region_id: String,
    pub network_provider: NetworkProvider,
    pub logging_provider: LoggingProvider,
}

impl HorizonServerContext {
    pub fn new(region_id: String) -> Self {
        Self {
            region_id,
            network_provider: NetworkProvider::new(),
            logging_provider: LoggingProvider::new("horizon".to_string()),
        }
    }

    pub async fn send_to_player(&self, player_id: u64, data: &[u8]) -> Result<(), PluginSystemError> {
        self.network_provider.send_message(&player_id.to_string(), data).await
    }

    pub async fn broadcast(&self, data: &[u8]) -> Result<(), PluginSystemError> {
        self.network_provider.broadcast(data).await
    }

    pub fn log(&self, level: LogLevel, message: &str) {
        self.logging_provider.log(level, message);
    }
}

// ============================================================================
// Horizon-equivalent Event Propagator (GORC-like)
// ============================================================================

/// Horizon-style propagator that supports core, client, plugin, and GORC events
pub struct HorizonPropagator {
    /// Spatial propagator for GORC events
    spatial: SpatialPropagator,
    /// Channel propagator for GORC channels
    channel: ChannelPropagator,
    /// Namespace filtering
    namespace: NamespacePropagator,
}

impl HorizonPropagator {
    pub fn new() -> Self {
        let spatial = SpatialPropagator::new(100.0); // 100 unit range
        
        let channel = ChannelPropagator::new()
            .add_channel(0, ChannelConfig {
                max_frequency: 60.0,
                max_distance: 50.0,
                priority: 255,
            })
            .add_channel(1, ChannelConfig {
                max_frequency: 30.0,
                max_distance: 200.0,
                priority: 192,
            })
            .add_channel(2, ChannelConfig {
                max_frequency: 15.0,
                max_distance: 500.0,
                priority: 128,
            })
            .add_channel(3, ChannelConfig {
                max_frequency: 5.0,
                max_distance: 1000.0,
                priority: 64,
            });

        let namespace = NamespacePropagator::new()
            .allow_namespaces(vec!["core", "client", "plugin", "gorc", "gorc_instance"]);

        Self {
            spatial,
            channel,
            namespace,
        }
    }

    pub async fn update_player_position(&self, player_id: &str, x: f32, y: f32, z: f32) {
        self.spatial.update_player_position(player_id, x, y, z).await;
    }
}

#[async_trait::async_trait]
impl EventPropagator for HorizonPropagator {
    async fn should_propagate(&self, event_key: &str, context: &PropagationContext) -> bool {
        // First check namespace
        if !self.namespace.should_propagate(event_key, context).await {
            return false;
        }

        // For GORC events, use spatial and channel filtering
        if event_key.starts_with("gorc") {
            if !self.spatial.should_propagate(event_key, context).await {
                return false;
            }
            if !self.channel.should_propagate(event_key, context).await {
                return false;
            }
        }

        true
    }

    async fn transform_event(
        &self,
        event: Arc<EventData>,
        context: &PropagationContext,
    ) -> Option<Arc<EventData>> {
        // For GORC events, add spatial information
        if context.event_key.starts_with("gorc") {
            return self.spatial.transform_event(event, context).await;
        }

        Some(event)
    }
}

// ============================================================================
// Horizon-equivalent EventBus Extensions
// ============================================================================

/// Extension trait to add Horizon-style event registration methods
pub trait HorizonEventBusExt {
    /// Register a handler for core server events (equivalent to on_core)
    async fn on_core<T, F>(&mut self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static;

    /// Register a handler for client events with namespace (equivalent to on_client)
    async fn on_client<T, F>(
        &mut self,
        namespace: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static;

    /// Register a handler for plugin-to-plugin events (equivalent to on_plugin)
    async fn on_plugin<T, F>(
        &mut self,
        plugin_name: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static;

    /// Register a handler for GORC object events (equivalent to on_gorc)
    async fn on_gorc<T, F>(
        &mut self,
        object_type: &str,
        channel: u8,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static;

    /// Emit a core server event (equivalent to emit_core)
    async fn emit_core<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event + Serialize;

    /// Emit a client event (equivalent to emit_client)
    async fn emit_client<T>(
        &self,
        namespace: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize;

    /// Emit a plugin event (equivalent to emit_plugin)
    async fn emit_plugin<T>(
        &self,
        plugin_name: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize;

    /// Emit a GORC event (equivalent to emit_gorc)
    async fn emit_gorc<T>(
        &self,
        object_type: &str,
        channel: u8,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize;
}

impl HorizonEventBusExt for EventBus<HorizonPropagator> {
    async fn on_core<T, F>(&mut self, event_name: &str, handler: F) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        self.on("core", event_name, handler).await
    }

    async fn on_client<T, F>(
        &mut self,
        namespace: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        self.on_categorized("client", namespace, event_name, handler).await
    }

    async fn on_plugin<T, F>(
        &mut self,
        plugin_name: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        self.on_categorized("plugin", plugin_name, event_name, handler).await
    }

    async fn on_gorc<T, F>(
        &mut self,
        object_type: &str,
        channel: u8,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let channel_str = channel.to_string();
        self.on_categorized("gorc", &format!("{}:{}", object_type, channel_str), event_name, handler).await
    }

    async fn emit_core<T>(&self, event_name: &str, event: &T) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        self.emit("core", event_name, event).await
    }

    async fn emit_client<T>(
        &self,
        namespace: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        self.emit_categorized("client", namespace, event_name, event).await
    }

    async fn emit_plugin<T>(
        &self,
        plugin_name: &str,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        self.emit_categorized("plugin", plugin_name, event_name, event).await
    }

    async fn emit_gorc<T>(
        &self,
        object_type: &str,
        channel: u8,
        event_name: &str,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        let channel_str = channel.to_string();
        self.emit_categorized("gorc", &format!("{}:{}", object_type, channel_str), event_name, event).await
    }
}

// ============================================================================
// Example Plugin using Horizon-equivalent API
// ============================================================================

/// Example plugin that demonstrates the Horizon-equivalent API
pub struct HorizonExamplePlugin {
    name: String,
    message_count: u64,
}

impl HorizonExamplePlugin {
    pub fn new() -> Self {
        Self {
            name: "horizon_example".to_string(),
            message_count: 0,
        }
    }
}

#[async_trait::async_trait]
impl SimplePlugin<HorizonPropagator> for HorizonExamplePlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn register_handlers(
        &mut self,
        event_bus: Arc<EventBus<HorizonPropagator>>,
        context: Arc<PluginContext<HorizonPropagator>>,
    ) -> Result<(), PluginSystemError> {
        // Cast to mutable reference - in practice, you'd handle this differently
        let mut event_bus_mut = (*event_bus).clone();
        
        // Register core event handler
        event_bus_mut.on_core::<ServerStartedEvent, _>("server_started", |event| {
            println!("🚀 Server started at {} in region {}", event.timestamp, event.region_id);
            Ok(())
        }).await.map_err(|e| PluginSystemError::EventError(e))?;

        // Register client event handler
        event_bus_mut.on_client::<RawClientMessageEvent, _>("chat", "message", |event| {
            println!("💬 Chat message from player {}: {} bytes", event.player_id, event.data.len());
            Ok(())
        }).await.map_err(|e| PluginSystemError::EventError(e))?;

        // Register GORC event handler
        event_bus_mut.on_gorc::<GorcEvent, _>("Asteroid", 0, "position_update", |event| {
            println!("🪨 Asteroid {} position update on channel {}", event.object_id, event.channel);
            Ok(())
        }).await.map_err(|e| PluginSystemError::EventError(e))?;

        // Get logging provider from context
        if let Some(logging) = context.get_provider::<LoggingProvider>() {
            logging.info("Event handlers registered successfully");
        }

        Ok(())
    }

    async fn on_init(&mut self, context: Arc<PluginContext<HorizonPropagator>>) -> Result<(), PluginSystemError> {
        if let Some(logging) = context.get_provider::<LoggingProvider>() {
            logging.info("Plugin initialized successfully");
        }
        Ok(())
    }

    async fn on_shutdown(&mut self, context: Arc<PluginContext<HorizonPropagator>>) -> Result<(), PluginSystemError> {
        if let Some(logging) = context.get_provider::<LoggingProvider>() {
            logging.info(&format!("Plugin shutting down. Processed {} messages", self.message_count));
        }
        Ok(())
    }
}

// ============================================================================
// Demonstration
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::init();

    println!("🌟 Horizon-equivalent Plugin System Demo");

    // Create the Horizon-style propagator
    let propagator = HorizonPropagator::new();
    
    // Create event bus with custom propagator
    let event_bus = Arc::new(EventBus::with_propagator(propagator));
    
    // Create context with Horizon-style providers
    let mut context = PluginContext::new(event_bus.clone());
    
    // Add Horizon-equivalent context providers
    let server_context = HorizonServerContext::new("region_us_west".to_string());
    context.add_provider(server_context.logging_provider.clone());
    context.add_provider(server_context.network_provider.clone());
    
    let context = Arc::new(context);
    
    // Create plugin manager
    let config = PluginConfig {
        allow_version_mismatch: false,
        allow_abi_mismatch: false,
        strict_versioning: false,
        max_plugins: Some(10),
        search_directories: vec![],
    };
    
    let manager = PluginManager::new(event_bus.clone(), context.clone(), config);
    
    // Create and load a plugin using the factory
    let factory = create_plugin_factory!(HorizonExamplePlugin, HorizonPropagator, "horizon_example", "1.0.0");
    let plugin_name = manager.load_plugin_from_factory(Box::new(factory)).await?;
    
    println!("✅ Loaded plugin: {}", plugin_name);
    
    // Get a mutable reference to the event bus for emitting events
    // Note: In practice, you'd want a better design for this
    
    // Demonstrate the Horizon-equivalent API
    let mut event_bus_mut = (*event_bus).clone(); // This is a simplified approach
    
    // Emit core event
    event_bus_mut.emit_core("server_started", &ServerStartedEvent {
        timestamp: current_timestamp(),
        region_id: "region_us_west".to_string(),
    }).await?;
    
    // Emit client event
    event_bus_mut.emit_client("chat", "message", &RawClientMessageEvent {
        player_id: 12345,
        namespace: "chat".to_string(),
        event_name: "message".to_string(),
        data: b"Hello, world!".to_vec(),
        timestamp: current_timestamp(),
    }).await?;
    
    // Emit GORC event
    event_bus_mut.emit_gorc("Asteroid", 0, "position_update", &GorcEvent {
        object_id: "asteroid_001".to_string(),
        object_type: "Asteroid".to_string(),
        channel: 0,
        data: b"position_data".to_vec(),
        priority: "high".to_string(),
        timestamp: current_timestamp(),
    }).await?;
    
    // Wait a moment for events to process
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Show statistics
    let stats = event_bus.stats().await;
    println!("📊 Event Stats: {} emitted, {} handled, {} failed", 
             stats.events_emitted, stats.events_handled, stats.handler_failures);
    
    // Shutdown
    manager.shutdown().await?;
    
    println!("🏁 Demo completed successfully!");
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_horizon_equivalent_system() -> Result<(), Box<dyn std::error::Error>> {
        // Test that the system can be created and used exactly like Horizon's
        let propagator = HorizonPropagator::new();
        let mut event_bus = EventBus::with_propagator(propagator);
        
        // Test core event registration and emission
        event_bus.on_core::<ServerStartedEvent, _>("server_started", |event| {
            assert!(!event.region_id.is_empty());
            Ok(())
        }).await?;
        
        event_bus.emit_core("server_started", &ServerStartedEvent {
            timestamp: current_timestamp(),
            region_id: "test_region".to_string(),
        }).await?;
        
        // Test client event registration and emission
        event_bus.on_client::<RawClientMessageEvent, _>("chat", "message", |event| {
            assert!(event.player_id > 0);
            Ok(())
        }).await?;
        
        event_bus.emit_client("chat", "message", &RawClientMessageEvent {
            player_id: 123,
            namespace: "chat".to_string(),
            event_name: "message".to_string(),
            data: b"test".to_vec(),
            timestamp: current_timestamp(),
        }).await?;
        
        // Test GORC event registration and emission
        event_bus.on_gorc::<GorcEvent, _>("Player", 0, "move", |event| {
            assert_eq!(event.object_type, "Player");
            assert_eq!(event.channel, 0);
            Ok(())
        }).await?;
        
        event_bus.emit_gorc("Player", 0, "move", &GorcEvent {
            object_id: "player_001".to_string(),
            object_type: "Player".to_string(),
            channel: 0,
            data: b"move_data".to_vec(),
            priority: "high".to_string(),
            timestamp: current_timestamp(),
        }).await?;
        
        // Give events time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        let stats = event_bus.stats().await;
        assert!(stats.events_emitted >= 3);
        
        Ok(())
    }
}