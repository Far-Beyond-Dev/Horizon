use async_trait::async_trait;
use universal_plugin_system::{
    SimplePlugin, PluginContext, PluginSystemError, EventBus, StructuredEventKey,
    propagation::AllEqPropagator
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, error};

// Define events for demonstration purposes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerConnectedEvent {
    pub player_id: u64,
    pub remote_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerDisconnectedEvent {
    pub player_id: u64,
    pub reason: String,
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
    pub height: f32,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
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
    pub player_id: Option<u64>,
    pub timestamp: u64,
    pub log_count: u32,
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[async_trait]
impl SimplePlugin<StructuredEventKey, AllEqPropagator> for LoggerPlugin {
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
        info!("📝 LoggerPlugin: Registering comprehensive event logging...");

        // Register handlers for core server events
        event_bus
            .on("core", "player_connected", |event: PlayerConnectedEvent| {
                info!("📝 LoggerPlugin: 🟢 CONNECTION - Player {} joined from {}", event.player_id, event.remote_addr);
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        event_bus
            .on("core", "player_disconnected", |event: PlayerDisconnectedEvent| {
                info!("📝 LoggerPlugin: 🔴 DISCONNECTION - Player {} left server: {}", event.player_id, event.reason);
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        event_bus
            .on("core", "plugin_loaded", |event: serde_json::Value| {
                info!("📝 LoggerPlugin: 🔌 PLUGIN LOADED - {:?}", event);
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        // Client events from players
        event_bus
            .on("chat", "message", |event: PlayerChatEvent| {
                info!("📝 LoggerPlugin: 💬 CHAT - Player {} in {}: '{}'", event.player_id, event.channel, event.message);
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        event_bus
            .on("movement", "jump", |event: PlayerJumpEvent| {
                info!("📝 LoggerPlugin: 🦘 MOVEMENT - Player {} jumped {:.1}m at {:?}", event.player_id, event.height, event.position);
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        // Inter-plugin communication
        event_bus
            .on("plugin", "greeter_startup", |event: serde_json::Value| {
                info!("📝 LoggerPlugin: 🤝 PLUGIN EVENT - Greeter started: {:?}", event);
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        event_bus
            .on("plugin", "greeter_shutdown", |event: serde_json::Value| {
                info!("📝 LoggerPlugin: 🤝 PLUGIN EVENT - Greeter shutting down: {:?}", event);
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        // Listen to logger activity events
        event_bus
            .on("plugin", "logger_activity", |event: serde_json::Value| {
                info!("📝 LoggerPlugin: 🌐 GENERAL ACTIVITY - {:?}", event);
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        // Server tick events
        event_bus
            .on("core", "server_tick", |event: serde_json::Value| {
                info!("📝 LoggerPlugin: 🕒 Server tick received: {:?}", event);
                Ok(())
            })
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        info!("📝 LoggerPlugin: ✅ Event logging system activated!");
        Ok(())
    }

    async fn on_init(&mut self, context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>) -> Result<(), PluginSystemError> {
        info!("📝 LoggerPlugin: Comprehensive event logging activated!");

        // Announce our logging service to other plugins
        let event_bus = context.event_bus();
        event_bus
            .emit("plugin", "service_started", &serde_json::json!({
                "service": "event_logging",
                "plugin": "logger",
                "version": self.version(),
                "start_time": current_timestamp(),
                "message": "Logger plugin is now monitoring all events!"
            }))
            .await
            .map_err(|e| PluginSystemError::InitializationFailed(e.to_string()))?;

        info!("📝 LoggerPlugin: ✅ Now monitoring all server events!");
        Ok(())
    }

    async fn on_shutdown(&mut self, context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>) -> Result<(), PluginSystemError> {
        let uptime = self.start_time.elapsed().unwrap_or_default();

        info!(
            "📝 LoggerPlugin: Shutting down. Logged {} events over {:.1} seconds",
            self.events_logged,
            uptime.as_secs_f64()
        );

        // Final log summary
        let event_bus = context.event_bus();
        event_bus
            .emit("plugin", "final_summary", &serde_json::json!({
                "plugin": "logger",
                "total_events_logged": self.events_logged,
                "uptime_seconds": uptime.as_secs(),
                "events_per_second": self.events_logged as f64 / uptime.as_secs_f64().max(1.0),
                "message": "Logger plugin final report",
                "timestamp": current_timestamp()
            }))
            .await
            .map_err(|e| PluginSystemError::RuntimeError(e.to_string()))?;

        info!("📝 LoggerPlugin: ✅ Final report submitted. Logging service offline.");
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
    let plugin = LoggerPlugin::new();
    let wrapper = universal_plugin_system::PluginWrapper::new(plugin);
    Box::into_raw(Box::new(wrapper))
}