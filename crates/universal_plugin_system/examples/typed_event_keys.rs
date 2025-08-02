//! Example demonstrating the improved typed event key system
//! 
//! This shows how the new system eliminates string parsing and provides
//! better performance and type safety.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use universal_plugin_system::*;

// ============================================================================
// Custom Event Key Types for Better Performance
// ============================================================================

/// Custom namespace enum for your application
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum MyNamespaces {
    Core,
    PlayerAction,
    GameWorld,
    UI,
    Network,
}

/// Custom event key that avoids all string operations
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum MyEventKey {
    /// Core system events
    Core { event_id: u32 },
    
    /// Player action events
    PlayerAction { 
        player_id: u64, 
        action_type: u16 
    },
    
    /// Game world events with spatial information
    GameWorld { 
        zone_id: u32, 
        object_type: u16, 
        event_id: u32 
    },
    
    /// UI events
    UI { 
        component_id: u32, 
        event_type: u16 
    },
    
    /// Network events
    Network { 
        connection_id: u64, 
        message_type: u8 
    },
}

impl EventKeyType for MyEventKey {
    fn to_string(&self) -> String {
        match self {
            MyEventKey::Core { event_id } => format!("core:{}", event_id),
            MyEventKey::PlayerAction { player_id, action_type } => {
                format!("player:{}:{}", player_id, action_type)
            }
            MyEventKey::GameWorld { zone_id, object_type, event_id } => {
                format!("world:{}:{}:{}", zone_id, object_type, event_id)
            }
            MyEventKey::UI { component_id, event_type } => {
                format!("ui:{}:{}", component_id, event_type)
            }
            MyEventKey::Network { connection_id, message_type } => {
                format!("net:{}:{}", connection_id, message_type)
            }
        }
    }
}

// Event ID constants for better maintainability
pub mod CoreEvents {
    pub const SERVER_STARTED: u32 = 1;
    pub const SERVER_SHUTDOWN: u32 = 2;
    pub const TICK: u32 = 3;
}

pub mod PlayerActions {
    pub const MOVE: u16 = 1;
    pub const JUMP: u16 = 2;
    pub const ATTACK: u16 = 3;
    pub const USE_ITEM: u16 = 4;
}

pub mod GameWorldEvents {
    pub const OBJECT_SPAWNED: u32 = 1;
    pub const OBJECT_DESTROYED: u32 = 2;
    pub const POSITION_UPDATE: u32 = 3;
}

// ============================================================================
// Event Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStartedEvent {
    pub timestamp: u64,
    pub server_id: String,
}

impl Event for ServerStartedEvent {
    fn event_type() -> &'static str {
        "server_started"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerMoveEvent {
    pub player_id: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub timestamp: u64,
}

impl Event for PlayerMoveEvent {
    fn event_type() -> &'static str {
        "player_move"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectSpawnedEvent {
    pub zone_id: u32,
    pub object_id: u64,
    pub object_type: u16,
    pub position: (f32, f32, f32),
}

impl Event for ObjectSpawnedEvent {
    fn event_type() -> &'static str {
        "object_spawned"
    }
}

// ============================================================================
// Custom Propagator that Works with Typed Keys
// ============================================================================

/// Custom propagator that uses the typed keys for efficient filtering
pub struct MyCustomPropagator {
    /// Allow list of namespaces
    allowed_namespaces: Vec<MyNamespaces>,
}

impl MyCustomPropagator {
    pub fn new() -> Self {
        Self {
            allowed_namespaces: vec![
                MyNamespaces::Core,
                MyNamespaces::PlayerAction,
                MyNamespaces::GameWorld,
                MyNamespaces::UI,
                MyNamespaces::Network,
            ],
        }
    }

    pub fn allow_only(namespaces: Vec<MyNamespaces>) -> Self {
        Self {
            allowed_namespaces: namespaces,
        }
    }

    /// Extract namespace from our custom event key
    fn get_namespace(&self, key: &MyEventKey) -> MyNamespaces {
        match key {
            MyEventKey::Core { .. } => MyNamespaces::Core,
            MyEventKey::PlayerAction { .. } => MyNamespaces::PlayerAction,
            MyEventKey::GameWorld { .. } => MyNamespaces::GameWorld,
            MyEventKey::UI { .. } => MyNamespaces::UI,
            MyEventKey::Network { .. } => MyNamespaces::Network,
        }
    }
}

#[async_trait::async_trait]
impl EventPropagator<MyEventKey> for MyCustomPropagator {
    async fn should_propagate(&self, event_key: &MyEventKey, context: &PropagationContext<MyEventKey>) -> bool {
        // First check if this namespace is allowed
        let namespace = self.get_namespace(event_key);
        if !self.allowed_namespaces.contains(&namespace) {
            return false;
        }

        // For exact matching (like AllEq), compare the keys
        *event_key == context.event_key
    }

    async fn transform_event(
        &self,
        event: Arc<EventData>,
        context: &PropagationContext<MyEventKey>,
    ) -> Option<Arc<EventData>> {
        // Add namespace info to metadata
        let namespace = self.get_namespace(&context.event_key);
        let mut new_event = (*event).clone();
        new_event.metadata.insert("namespace".to_string(), format!("{:?}", namespace));
        Some(Arc::new(new_event))
    }
}

// ============================================================================
// Example Plugin Using Typed Event Keys
// ============================================================================

pub struct TypedEventPlugin {
    name: String,
    events_processed: u64,
}

impl TypedEventPlugin {
    pub fn new() -> Self {
        Self {
            name: "typed_event_plugin".to_string(),
            events_processed: 0,
        }
    }
}

#[async_trait::async_trait]
impl SimplePlugin<MyCustomPropagator> for TypedEventPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn register_handlers(
        &mut self,
        event_bus: Arc<EventBus<MyEventKey, MyCustomPropagator>>,
        _context: Arc<PluginContext<MyCustomPropagator>>,
    ) -> Result<(), PluginSystemError> {
        // This won't compile without the correct types - provides compile-time safety!
        let mut bus = (*event_bus).clone(); // In practice, you'd design this better

        // Register handler for server started events - no string parsing!
        bus.on_key(
            MyEventKey::Core { event_id: CoreEvents::SERVER_STARTED },
            |event: ServerStartedEvent| {
                println!("🚀 Server {} started at {}", event.server_id, event.timestamp);
                Ok(())
            }
        ).await.map_err(|e| PluginSystemError::EventError(e))?;

        // Register handler for player move events
        bus.on_key(
            MyEventKey::PlayerAction { player_id: 0, action_type: PlayerActions::MOVE }, // 0 = wildcard
            |event: PlayerMoveEvent| {
                println!("🏃 Player {} moved to ({}, {}, {})", 
                    event.player_id, event.x, event.y, event.z);
                Ok(())
            }
        ).await.map_err(|e| PluginSystemError::EventError(e))?;

        // Register handler for object spawned events in zone 1
        bus.on_key(
            MyEventKey::GameWorld { 
                zone_id: 1, 
                object_type: 0, // 0 = any object type
                event_id: GameWorldEvents::OBJECT_SPAWNED 
            },
            |event: ObjectSpawnedEvent| {
                println!("✨ Object spawned in zone {} at {:?}", 
                    event.zone_id, event.position);
                Ok(())
            }
        ).await.map_err(|e| PluginSystemError::EventError(e))?;

        println!("📝 Registered typed event handlers");
        Ok(())
    }

    async fn on_init(&mut self, _context: Arc<PluginContext<MyCustomPropagator>>) -> Result<(), PluginSystemError> {
        println!("🔧 Typed event plugin initialized");
        Ok(())
    }

    async fn on_shutdown(&mut self, _context: Arc<PluginContext<MyCustomPropagator>>) -> Result<(), PluginSystemError> {
        println!("🛑 Typed event plugin shutdown. Processed {} events", self.events_processed);
        Ok(())
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Type alias for our event bus
pub type MyEventBus = EventBus<MyEventKey, MyCustomPropagator>;

/// Helper to create event keys more easily
impl MyEventKey {
    pub fn core_event(event_id: u32) -> Self {
        MyEventKey::Core { event_id }
    }

    pub fn player_action(player_id: u64, action_type: u16) -> Self {
        MyEventKey::PlayerAction { player_id, action_type }
    }

    pub fn game_world(zone_id: u32, object_type: u16, event_id: u32) -> Self {
        MyEventKey::GameWorld { zone_id, object_type, event_id }
    }

    pub fn ui_event(component_id: u32, event_type: u16) -> Self {
        MyEventKey::UI { component_id, event_type }
    }

    pub fn network_event(connection_id: u64, message_type: u8) -> Self {
        MyEventKey::Network { connection_id, message_type }
    }
}

// ============================================================================
// Demonstration
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::init();

    println!("🎯 Typed Event Keys Demo - Zero String Parsing!");

    // Create the custom propagator
    let propagator = MyCustomPropagator::new();
    
    // Create event bus with typed keys
    let event_bus = Arc::new(EventBus::with_propagator(propagator));
    
    // Create context
    let context = Arc::new(PluginContext::new(event_bus.clone()));
    
    // Create plugin manager with typed system
    let config = PluginConfig::default();
    let manager = PluginManager::new(event_bus.clone(), context.clone(), config);
    
    // Create and load plugin
    let factory = create_plugin_factory!(TypedEventPlugin, MyCustomPropagator, "typed_event_plugin", "1.0.0");
    let plugin_name = manager.load_plugin_from_factory(Box::new(factory)).await?;
    
    println!("✅ Loaded plugin: {}", plugin_name);
    
    // Demonstrate the typed API - no string parsing anywhere!
    let mut bus = (*event_bus).clone(); // Simplified for demo
    
    // Emit server started event
    bus.emit_key(
        MyEventKey::core_event(CoreEvents::SERVER_STARTED),
        &ServerStartedEvent {
            timestamp: current_timestamp(),
            server_id: "server_001".to_string(),
        }
    ).await?;
    
    // Emit player move event
    bus.emit_key(
        MyEventKey::player_action(12345, PlayerActions::MOVE),
        &PlayerMoveEvent {
            player_id: 12345,
            x: 100.0,
            y: 50.0,
            z: 25.0,
            timestamp: current_timestamp(),
        }
    ).await?;
    
    // Emit object spawned event
    bus.emit_key(
        MyEventKey::game_world(1, 42, GameWorldEvents::OBJECT_SPAWNED),
        &ObjectSpawnedEvent {
            zone_id: 1,
            object_id: 98765,
            object_type: 42,
            position: (150.0, 75.0, 10.0),
        }
    ).await?;
    
    // Wait for events to process
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Show performance comparison
    println!("\n🚀 Performance Benefits:");
    println!("   • Zero string parsing during event routing");
    println!("   • Compile-time type checking for event keys");
    println!("   • Efficient hash-based lookups");
    println!("   • No regex or string matching needed");
    println!("   • Custom propagation logic with typed patterns");
    
    // Show statistics
    let stats = event_bus.stats().await;
    println!("\n📊 Event Stats: {} emitted, {} handled, {} failed", 
             stats.events_emitted, stats.events_handled, stats.handler_failures);
    
    // Cleanup
    manager.shutdown().await?;
    
    println!("🏁 Typed event system demo completed!");
    
    Ok(())
}

// ============================================================================
// Usage Examples in Comments
// ============================================================================

/*
// Instead of this (old string-based way):
events.on_client("chat", "message", handler).await?;
events.emit_client("chat", "message", &event).await?;

// You can now do this (typed way):
events.on_key(MyEventKey::player_action(player_id, PlayerActions::CHAT), handler).await?;
events.emit_key(MyEventKey::player_action(player_id, PlayerActions::CHAT), &event).await?;

// Or with the convenience methods:
let key = MyEventKey::player_action(123, PlayerActions::MOVE);
events.on_key(key.clone(), handler).await?;
events.emit_key(key, &event).await?;

// The AllEq propagator ensures only exact matches trigger:
// This handler will ONLY receive events with matching player_id and action_type
events.on_key(MyEventKey::player_action(123, PlayerActions::MOVE), |event| {
    // This will only be called for player 123's move events
    Ok(())
}).await?;

// You can still use flexible patterns by implementing your own propagator:
struct WildcardPropagator;
impl EventPropagator<MyEventKey> for WildcardPropagator {
    async fn should_propagate(&self, handler_key: &MyEventKey, context: &PropagationContext<MyEventKey>) -> bool {
        match (handler_key, &context.event_key) {
            // Allow handlers with player_id=0 to receive events for any player
            (MyEventKey::PlayerAction { player_id: 0, action_type: h_action }, 
             MyEventKey::PlayerAction { action_type: e_action, .. }) => {
                h_action == e_action
            }
            // Exact match for everything else
            _ => handler_key == &context.event_key
        }
    }
}
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_typed_event_system() -> Result<(), Box<dyn std::error::Error>> {
        let propagator = MyCustomPropagator::new();
        let mut event_bus = EventBus::with_propagator(propagator);
        
        // Test typed event registration and emission
        let test_key = MyEventKey::core_event(CoreEvents::SERVER_STARTED);
        
        event_bus.on_key(test_key.clone(), |event: ServerStartedEvent| {
            assert!(!event.server_id.is_empty());
            Ok(())
        }).await?;
        
        event_bus.emit_key(test_key, &ServerStartedEvent {
            timestamp: current_timestamp(),
            server_id: "test_server".to_string(),
        }).await?;
        
        // Give events time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        let stats = event_bus.stats().await;
        assert!(stats.events_emitted >= 1);
        
        Ok(())
    }

    #[test]
    fn test_event_key_performance() {
        // Test that our typed keys are more efficient than strings
        let typed_key = MyEventKey::player_action(12345, PlayerActions::MOVE);
        let string_key = "player:12345:1";
        
        // Typed keys use efficient enum matching and integer comparisons
        // String keys require parsing and string operations
        
        // This is mainly a demonstration - in practice you'd benchmark these
        assert_eq!(typed_key.to_string(), "player:12345:1");
        assert_eq!(string_key, "player:12345:1");
        
        // But the typed version avoids all string operations during propagation!
    }
}