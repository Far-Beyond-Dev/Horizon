use async_trait::async_trait;
use chrono::prelude::*;
use event_system::{
    create_simple_plugin, current_timestamp, on_event, register_handlers, EventSystem, LogLevel,
    PlayerId, PluginError, Position, ServerContext, SimplePlugin,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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

// Define some simple events for demonstration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WelcomeEvent {
    pub player_id: PlayerId,
    pub welcome_message: String,
    pub welcome_count: u32,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerChatEvent {
    pub player_id: PlayerId,
    pub message: String,
    pub channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerJumpEvent {
    pub player_id: PlayerId,
    pub height: f64,
    pub position: Position,
}

#[async_trait]
impl SimplePlugin for GreeterPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
        println!("👋 GreeterPlugin: Registering event handlers...");

        // Register core events
        register_handlers!(events; core {
            "player_connected" => |event: serde_json::Value| {
                println!("👋 GreeterPlugin: New player connected! {:?}", event);
                Ok(())
            },

            "player_disconnected" => |event: serde_json::Value| {
                println!("👋 GreeterPlugin: Player disconnected. Farewell! {:?}", event);
                Ok(())
            }
        })?;

        // Register client events
        register_handlers!(events; client {
            "chat", "message" => |event: PlayerChatEvent| {
                println!("👋 GreeterPlugin: Player {} said: '{}' in {}",
                         event.player_id, event.message, event.channel);

                // Respond to greetings
                if event.message.to_lowercase().contains("hello") ||
                   event.message.to_lowercase().contains("hi") {
                    println!("👋 GreeterPlugin: Detected greeting! Preparing response...");
                }
                Ok(())
            },

            "movement", "jump" => |event: PlayerJumpEvent| {
                println!("👋 GreeterPlugin: Player {} jumped {:.1}m high! 🦘",
                         event.player_id, event.height);

                if event.height > 5.0 {
                    println!("👋 GreeterPlugin: Wow, that's a high jump!");
                }
                Ok(())
            }
        })?;

        // Register plugin events
        register_handlers!(events; plugin {
            "logger", "activity_logged" => |event: serde_json::Value| {
                println!("👋 GreeterPlugin: Logger plugin recorded activity: {:?}", event);
                Ok(())
            }
        })?;

        println!("👋 GreeterPlugin: ✅ All handlers registered successfully!");
        Ok(())
    }

    async fn on_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            "👋 GreeterPlugin: Starting up! Ready to welcome players!",
        );

        // Announce our presence to other plugins
        let events = context.events();
        events
            .emit_plugin(
                "mygreeter",
                "startup",
                &serde_json::json!({
                    "plugin": "greeter",
                    "version": self.version(),
                    "message": "Greeter plugin is now online!",
                    "timestamp": current_timestamp()
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        println!("Sending inventory a message!");

        events
            .emit_plugin(
                "InventorySystem",
                "PickupItem",
                &serde_json::json!({
                    "id": "701d617f-3e4f-41b4-b4c6-c1b53709fc63",
                    "item_count": 5,
                    "item_id": 42
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        println!("Setting up inventory!");

        events
            .emit_plugin(
                "InventorySystem",
                "SetupInventory",
                &serde_json::json!({
                    "slot_count": 8,
                    "inventory_count": 2
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        {
            let time = Utc::now();

            events
                .emit_plugin(
                    "GuildComms",
                    "Chat",
                    &serde_json::json!({
                        "id": "fc326f20-a5f8-43c4-85ff-d5be9a5bffd7",
                        "name": "Example Guild Name",
                        "time": time,
                    }),
                )
                .await
                .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;
        }

        events
            .emit_plugin(
                "GuildComms",
                "Clan",
                &serde_json::json!({
                    "clan_id": "8b81645b-fa02-47ff-80c3-fb3f76c36bf1",
                    "clan_name": "Example clan",
                    "player_count": 100_000,
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        println!("👋 GreeterPlugin: ✅ Initialization complete!");
        Ok(())
    }

    async fn on_shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            &format!(
                "👋 GreeterPlugin: Shutting down. Welcomed {} players total!",
                self.welcome_count
            ),
        );

        // Say goodbye to other plugins
        let events = context.events();
        events
            .emit_plugin(
                "greeter",
                "shutdown",
                &serde_json::json!({
                    "plugin": "greeter",
                    "total_welcomes": self.welcome_count,
                    "message": "Greeter plugin going offline. Goodbye!",
                    "timestamp": current_timestamp()
                }),
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        println!("👋 GreeterPlugin: ✅ Shutdown complete!");
        Ok(())
    }
}

// Create the plugin using our macro - zero unsafe code!
create_simple_plugin!(GreeterPlugin);
