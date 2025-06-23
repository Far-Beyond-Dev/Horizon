use async_trait::async_trait;
use horizon_event_system::{
    create_simple_plugin, current_timestamp, on_event, register_handlers, EventSystem, LogLevel,
    PlayerId, PluginError, Position, ServerContext, SimplePlugin,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};

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
        info!("🎉 GreeterPlugin: Creating new instance");
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
        info!("👋 GreeterPlugin: Registering event handlers...");

        // Register core events
        register_handlers!(events; core {
            "player_connected" => |event: serde_json::Value| {
                info!("👋 GreeterPlugin: New player connected! {:?}", event);
                Ok(())
            },

            "player_disconnected" => |event: serde_json::Value| {
                info!("👋 GreeterPlugin: Player disconnected. Farewell! {:?}", event);
                Ok(())
            }
        })?;

        // Register client events
        register_handlers!(events; client {
            "chat", "message" => |event: PlayerChatEvent| {
                info!("👋 GreeterPlugin: Player {} said: '{}' in {}",
                         event.player_id, event.message, event.channel);

                // Respond to greetings
                if event.message.to_lowercase().contains("hello") ||
                   event.message.to_lowercase().contains("hi") {
                    info!("👋 GreeterPlugin: Detected greeting! Preparing response...");
                }
                Ok(())
            },

            "movement", "jump" => |event: PlayerJumpEvent| {
                info!("👋 GreeterPlugin: Player {} jumped {:.1}m high! 🦘",
                         event.player_id, event.height);

                if event.height > 5.0 {
                    info!("👋 GreeterPlugin: Wow, that's a high jump!");
                }
                Ok(())
            }
        })?;

        // Register plugin events
        register_handlers!(events; plugin {
            "logger", "activity_logged" => |event: serde_json::Value| {
                info!("👋 GreeterPlugin: Logger plugin recorded activity: {:?}", event);
                Ok(())
            }
        })?;

        info!("👋 GreeterPlugin: ✅ All handlers registered successfully!");
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

        info!("Sending inventory a message!");

        events
            .emit_plugin(
                "InventorySystem",
                "PickupItem",
                &serde_json::json!({
                    "id": "701d617f-3e4f-41b4-b4c6-c1b53709fc63",
                    "item_count": 5,
                    "item_id": 1
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        info!("Setting up inventory!");

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

        info!("👋 GreeterPlugin: ✅ Initialization complete!");
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

        info!("👋 GreeterPlugin: ✅ Shutdown complete!");
        Ok(())
    }
}

// Create the plugin using our macro - zero unsafe code!
create_simple_plugin!(GreeterPlugin);
