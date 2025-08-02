//! Example demonstrating how to use the universal plugin system to generate
//! domain-specific APIs like Horizon's `on_core_async` and `emit_core` methods.
//!
//! This shows how the universal system "facilitates the creation" of such methods
//! with minimal code, allowing existing plugins to work unchanged.

use universal_plugin_system::{
    event::{Event, EventBus, StructuredEventKey},
    propagation::AllEqPropagator,
    EventError,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Example event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerTickEvent {
    pub tick_count: u64,
    pub timestamp: u64,
}

impl Event for ServerTickEvent {
    fn event_type() -> &'static str {
        "ServerTickEvent"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerConnectedEvent {
    pub player_id: String,
    pub remote_addr: String,
}

impl Event for PlayerConnectedEvent {
    fn event_type() -> &'static str {
        "PlayerConnectedEvent"
    }
}

/// This is how Horizon can create its familiar API using the universal system
/// with minimal code. This wrapper provides all the `on_core_async`, `emit_core`, etc. methods.
pub struct HorizonEventSystem {
    event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
}

impl HorizonEventSystem {
    /// Create a new Horizon event system
    pub fn new() -> Self {
        Self {
            event_bus: Arc::new(EventBus::with_propagator(AllEqPropagator::new())),
        }
    }

    /// Get the underlying event bus for direct access if needed
    pub fn event_bus(&self) -> &Arc<EventBus<StructuredEventKey, AllEqPropagator>> {
        &self.event_bus
    }
}

// Use the universal system's macros to generate all the familiar Horizon APIs!
// This is what makes it easy to restore the original plugin code with no changes.

// Generate core domain methods: on_core, on_core_async, emit_core
universal_plugin_system::impl_domain_methods!(HorizonEventSystem, core);

// Generate client domain methods: on_client, on_client_async, emit_client  
universal_plugin_system::impl_domain_methods!(HorizonEventSystem, client);

// Generate plugin domain methods: on_plugin, emit_plugin
universal_plugin_system::impl_domain_methods!(HorizonEventSystem, plugin);

// Generate GORC domain methods: on_gorc, emit_gorc
universal_plugin_system::impl_domain_methods!(HorizonEventSystem, gorc);

// You can also add custom methods specific to Horizon
impl HorizonEventSystem {
    /// Custom Horizon-specific method for connection-aware handlers
    pub async fn on_client_with_connection<T, F>(
        &self,
        namespace: &str,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T, String) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        // Create a wrapper that extracts connection info and calls the connection-aware handler
        let conn_aware_wrapper = move |event: T| -> Result<(), EventError> {
            // For this example, we'll use a placeholder connection ID
            let connection_id = "conn_123".to_string();
            handler(event, connection_id)
        };
        
        self.event_bus.on_categorized("client", namespace, event_name, conn_aware_wrapper).await
    }

    /// Custom method for GORC instance events
    pub async fn on_gorc_instance<T, F>(
        &self,
        object_type: &str,
        channel: u8,
        event_name: &str,
        handler: F,
    ) -> Result<(), EventError>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        // Use the GORC class system for instance-specific events
        self.event_bus.on_gorc_class("gorc_instance", object_type, channel, event_name, false, handler).await
    }
}

/// Demonstrate how to use the generated API
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎉 Demonstrating Universal Plugin System Domain API Generation");
    
    // Create the Horizon event system using the universal system underneath
    let horizon_events = HorizonEventSystem::new();
    
    println!("✅ Created HorizonEventSystem with familiar API methods");
    
    // Now we can use ALL the familiar Horizon methods!
    // This is exactly what existing plugins expect to see:
    
    // ========================================================================
    // CORE EVENTS - just like the original Horizon API!
    // ========================================================================
    
    println!("📝 Registering core event handlers...");
    
    // on_core_async - just like the original!
    horizon_events.on_core_async("server_tick", |event: ServerTickEvent| {
        println!("🔄 Server tick #{} at {}", event.tick_count, event.timestamp);
        Ok(())
    }).await?;
    
    // on_core - synchronous version
    horizon_events.on_core("player_connected", |event: PlayerConnectedEvent| {
        println!("👤 Player {} connected from {}", event.player_id, event.remote_addr);
        Ok(())
    }).await?;
    
    println!("✅ Core handlers registered!");
    
    // ========================================================================
    // CLIENT EVENTS - also just like the original!
    // ========================================================================
    
    println!("📝 Registering client event handlers...");
    
    // on_client - with namespace support
    horizon_events.on_client("chat", "send_message", |event: serde_json::Value| {
        println!("💬 Chat message: {:?}", event);
        Ok(())
    }).await?;
    
    // on_client_with_connection - Horizon-specific feature
    horizon_events.on_client_with_connection("inventory", "use_item", 
        |event: serde_json::Value, connection_id: String| {
            println!("🎒 Player {} used item: {:?}", connection_id, event);
            Ok(())
        }
    ).await?;
    
    println!("✅ Client handlers registered!");
    
    // ========================================================================
    // GORC EVENTS - spatial object replication
    // ========================================================================
    
    println!("📝 Registering GORC event handlers...");
    
    // on_gorc - for object types and channels
    horizon_events.on_gorc("Player", 0, "position_update", |event: serde_json::Value| {
        println!("🏃 Player position update: {:?}", event);
        Ok(())
    }).await?;
    
    // on_gorc_instance - for specific object instances
    horizon_events.on_gorc_instance("Asteroid", 1, "health_changed", |event: serde_json::Value| {
        println!("💥 Asteroid health changed: {:?}", event);
        Ok(())
    }).await?;
    
    println!("✅ GORC handlers registered!");
    
    // ========================================================================
    // EMIT EVENTS - just like the original API!
    // ========================================================================
    
    println!("📤 Emitting events using familiar API...");
    
    // emit_core - exactly what existing code expects!
    horizon_events.emit_core("server_tick", &ServerTickEvent {
        tick_count: 1,
        timestamp: 1700000000,
    }).await?;
    
    horizon_events.emit_core("player_connected", &PlayerConnectedEvent {
        player_id: "player_123".to_string(),
        remote_addr: "192.168.1.100:8080".to_string(),
    }).await?;
    
    // emit_client - with namespace
    horizon_events.emit_client("chat", "send_message", &serde_json::json!({
        "message": "Hello, world!",
        "sender": "player_123"
    })).await?;
    
    // emit_gorc - for object replication
    horizon_events.emit_gorc("Player", 0, "position_update", &serde_json::json!({
        "x": 100.0,
        "y": 200.0,
        "z": 0.0
    })).await?;
    
    println!("✅ All events emitted successfully!");
    
    // ========================================================================
    // STATISTICS - show that the universal system is working
    // ========================================================================
    
    let stats = horizon_events.event_bus().stats().await;
    println!("📊 Event System Statistics:");
    println!("   - Events emitted: {}", stats.events_emitted);
    println!("   - Events handled: {}", stats.events_handled);
    println!("   - Total handlers: {}", stats.total_handlers);
    println!("   - Handler failures: {}", stats.handler_failures);
    
    println!("\n🎉 SUCCESS! The universal plugin system successfully facilitates");
    println!("    creation of familiar domain-specific APIs like Horizon's!");
    println!("    Existing plugins can use on_core_async, emit_core, etc. unchanged!");
    
    Ok(())
}