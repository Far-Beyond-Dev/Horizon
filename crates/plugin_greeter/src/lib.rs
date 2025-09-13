use async_trait::async_trait;
use chrono::prelude::*;
use horizon_event_system::{
    create_simple_plugin, current_timestamp, register_handlers, EventSystem, LogLevel,
    PlayerId, PluginError, Position, ServerContext, SimplePlugin,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, debug};

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

impl Default for GreeterPlugin {
    fn default() -> Self {
        Self::new()
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

    async fn register_handlers(&mut self, events: Arc<EventSystem>, _context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
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
            "chat", "message" => |event: PlayerChatEvent, _player_id: horizon_event_system::PlayerId, _connection: horizon_event_system::ClientConnectionRef| {
                debug!("👋 GreeterPlugin: Player {} said: '{}' in {}",
                         event.player_id, event.message, event.channel);

                // Respond to greetings
                if event.message.to_lowercase().contains("hello") ||
                   event.message.to_lowercase().contains("hi") {
                    info!("👋 GreeterPlugin: Detected greeting! Preparing response...");
                }
                Ok(())
            },

            "movement", "jump" => |event: PlayerJumpEvent, _player_id: horizon_event_system::PlayerId, _connection: horizon_event_system::ClientConnectionRef| {
                debug!("👋 GreeterPlugin: Player {} jumped {:.1}m high! 🦘",
                         event.player_id, event.height);

                if event.height > 5.0 {
                    info!("👋 GreeterPlugin: Wow, that's a high jump!");
                }
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
                    "item_id": 42
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

        events
            .emit_plugin(
                "GuildComms",
                "Role",
                &serde_json::json!({
                    "permission": 1,
                    "role_name": "Member",
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        events
            .emit_plugin(
                "GuildComms",
                "Channel",
                &serde_json::json!({
                    "channel_name": "Memes",
                    "roles_with_access": [{
                        "permission": 1,
                        "role_name": "Member"
                    }],
                    "active_users_in_channel": 100_000,
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        // ============================================================================
        // Housing Plugin Events
        // ============================================================================

        info!("🏠 Sending housing events to Housing plugin!");

        // Create a new house
        events
            .emit_plugin(
                "Housing",
                "CreateHouse",
                &serde_json::json!({
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
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        // Add a room to the house
        events
            .emit_plugin(
                "Housing",
                "AddRoom",
                &serde_json::json!({
                    "room_id": "3fdf159b-2463-42b9-b44a-585239284e3f",
                    "room_name": "Welcome Living Room",
                    "dimensions": {
                        "x": 15,
                        "y": 15,
                        "z": 10
                    },
                    "room_type": "LivingRoom"
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        // Add another room
        events
            .emit_plugin(
                "Housing",
                "AddRoom",
                &serde_json::json!({
                    "room_id": "a5cf2191-bed4-447f-b82c-f63f99666e54",
                    "room_name": "Hospitality Kitchen",
                    "dimensions": {
                        "x": 12,
                        "y": 10,
                        "z": 8
                    },
                    "room_type": "Kitchen"
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        // Update house information
        events
            .emit_plugin(
                "Housing",
                "UpdateHouse",
                &serde_json::json!({
                    "house_id": "5d466319-2a3e-4389-b33b-a801579db2a9",
                    "house_name": "Greeter's Updated Welcome Home",
                    "last_modified": Utc::now()
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        // Create a second house for demonstration
        events
            .emit_plugin(
                "Housing",
                "CreateHouse",
                &serde_json::json!({
                    "house_id": "1b1f76cf-0c8d-43be-9eb6-ba9fef3d5b71",
                    "owner_id": "ddc15a1d-3b26-43c9-ab3f-e51a433b91fd",
                    "house_name": "Guest House",
                    "dimensions": {
                        "x": 30,
                        "y": 30,
                        "z": 15
                    },
                    "location": {
                        "x": 150.0,
                        "y": 64.0,
                        "z": 250.0,
                        "world": "overworld"
                    },
                    "created_at": Utc::now(),
                    "last_modified": Utc::now()
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

        // Clean up housing data before shutdown
        events
            .emit_plugin(
                "Housing",
                "DeleteHouse",
                &serde_json::json!({
                    "house_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
                    "owner_id": "701d617f-3e4f-41b4-b4c6-c1b53709fc63"
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
