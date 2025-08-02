use async_trait::async_trait;
use chrono::prelude::*;
use universal_plugin_system::{
    SimplePlugin, PluginContext, PluginSystemError, EventBus, StructuredEventKey,
    propagation::AllEqPropagator
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, error};

// ============================================================================
// Sample Plugin 1: Greeter Plugin
// ============================================================================

/// A simple greeter plugin that welcomes players and announces activities
pub struct GreeterPlugin {
    name: String,
    welcome_count: u32,
}

impl GreeterPlugin {
    pub fn new() -> Self {
        println!("🎉 GreeterPlugin: Creating new instance");
        Self {
            name: "greeter".to_string(),
            welcome_count: 0,
        }
    }
}

impl Default for GreeterPlugin {
    fn default() -> Self {
        Self::new()
    }
}

// Define some simple events for demonstration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WelcomeEvent {
    pub player_id: u64,
    pub welcome_message: String,
    pub welcome_count: u32,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerChatEvent {
    pub player_id: u64,
    pub message: String,
    pub channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerJumpEvent {
    pub player_id: u64,
    pub height: f64,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[async_trait]
impl SimplePlugin<StructuredEventKey, AllEqPropagator> for GreeterPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn register_handlers(
        &mut self,
        event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
        _context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>,
    ) -> Result<(), PluginSystemError> {
        println!("👋 GreeterPlugin: Registering event handlers...");

        // Register core events
        event_bus
            .on("core", "player_connected", |event: serde_json::Value| {
                println!("👋 GreeterPlugin: New player connected! {:?}", event);
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        event_bus
            .on("core", "player_disconnected", |event: serde_json::Value| {
                println!("👋 GreeterPlugin: Player disconnected. Farewell! {:?}", event);
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        // Register client events
        event_bus
            .on("chat", "message", |event: PlayerChatEvent| {
                println!("👋 GreeterPlugin: Player {} said: '{}' in {}",
                         event.player_id, event.message, event.channel);

                // Respond to greetings
                if event.message.to_lowercase().contains("hello") ||
                   event.message.to_lowercase().contains("hi") {
                    println!("👋 GreeterPlugin: Detected greeting! Preparing response...");
                }
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        event_bus
            .on("movement", "jump", |event: PlayerJumpEvent| {
                println!("👋 GreeterPlugin: Player {} jumped {:.1}m high! 🦘",
                         event.player_id, event.height);

                if event.height > 5.0 {
                    println!("👋 GreeterPlugin: Wow, that's a high jump!");
                }
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        // Register plugin events
        event_bus
            .on("plugin", "logger_activity", |event: serde_json::Value| {
                println!("👋 GreeterPlugin: Logger plugin recorded activity: {:?}", event);
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        println!("👋 GreeterPlugin: ✅ All handlers registered successfully!");
        Ok(())
    }

    async fn on_init(&mut self, context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>) -> Result<(), PluginSystemError> {
        info!("👋 GreeterPlugin: Starting up! Ready to welcome players!");

        // Announce our presence to other plugins
        let event_bus = context.event_bus();
        event_bus
            .emit("plugin", "greeter_startup", &serde_json::json!({
                "plugin": "greeter",
                "version": self.version(),
                "message": "Greeter plugin is now online!",
                "timestamp": current_timestamp()
            }))
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        println!("Sending messages to other plugins!");

        // Send various plugin events for demonstration
        event_bus
            .emit("plugin", "inventory_pickup", &serde_json::json!({
                "id": "701d617f-3e4f-41b4-b4c6-c1b53709fc63",
                "item_count": 5,
                "item_id": 42
            }))
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        event_bus
            .emit("plugin", "inventory_setup", &serde_json::json!({
                "slot_count": 8,
                "inventory_count": 2
            }))
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        // Guild communications
        let time = Utc::now();
        event_bus
            .emit("plugin", "guild_chat", &serde_json::json!({
                "id": "fc326f20-a5f8-43c4-85ff-d5be9a5bffd7",
                "name": "Example Guild Name",
                "time": time,
            }))
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        event_bus
            .emit("plugin", "guild_clan", &serde_json::json!({
                "clan_id": "8b81645b-fa02-47ff-80c3-fb3f76c36bf1",
                "clan_name": "Example clan",
                "player_count": 100_000,
            }))
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        // Housing plugin events
        println!("🏠 Sending housing events!");
        
        event_bus
            .emit("plugin", "housing_create", &serde_json::json!({
                "house_id": "a09989fc-9957-4389-935e-f70c182b3ee5",
                "owner_id": "79dc25a1-22f5-4531-bbce-9cb3400f005d",
                "house_name": "Greeter's Welcome Home",
                "dimensions": {
                    "x": 50,
                    "y": 50,
                    "z": 20
                },
                "location": {
                    "x": 100.5,
                    "y": 64.0,
                    "z": 200.3,
                    "world": "overworld"
                },
                "created_at": Utc::now(),
                "last_modified": Utc::now()
            }))
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        println!("👋 GreeterPlugin: ✅ Initialization complete!");
        Ok(())
    }

    async fn on_shutdown(&mut self, context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>) -> Result<(), PluginSystemError> {
        info!(
            "👋 GreeterPlugin: Shutting down. Welcomed {} players total!",
            self.welcome_count
        );

        // Say goodbye to other plugins
        let event_bus = context.event_bus();
        event_bus
            .emit("plugin", "greeter_shutdown", &serde_json::json!({
                "plugin": "greeter",
                "total_welcomes": self.welcome_count,
                "message": "Greeter plugin going offline. Goodbye!",
                "timestamp": current_timestamp()
            }))
            .await
            .map_err(|e| PluginSystemError::RuntimeError(e.to_string()))?;

        // Clean up housing data before shutdown
        event_bus
            .emit("plugin", "housing_delete", &serde_json::json!({
                "house_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
                "owner_id": "701d617f-3e4f-41b4-b4c6-c1b53709fc63"
            }))
            .await
            .map_err(|e| PluginSystemError::RuntimeError(e.to_string()))?;

        println!("👋 GreeterPlugin: ✅ Shutdown complete!");
        Ok(())
    }
}

// Export functions for the universal plugin system
#[no_mangle]
pub extern "C" fn get_plugin_version() -> *const std::os::raw::c_char {
    static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), ":", env!("RUSTC_VERSION"));
    VERSION.as_ptr() as *const std::os::raw::c_char
}

#[no_mangle]
pub extern "C" fn create_plugin() -> *mut dyn universal_plugin_system::Plugin<StructuredEventKey, AllEqPropagator> {
    let plugin = GreeterPlugin::new();
    let wrapper = universal_plugin_system::PluginWrapper::new(plugin);
    Box::into_raw(Box::new(wrapper))
}
