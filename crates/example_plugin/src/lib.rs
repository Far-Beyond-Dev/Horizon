//! Enhanced Welcome Plugin - Demonstrates the new type-safe client event API
//! 
//! This plugin showcases the clean, type-safe event subscription system
//! without any manual JSON parsing or type checking, providing a comprehensive
//! example of modern plugin development for the Horizon game server.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::ser;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error, debug};

// Import types from the main project
use types::{
    Plugin, ServerContext, Player, PlayerId, PluginError, LogLevel, 
    PlayerJoinedEvent, PlayerLeftEvent, ChatMessageEvent, MoveCommandEvent, 
    CombatActionEvent, CraftingRequestEvent, PlayerInteractionEvent, UseItemEvent,
    MovementType, CombatActionType, InteractionType, Position, ClientEventSystemExt,
    EventSystemExt, EventId
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

/// Internal state of the enhanced welcome plugin
pub struct EnhancedWelcomePlugin {
    /// Player statistics with detailed tracking
    player_stats: Arc<RwLock<HashMap<PlayerId, PlayerStats>>>,
    /// Player positions for distance tracking
    player_positions: Arc<RwLock<HashMap<PlayerId, Position>>>,
    /// Server context for interacting with the game world
    server_context: Arc<RwLock<Option<Arc<dyn ServerContext>>>>,
    /// Plugin configuration
    config: WelcomeConfig,
    /// Event counters for diagnostics
    event_counters: Arc<RwLock<HashMap<String, u64>>>,
    /// Command usage statistics
    command_stats: Arc<RwLock<HashMap<String, u64>>>,
    /// Active player sessions
    active_sessions: Arc<RwLock<HashMap<PlayerId, SessionInfo>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionInfo {
    start_time: u64,
    events_processed: u32,
    last_activity: u64,
}

impl EnhancedWelcomePlugin {
    pub fn new() -> Self {
        Self {
            player_stats: Arc::new(RwLock::new(HashMap::new())),
            player_positions: Arc::new(RwLock::new(HashMap::new())),
            server_context: Arc::new(RwLock::new(None)),
            config: WelcomeConfig::default(),
            event_counters: Arc::new(RwLock::new(HashMap::new())),
            command_stats: Arc::new(RwLock::new(HashMap::new())),
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get current timestamp
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Increment event counter for diagnostics
    async fn increment_event_counter(&self, event_type: &str) {
        let mut counters = self.event_counters.write().await;
        *counters.entry(event_type.to_string()).or_insert(0) += 1;
    }

    /// Increment command usage counter
    async fn increment_command_counter(&self, command: &str) {
        let mut stats = self.command_stats.write().await;
        *stats.entry(command.to_string()).or_insert(0) += 1;
    }

    /// Update player statistics with new event
    async fn update_player_stats(&self, player_id: PlayerId, event_type: &str) -> Result<(), PluginError> {
        if !self.config.track_detailed_stats {
            return Ok(());
        }

        let timestamp = Self::current_timestamp();
        let mut stats_map = self.player_stats.write().await;
        
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
        let mut sessions = self.active_sessions.write().await;
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

        self.increment_event_counter(event_type).await;
        Ok(())
    }

    /// Send a response message to a player
    async fn send_response(&self, player_id: PlayerId, message_type: &str, content: serde_json::Value) -> Result<(), PluginError> {
        let guard = self.server_context.read().await;
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
    async fn send_welcome_message(&self, player: &Player) -> Result<(), PluginError> {
        let guard = self.server_context.read().await;
        let context = guard.as_ref()
            .ok_or_else(|| PluginError::ExecutionError("Server context not available".to_string()))?;

        // Check if this is a returning player
        let is_returning = {
            let stats_map = self.player_stats.read().await;
            stats_map.get(&player.id).map_or(false, |stats| stats.total_joins > 1)
        };

        let message = if is_returning && self.config.auto_greet_returning_players {
            let stats_map = self.player_stats.read().await;
            if let Some(stats) = stats_map.get(&player.id) {
                format!("🎉 Welcome back, {}! You've joined {} times. Total events: {}", 
                       player.name, stats.total_joins, stats.total_events)
            } else {
                self.config.welcome_message.replace("{name}", &player.name)
            }
        } else {
            self.config.welcome_message.replace("{name}", &player.name)
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

        self.send_response(player.id, "welcome", welcome_data).await?;

        context.log(LogLevel::Info, &format!("Sent welcome message to player {}", player.name));

        Ok(())
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
// Event Handlers Using the New Clean API
// ============================================================================

impl EnhancedWelcomePlugin {
    /// Handle chat messages with comprehensive command processing
    async fn handle_chat_message(&self, event: ChatMessageEvent) -> Result<(), PluginError> {
        info!("💬 Player {} in channel {:?}: {}", 
              event.player_id, event.channel, event.message);

        self.update_player_stats(event.player_id, "chat_message").await?;

        // Update favorite channel statistics
        if let Some(channel) = &event.channel {
            let mut stats_map = self.player_stats.write().await;
            if let Some(stats) = stats_map.get_mut(&event.player_id) {
                stats.update_favorite_channel(Some(channel.clone()));
            }
        }

        if !self.config.respond_to_commands {
            return Ok(());
        }

        // Process commands with enhanced functionality
        match event.message.as_str() {
            msg if msg.starts_with("!stats") => {
                self.increment_command_counter("stats").await;
                let stats_map = self.player_stats.read().await;
                if let Some(stats) = stats_map.get(&event.player_id) {
                    let sessions = self.active_sessions.read().await;
                    let session_data = sessions.get(&event.player_id).cloned();
                    let detailed_stats = serde_json::json!({
                        "basic": stats,
                        "session": session_data
                    });
                    self.send_response(event.player_id, "stats_response", detailed_stats).await?;
                }
            }
            
            msg if msg.starts_with("!help") => {
                self.increment_command_counter("help").await;
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
                self.send_response(event.player_id, "help_response", help_content).await?;
            }
            
            msg if msg.starts_with("!time") => {
                self.increment_command_counter("time").await;
                let time_content = serde_json::json!({
                    "server_time": Self::current_timestamp(),
                    "formatted_time": format!("{}", Self::current_timestamp()), // In real implementation, use proper formatting
                    "uptime_seconds": Self::current_timestamp() // Would calculate actual uptime
                });
                self.send_response(event.player_id, "time_response", time_content).await?;
            }
            
            msg if msg.starts_with("!leaderboard") => {
                self.increment_command_counter("leaderboard").await;
                let stats_map = self.player_stats.read().await;
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
                self.send_response(event.player_id, "leaderboard_response", leaderboard_data).await?;
            }
            
            msg if msg.starts_with("!session") => {
                self.increment_command_counter("session").await;
                let sessions = self.active_sessions.read().await;
                if let Some(session) = sessions.get(&event.player_id) {
                    let session_data = serde_json::json!({
                        "start_time": session.start_time,
                        "duration_seconds": Self::current_timestamp() - session.start_time,
                        "events_processed": session.events_processed,
                        "last_activity": session.last_activity
                    });
                    self.send_response(event.player_id, "session_response", session_data).await?;
                }
            }
            
            msg if msg.starts_with("!events") => {
                self.increment_command_counter("events").await;
                let counters = self.event_counters.read().await;
                let command_stats = self.command_stats.read().await;
                let events_data = serde_json::json!({
                    "event_counters": *counters,
                    "command_usage": *command_stats
                });
                self.send_response(event.player_id, "events_response", events_data).await?;
            }
            
            _ => {
                // Regular chat message, maybe respond to greetings
                let lower_msg = event.message.to_lowercase();
                if lower_msg.contains("hello") || lower_msg.contains("hi") || lower_msg.contains("hey") {
                    let greeting = serde_json::json!({
                        "message": "Hello there! Welcome to Horizon! 👋 Type !help for available commands."
                    });
                    self.send_response(event.player_id, "greeting_response", greeting).await?;
                }
            }
        }

        Ok(())
    }

    /// Handle player movement commands with distance tracking
    async fn handle_move_command(&self, event: MoveCommandEvent) -> Result<(), PluginError> {
        if self.config.enable_movement_tracking {
            info!("🚶 Player {} moving to ({:.1}, {:.1}, {:.1}) via {:?}", 
                  event.player_id, event.target_x, event.target_y, event.target_z, event.movement_type);
        }

        self.update_player_stats(event.player_id, "move_command").await?;

        // Track distance moved
        let new_position = Position::new(event.target_x, event.target_y, event.target_z);
        let mut positions = self.player_positions.write().await;
        
        if let Some(old_position) = positions.get(&event.player_id) {
            let distance = Self::calculate_distance(old_position, &new_position);
            
            // Update stats with distance
            let mut stats_map = self.player_stats.write().await;
            if let Some(stats) = stats_map.get_mut(&event.player_id) {
                stats.update_distance(distance);
            }
            
            if self.config.enable_movement_tracking && distance > 100.0 {
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

        Ok(())
    }

    /// Handle combat actions with ability tracking
    async fn handle_combat_action(&self, event: CombatActionEvent) -> Result<(), PluginError> {
        if self.config.enable_combat_logging {
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

        self.update_player_stats(event.player_id, "combat_action").await?;
        
        // Track most used ability
        let mut stats_map = self.player_stats.write().await;
        if let Some(stats) = stats_map.get_mut(&event.player_id) {
            stats.update_most_used_ability(event.ability_id);
        }

        Ok(())
    }

    /// Handle crafting requests with assistance
    async fn handle_crafting_request(&self, event: CraftingRequestEvent) -> Result<(), PluginError> {
        info!("🔨 Player {} crafting recipe {} (quantity: {})", 
              event.player_id, event.recipe_id, event.quantity);

        self.update_player_stats(event.player_id, "crafting_request").await?;

        if self.config.enable_crafting_assistance {
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

            self.send_response(event.player_id, "crafting_response", crafting_response).await?;
        }

        Ok(())
    }

    /// Handle player interactions with detailed logging
    async fn handle_player_interaction(&self, event: PlayerInteractionEvent) -> Result<(), PluginError> {
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

        self.update_player_stats(event.player_id, "player_interaction").await?;
        
        // Send interaction confirmation
        let interaction_response = serde_json::json!({
            "interaction_type": format!("{:?}", event.interaction_type),
            "target_player": event.target_player_id,
            "status": "processed",
            "message": format!("{:?} interaction initiated", event.interaction_type)
        });
        
        self.send_response(event.player_id, "interaction_response", interaction_response).await?;

        Ok(())
    }

    /// Handle item usage from other plugins
    async fn handle_item_used(&self, event: UseItemEvent) -> Result<(), PluginError> {
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

        self.send_response(event.player_id, "item_use_response", response).await?;

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
        info!("🚀 Enhanced Welcome Plugin: Starting pre-initialization with type-safe event API");

        // Store server context
        {
            let mut server_context = self.server_context.write().await;
            *server_context = Some(context.clone());
        }

        let events = context.events();

        // ============================================================================
        // CLEAN TYPE-SAFE EVENT SUBSCRIPTIONS - No more JSON parsing!
        // ============================================================================

        // Subscribe to core server events
        {
            let stats = self.player_stats.clone();
            let config = self.config.clone();
            let plugin_self = EnhancedWelcomePlugin {
                player_stats: self.player_stats.clone(),
                player_positions: self.player_positions.clone(),
                server_context: self.server_context.clone(),
                config: self.config.clone(),
                event_counters: self.event_counters.clone(),
                command_stats: self.command_stats.clone(),
                active_sessions: self.active_sessions.clone(),
            };
            
            events.on_core("player_joined", move |event: PlayerJoinedEvent| {
                let stats = stats.clone();
                let config = config.clone();
                let plugin = EnhancedWelcomePlugin {
                    player_stats: plugin_self.player_stats.clone(),
                    player_positions: plugin_self.player_positions.clone(),
                    server_context: plugin_self.server_context.clone(),
                    config: plugin_self.config.clone(),
                    event_counters: plugin_self.event_counters.clone(),
                    command_stats: plugin_self.command_stats.clone(),
                    active_sessions: plugin_self.active_sessions.clone(),
                };
                
                tokio::spawn(async move {
                    info!("🎉 Player {} joined the server!", event.player.name);
                    
                    // Initialize player stats
                    let mut stats_map = stats.write().await;
                    let existing = stats_map.get_mut(&event.player.id);
                    
                    if let Some(existing_stats) = existing {
                        existing_stats.total_joins += 1;
                        existing_stats.last_seen = event.timestamp;
                    } else {
                        stats_map.insert(event.player.id, PlayerStats::new(event.player.id, event.timestamp));
                    }
                    
                    // Send welcome message
                    if let Err(e) = plugin.send_welcome_message(&event.player).await {
                        error!("Failed to send welcome message: {}", e);
                    }
                });
                Ok(())
            }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register player_joined handler: {}", e)))?;
        }

        {
            let active_sessions = self.active_sessions.clone();
            events.on_core("player_left", move |event: PlayerLeftEvent| {
                let active_sessions = active_sessions.clone();
                tokio::spawn(async move {
                    info!("👋 Player {} left the server", event.player_id);
                    
                    // Clean up session
                    let mut sessions = active_sessions.write().await;
                    sessions.remove(&event.player_id);
                });
                Ok(())
            }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register player_left handler: {}", e)))?;
        }

        // Subscribe to client events with full type safety!
        {
            let plugin_clone = EnhancedWelcomePlugin {
                player_stats: self.player_stats.clone(),
                player_positions: self.player_positions.clone(),
                server_context: self.server_context.clone(),
                config: self.config.clone(),
                event_counters: self.event_counters.clone(),
                command_stats: self.command_stats.clone(),
                active_sessions: self.active_sessions.clone(),
            };
            events.on_client("chat_message", move |event: ChatMessageEvent| {
                let plugin = EnhancedWelcomePlugin {
                    player_stats: plugin_clone.player_stats.clone(),
                    player_positions: plugin_clone.player_positions.clone(),
                    server_context: plugin_clone.server_context.clone(),
                    config: plugin_clone.config.clone(),
                    event_counters: plugin_clone.event_counters.clone(),
                    command_stats: plugin_clone.command_stats.clone(),
                    active_sessions: plugin_clone.active_sessions.clone(),
                };
                tokio::spawn(async move {
                    if let Err(e) = plugin.handle_chat_message(event).await {
                        error!("Failed to handle chat message: {}", e);
                    }
                });
                Ok(())
            }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register chat_message handler: {}", e)))?;
        }

        {
            let plugin_clone = EnhancedWelcomePlugin {
                player_stats: self.player_stats.clone(),
                player_positions: self.player_positions.clone(),
                server_context: self.server_context.clone(),
                config: self.config.clone(),
                event_counters: self.event_counters.clone(),
                command_stats: self.command_stats.clone(),
                active_sessions: self.active_sessions.clone(),
            };
            events.on_client("move_command", move |event: MoveCommandEvent| {
                let plugin = EnhancedWelcomePlugin {
                    player_stats: plugin_clone.player_stats.clone(),
                    player_positions: plugin_clone.player_positions.clone(),
                    server_context: plugin_clone.server_context.clone(),
                    config: plugin_clone.config.clone(),
                    event_counters: plugin_clone.event_counters.clone(),
                    command_stats: plugin_clone.command_stats.clone(),
                    active_sessions: plugin_clone.active_sessions.clone(),
                };
                tokio::spawn(async move {
                    if let Err(e) = plugin.handle_move_command(event).await {
                        error!("Failed to handle move command: {}", e);
                    }
                });
                Ok(())
            }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register move_command handler: {}", e)))?;
        }

        {
            let plugin_clone = EnhancedWelcomePlugin {
                player_stats: self.player_stats.clone(),
                player_positions: self.player_positions.clone(),
                server_context: self.server_context.clone(),
                config: self.config.clone(),
                event_counters: self.event_counters.clone(),
                command_stats: self.command_stats.clone(),
                active_sessions: self.active_sessions.clone(),
            };
            events.on_client("combat_action", move |event: CombatActionEvent| {
                let plugin = EnhancedWelcomePlugin {
                    player_stats: plugin_clone.player_stats.clone(),
                    player_positions: plugin_clone.player_positions.clone(),
                    server_context: plugin_clone.server_context.clone(),
                    config: plugin_clone.config.clone(),
                    event_counters: plugin_clone.event_counters.clone(),
                    command_stats: plugin_clone.command_stats.clone(),
                    active_sessions: plugin_clone.active_sessions.clone(),
                };
                tokio::spawn(async move {
                    if let Err(e) = plugin.handle_combat_action(event).await {
                        error!("Failed to handle combat action: {}", e);
                    }
                });
                Ok(())
            }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register combat_action handler: {}", e)))?;
        }

        {
            let plugin_clone = EnhancedWelcomePlugin {
                player_stats: self.player_stats.clone(),
                player_positions: self.player_positions.clone(),
                server_context: self.server_context.clone(),
                config: self.config.clone(),
                event_counters: self.event_counters.clone(),
                command_stats: self.command_stats.clone(),
                active_sessions: self.active_sessions.clone(),
            };
            events.on_client("crafting_request", move |event: CraftingRequestEvent| {
                let plugin = EnhancedWelcomePlugin {
                    player_stats: plugin_clone.player_stats.clone(),
                    player_positions: plugin_clone.player_positions.clone(),
                    server_context: plugin_clone.server_context.clone(),
                    config: plugin_clone.config.clone(),
                    event_counters: plugin_clone.event_counters.clone(),
                    command_stats: plugin_clone.command_stats.clone(),
                    active_sessions: plugin_clone.active_sessions.clone(),
                };
                tokio::spawn(async move {
                    if let Err(e) = plugin.handle_crafting_request(event).await {
                        error!("Failed to handle crafting request: {}", e);
                    }
                });
                Ok(())
            }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register crafting_request handler: {}", e)))?;
        }

        {
            let plugin_clone = EnhancedWelcomePlugin {
                player_stats: self.player_stats.clone(),
                player_positions: self.player_positions.clone(),
                server_context: self.server_context.clone(),
                config: self.config.clone(),
                event_counters: self.event_counters.clone(),
                command_stats: self.command_stats.clone(),
                active_sessions: self.active_sessions.clone(),
            };
            events.on_client("player_interaction", move |event: PlayerInteractionEvent| {
                let plugin = EnhancedWelcomePlugin {
                    player_stats: plugin_clone.player_stats.clone(),
                    player_positions: plugin_clone.player_positions.clone(),
                    server_context: plugin_clone.server_context.clone(),
                    config: plugin_clone.config.clone(),
                    event_counters: plugin_clone.event_counters.clone(),
                    command_stats: plugin_clone.command_stats.clone(),
                    active_sessions: plugin_clone.active_sessions.clone(),
                };
                tokio::spawn(async move {
                    if let Err(e) = plugin.handle_player_interaction(event).await {
                        error!("Failed to handle player interaction: {}", e);
                    }
                });
                Ok(())
            }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register player_interaction handler: {}", e)))?;
        }

        // Subscribe to events from other plugins
        {
            let plugin_clone = EnhancedWelcomePlugin {
                player_stats: self.player_stats.clone(),
                player_positions: self.player_positions.clone(),
                server_context: self.server_context.clone(),
                config: self.config.clone(),
                event_counters: self.event_counters.clone(),
                command_stats: self.command_stats.clone(),
                active_sessions: self.active_sessions.clone(),
            };
            events.on_plugin("inventory", "item_used", move |event: UseItemEvent| {
                let plugin = EnhancedWelcomePlugin {
                    player_stats: plugin_clone.player_stats.clone(),
                    player_positions: plugin_clone.player_positions.clone(),
                    server_context: plugin_clone.server_context.clone(),
                    config: plugin_clone.config.clone(),
                    event_counters: plugin_clone.event_counters.clone(),
                    command_stats: plugin_clone.command_stats.clone(),
                    active_sessions: plugin_clone.active_sessions.clone(),
                };
                tokio::spawn(async move {
                    if let Err(e) = plugin.handle_item_used(event).await {
                        error!("Failed to handle item used event: {}", e);
                    }
                });
                Ok(())
            }).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to register item_used handler: {}", e)))?;
        }

        info!("✅ Enhanced Welcome Plugin: Pre-initialization completed with {} event handlers", 7);
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
                "leaderboards"
            ],
            "config": {
                "track_detailed_stats": self.config.track_detailed_stats,
                "respond_to_commands": self.config.respond_to_commands,
                "enable_combat_logging": self.config.enable_combat_logging,
                "enable_movement_tracking": self.config.enable_movement_tracking,
                "enable_crafting_assistance": self.config.enable_crafting_assistance,
                "auto_greet_returning_players": self.config.auto_greet_returning_players
            },
            "timestamp": Self::current_timestamp()
        });

        // Emit to plugin namespace for inter-plugin communication
        context.events().emit_namespaced(
            EventId::plugin("enhanced_welcome", "plugin_ready"),
            &plugin_ready_event
        ).await.map_err(|e| PluginError::InitializationFailed(format!("Failed to emit plugin ready event: {}", e)))?;

        context.log(LogLevel::Info, "Enhanced Welcome Plugin initialized successfully with comprehensive type-safe event system");
        info!("🚀 Enhanced Welcome Plugin: Initialization completed!");
        Ok(())
    }

    async fn shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        info!("🛑 Enhanced Welcome Plugin: Starting shutdown sequence");

        // Log final statistics
        let stats_map = self.player_stats.read().await;
        let counters = self.event_counters.read().await;
        let command_stats = self.command_stats.read().await;
        let sessions = self.active_sessions.read().await;
        
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