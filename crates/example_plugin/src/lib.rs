//! Welcome Plugin - A sample plugin for the Horizon game server
//! 
//! This plugin demonstrates:
//! - Event handling (player join/leave)
//! - Custom event creation and emission
//! - Player interaction via ServerContext
//! - Inter-plugin communication
//! - State management within plugins

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

// Import types from the main project
use types::{Plugin, ServerContext, EventId, Player, PlayerId, PluginError, LogLevel, EventSystemExt, PlayerJoinedEvent, PlayerLeftEvent, ClientMessageEvent};

// ============================================================================
// Plugin State
// ============================================================================

/// Player statistics tracked by the welcome plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStats {
    pub player_id: PlayerId,
    pub first_joined: u64,
    pub total_joins: u32,
    pub last_seen: u64,
    pub messages_sent: u32,
}

impl PlayerStats {
    pub fn new(player_id: PlayerId, timestamp: u64) -> Self {
        Self {
            player_id,
            first_joined: timestamp,
            total_joins: 1,
            last_seen: timestamp,
            messages_sent: 0,
        }
    }
}

/// Internal state of the welcome plugin
pub struct WelcomePluginState {
    /// Player statistics
    player_stats: RwLock<HashMap<PlayerId, PlayerStats>>,
    /// Server context for interacting with the game world
    server_context: Option<Arc<dyn ServerContext>>,
    /// Plugin configuration
    config: WelcomeConfig,
}

#[derive(Debug, Clone)]
pub struct WelcomeConfig {
    pub welcome_message: String,
    pub goodbye_message: String,
    pub track_statistics: bool,
    pub respond_to_chat: bool,
}

impl Default for WelcomeConfig {
    fn default() -> Self {
        Self {
            welcome_message: "🎉 Welcome to the server, {name}! Enjoy your stay!".to_string(),
            goodbye_message: "👋 Goodbye {name}! Thanks for playing!".to_string(),
            track_statistics: true,
            respond_to_chat: true,
        }
    }
}

// ============================================================================
// Custom Events
// ============================================================================

/// Event emitted when a player's statistics are updated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStatsUpdatedEvent {
    pub player_id: PlayerId,
    pub stats: PlayerStats,
    pub timestamp: u64,
}

/// Chat message event (custom client event)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageEvent {
    pub player_id: PlayerId,
    pub message: String,
    pub timestamp: u64,
}

/// Welcome message sent event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WelcomeMessageSentEvent {
    pub player_id: PlayerId,
    pub message: String,
    pub timestamp: u64,
}

// ============================================================================
// Main Plugin Implementation
// ============================================================================

/// The Welcome Plugin
pub struct WelcomePlugin {
    state: Arc<WelcomePluginState>,
}

impl WelcomePlugin {
    pub fn new() -> Self {
        let state = WelcomePluginState {
            player_stats: RwLock::new(HashMap::new()),
            server_context: None,
            config: WelcomeConfig::default(),
        };

        Self {
            state: Arc::new(state),
        }
    }

    /// Get current timestamp
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Send a welcome message to a player
    async fn send_welcome_message(&self, player: &Player) -> Result<(), PluginError> {
        let context = self.state.server_context.as_ref()
            .ok_or_else(|| PluginError::ExecutionError("Server context not available".to_string()))?;

        let message = self.state.config.welcome_message
            .replace("{name}", &player.name);

        // Create welcome message as JSON
        let welcome_data = serde_json::json!({
            "type": "welcome",
            "message": message,
            "timestamp": Self::current_timestamp()
        });

        // Send to player
        let message_bytes = serde_json::to_vec(&welcome_data)
            .map_err(|e| PluginError::ExecutionError(format!("Failed to serialize welcome message: {}", e)))?;

        context.send_to_player(player.id, &message_bytes).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to send welcome message: {}", e)))?;

        context.log(LogLevel::Info, &format!("Sent welcome message to player {}", player.name));

        // Emit welcome message sent event
        let event = WelcomeMessageSentEvent {
            player_id: player.id,
            message,
            timestamp: Self::current_timestamp(),
        };

        context.events().emit_namespaced(
            EventId::plugin("welcome", "welcome_message_sent"),
            &event
        ).await.map_err(|e| PluginError::ExecutionError(format!("Failed to emit welcome event: {}", e)))?;

        Ok(())
    }

    /// Update player statistics
    async fn update_player_stats(&self, player_id: PlayerId, event_type: &str) -> Result<(), PluginError> {
        if !self.state.config.track_statistics {
            return Ok(());
        }

        let context = self.state.server_context.as_ref()
            .ok_or_else(|| PluginError::ExecutionError("Server context not available".to_string()))?;

        let timestamp = Self::current_timestamp();
        let mut stats_map = self.state.player_stats.write().await;
        
        let stats = match stats_map.get_mut(&player_id) {
            Some(existing_stats) => {
                match event_type {
                    "player_joined" => {
                        existing_stats.total_joins += 1;
                        existing_stats.last_seen = timestamp;
                    }
                    "chat_message" => {
                        existing_stats.messages_sent += 1;
                        existing_stats.last_seen = timestamp;
                    }
                    _ => {
                        existing_stats.last_seen = timestamp;
                    }
                }
                existing_stats.clone()
            }
            None => {
                let new_stats = PlayerStats::new(player_id, timestamp);
                stats_map.insert(player_id, new_stats.clone());
                new_stats
            }
        };

        debug!("Updated stats for player {}: {:?}", player_id, stats);

        // Emit stats updated event
        let event = PlayerStatsUpdatedEvent {
            player_id,
            stats,
            timestamp,
        };

        context.events().emit_namespaced(
            EventId::plugin("welcome", "player_stats_updated"),
            &event
        ).await.map_err(|e| PluginError::ExecutionError(format!("Failed to emit stats event: {}", e)))?;

        Ok(())
    }

    /// Handle chat messages
    async fn handle_chat_message(&self, event: ChatMessageEvent) -> Result<(), PluginError> {
        if !self.state.config.respond_to_chat {
            return Ok(());
        }

        let context = self.state.server_context.as_ref()
            .ok_or_else(|| PluginError::ExecutionError("Server context not available".to_string()))?;

        // Update stats
        self.update_player_stats(event.player_id, "chat_message").await?;

        // Respond to specific commands
        if event.message.starts_with("!stats") {
            let stats_map = self.state.player_stats.read().await;
            
            if let Some(stats) = stats_map.get(&event.player_id) {
                let response = serde_json::json!({
                    "type": "stats_response",
                    "stats": stats,
                    "timestamp": Self::current_timestamp()
                });

                let response_bytes = serde_json::to_vec(&response)
                    .map_err(|e| PluginError::ExecutionError(format!("Failed to serialize stats response: {}", e)))?;

                context.send_to_player(event.player_id, &response_bytes).await
                    .map_err(|e| PluginError::ExecutionError(format!("Failed to send stats response: {}", e)))?;
            }
        } else if event.message.starts_with("!help") {
            let help_message = serde_json::json!({
                "type": "help_response",
                "message": "Available commands: !stats, !help, !welcome",
                "timestamp": Self::current_timestamp()
            });

            let help_bytes = serde_json::to_vec(&help_message)
                .map_err(|e| PluginError::ExecutionError(format!("Failed to serialize help response: {}", e)))?;

            context.send_to_player(event.player_id, &help_bytes).await
                .map_err(|e| PluginError::ExecutionError(format!("Failed to send help response: {}", e)))?;
        }

        Ok(())
    }
}

#[async_trait]
impl Plugin for WelcomePlugin {
    fn name(&self) -> &str {
        "welcome"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn pre_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        info!("Welcome Plugin: Starting pre-initialization");

        // Store server context for later use
        unsafe {
            let state_ptr = Arc::as_ptr(&self.state) as *mut WelcomePluginState;
            (*state_ptr).server_context = Some(context.clone());
        }

        let events = context.events();

        // Register handler for player joined events
        {
            let state = self.state.clone();
            events.on_namespaced(
                EventId::core("player_joined"),
                move |event: PlayerJoinedEvent| {
                    let state = state.clone();
                    tokio::spawn(async move {
                        info!("Welcome Plugin: Player {} joined", event.player.name);
                        
                        // Send welcome message
                        if let Err(e) = (WelcomePlugin { state: state.clone() }).send_welcome_message(&event.player).await {
                            error!("Failed to send welcome message: {}", e);
                        }

                        // Update statistics
                        if let Err(e) = (WelcomePlugin { state: state.clone() }).update_player_stats(event.player.id, "player_joined").await {
                            error!("Failed to update player stats: {}", e);
                        }
                    });
                    Ok(())
                }
            ).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register player_joined handler: {}", e)))?;
        }

        // Register handler for player left events
        {
            let state = self.state.clone();
            events.on_namespaced(
                EventId::core("player_left"),
                move |event: PlayerLeftEvent| {
                    let state = state.clone();
                    tokio::spawn(async move {
                        info!("Welcome Plugin: Player {} left", event.player_id);
                        
                        // Update last seen time
                        if let Err(e) = (WelcomePlugin { state: state.clone() }).update_player_stats(event.player_id, "player_left").await {
                            error!("Failed to update player stats on leave: {}", e);
                        }
                    });
                    Ok(())
                }
            ).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register player_left handler: {}", e)))?;
        }

        // Register handler for custom chat messages (from client namespace)
        {
            let state = self.state.clone();
            events.on_namespaced(
                EventId::client("chat_message"),
                move |event: ClientMessageEvent| {
                    let state = state.clone();
                    tokio::spawn(async move {
                        // Parse chat message from client data
                        if let Ok(message) = serde_json::from_value::<String>(event.data.clone()) {
                            let chat_event = ChatMessageEvent {
                                player_id: event.player_id,
                                message,
                                timestamp: WelcomePlugin::current_timestamp(),
                            };

                            if let Err(e) = (WelcomePlugin { state: state.clone() }).handle_chat_message(chat_event).await {
                                error!("Failed to handle chat message: {}", e);
                            }
                        }
                    });
                    Ok(())
                }
            ).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register chat_message handler: {}", e)))?;
        }

        info!("Welcome Plugin: Pre-initialization completed");
        Ok(())
    }

    async fn init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        info!("Welcome Plugin: Starting initialization");

        // Emit plugin ready event
        let plugin_ready_event = serde_json::json!({
            "plugin_name": self.name(),
            "plugin_version": self.version(),
            "features": ["welcome_messages", "player_stats", "chat_commands"],
            "timestamp": Self::current_timestamp()
        });

        context.events().emit_namespaced(
            EventId::plugin("welcome", "plugin_ready"),
            &plugin_ready_event
        ).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to emit plugin ready event: {}", e)))?;

        // Log current players (if any)
        match context.get_players().await {
            Ok(players) => {
                info!("Welcome Plugin: Found {} existing players", players.len());
                for player in players {
                    // Send welcome message to existing players
                    if let Err(e) = self.send_welcome_message(&player).await {
                        warn!("Failed to send welcome message to existing player {}: {}", player.name, e);
                    }
                }
            }
            Err(e) => {
                warn!("Welcome Plugin: Failed to get existing players: {}", e);
            }
        }

        context.log(LogLevel::Info, "Welcome Plugin initialized successfully");
        info!("Welcome Plugin: Initialization completed");
        Ok(())
    }

    async fn shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        info!("Welcome Plugin: Starting shutdown");

        // Emit plugin shutdown event
        let shutdown_event = serde_json::json!({
            "plugin_name": self.name(),
            "timestamp": Self::current_timestamp()
        });

        context.events().emit_namespaced(
            EventId::plugin("welcome", "plugin_shutdown"),
            &shutdown_event
        ).await.map_err(|e| PluginError::ExecutionError(format!("Failed to emit shutdown event: {}", e)))?;

        // Save statistics if needed (could write to file or database)
        let stats_map = self.state.player_stats.read().await;
        info!("Welcome Plugin: Shutting down with {} player records", stats_map.len());

        context.log(LogLevel::Info, "Welcome Plugin shutdown completed");
        info!("Welcome Plugin: Shutdown completed");
        Ok(())
    }
}

// ============================================================================
// Plugin Export Functions
// ============================================================================

/// Create the plugin instance - required export for dynamic loading
#[no_mangle]
pub unsafe extern "C" fn create_plugin() -> *mut dyn Plugin {
    let plugin = Box::new(WelcomePlugin::new());
    Box::into_raw(plugin)
}

/// Destroy the plugin instance - required export for cleanup
#[no_mangle]
pub unsafe extern "C" fn destroy_plugin(plugin: *mut dyn Plugin) {
    if !plugin.is_null() {
        let _ = Box::from_raw(plugin);
    }
}