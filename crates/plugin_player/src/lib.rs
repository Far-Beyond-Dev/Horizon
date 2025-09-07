//! # Pure GORC Player Plugin - Direct Client Event Processing
//!
//! This plugin demonstrates the cleanest GORC architecture:
//! - Clients send GORC-formatted events directly to object instances
//! - GORC instance handlers process and validate server-side
//! - Automatic client replication with zero manual networking code

use async_trait::async_trait;
use horizon_event_system::{
    create_simple_plugin, EventSystem, LogLevel, PlayerId, PluginError, ServerContext, SimplePlugin,
    Vec3, GorcObjectId, GorcEvent, ObjectInstance,
    PlayerConnectedEvent, PlayerDisconnectedEvent,
};
use std::sync::Arc;
use dashmap::DashMap;
use chrono::Utc;

pub mod player;
pub mod events;

use player::{GorcPlayer, PlayerPluginError};
use events::*;

/// Minimal GORC player plugin - clients talk directly to GORC instances
pub struct PlayerPlugin {
    name: String,
    /// Player registry: PlayerId → GorcObjectId  
    players: Arc<DashMap<PlayerId, GorcObjectId>>,
}

impl PlayerPlugin {
    pub fn new() -> Self {
        tracing::info!("🎮 PlayerPlugin: Minimal GORC architecture - direct client processing");
        Self {
            name: "PlayerPlugin".to_string(),
            players: Arc::new(DashMap::new()),
        }
    }

    /// Handle player connection - register with GORC for automatic replication
    async fn handle_player_connected(&self, event: PlayerConnectedEvent) -> Result<(), PlayerPluginError> {
        let spawn_position = Vec3::new(0.0, 0.0, 0.0);
        let player_name = format!("Player_{}", event.player_id);
        let _player = GorcPlayer::new(event.player_id, player_name, spawn_position);
        
        // Simulate GORC object registration
        let gorc_id = GorcObjectId::new();
        self.players.insert(event.player_id, gorc_id);
        
        tracing::info!(
            "🎮 GORC: Player {} registered with ID {:?} - accepting direct client events",
            event.player_id, gorc_id
        );

        Ok(())
    }

    /// Handle player disconnection - clean up GORC registration  
    async fn handle_player_disconnected(&self, event: PlayerDisconnectedEvent) -> Result<(), PlayerPluginError> {
        if let Some((_, _gorc_id)) = self.players.remove(&event.player_id) {
            tracing::info!(
                "🎮 GORC: Player {} unregistered - no longer processing client events",
                event.player_id
            );
        }
        
        Ok(())
    }
}

impl Default for PlayerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SimplePlugin for PlayerPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn register_handlers(&mut self, events: Arc<EventSystem>, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        tracing::info!("🎮 PlayerPlugin: Registering direct GORC instance handlers...");

        let tokio_handle = context.tokio_handle();

        // Create self-reference for handlers 
        let players = Arc::clone(&self.players);
        let players_conn = Arc::clone(&self.players);
        let players_disc = Arc::clone(&self.players);

        let tokio_handle_clone = tokio_handle.clone();

        // Register core events for player lifecycle
        let events_for_player = Arc::clone(&events);
        events.on_core("player_connected", move |event: PlayerConnectedEvent| {
            let players = Arc::clone(&players_conn);
            let events_clone = Arc::clone(&events_for_player);
            tokio_handle_clone.block_on(async move {
                let spawn_position = Vec3::new(0.0, 0.0, 0.0);
                
                // Create GORC player object
                let player = GorcPlayer::new(event.player_id, format!("Player_{}", event.player_id), spawn_position);
                
                // Register with GORC system if available
                if let Some(gorc_instances) = events_clone.get_gorc_instances() {
                    let gorc_id = gorc_instances.register_object(player, spawn_position).await;
                    
                    // Also add player to GORC position tracking
                    gorc_instances.add_player(event.player_id, spawn_position).await;
                    
                    players.insert(event.player_id, gorc_id);
                    
                    tracing::info!(
                        "🎮 GORC: Player {} connected and registered with GORC ID {:?}",
                        event.player_id, gorc_id
                    );
                } else {
                    tracing::warn!("🎮 GORC: No GORC instances manager available for player {}", event.player_id);
                }
            });
            Ok(())
        }).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        let tokio_handle_clone = tokio_handle.clone();
        events.on_core("player_disconnected", move |event: PlayerDisconnectedEvent| {
            let players = Arc::clone(&players_disc);
            tokio_handle_clone.block_on(async move {
                if let Some((_, gorc_id)) = players.remove(&event.player_id) {
                    tracing::info!(
                        "🎮 GORC: Player {} disconnected and unregistered (ID {:?})",
                        event.player_id, gorc_id
                    );
                }
            });
            Ok(())
        }).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // ===============================
        // DIRECT GORC CLIENT EVENT HANDLERS
        // ===============================
        // Clients send GORC-formatted events directly to these handlers
        // No client event bridging needed - GORC processes everything

        // Channel 0 (Critical): Movement and health - 25m range, 60Hz
        let events_clone = Arc::clone(&events);

        let tokio_handle_clone = tokio_handle.clone();
        events.on_gorc_instance("GorcPlayer", 0, "move", 
            move |gorc_event: GorcEvent, instance: &mut ObjectInstance| {

                // Emit move event to trigger GORC's automatic replication
                let instance_id = instance.object_id;

                if let Err(e) = tokio_handle_clone.block_on(events_clone.emit_gorc_instance(instance_id, 0, "position_update", &gorc_event, horizon_event_system::Dest::Client)) {
                    tracing::warn!("🌐 GORC Direct: Failed to emit move event to instance {:?}: {}", instance_id, e);
                }

                tracing::debug!("🌐 GORC Direct: Emitting move event to instance {:?}", instance_id);

                if let Some(player) = instance.get_object_mut::<GorcPlayer>() {
                    if let Ok(move_data) = serde_json::from_slice::<PlayerMoveRequest>(&gorc_event.data) {
                        // Server-side validation and update
                        if let Ok(()) = player.validate_and_apply_movement(
                            move_data.new_position, 
                            move_data.velocity
                        ) {
                            player.movement_state = move_data.movement_state;
                            player.last_update = Utc::now();
                            
                            tracing::debug!(
                                "🌐 GORC Direct: Player {} moved to {:?} - auto-replicating to critical zone (25m)",
                                move_data.player_id, move_data.new_position
                            );

                            // GORC detects the object change and automatically:
                            // 1. Calculates which clients are within 25m
                            // 2. Creates delta-compressed position updates
                            // 3. Sends at 60Hz to each relevant client
                            // 4. Uses NetworkEngine.send_to_player() internally
                        } else {
                            tracing::warn!("🌐 GORC: Invalid movement rejected for player {}", move_data.player_id);
                        }
                    }
                }
                Ok(())
            }
        ).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Channel 1 (Detailed): Combat and equipment - 100m range, 30Hz
        let events_clone_attack = Arc::clone(&events);
        let tokio_handle_attack = tokio_handle.clone();
        events.on_gorc_instance("GorcPlayer", 1, "attack",
            move |gorc_event: GorcEvent, instance: &mut ObjectInstance| {
                let events_clone = Arc::clone(&events_clone_attack);
                let tokio_handle = tokio_handle_attack.clone();
                
                if let Some(player) = instance.get_object_mut::<GorcPlayer>() {
                    if let Ok(attack_data) = serde_json::from_slice::<PlayerAttackRequest>(&gorc_event.data) {
                        // Process attack server-side
                        if let Ok(damage) = player.perform_attack(attack_data.target_position) {
                            tracing::info!(
                                "🌐 GORC Direct: Player {} attacked dealing {} damage",
                                attack_data.player_id, damage
                            );

                            // Emit combat event to other players within 100m
                            let instance_id = instance.object_id;
                            if let Err(e) = tokio_handle.block_on(events_clone.emit_gorc_instance(instance_id, 1, "combat_event", &gorc_event, horizon_event_system::Dest::Client)) {
                                tracing::warn!("🌐 GORC: Failed to emit combat event: {}", e);
                            }
                        }
                    }
                }
                Ok(())
            }
        ).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Channel 2 (Social): Chat and emotes - 200m range, 15Hz
        let events_clone_chat = Arc::clone(&events);
        let tokio_handle_chat = tokio_handle.clone();
        events.on_gorc_instance("GorcPlayer", 2, "chat",
            move |gorc_event: GorcEvent, instance: &mut ObjectInstance| {
                let events_clone = Arc::clone(&events_clone_chat);
                let tokio_handle = tokio_handle_chat.clone();
                
                if let Some(player) = instance.get_object_mut::<GorcPlayer>() {
                    if let Ok(chat_data) = serde_json::from_slice::<PlayerChatRequest>(&gorc_event.data) {
                        // Validate and update chat bubble
                        if !chat_data.message.trim().is_empty() && chat_data.message.len() <= 500 {
                            player.set_chat_bubble(chat_data.message.clone());
                            
                            tracing::info!(
                                "🌐 GORC Direct: Player {} says '{}'",
                                chat_data.player_id, chat_data.message
                            );

                            // Emit chat message to other players within 200m
                            let instance_id = instance.object_id;
                            if let Err(e) = tokio_handle.block_on(events_clone.emit_gorc_instance(instance_id, 2, "chat_message", &gorc_event, horizon_event_system::Dest::Client)) {
                                tracing::warn!("🌐 GORC: Failed to emit chat event: {}", e);
                            }
                        }
                    }
                }
                Ok(())
            }
        ).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Channel 3 (Metadata): Level/guild updates - 1000m range, 2Hz
        events.on_gorc_instance("GorcPlayer", 3, "level_up",
            |gorc_event: GorcEvent, instance: &mut ObjectInstance| {
                if let Some(player) = instance.get_object_mut::<GorcPlayer>() {
                    if let Ok(level_data) = serde_json::from_slice::<serde_json::Value>(&gorc_event.data) {
                        if let Some(new_level) = level_data.get("level").and_then(|l| l.as_u64()) {
                            player.level = new_level as u32;
                            player.last_update = Utc::now();
                            
                            tracing::info!(
                                "🌐 GORC Direct: Player {} leveled up to {} - auto-replicating to metadata zone (1000m)",
                                player.player_id, new_level
                            );

                            // GORC detects the level change and automatically:
                            // 1. Finds all clients within 1000m (strategic awareness)
                            // 2. Creates highly-compressed metadata updates  
                            // 3. Sends at 2Hz to each relevant client (low frequency)
                            // 4. Clients update player nameplates and strategic info
                        }
                    }
                }
                Ok(())
            }
        ).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        tracing::info!("🎮 PlayerPlugin: ✅ Direct GORC client event processing active!");
        tracing::info!(
            "📡 Client Event Flow:\n\
             • Client → GORC Instance Handler (direct)\n\
             • Server validates & updates object\n\
             • GORC → Clients (automatic zone-based replication)\n\
             • Zero manual networking code required!"
        );

        Ok(())
    }

    async fn on_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            "🎮 PlayerPlugin: Direct GORC client processing online",
        );

        tracing::info!(
            "🌐 Direct GORC Architecture:\n\
             • Clients send GORC-formatted events directly\n\
             • No client event bridging layer needed\n\
             • Automatic zone-based replication\n\
             • 4-channel proximity filtering built-in\n\
             • Minimal code, maximum performance"
        );

        Ok(())
    }

    async fn on_shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            &format!(
                "🎮 PlayerPlugin: Direct GORC shutdown - {} players",
                self.players.len()
            ),
        );

        self.players.clear();
        tracing::info!("🎮 PlayerPlugin: ✅ Direct GORC shutdown complete");
        Ok(())
    }
}

create_simple_plugin!(PlayerPlugin);