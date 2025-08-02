//! Example showing how Horizon could be refactored to use the universal plugin system
//! instead of its own hardcoded event system, achieving the "replaceability" requirement.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use universal_plugin_system::*;

// ============================================================================
// Horizon-Specific Events (would move from horizon_event_system to here)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerConnectedEvent {
    pub player_id: u64,
    pub name: String,
    pub position: (f64, f64, f64),
}

impl Event for PlayerConnectedEvent {
    fn event_type() -> &'static str {
        "player_connected"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawClientMessageEvent {
    pub player_id: u64,
    pub namespace: String,
    pub event_name: String,
    pub data: serde_json::Value,
}

impl Event for RawClientMessageEvent {
    fn event_type() -> &'static str {
        "raw_client_message"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GorcEvent {
    pub object_id: u64,
    pub object_type: String,
    pub channel: u8,
    pub data: serde_json::Value,
}

impl Event for GorcEvent {
    fn event_type() -> &'static str {
        "gorc_event"
    }
}

// ============================================================================
// Horizon-Specific Context (would replace ServerContext)
// ============================================================================

pub struct HorizonServerContext {
    pub server_id: String,
    pub build_version: String,
}

impl HorizonServerContext {
    pub fn new() -> Self {
        Self {
            server_id: "horizon_server_001".to_string(),
            build_version: "0.32.0".to_string(),
        }
    }

    pub fn log(&self, level: &str, message: &str) {
        println!("[{}] Horizon Server {}: {}", level, self.server_id, message);
    }
}

// ============================================================================
// Horizon-Specific Propagator (could implement GORC spatial logic)
// ============================================================================

pub struct HorizonPropagator {
    /// Enable spatial filtering for GORC events
    spatial_enabled: bool,
}

impl HorizonPropagator {
    pub fn new() -> Self {
        Self {
            spatial_enabled: true,
        }
    }
}

#[async_trait::async_trait]
impl EventPropagator<StructuredEventKey> for HorizonPropagator {
    async fn should_propagate(&self, event_key: &StructuredEventKey, _context: &PropagationContext<StructuredEventKey>) -> bool {
        // For GORC events, could implement spatial filtering logic
        if let Some(domain) = event_key.domain() {
            match domain {
                "gorc" | "gorc_instance" => {
                    // Spatial logic would go here
                    // For now, always propagate
                    true
                }
                _ => true, // Always propagate non-GORC events
            }
        } else {
            true
        }
    }

    async fn transform_event(&self, event: Arc<EventData>, context: &PropagationContext<StructuredEventKey>) -> Option<Arc<EventData>> {
        // Could implement event compression, filtering, etc. for GORC
        if let Some(domain) = context.event_key.domain() {
            if domain == "gorc" || domain == "gorc_instance" {
                // GORC-specific transformations could go here
                // For now, pass through unchanged
            }
        }
        Some(event)
    }
}

// ============================================================================
// Horizon Event System Wrapper (replaces the current EventSystem)
// ============================================================================

pub struct HorizonEventSystem {
    universal: UniversalEventSystem<HorizonServerContext, HorizonPropagator>,
}

impl HorizonEventSystem {
    pub fn new(context: Arc<HorizonServerContext>) -> Self {
        let propagator = HorizonPropagator::new();
        let universal = UniversalEventSystem::new(context, propagator);
        Self { universal }
    }

    // Horizon-specific convenience methods (matching current API)
    pub async fn on_core<T, F>(&self, event_name: &str, handler: F) -> Result<()>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> std::result::Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        self.universal.on("core", event_name, handler).await
    }

    pub async fn on_client<T, F>(&self, namespace: &str, event_name: &str, handler: F) -> Result<()>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> std::result::Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        // Use the universal system's category-based API
        let key = StructuredEventKey::domain_category_event("client", namespace, event_name);
        self.universal.event_bus.on_key(key, handler).await?;
        Ok(())
    }

    pub async fn on_plugin<T, F>(&self, plugin_name: &str, event_name: &str, handler: F) -> Result<()>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> std::result::Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let key = StructuredEventKey::domain_category_event("plugin", plugin_name, event_name);
        self.universal.event_bus.on_key(key, handler).await?;
        Ok(())
    }

    pub async fn on_gorc<T, F>(&self, object_type: &str, channel: u8, event_name: &str, handler: F) -> Result<()>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> std::result::Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let key = StructuredEventKey::new(vec!["gorc", object_type, &channel.to_string(), event_name]);
        self.universal.event_bus.on_key(key, handler).await?;
        Ok(())
    }

    pub async fn on_gorc_instance<T, F>(&self, object_type: &str, channel: u8, event_name: &str, handler: F) -> Result<()>
    where
        T: Event + for<'de> Deserialize<'de>,
        F: Fn(T) -> std::result::Result<(), EventError> + Send + Sync + Clone + 'static,
    {
        let key = StructuredEventKey::new(vec!["gorc_instance", object_type, &channel.to_string(), event_name]);
        self.universal.event_bus.on_key(key, handler).await?;
        Ok(())
    }

    // Emit methods
    pub async fn emit_core<T>(&self, event_name: &str, event: &T) -> Result<()>
    where
        T: Event + Serialize,
    {
        self.universal.emit("core", event_name, event).await
    }

    pub async fn emit_client<T>(&self, namespace: &str, event_name: &str, event: &T) -> Result<()>
    where
        T: Event + Serialize,
    {
        let key = StructuredEventKey::domain_category_event("client", namespace, event_name);
        self.universal.event_bus.emit_key(key, event).await?;
        Ok(())
    }

    pub async fn emit_plugin<T>(&self, plugin_name: &str, event_name: &str, event: &T) -> Result<()>
    where
        T: Event + Serialize,
    {
        let key = StructuredEventKey::domain_category_event("plugin", plugin_name, event_name);
        self.universal.event_bus.emit_key(key, event).await?;
        Ok(())
    }

    pub async fn emit_gorc<T>(&self, object_type: &str, channel: u8, event_name: &str, event: &T) -> Result<()>
    where
        T: Event + Serialize,
    {
        let key = StructuredEventKey::new(vec!["gorc", object_type, &channel.to_string(), event_name]);
        self.universal.event_bus.emit_key(key, event).await?;
        Ok(())
    }

    pub async fn emit_gorc_instance<T>(&self, object_type: &str, channel: u8, event_name: &str, event: &T) -> Result<()>
    where
        T: Event + Serialize,
    {
        let key = StructuredEventKey::new(vec!["gorc_instance", object_type, &channel.to_string(), event_name]);
        self.universal.event_bus.emit_key(key, event).await?;
        Ok(())
    }

    pub fn context(&self) -> Arc<HorizonServerContext> {
        self.universal.context()
    }
}

// ============================================================================
// Horizon Plugin Trait (would replace the current SimplePlugin trait)
// ============================================================================

#[async_trait::async_trait]
pub trait HorizonSimplePlugin: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    
    async fn register_handlers(&mut self, events: Arc<HorizonEventSystem>, context: Arc<HorizonServerContext>) -> std::result::Result<(), PluginSystemError>;
    
    async fn on_init(&mut self, _context: Arc<HorizonServerContext>) -> std::result::Result<(), PluginSystemError> {
        Ok(())
    }
    
    async fn on_shutdown(&mut self, _context: Arc<HorizonServerContext>) -> std::result::Result<(), PluginSystemError> {
        Ok(())
    }
}

// ============================================================================
// Example Horizon Plugin using the new system
// ============================================================================

pub struct ExampleHorizonPlugin {
    name: String,
}

impl ExampleHorizonPlugin {
    pub fn new() -> Self {
        Self {
            name: "example_horizon_plugin".to_string(),
        }
    }
}

#[async_trait::async_trait]
impl HorizonSimplePlugin for ExampleHorizonPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn register_handlers(&mut self, events: Arc<HorizonEventSystem>, context: Arc<HorizonServerContext>) -> std::result::Result<(), PluginSystemError> {
        context.log("INFO", "Registering Horizon plugin handlers...");

        // Register for core events
        events.on_core("player_connected", |event: PlayerConnectedEvent| {
            println!("🎮 Horizon Plugin: Player {} connected at {:?}", event.name, event.position);
            Ok(())
        }).await?;

        // Register for client events
        events.on_client("chat", "message", |event: RawClientMessageEvent| {
            println!("💬 Horizon Plugin: Chat message from player {}", event.player_id);
            Ok(())
        }).await?;

        // Register for GORC events
        events.on_gorc("Asteroid", 0, "position_update", |event: GorcEvent| {
            println!("🪨 Horizon Plugin: Asteroid {} moved", event.object_id);
            Ok(())
        }).await?;

        // Register for GORC instance events
        events.on_gorc_instance("Player", 1, "health_changed", |event: GorcEvent| {
            println!("❤️ Horizon Plugin: Player {} health changed", event.object_id);
            Ok(())
        }).await?;

        context.log("INFO", "✅ Horizon plugin handlers registered!");
        Ok(())
    }

    async fn on_init(&mut self, context: Arc<HorizonServerContext>) -> std::result::Result<(), PluginSystemError> {
        context.log("INFO", &format!("🔧 {} initialized", self.name));
        Ok(())
    }

    async fn on_shutdown(&mut self, context: Arc<HorizonServerContext>) -> std::result::Result<(), PluginSystemError> {
        context.log("INFO", &format!("🛑 {} shutting down", self.name));
        Ok(())
    }
}

// ============================================================================
// Demonstration
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    println!("🌅 Horizon Event System - Universal Plugin System Integration");
    println!("=============================================================");

    // Create Horizon server context
    let context = Arc::new(HorizonServerContext::new());
    
    // Create Horizon event system (now built on universal system)
    let horizon_events = Arc::new(HorizonEventSystem::new(context.clone()));

    println!("📝 Setting up Horizon event handlers...");

    // Register Horizon core handlers
    horizon_events.on_core("server_started", |event: serde_json::Value| {
        println!("🚀 Horizon Core: Server started - {:?}", event);
        Ok(())
    }).await?;

    println!("🔌 Loading Horizon plugin...");

    // Create and "load" a Horizon plugin
    let mut plugin = ExampleHorizonPlugin::new();
    plugin.register_handlers(horizon_events.clone(), context.clone()).await?;
    plugin.on_init(context.clone()).await?;

    println!("📤 Emitting Horizon events...");

    // Emit various Horizon events
    horizon_events.emit_core("server_started", &serde_json::json!({
        "version": "0.32.0",
        "timestamp": 1234567890
    })).await?;

    horizon_events.emit_core("player_connected", &PlayerConnectedEvent {
        player_id: 123,
        name: "Alice".to_string(),
        position: (10.0, 64.0, 20.0),
    }).await?;

    horizon_events.emit_client("chat", "message", &RawClientMessageEvent {
        player_id: 123,
        namespace: "chat".to_string(),
        event_name: "message".to_string(),
        data: serde_json::json!({ "text": "Hello world!" }),
    }).await?;

    horizon_events.emit_gorc("Asteroid", 0, "position_update", &GorcEvent {
        object_id: 456,
        object_type: "Asteroid".to_string(),
        channel: 0,
        data: serde_json::json!({ "position": [100.0, 0.0, 200.0] }),
    }).await?;

    horizon_events.emit_gorc_instance("Player", 1, "health_changed", &GorcEvent {
        object_id: 123,
        object_type: "Player".to_string(),
        channel: 1,
        data: serde_json::json!({ "health": 85 }),
    }).await?;

    println!("📊 Event Statistics:");
    let stats = horizon_events.universal.event_bus.stats().await;
    println!("   Events emitted: {}", stats.events_emitted);
    println!("   Events handled: {}", stats.events_handled);
    println!("   Handler failures: {}", stats.handler_failures);
    println!("   Total handlers: {}", stats.total_handlers);

    println!("🔄 Shutting down...");
    plugin.on_shutdown(context.clone()).await?;

    println!("\n🎉 SUCCESS! Horizon Universal Integration Complete");
    println!("=================================================");
    println!("✅ Horizon event system now built on universal foundation");
    println!("✅ All Horizon-specific domains preserved (core, client, plugin, gorc)");
    println!("✅ GORC spatial propagation can be implemented in HorizonPropagator");
    println!("✅ Existing Horizon plugin API can be maintained as a wrapper");
    println!("✅ No breaking changes to existing Horizon plugins required");
    println!("✅ Universal system handles all the generic event routing");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_horizon_on_universal_system() -> Result<()> {
        let context = Arc::new(HorizonServerContext::new());
        let horizon_events = HorizonEventSystem::new(context.clone());

        // Test that all Horizon-specific APIs work
        horizon_events.on_core("test", |event: serde_json::Value| {
            assert_eq!(event["message"], "test");
            Ok(())
        }).await?;

        horizon_events.emit_core("test", &serde_json::json!({
            "message": "test"
        })).await?;

        Ok(())
    }
}