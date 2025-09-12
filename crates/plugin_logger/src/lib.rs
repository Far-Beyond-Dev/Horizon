use async_trait::async_trait;
use horizon_event_system::{
    create_simple_plugin, current_timestamp, ClientEventWrapper, EventSystem, LogLevel, PlayerId,
    PlayerMovementEvent, PluginError, Position, ServerContext, SimplePlugin,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// Define PlayerChatEvent and PlayerJumpEvent for simulation/demo purposes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerChatEvent {
    pub data: PlayerChatData,
    pub player_id: PlayerId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerChatData {
    pub channel: String,
    pub message: String,
    pub player_id: String,
    pub timestamp: String,
    pub uuid: String,
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
        Self {
            name: "logger".to_string(),
            events_logged: 0,
            start_time: std::time::SystemTime::now(),
        }
    }
}

impl Default for LoggerPlugin {
    fn default() -> Self {
        Self::new()
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

    async fn register_handlers(
        &mut self,
        events: Arc<EventSystem>,
        context: Arc<dyn ServerContext>,
    ) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            "📝 LoggerPlugin: Registering comprehensive event logging...",
        );

        // Use individual registrations to show different API styles

        let context_clone = context.clone();
        events
            .on_core(
                "player_connected",
                move |event: horizon_event_system::PlayerConnectedEvent| {
                    context_clone.log(
                        LogLevel::Info,
                        format!(
                            "📝 LoggerPlugin: 🟢 CONNECTION - Player {} joined from {}",
                            event.player_id, event.remote_addr
                        )
                        .as_str(),
                    );
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        let context_clone = context.clone();
        events
            .on_core(
                "player_disconnected",
                move |event: horizon_event_system::PlayerDisconnectedEvent| {
                    context_clone.log(
                        LogLevel::Info,
                        format!(
                        "📝 LoggerPlugin: 🔴 DISCONNECTION - Player {} left server (reason: {:?})",
                        event.player_id, event.reason
                    )
                        .as_str(),
                    );
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        let context_clone = context.clone();
        events
            .on_core(
                "plugin_loaded",
                move |event: horizon_event_system::PluginLoadedEvent| {
                    context_clone.log(
                        LogLevel::Info,
                        format!(
                            "📝 LoggerPlugin: 🔌 PLUGIN LOADED - {} v{} with capabilities: {:?}",
                            event.plugin_name, event.version, event.capabilities
                        )
                        .as_str(),
                    );
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Client events from players
        let context_clone = context.clone();
        events
            .on_client(
                "chat",
                "message",
                move |wrapper: ClientEventWrapper<PlayerChatEvent>, player_id: horizon_event_system::PlayerId, connection| {
                    context_clone.log(LogLevel::Info, format!("📝 LoggerPlugin: 💬 CHAT - Player {} in {}: '{}'", wrapper.data.data.player_id, wrapper.data.data.channel, wrapper.data.data.message).as_str());

                    let response = serde_json::json!({
                        "status": "ok",
                        "message": "Chat message logged successfully"
                    });

                    let context_for_async = context_clone.clone();
                    context_clone.luminal_handle().spawn(async move {
                        if let Err(e) = connection.respond_json(&response).await {
                            context_for_async.log(
                                LogLevel::Error,
                                &format!("📝 LoggerPlugin: Failed to send chat response: {}", e),
                            );
                        }
                    });
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Listen for client movement events and emit core events
        let context_clone = context.clone();
        let events_clone = events.clone();
        events
            .on_client(
                "movement",
                "update_position",
                move |wrapper: ClientEventWrapper<serde_json::Value>, player_id: horizon_event_system::PlayerId, _connection| {
                    context_clone.log(LogLevel::Info, format!("📝 LoggerPlugin: 🦘 Client movement from player {}", wrapper.player_id).as_str(),);

                    // Parse the movement data
                    #[derive(serde::Deserialize)]
                    struct PlayerMovementData {
                        position: ue_types::types::Transform,
                    }

                    match serde_json::from_value::<PlayerMovementData>(wrapper.data.clone()) {
                        Ok(movement_data) => {
                            // Convert Transform to Vec3 for core event
                            let new_position = horizon_event_system::Vec3 {
                                x: movement_data.position.location.x as f64,
                                y: movement_data.position.location.y as f64,
                                z: movement_data.position.location.z as f64,
                            };

                            // Create and emit core movement event for GORC and other systems
                            let core_movement_event = PlayerMovementEvent {
                                player_id: wrapper.player_id,
                                old_position: None,
                                new_position,
                                timestamp: current_timestamp(),
                            };

                            let events_system = events_clone.clone();
                            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                handle.spawn(async move {
                                    if let Err(_e) = events_system
                                        .emit_core("player_movement", &core_movement_event)
                                        .await
                                    {
                                        // Best effort - don't fail if core event emission fails
                                    }
                                });
                            }
                        }
                        Err(e) => {
                            context_clone.log(
                                LogLevel::Error,
                                format!("📝 LoggerPlugin: Failed to parse movement: {}", e)
                                    .as_str(),
                            );
                        }
                    }
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Inter-plugin communication
        let context_clone = context.clone();
        events
            .on_plugin("mygreeter", "startup", move |event: serde_json::Value| {
                context_clone.log(
                    LogLevel::Info,
                    format!(
                        "📝 LoggerPlugin: 🤝 PLUGIN EVENT - Greeter started: {:?}",
                        event
                    )
                    .as_str(),
                );
                Ok(())
            })
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        let context_clone = context.clone();
        events
            .on_plugin("greeter", "shutdown", move |event: serde_json::Value| {
                context_clone.log(
                    LogLevel::Info,
                    format!(
                        "📝 LoggerPlugin: 🤝 PLUGIN EVENT - Greeter shutting down: {:?}",
                        event
                    )
                    .as_str(),
                );
                Ok(())
            })
            .await
            .map_err(|e: horizon_event_system::EventError| {
                PluginError::ExecutionError(e.to_string())
            })?;

        // Listen to any plugin events (wildcard-style)
        let context_clone = context.clone();
        events
            .on_plugin("logger", "activity", move |event: serde_json::Value| {
                context_clone.log(
                    LogLevel::Info,
                    format!("📝 LoggerPlugin: 🌐 GENERAL ACTIVITY - {:?}", event).as_str(),
                );
                Ok(())
            })
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        let context_clone = context.clone();
        events
            .on_plugin(
                "InventorySystem",
                "service_started",
                move |event: serde_json::Value| {
                    context_clone.log(
                        LogLevel::Info,
                        format!("Plugin event received: {:?}", event).as_str(),
                    );
                    Ok(())
                },
            )
            .await
            .expect("Failed to register InventorySystem event handler");

        context.log(
            LogLevel::Info,
            "📝 LoggerPlugin: ✅ Event logging system activated!",
        );
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

        context.log(
            LogLevel::Info,
            "📝 LoggerPlugin: ✅ Now monitoring all server events!",
        );

        // Set up a periodic summary using async event emission with tokio handle from context
        let events_clone = context.events();
        let events_ref = events_clone.clone();
        let luminal_handle = context.luminal_handle();
        let context_clone = context.clone();

        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;
        let tick_counter = Arc::new(AtomicU32::new(0));
        let tick_counter_clone = tick_counter.clone();

        events_clone
            .on_core_async("server_tick", move |_event: serde_json::Value| {
                context_clone.log(LogLevel::Trace, "📝 LoggerPlugin: 🕒 Server tick received, updating activity log...");
                let events_inner = events_ref.clone();
                let tick_counter = tick_counter_clone.clone();
                let context_inner = context_clone.clone();

                // Use the tokio runtime handle passed from the main process via context
                luminal_handle.spawn(async move {
                    // Emit periodic summary every 30 server ticks (assuming ~1 tick per second)
                    let tick = tick_counter.fetch_add(1, Ordering::SeqCst) + 1;
                    if tick % 2 == 0 {
                        let summary_count = tick / 30;
                        let _ = events_inner.emit_plugin("logger", "activity_logged", &serde_json::json!({
                                "activity_type": "periodic_summary",
                                "details": format!("Summary #{} - Logger still active", summary_count),
                                "timestamp": current_timestamp()
                            })).await;
                            context_inner.log(LogLevel::Trace, format!("📝 LoggerPlugin: 📊 Periodic Summary #{} - Still logging events...", summary_count).as_str());
                        }
                    });
                    Ok(())
                })
                .await.unwrap();
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

        context.log(
            LogLevel::Info,
            "📝 LoggerPlugin: ✅ Final report submitted. Logging service offline.",
        );
        Ok(())
    }
}

// Create the plugin using our macro - zero unsafe code!
create_simple_plugin!(LoggerPlugin);
