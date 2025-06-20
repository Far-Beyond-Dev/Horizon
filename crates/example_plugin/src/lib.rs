//! Enhanced Welcome Plugin - Demonstrates the new type-safe client event API
//! 
//! This plugin showcases the clean, type-safe event subscription system
//! without any manual JSON parsing or type checking, providing a comprehensive
//! example of modern plugin development for the Horizon game server.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::runtime::Handle;
use tracing::{info, error, debug};

// Import types from the main project
use event_system::types::{
    Plugin, ServerContext, Player, PlayerId, PluginError, LogLevel, 
    PlayerJoinedEvent, PlayerLeftEvent, ChatMessageEvent, MoveCommandEvent, 
    CombatActionEvent, CraftingRequestEvent, PlayerInteractionEvent, UseItemEvent,
    MovementType, CombatActionType, InteractionType, Position, ClientEventSystemExt,
    EventSystemExt, EventId, EventSystemImpl
};

// ============================================================================
// Enhanced Plugin State and Configuration
// ============================================================================

/// Enhanced player statistics with comprehensive tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStats {
    pub player_id: PlayerId,
    pub first_joined: u64,
    pub total_joins: u32,
    pub last_seen: u64,
    pub messages_sent: u32,
    pub movement_commands: u32,
    pub combat_actions: u32,
    pub crafting_requests: u32,
    pub interactions: u32,
    pub total_events: u32,
    pub favorite_channel: Option<String>,
    pub most_used_ability: Option<u32>,
    pub total_distance_moved: f64,
}

impl PlayerStats {
    pub fn new(player_id: PlayerId, timestamp: u64) -> Self {
        Self {
            player_id,
            first_joined: timestamp,
            total_joins: 1,
            last_seen: timestamp,
            messages_sent: 0,
            movement_commands: 0,
            combat_actions: 0,
            crafting_requests: 0,
            interactions: 0,
            total_events: 0,
            favorite_channel: None,
            most_used_ability: None,
            total_distance_moved: 0.0,
        }
    }

    pub fn increment_event(&mut self, event_type: &str, timestamp: u64) {
        self.last_seen = timestamp;
        self.total_events += 1;
        
        match event_type {
            "chat_message" => self.messages_sent += 1,
            "move_command" => self.movement_commands += 1,
            "combat_action" => self.combat_actions += 1,
            "crafting_request" => self.crafting_requests += 1,
            "player_interaction" => self.interactions += 1,
            _ => {}
        }
    }
    
    pub fn update_distance(&mut self, distance: f64) {
        self.total_distance_moved += distance;
    }
    
    pub fn update_favorite_channel(&mut self, channel: Option<String>) {
        if let Some(channel) = channel {
            self.favorite_channel = Some(channel);
        }
    }
    
    pub fn update_most_used_ability(&mut self, ability_id: u32) {
        self.most_used_ability = Some(ability_id);
    }
}

/// Plugin configuration with extensive customization options
#[derive(Debug, Clone)]
pub struct WelcomeConfig {
    pub welcome_message: String,
    pub goodbye_message: String,
    pub track_detailed_stats: bool,
    pub respond_to_commands: bool,
    pub enable_combat_logging: bool,
    pub enable_movement_tracking: bool,
    pub enable_crafting_assistance: bool,
    pub auto_greet_returning_players: bool,
    pub stats_broadcast_interval: u64, // seconds
    pub max_stats_retention: u64, // seconds
}

impl Default for WelcomeConfig {
    fn default() -> Self {
        Self {
            welcome_message: "🎉 Welcome to Horizon, {name}! Type !help for commands.".to_string(),
            goodbye_message: "👋 Goodbye {name}! Thanks for playing on Horizon!".to_string(),
            track_detailed_stats: true,
            respond_to_commands: true,
            enable_combat_logging: true,
            enable_movement_tracking: false, // Can be noisy
            enable_crafting_assistance: true,
            auto_greet_returning_players: true,
            stats_broadcast_interval: 300, // 5 minutes
            max_stats_retention: 86400 * 7, // 1 week
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionInfo {
    start_time: u64,
    events_processed: u32,
    last_activity: u64,
}

/// Shared plugin state - lightweight and cloneable
#[derive(Clone)]
pub struct PluginState {
    /// Player statistics with detailed tracking
    pub player_stats: Arc<RwLock<HashMap<PlayerId, PlayerStats>>>,
    /// Player positions for distance tracking
    pub player_positions: Arc<RwLock<HashMap<PlayerId, Position>>>,
    /// Server context for interacting with the game world
    pub server_context: Arc<RwLock<Option<Arc<dyn ServerContext>>>>,
    /// Event counters for diagnostics
    pub event_counters: Arc<RwLock<HashMap<String, u64>>>,
    /// Command usage statistics
    pub command_stats: Arc<RwLock<HashMap<String, u64>>>,
    /// Active player sessions
    pub active_sessions: Arc<RwLock<HashMap<PlayerId, SessionInfo>>>,
    /// Runtime handle for spawning tasks
    pub runtime_handle: Option<Handle>,
}

impl PluginState {
    pub fn new() -> Self {
        Self {
            player_stats: Arc::new(RwLock::new(HashMap::new())),
            player_positions: Arc::new(RwLock::new(HashMap::new())),
            server_context: Arc::new(RwLock::new(None)),
            event_counters: Arc::new(RwLock::new(HashMap::new())),
            command_stats: Arc::new(RwLock::new(HashMap::new())),
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
            runtime_handle: None,
        }
    }

    pub fn with_runtime_handle(mut self, handle: Handle) -> Self {
        self.runtime_handle = Some(handle);
        self
    }

    pub async fn set_server_context(&self, context: Arc<dyn ServerContext>) {
        let mut guard = self.server_context.write().await;
        *guard = Some(context);
    }

    /// Safely spawn a task using the stored runtime handle
    pub fn spawn_task<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if let Some(handle) = &self.runtime_handle {
            handle.spawn(future);
        } else {
            // Fallback: try to get current runtime handle
            match Handle::try_current() {
                Ok(handle) => {
                    handle.spawn(future);
                }
                Err(e) => {
                    error!("No tokio runtime available to spawn task: {}", e);
                }
            }
        }
    }
}

/// Main plugin struct
pub struct EnhancedWelcomePlugin {
    /// Plugin configuration
    config: WelcomeConfig,
    /// Shared state
    state: PluginState,
}

impl EnhancedWelcomePlugin {
    pub fn new() -> Self {
        Self {
            config: WelcomeConfig::default(),
            state: PluginState::new(),
        }
    }

    /// Get current timestamp
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Calculate distance between two positions
    fn calculate_distance(pos1: &Position, pos2: &Position) -> f64 {
        let dx = pos1.x - pos2.x;
        let dy = pos1.y - pos2.y;
        let dz = pos1.z - pos2.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

// ============================================================================
// Static Event Handlers - Clean and Efficient
// ============================================================================

impl EnhancedWelcomePlugin {
    /// Increment event counter for diagnostics
    async fn increment_event_counter(state: &PluginState, event_type: &str) {
        let mut counters = state.event_counters.write().await;
        *counters.entry(event_type.to_string()).or_insert(0) += 1;
    }

    /// Increment command usage counter
    async fn increment_command_counter(state: &PluginState, command: &str) {
        let mut stats = state.command_stats.write().await;
        *stats.entry(command.to_string()).or_insert(0) += 1;
    }

    /// Update player statistics with new event
    async fn update_player_stats(
        state: &PluginState, 
        config: &WelcomeConfig,
        player_id: PlayerId, 
        event_type: &str
    ) -> Result<(), PluginError> {
        if !config.track_detailed_stats {
            return Ok(());
        }

        let timestamp = Self::current_timestamp();
        let mut stats_map = state.player_stats.write().await;
        
        match stats_map.get_mut(&player_id) {
            Some(stats) => {
                stats.increment_event(event_type, timestamp);
            }
            None => {
                let mut new_stats = PlayerStats::new(player_id, timestamp);
                new_stats.increment_event(event_type, timestamp);
                stats_map.insert(player_id, new_stats);
            }
        }

        // Update session info
        let mut sessions = state.active_sessions.write().await;
        if let Some(session) = sessions.get_mut(&player_id) {
            session.events_processed += 1;
            session.last_activity = timestamp;
        } else {
            sessions.insert(player_id, SessionInfo {
                start_time: timestamp,
                events_processed: 1,
                last_activity: timestamp,
            });
        }

        Self::increment_event_counter(state, event_type).await;
        Ok(())
    }

    /// Send a response message to a player
    async fn send_response(
        state: &PluginState,
        player_id: PlayerId, 
        message_type: &str, 
        content: serde_json::Value
    ) -> Result<(), PluginError> {
        let guard = state.server_context.read().await;
        let context = guard.as_ref()
            .ok_or_else(|| PluginError::ExecutionError("Server context not available".to_string()))?;

        let response = serde_json::json!({
            "type": message_type,
            "content": content,
            "timestamp": Self::current_timestamp(),
            "plugin": "welcome"
        });

        let response_bytes = serde_json::to_vec(&response)
            .map_err(|e| PluginError::ExecutionError(format!("Failed to serialize response: {}", e)))?;

        context.send_to_player(player_id, &response_bytes).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to send response: {}", e)))?;

        Ok(())
    }

    /// Send a welcome message to a player
    async fn send_welcome_message(
        state: &PluginState,
        config: &WelcomeConfig,
        player: &Player
    ) -> Result<(), PluginError> {
        let guard = state.server_context.read().await;
        let context = guard.as_ref()
            .ok_or_else(|| PluginError::ExecutionError("Server context not available".to_string()))?;

        // Check if this is a returning player
        let is_returning = {
            let stats_map = state.player_stats.read().await;
            stats_map.get(&player.id).map_or(false, |stats| stats.total_joins > 1)
        };

        let message = if is_returning && config.auto_greet_returning_players {
            let stats_map = state.player_stats.read().await;
            if let Some(stats) = stats_map.get(&player.id) {
                format!("🎉 Welcome back, {}! You've joined {} times. Total events: {}", 
                       player.name, stats.total_joins, stats.total_events)
            } else {
                config.welcome_message.replace("{name}", &player.name)
            }
        } else {
            config.welcome_message.replace("{name}", &player.name)
        };

        let welcome_data = serde_json::json!({
            "message": message,
            "is_returning_player": is_returning,
            "player_id": player.id,
            "available_commands": [
                "!stats - View your statistics",
                "!help - Show help",
                "!time - Server time",
                "!leaderboard - Top players",
                "!session - Current session info"
            ]
        });

        Self::send_response(state, player.id, "welcome", welcome_data).await?;

        context.log(LogLevel::Info, &format!("Sent welcome message to player {}", player.name));

        Ok(())
    }

    /// Handle player joined events
    async fn handle_player_joined(
        state: PluginState,
        config: WelcomeConfig,
        event: PlayerJoinedEvent
    ) {
        info!("🎉 Player {} joined the server!", event.player.name);
        
        // Initialize player stats
        let mut stats_map = state.player_stats.write().await;
        let existing = stats_map.get_mut(&event.player.id);
        
        if let Some(existing_stats) = existing {
            existing_stats.total_joins += 1;
            existing_stats.last_seen = event.timestamp;
        } else {
            stats_map.insert(event.player.id, PlayerStats::new(event.player.id, event.timestamp));
        }
        drop(stats_map); // Release lock early
        
        // Send welcome message
        if let Err(e) = Self::send_welcome_message(&state, &config, &event.player).await {
            error!("Failed to send welcome message: {}", e);
        }
    }

    /// Handle player left events
    async fn handle_player_left(
        state: PluginState,
        _config: WelcomeConfig,
        event: PlayerLeftEvent
    ) {
        info!("👋 Player {} left the server", event.player_id);
        
        // Clean up session
        let mut sessions = state.active_sessions.write().await;
        sessions.remove(&event.player_id);
    }

    /// Handle chat messages with comprehensive command processing
    async fn handle_chat_message(
        state: PluginState,
        config: WelcomeConfig,
        event: ChatMessageEvent
    ) {
        info!("💬 Player {} in channel {:?}: {}", 
              event.player_id, event.channel, event.message);

        if let Err(e) = Self::update_player_stats(&state, &config, event.player_id, "chat_message").await {
            error!("Failed to update player stats: {}", e);
            return;
        }

        // Update favorite channel statistics
        if let Some(channel) = &event.channel {
            let mut stats_map = state.player_stats.write().await;
            if let Some(stats) = stats_map.get_mut(&event.player_id) {
                stats.update_favorite_channel(Some(channel.clone()));
            }
        }

        if !config.respond_to_commands {
            return;
        }

        // Process commands with enhanced functionality
        match event.message.as_str() {
            msg if msg.starts_with("!stats") => {
                Self::increment_command_counter(&state, "stats").await;
                let stats_map = state.player_stats.read().await;
                if let Some(stats) = stats_map.get(&event.player_id) {
                    let sessions = state.active_sessions.read().await;
                    let session_data = sessions.get(&event.player_id).cloned();
                    let detailed_stats = serde_json::json!({
                        "basic": stats,
                        "session": session_data
                    });
                    if let Err(e) = Self::send_response(&state, event.player_id, "stats_response", detailed_stats).await {
                        error!("Failed to send stats response: {}", e);
                    }
                }
            }
            
            msg if msg.starts_with("!help") => {
                Self::increment_command_counter(&state, "help").await;
                let help_content = serde_json::json!({
                    "commands": [
                        "!stats - View detailed statistics",
                        "!help - Show this help message",
                        "!time - Show server time",
                        "!leaderboard - Show top players",
                        "!session - Current session information",
                        "!events - Event counter statistics"
                    ],
                    "features": [
                        "Automatic welcome messages",
                        "Player statistics tracking",
                        "Combat action logging",
                        "Movement tracking",
                        "Crafting assistance"
                    ]
                });
                if let Err(e) = Self::send_response(&state, event.player_id, "help_response", help_content).await {
                    error!("Failed to send help response: {}", e);
                }
            }
            
            msg if msg.starts_with("!time") => {
                Self::increment_command_counter(&state, "time").await;
                let time_content = serde_json::json!({
                    "server_time": Self::current_timestamp(),
                    "formatted_time": "2025-06-20 22:24:24", // Updated to current time
                    "uptime_seconds": Self::current_timestamp()
                });
                if let Err(e) = Self::send_response(&state, event.player_id, "time_response", time_content).await {
                    error!("Failed to send time response: {}", e);
                }
            }
            
            msg if msg.starts_with("!leaderboard") => {
                Self::increment_command_counter(&state, "leaderboard").await;
                let stats_map = state.player_stats.read().await;
                let mut leaderboard: Vec<_> = stats_map.values().collect();
                leaderboard.sort_by(|a, b| b.total_events.cmp(&a.total_events));
                leaderboard.truncate(10); // Top 10
                
                let leaderboard_data = serde_json::json!({
                    "top_players": leaderboard.iter().enumerate().map(|(i, stats)| {
                        serde_json::json!({
                            "rank": i + 1,
                            "player_id": stats.player_id,
                            "total_events": stats.total_events,
                            "messages_sent": stats.messages_sent,
                            "distance_moved": stats.total_distance_moved
                        })
                    }).collect::<Vec<_>>()
                });
                if let Err(e) = Self::send_response(&state, event.player_id, "leaderboard_response", leaderboard_data).await {
                    error!("Failed to send leaderboard response: {}", e);
                }
            }
            
            msg if msg.starts_with("!session") => {
                Self::increment_command_counter(&state, "session").await;
                let sessions = state.active_sessions.read().await;
                if let Some(session) = sessions.get(&event.player_id) {
                    let session_data = serde_json::json!({
                        "start_time": session.start_time,
                        "duration_seconds": Self::current_timestamp() - session.start_time,
                        "events_processed": session.events_processed,
                        "last_activity": session.last_activity
                    });
                    if let Err(e) = Self::send_response(&state, event.player_id, "session_response", session_data).await {
                        error!("Failed to send session response: {}", e);
                    }
                }
            }
            
            msg if msg.starts_with("!events") => {
                Self::increment_command_counter(&state, "events").await;
                let counters = state.event_counters.read().await;
                let command_stats = state.command_stats.read().await;
                let events_data = serde_json::json!({
                    "event_counters": *counters,
                    "command_usage": *command_stats
                });
                if let Err(e) = Self::send_response(&state, event.player_id, "events_response", events_data).await {
                    error!("Failed to send events response: {}", e);
                }
            }
            
            _ => {
                // Regular chat message, maybe respond to greetings
                let lower_msg = event.message.to_lowercase();
                if lower_msg.contains("hello") || lower_msg.contains("hi") || lower_msg.contains("hey") {
                    let greeting = serde_json::json!({
                        "message": "Hello there! Welcome to Horizon! 👋 Type !help for available commands."
                    });
                    if let Err(e) = Self::send_response(&state, event.player_id, "greeting_response", greeting).await {
                        error!("Failed to send greeting response: {}", e);
                    }
                }
            }
        }
    }

    /// Handle player movement commands with distance tracking
    async fn handle_move_command(
        state: PluginState,
        config: WelcomeConfig,
        event: MoveCommandEvent
    ) {
        if config.enable_movement_tracking {
            info!("🚶 Player {} moving to ({:.1}, {:.1}, {:.1}) via {:?}", 
                  event.player_id, event.target_x, event.target_y, event.target_z, event.movement_type);
        }

        if let Err(e) = Self::update_player_stats(&state, &config, event.player_id, "move_command").await {
            error!("Failed to update player stats: {}", e);
            return;
        }

        // Track distance moved
        let new_position = Position::new(event.target_x, event.target_y, event.target_z);
        let mut positions = state.player_positions.write().await;
        
        if let Some(old_position) = positions.get(&event.player_id) {
            let distance = Self::calculate_distance(old_position, &new_position);
            
            // Update stats with distance
            let mut stats_map = state.player_stats.write().await;
            if let Some(stats) = stats_map.get_mut(&event.player_id) {
                stats.update_distance(distance);
            }
            
            if config.enable_movement_tracking && distance > 100.0 {
                info!("📏 Player {} moved {:.1} units (total: {:.1})", 
                      event.player_id, distance,
                      stats_map.get(&event.player_id).map_or(0.0, |s| s.total_distance_moved));
            }
        }
        
        positions.insert(event.player_id, new_position);

        // Log special movement types
        match event.movement_type {
            MovementType::Teleport => {
                info!("✨ Player {} teleported to ({:.1}, {:.1}, {:.1})", 
                      event.player_id, event.target_x, event.target_y, event.target_z);
            }
            MovementType::Run => {
                debug!("🏃 Player {} running to target location", event.player_id);
            }
            MovementType::Walk => {
                debug!("🚶 Player {} walking to target location", event.player_id);
            }
        }
    }

    /// Handle combat actions with ability tracking
    async fn handle_combat_action(
        state: PluginState,
        config: WelcomeConfig,
        event: CombatActionEvent
    ) {
        if config.enable_combat_logging {
            match event.action_type {
                CombatActionType::Attack => {
                    if let Some(target_id) = event.target_id {
                        info!("⚔️ Player {} attacks Player {} with ability {}", 
                              event.player_id, target_id, event.ability_id);
                    } else if let Some(pos) = event.target_position {
                        info!("⚔️ Player {} attacks position ({:.1}, {:.1}, {:.1}) with ability {}", 
                              event.player_id, pos.x, pos.y, pos.z, event.ability_id);
                    }
                }
                CombatActionType::Defend => {
                    info!("🛡️ Player {} defends with ability {}", event.player_id, event.ability_id);
                }
                CombatActionType::Cast => {
                    info!("✨ Player {} casts spell {} (ability {})", 
                          event.player_id, event.ability_id, event.ability_id);
                }
                CombatActionType::Dodge => {
                    info!("💨 Player {} dodges", event.player_id);
                }
            }
        }

        if let Err(e) = Self::update_player_stats(&state, &config, event.player_id, "combat_action").await {
            error!("Failed to update player stats: {}", e);
            return;
        }
        
        // Track most used ability
        let mut stats_map = state.player_stats.write().await;
        if let Some(stats) = stats_map.get_mut(&event.player_id) {
            stats.update_most_used_ability(event.ability_id);
        }
    }

    /// Handle crafting requests with assistance
    async fn handle_crafting_request(
        state: PluginState,
        config: WelcomeConfig,
        event: CraftingRequestEvent
    ) {
        info!("🔨 Player {} crafting recipe {} (quantity: {})", 
              event.player_id, event.recipe_id, event.quantity);

        if let Err(e) = Self::update_player_stats(&state, &config, event.player_id, "crafting_request").await {
            error!("Failed to update player stats: {}", e);
            return;
        }

        if config.enable_crafting_assistance {
            // Provide crafting assistance
            let assistance = match event.recipe_id {
                1..=100 => "Basic crafting recipe - should complete quickly!",
                101..=500 => "Intermediate recipe - this might take a while.",
                501..=1000 => "Advanced recipe - make sure you have all materials!",
                _ => "Unknown recipe - experimental crafting detected!",
            };

            let crafting_response = serde_json::json!({
                "recipe_id": event.recipe_id,
                "quantity": event.quantity,
                "status": "processing",
                "estimated_time": std::cmp::min(event.quantity * 2, 60), // Max 60 seconds
                "assistance_tip": assistance,
                "ingredients_used": event.ingredient_sources.len()
            });

            if let Err(e) = Self::send_response(&state, event.player_id, "crafting_response", crafting_response).await {
                error!("Failed to send crafting response: {}", e);
            }
        }
    }

    /// Handle player interactions with detailed logging
    async fn handle_player_interaction(
        state: PluginState,
        config: WelcomeConfig,
        event: PlayerInteractionEvent
    ) {
        match event.interaction_type {
            InteractionType::Trade => {
                info!("💰 Player {} wants to trade with Player {}", 
                      event.player_id, event.target_player_id);
            }
            InteractionType::Challenge => {
                info!("⚔️ Player {} challenges Player {} to combat", 
                      event.player_id, event.target_player_id);
            }
            InteractionType::Friend => {
                info!("👥 Player {} sends friend request to Player {}", 
                      event.player_id, event.target_player_id);
            }
            InteractionType::Inspect => {
                info!("👁️ Player {} inspects Player {}", 
                      event.player_id, event.target_player_id);
            }
            InteractionType::Follow => {
                info!("🚶 Player {} follows Player {}", 
                      event.player_id, event.target_player_id);
            }
        }

        if let Err(e) = Self::update_player_stats(&state, &config, event.player_id, "player_interaction").await {
            error!("Failed to update player stats: {}", e);
            return;
        }
        
        // Send interaction confirmation
        let interaction_response = serde_json::json!({
            "interaction_type": format!("{:?}", event.interaction_type),
            "target_player": event.target_player_id,
            "status": "processed",
            "message": format!("{:?} interaction initiated", event.interaction_type)
        });
        
        if let Err(e) = Self::send_response(&state, event.player_id, "interaction_response", interaction_response).await {
            error!("Failed to send interaction response: {}", e);
        }
    }

    /// Handle item usage from other plugins
    async fn handle_item_used(
        state: PluginState,
        _config: WelcomeConfig,
        event: UseItemEvent
    ) {
        info!("🎒 Player {} used item {}", event.player_id, event.item_id);
        
        // Respond with congratulations for special items
        let response = match event.item_id {
            999 => {
                serde_json::json!({
                    "message": "Congratulations on using the legendary item! ✨",
                    "special_effect": "You feel a surge of power!",
                    "bonus_applied": true
                })
            }
            1..=100 => {
                serde_json::json!({
                    "message": "You used a common item.",
                    "effect": "Basic effect applied"
                })
            }
            101..=500 => {
                serde_json::json!({
                    "message": "You used a rare item!",
                    "effect": "Enhanced effect applied",
                    "bonus_experience": 50
                })
            }
            _ => {
                serde_json::json!({
                    "message": "Unknown item used - experimental effects!",
                    "effect": "Mysterious effect applied"
                })
            }
        };

        if let Err(e) = Self::send_response(&state, event.player_id, "item_use_response", response).await {
            error!("Failed to send item use response: {}", e);
        }
    }

    /// Register all event handlers with proper runtime context
    async fn register_event_handlers(
        &mut self,
        events: Arc<EventSystemImpl>,
        state: PluginState,
        config: WelcomeConfig,
    ) -> Result<(), PluginError> {
        info!("🔗 Registering event handlers with runtime-aware spawning");

        // Subscribe to core server events
        // FIX: Clone the values BEFORE moving them into the closure
        let player_joined_state = state.clone();
        let player_joined_config = config.clone();
        events.on_core("player_joined", move |event: PlayerJoinedEvent| {
            let handler_state = player_joined_state.clone();
            let handler_config = player_joined_config.clone();
            let task_state = handler_state.clone();
            handler_state.spawn_task(async move {
                Self::handle_player_joined(task_state, handler_config, event).await;
            });
            Ok(())
        }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register player_joined handler: {}", e)))?;

        let player_left_state = state.clone();
        let player_left_config = config.clone();
        events.on_core("player_left", move |event: PlayerLeftEvent| {
            let handler_state = player_left_state.clone();
            let handler_config = player_left_config.clone();
            let task_state = handler_state.clone();
            handler_state.spawn_task(async move {
                Self::handle_player_left(task_state, handler_config, event).await;
            });
            Ok(())
        }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register player_left handler: {}", e)))?;

        // Subscribe to client events with full type safety!
        let chat_message_state = state.clone();
        let chat_message_config = config.clone();
        events.on_client("chat_message", move |event: ChatMessageEvent| {
            let handler_state = chat_message_state.clone();
            let handler_config = chat_message_config.clone();
            let task_state = handler_state.clone();
            handler_state.spawn_task(async move {
                Self::handle_chat_message(task_state, handler_config, event).await;
            });
            Ok(())
        }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register chat_message handler: {}", e)))?;

        let move_command_state = state.clone();
        let move_command_config = config.clone();
        events.on_client("move_command", move |event: MoveCommandEvent| {
            let handler_state = move_command_state.clone();
            let handler_config = move_command_config.clone();
            let task_state = handler_state.clone();
            handler_state.spawn_task(async move {
                Self::handle_move_command(task_state, handler_config, event).await;
            });
            Ok(())
        }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register move_command handler: {}", e)))?;

        let combat_action_state = state.clone();
        let combat_action_config = config.clone();
        events.on_client("combat_action", move |event: CombatActionEvent| {
            let handler_state = combat_action_state.clone();
            let handler_config = combat_action_config.clone();
            let task_state = handler_state.clone();
            handler_state.spawn_task(async move {
                Self::handle_combat_action(task_state, handler_config, event).await;
            });
            Ok(())
        }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register combat_action handler: {}", e)))?;

        let crafting_request_state = state.clone();
        let crafting_request_config = config.clone();
        events.on_client("crafting_request", move |event: CraftingRequestEvent| {
            let handler_state = crafting_request_state.clone();
            let handler_config = crafting_request_config.clone();
            let task_state = handler_state.clone();
            handler_state.spawn_task(async move {
                Self::handle_crafting_request(task_state, handler_config, event).await;
            });
            Ok(())
        }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register crafting_request handler: {}", e)))?;

        let player_interaction_state = state.clone();
        let player_interaction_config = config.clone();
        events.on_client("player_interaction", move |event: PlayerInteractionEvent| {
            let handler_state = player_interaction_state.clone();
            let handler_config = player_interaction_config.clone();
            let task_state = handler_state.clone();
            handler_state.spawn_task(async move {
                Self::handle_player_interaction(task_state, handler_config, event).await;
            });
            Ok(())
        }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register player_interaction handler: {}", e)))?;

        // Subscribe to events from other plugins
        let item_used_state = state.clone();
        let item_used_config = config.clone();
        events.on_plugin("inventory", "item_used", move |event: UseItemEvent| {
            let handler_state = item_used_state.clone();
            let handler_config = item_used_config.clone();
            let task_state = handler_state.clone();
            handler_state.spawn_task(async move {
                Self::handle_item_used(task_state, handler_config, event).await;
            });
            Ok(())
        }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register item_used handler: {}", e)))?;

        info!("✅ Successfully registered {} event handlers", 8);
        Ok(())
    }
}

// ============================================================================
// Plugin Implementation with Clean Event Subscriptions
// ============================================================================

#[async_trait]
impl Plugin for EnhancedWelcomePlugin {
    fn name(&self) -> &str {
        "enhanced_welcome"
    }

    fn version(&self) -> &str {
        "2.0.0"
    }

    async fn pre_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        info!("🚀 Enhanced Welcome Plugin: Starting pre-initialization with runtime-aware event API");

        // Try to get the current runtime handle - this should work in DLL plugins
        // as long as the host application is running a tokio runtime
        let runtime_handle = Handle::try_current()
            .map_err(|e| PluginError::InitializationFailed(format!("No tokio runtime available: {}. Make sure the host application is running a tokio runtime.", e)))?;

        info!("✅ Successfully obtained tokio runtime handle");

        // Store server context
        self.state.set_server_context(context.clone()).await;

        // Update state with runtime handle
        self.state = self.state.clone().with_runtime_handle(runtime_handle);

        let events: Arc<EventSystemImpl> = context.events();
        
        // Clone lightweight state and config for event handlers
        let state = self.state.clone();
        let config = self.config.clone();

        // Register all event handlers with proper runtime context
        self.register_event_handlers(events, state, config).await?;

        info!("✅ Enhanced Welcome Plugin: Pre-initialization completed with runtime-aware event system");
        Ok(())
    }

    async fn init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        info!("🎯 Enhanced Welcome Plugin: Initialization phase starting");

        // Emit plugin ready event
        let plugin_ready_event = serde_json::json!({
            "plugin_name": self.name(),
            "plugin_version": self.version(),
            "features": [
                "type_safe_events",
                "enhanced_stats",
                "command_processing",
                "combat_logging",
                "crafting_support",
                "interaction_tracking",
                "distance_tracking",
                "session_management",
                "leaderboards",
                "runtime_aware_spawning"
            ],
            "config": {
                "track_detailed_stats": self.config.track_detailed_stats,
                "respond_to_commands": self.config.respond_to_commands,
                "enable_combat_logging": self.config.enable_combat_logging,
                "enable_movement_tracking": self.config.enable_movement_tracking,
                "enable_crafting_assistance": self.config.enable_crafting_assistance,
                "auto_greet_returning_players": self.config.auto_greet_returning_players
            },
            "runtime_info": {
                "has_runtime_handle": self.state.runtime_handle.is_some(),
                "dll_plugin": true
            },
            "timestamp": Self::current_timestamp()
        });

        // Emit to plugin namespace for inter-plugin communication
        context.events().emit_namespaced(
            EventId::plugin("enhanced_welcome", "plugin_ready"),
            &plugin_ready_event
        ).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to emit plugin ready event: {}", e)))?;

        context.log(LogLevel::Info, "Enhanced Welcome Plugin initialized successfully with runtime-aware type-safe event system");
        info!("🚀 Enhanced Welcome Plugin: Initialization completed!");
        Ok(())
    }

    async fn shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        info!("🛑 Enhanced Welcome Plugin: Starting shutdown sequence");

        // Log final statistics
        let stats_map = self.state.player_stats.read().await;
        let counters = self.state.event_counters.read().await;
        let command_stats = self.state.command_stats.read().await;
        let sessions = self.state.active_sessions.read().await;
        
        info!("📊 Final Statistics:");
        info!("  - Player records: {}", stats_map.len());
        info!("  - Event counters: {:?}", *counters);
        info!("  - Command usage: {:?}", *command_stats);
        info!("  - Active sessions: {}", sessions.len());

        // Calculate total stats
        let total_events: u32 = stats_map.values().map(|s| s.total_events).sum();
        let total_distance: f64 = stats_map.values().map(|s| s.total_distance_moved).sum();
        
        info!("  - Total events processed: {}", total_events);
        info!("  - Total distance moved by all players: {:.1}", total_distance);

        // Emit shutdown event
        let shutdown_event = serde_json::json!({
            "plugin_name": self.name(),
            "final_stats": {
                "player_records": stats_map.len(),
                "event_counters": *counters,
                "command_usage": *command_stats,
                "active_sessions": sessions.len(),
                "total_events_processed": total_events,
                "total_distance_moved": total_distance
            },
            "runtime_info": {
                "had_runtime_handle": self.state.runtime_handle.is_some()
            },
            "timestamp": Self::current_timestamp()
        });

        context.events().emit_namespaced(
            EventId::plugin("enhanced_welcome", "plugin_shutdown"),
            &shutdown_event
        ).await.map_err(|e| PluginError::ExecutionError(format!("Failed to emit shutdown event: {}", e)))?;

        context.log(LogLevel::Info, "Enhanced Welcome Plugin shutdown completed");
        info!("✅ Enhanced Welcome Plugin: Shutdown completed successfully");
        Ok(())
    }
}

// ============================================================================
// Plugin Export Functions
// ============================================================================

/// Create the plugin instance - required export for dynamic loading
#[no_mangle]
pub unsafe extern "C" fn create_plugin() -> *mut dyn Plugin {
    let plugin = Box::new(EnhancedWelcomePlugin::new());
    Box::into_raw(plugin)
}

/// Destroy the plugin instance - required export for cleanup
#[no_mangle]
pub unsafe extern "C" fn destroy_plugin(plugin: *mut dyn Plugin) {
    if !plugin.is_null() {
        let _ = Box::from_raw(plugin);
    }
}