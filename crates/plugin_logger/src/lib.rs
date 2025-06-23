use async_trait::async_trait;
use horizon_event_system::{
    create_simple_plugin, current_timestamp, on_event, register_handlers, EventSystem, LogLevel,
    PlayerId, PluginError, Position, ServerContext, SimplePlugin,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use std::sync::Arc;

// Define PlayerChatEvent and PlayerJumpEvent for simulation/demo purposes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerChatEvent {
    pub player_id: PlayerId,
    pub message: String,
    pub channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerJumpEvent {
    pub player_id: PlayerId,
    pub height: f32,
    pub position: Position,
}

/// A simple logger plugin that tracks and logs various server activities
pub struct LoggerPlugin {
    name: String,
    events_logged: u32,
    start_time: std::time::SystemTime,
}

impl LoggerPlugin {
    pub fn new() -> Self {
        info!("📝 LoggerPlugin: Creating new instance");
        Self {
            name: "logger".to_string(),
            events_logged: 0,
            start_time: std::time::SystemTime::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLogEvent {
    pub activity_type: String,
    pub details: String,
    pub player_id: Option<PlayerId>,
    pub timestamp: u64,
    pub log_count: u32,
}

#[async_trait]
impl SimplePlugin for LoggerPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
        info!("📝 LoggerPlugin: Registering comprehensive event logging...");

        // Use individual registrations to show different API styles

        events
            .on_core("player_connected", |event: serde_json::Value| {
                info!(
                    "📝 LoggerPlugin: 🟢 CONNECTION - Player joined server: {:?}",
                    event
                );
                Ok(())
            })
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        events
            .on_core("player_disconnected", |event: serde_json::Value| {
                info!(
                    "📝 LoggerPlugin: 🔴 DISCONNECTION - Player left server: {:?}",
                    event
                );
                Ok(())
            })
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        events
            .on_core("plugin_loaded", |event: serde_json::Value| {
                info!("📝 LoggerPlugin: 🔌 PLUGIN LOADED - {:?}", event);
                Ok(())
            })
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Client events from players
        events
            .on_client("chat", "message", |event: PlayerChatEvent| {
                info!(
                    "📝 LoggerPlugin: 💬 CHAT - Player {} in {}: '{}'",
                    event.player_id, event.channel, event.message
                );
                Ok(())
            })
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        events
            .on_client("movement", "jump", |event: PlayerJumpEvent| {
                info!(
                    "📝 LoggerPlugin: 🦘 MOVEMENT - Player {} jumped {:.1}m at {:?}",
                    event.player_id, event.height, event.position
                );
                Ok(())
            })
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Inter-plugin communication
        events
            .on_plugin("mygreeter", "startup", |event: serde_json::Value| {
                info!(
                    "📝 LoggerPlugin: 🤝 PLUGIN EVENT - Greeter started: {:?}",
                    event
                );
                Ok(())
            })
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        events
            .on_plugin("greeter", "shutdown", |event: serde_json::Value| {
                info!(
                    "📝 LoggerPlugin: 🤝 PLUGIN EVENT - Greeter shutting down: {:?}",
                    event
                );
                Ok(())
            })
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Listen to any plugin events (wildcard-style)
        events
            .on_plugin("*", "activity", |event: serde_json::Value| {
                info!("📝 LoggerPlugin: 🌐 GENERAL ACTIVITY - {:?}", event);
                Ok(())
            })
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        info!("📝 LoggerPlugin: ✅ Event logging system activated!");
        Ok(())
    }

    async fn on_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            "📝 LoggerPlugin: Comprehensive event logging activated!",
        );

        // Announce our logging service to other plugins
        let events = context.events();
        events
            .emit_plugin(
                "logger",
                "service_started",
                &serde_json::json!({
                    "service": "event_logging",
                    "version": self.version(),
                    "start_time": current_timestamp(),
                    "message": "Logger plugin is now monitoring all events!"
                }),
            )
            .await
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        info!("📝 LoggerPlugin: ✅ Now monitoring all server events!");

        // TODO: TOKIO SUPPORT!!
        // // Start a periodic summary task
        // let events_clone = events.clone();
        // tokio::spawn(async move {
        //     let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        //     let mut summary_count = 0;

        //     loop {
        //         interval.tick().await;
        //         summary_count += 1;

        //         let _ = events_clone.emit_plugin("logger", "activity_logged", &serde_json::json!({
        //             "activity_type": "periodic_summary",
        //             "details": format!("Summary #{} - Logger still active", summary_count),
        //             "timestamp": current_timestamp()
        //         })).await;

        //         info!("📝 LoggerPlugin: 📊 Periodic Summary #{} - Still logging events...", summary_count);
        //     }
        // });

        Ok(())
    }

    async fn on_shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        let uptime = self.start_time.elapsed().unwrap_or_default();

        context.log(
            LogLevel::Info,
            &format!(
                "📝 LoggerPlugin: Shutting down. Logged {} events over {:.1} seconds",
                self.events_logged,
                uptime.as_secs_f64()
            ),
        );

        // Final log summary
        let events = context.events();
        events
            .emit_plugin(
                "logger",
                "final_summary",
                &serde_json::json!({
                    "total_events_logged": self.events_logged,
                    "uptime_seconds": uptime.as_secs(),
                    "events_per_second": self.events_logged as f64 / uptime.as_secs_f64().max(1.0),
                    "message": "Logger plugin final report",
                    "timestamp": current_timestamp()
                }),
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        info!("📝 LoggerPlugin: ✅ Final report submitted. Logging service offline.");
        Ok(())
    }
}

// Create the plugin using our macro - zero unsafe code!
create_simple_plugin!(LoggerPlugin);

// ============================================================================
// Demo Helper Functions
// ============================================================================

/// Helper function to simulate some player events for testing
pub async fn simulate_player_activity(events: Arc<EventSystem>) {
    info!("\n🎮 Starting player activity simulation...\n");

    let player_id = PlayerId::new();

    // Simulate player connection
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Simulate chat message
    events
        .emit_client(
            "chat",
            "message",
            &PlayerChatEvent {
                player_id,
                message: "Hello everyone!".to_string(),
                channel: "general".to_string(),
            },
        )
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Simulate jumping
    events
        .emit_client(
            "movement",
            "jump",
            &PlayerJumpEvent {
                player_id,
                height: 3.5,
                position: Position::new(100.0, 200.0, 0.0),
            },
        )
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;

    // Simulate high jump
    events
        .emit_client(
            "movement",
            "jump",
            &PlayerJumpEvent {
                player_id,
                height: 8.2,
                position: Position::new(105.0, 205.0, 0.0),
            },
        )
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(1200)).await;

    // Simulate another chat
    events
        .emit_client(
            "chat",
            "message",
            &PlayerChatEvent {
                player_id,
                message: "Wow, this server is awesome!".to_string(),
                channel: "general".to_string(),
            },
        )
        .await
        .unwrap();

    info!("\n🎮 Player activity simulation complete!\n");
}

// ============================================================================
// Integration Test and Demo
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_event_system::create_horizon_event_system;

    #[tokio::test]
    async fn test_plugin_communication() {
        info!("\n🧪 Testing inter-plugin communication...\n");

        let events = create_horizon_event_system();

        // Test that events can be emitted and received
        events
            .on_plugin("test", "message", |event: serde_json::Value| {
                info!("✅ Test: Received plugin event: {:?}", event);
                Ok(())
            })
            .await
            .unwrap();

        events
            .emit_plugin(
                "test",
                "message",
                &serde_json::json!({
                    "test": "data",
                    "timestamp": current_timestamp()
                }),
            )
            .await
            .unwrap();

        info!("✅ Plugin communication test passed!\n");
    }

    #[tokio::test]
    async fn test_horizon_event_system_integration() {
        info!("\n🧪 Testing complete event system integration...\n");

        let events = create_horizon_event_system();

        // Register handlers for all event types
        events
            .on_core("test_core", |event: serde_json::Value| {
                info!("✅ Core event received: {:?}", event);
                Ok(())
            })
            .await
            .unwrap();

        events
            .on_client("test", "client_event", |event: serde_json::Value| {
                info!("✅ Client event received: {:?}", event);
                Ok(())
            })
            .await
            .unwrap();

        events
            .on_plugin("test", "plugin_event", |event: serde_json::Value| {
                info!("✅ Plugin event received: {:?}", event);
                Ok(())
            })
            .await
            .unwrap();

        // Emit test events
        events
            .emit_core("test_core", &serde_json::json!({"test": "core"}))
            .await
            .unwrap();
        events
            .emit_client(
                "test",
                "client_event",
                &serde_json::json!({"test": "client"}),
            )
            .await
            .unwrap();
        events
            .emit_plugin(
                "test",
                "plugin_event",
                &serde_json::json!({"test": "plugin"}),
            )
            .await
            .unwrap();

        info!("✅ Complete integration test passed!\n");
    }
}
