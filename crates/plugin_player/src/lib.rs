use async_trait::async_trait;
use horizon_event_system::{
    create_simple_plugin, EventSystem, LogLevel, PlayerId, PluginError, 
    ServerContext, SimplePlugin, Vec3, GorcObjectId,
    PlayerConnectedEvent, PlayerDisconnectedEvent,
};
use tracing::{debug, error};
use std::sync::Arc;
use dashmap::DashMap;

pub mod player;
pub mod events;

use player::GorcPlayer;


/// Pure GORC Player Plugin - Direct Client Event Processing
pub struct PlayerPlugin {
    name: String,
    /// Player registry: PlayerId → GorcObjectId  
    players: Arc<DashMap<PlayerId, GorcObjectId>>,
}

impl PlayerPlugin {
    pub fn new() -> Self {
        println!("🎮 PlayerPlugin: Creating new instance with GORC architecture");
        Self {
            name: "PlayerPlugin".to_string(),
            players: Arc::new(DashMap::new()),
        }
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

    async fn register_handlers(
        &mut self,
        events: Arc<EventSystem>,
        context: Arc<dyn ServerContext>,
    ) -> Result<(), PluginError> {
        println!("🎮 DEBUG: PlayerPlugin register_handlers method called!");
        context.log(
            LogLevel::Info,
            "🎮 PlayerPlugin: Registering GORC player event handlers...",
        );
        let luminal_handle = context.luminal_handle();

        // Register player connection handler as async to avoid deadlock
        let players_conn = Arc::clone(&self.players);
        let events_for_conn = Arc::clone(&events);
        let luminal_handle_player_connect = luminal_handle.clone();
        events
            .on_core(
                "player_connected",
                move |event: PlayerConnectedEvent| {
                    println!("🎮 GORC: PlayerPlugin received player_connected event: {:?}", event);
                    
                    let spawn_position = Vec3::new(0.0, 0.0, 0.0);
                    
                    // Check if GORC instances manager is available
                    if let Some(gorc_instances) = events_for_conn.get_gorc_instances() {
                        println!("🎮 GORC: ✅ GORC instances manager available, registering player {}", event.player_id);
                        
                        // Create real GORC player object
                        let player = GorcPlayer::new(
                            event.player_id, 
                            format!("Player_{}", event.player_id), 
                            spawn_position
                        );
                        
                        // Use tokio::spawn to handle async operations without blocking
                        let players_conn_clone = players_conn.clone();
                        let events_clone = Arc::clone(&events_for_conn);
                        
                        println!("🎮 GORC: Spawning async task to register player {} with GORC", event.player_id);
                        luminal_handle_player_connect.spawn(async move {
                            println!("🎮 GORC: Async task started for player {}", event.player_id);
                            // Register the player object with GORC
                            let gorc_id = gorc_instances.register_object(player, spawn_position).await;
                            
                            // Store the GORC ID for cleanup later
                            players_conn_clone.insert(event.player_id, gorc_id);
                            
                            println!("🎮 GORC: ✅ Player {} registered with GORC instance ID {:?} at position {:?}", 
                                event.player_id, gorc_id, spawn_position);

                            // CRITICAL FIX: Use EventSystem's update_player_position to trigger zone messages
                            if let Err(e) = events_clone.update_player_position(event.player_id, spawn_position).await {
                                println!("🎮 GORC: ❌ Failed to update player position via EventSystem: {}", e);
                            } else {
                                println!("🎮 GORC: ✅ EventSystem.update_player_position completed successfully");
                            }
                            
                            // Add player to GORC position tracking AFTER zone messages are sent
                            gorc_instances.add_player(event.player_id, spawn_position).await;
                        });
                    } else {
                        println!("🎮 GORC: ❌ No GORC instances manager available for player {}", event.player_id);
                    }
                    
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        let players_disc = Arc::clone(&self.players);
        events
            .on_core(
                "player_disconnected",
                move |event: PlayerDisconnectedEvent| {
                    println!("🎮 GORC: PlayerPlugin received player_disconnected event: {:?}", event);
                    
                    if let Some((_, gorc_id)) = players_disc.remove(&event.player_id) {
                        println!("🎮 GORC: Player {} disconnected and unregistered (ID {:?})", event.player_id, gorc_id);
                    }
                    
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Register GORC event handlers to process incoming client GORC events
        
        // Handle GORC client movement events (channel 0)  
        // This processes client requests to move their ship
        let events_for_move = Arc::clone(&events);
        events
            .on_gorc_client(
                luminal_handle.clone(),
                "GorcPlayer",
                0,
                "move", 
                move |gorc_event: horizon_event_system::GorcEvent, client_player: horizon_event_system::PlayerId, connection: horizon_event_system::ClientConnectionRef, object_instance: &mut horizon_event_system::ObjectInstance| {
                    // Validate connection authentication
                    if !connection.is_authenticated() {
                        error!("🚀 GORC: ❌ Unauthenticated movement request from {}", connection.remote_addr);
                        return Err(horizon_event_system::EventError::HandlerExecution("Unauthenticated request".to_string()));
                    }
                    
                    // Parse movement data from the GORC event
                    if let Ok(event_data) = serde_json::from_slice::<serde_json::Value>(&gorc_event.data) {
                        if let Ok(move_data) = serde_json::from_value::<events::PlayerMoveRequest>(event_data) {
                            debug!("🚀 GORC: Processing movement for ship {} to position {:?}", move_data.player_id, move_data.new_position);
                            
                            // Validate the player owns this ship
                            if move_data.player_id != client_player {
                                error!("🚀 GORC: ❌ Security violation: Player {} tried to move ship belonging to {}", client_player, move_data.player_id);
                                return Err(horizon_event_system::EventError::HandlerExecution("Unauthorized ship movement".to_string()));
                            }
                            
                            // Update the object instance position directly
                            object_instance.object.update_position(move_data.new_position);
                            debug!("🚀 GORC: ✅ Updated ship position for {} to {:?}", client_player, move_data.new_position);
                            
                            // Emit position update to nearby players via GORC instance event
                            let events_clone = events_for_move.clone();
                            let object_id_str = gorc_event.object_id.clone();
                            let position_update = serde_json::json!({
                                "player_id": client_player,
                                "position": move_data.new_position,
                                "velocity": move_data.velocity,
                                "movement_state": move_data.movement_state,
                                "timestamp": chrono::Utc::now()
                            });
                            
                            tokio::spawn(async move {
                                if let Ok(gorc_id) = horizon_event_system::GorcObjectId::from_str(&object_id_str) {
                                    if let Err(e) = events_clone.emit_gorc_instance(gorc_id, 0, "position_update", &position_update, horizon_event_system::Dest::Client).await {
                                        error!("🚀 GORC: ❌ Failed to broadcast position update: {}", e);
                                    } else {
                                        debug!("🚀 GORC: ✅ Broadcasted position update for ship {}", client_player);
                                    }
                                }
                            });
                        } else {
                            error!("🚀 GORC: ❌ Failed to parse PlayerMoveRequest from event data");
                        }
                    } else {
                        error!("🚀 GORC: ❌ Failed to parse JSON from GORC event data");
                    }
                    
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Handle GORC client combat events (channel 1) - Weapon fire
        let events_for_combat = Arc::clone(&events);
        events
            .on_gorc_client(
                luminal_handle.clone(),
                "GorcPlayer",
                1,
                "attack",
                move |gorc_event: horizon_event_system::GorcEvent, client_player: horizon_event_system::PlayerId, _connection: horizon_event_system::ClientConnectionRef, _object_instance: &mut horizon_event_system::ObjectInstance| {
                    debug!("⚡ GORC: Received client combat request from ship {}: {:?}", client_player, gorc_event);
                    
                    // Parse attack data from GORC event
                    if let Ok(event_data) = serde_json::from_slice::<serde_json::Value>(&gorc_event.data) {
                        if let Ok(attack_data) = serde_json::from_value::<events::PlayerAttackRequest>(event_data) {
                            debug!("⚡ GORC: Ship {} fires {} at {:?}", attack_data.player_id, attack_data.attack_type, attack_data.target_position);
                            
                            // Validate the player owns this ship
                            if attack_data.player_id != client_player {
                                error!("⚡ GORC: ❌ Security violation: Player {} tried to fire weapons as {}", client_player, attack_data.player_id);
                                return Err(horizon_event_system::EventError::HandlerExecution("Unauthorized weapon fire".to_string()));
                            }
                            
                            // Create weapon fire broadcast for replication to nearby ships (500m range)
                            let weapon_fire = serde_json::json!({
                                "attacker_player": attack_data.player_id,
                                "weapon_type": attack_data.attack_type,
                                "target_position": attack_data.target_position,
                                "fire_timestamp": chrono::Utc::now()
                            });
                            
                            // Emit as gorc_instance event - this will replicate to nearby ships (500m range)
                            let events_clone = events_for_combat.clone();
                            let object_id_str = gorc_event.object_id.clone();
                            tokio::spawn(async move {
                                if let Ok(gorc_id) = horizon_event_system::GorcObjectId::from_str(&object_id_str) {
                                    if let Err(e) = events_clone.emit_gorc_instance(gorc_id, 1, "weapon_fire", &weapon_fire, horizon_event_system::Dest::Client).await {
                                        error!("⚡ GORC: ❌ Failed to broadcast weapon fire: {}", e);
                                    } else {
                                        debug!("⚡ GORC: ✅ Broadcasting weapon fire from ship {} (auto-replicated to ships within 500m)", attack_data.player_id);
                                    }
                                }
                            });
                        } else {
                            error!("⚡ GORC: ❌ Failed to parse PlayerAttackRequest from event data");
                        }
                    } else {
                        error!("⚡ GORC: ❌ Failed to parse JSON from GORC combat event data");
                    }
                    
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Handle GORC client chat events (channel 2) - Space communications
        let luminal_handle_chat = context.luminal_handle();
        let events_for_chat = Arc::clone(&events);
        events
            .on_gorc_client(
                luminal_handle.clone(),
                "GorcPlayer", 
                2,
                "chat",
                move |gorc_event: horizon_event_system::GorcEvent, client_player: horizon_event_system::PlayerId, _connection: horizon_event_system::ClientConnectionRef, _object_instance: &mut horizon_event_system::ObjectInstance| {
                    println!("📡 GORC: Received client communication request from ship {}: {:?}", client_player, gorc_event);
                    
                    // Parse chat data from GORC event
                    if let Ok(event_data) = serde_json::from_slice::<serde_json::Value>(&gorc_event.data) {
                        if let Ok(chat_data) = serde_json::from_value::<events::PlayerChatRequest>(event_data) {
                            println!("📡 GORC: Ship {} requests to transmit: '{}'", chat_data.player_id, chat_data.message);
                            
                            // Validate the player owns this ship
                            if chat_data.player_id != client_player {
                                println!("📡 GORC: ❌ Security violation: Player {} tried to send message as {}", client_player, chat_data.player_id);
                                return Err(horizon_event_system::EventError::HandlerExecution("Unauthorized communication".to_string()));
                            }
                            
                            // Create communication for replication to nearby ships (300m range)
                            let chat_broadcast = serde_json::json!({
                                "sender_player": chat_data.player_id,
                                "message": chat_data.message,
                                "channel": chat_data.channel,
                                "timestamp": chrono::Utc::now()
                            });
                            
                            // Emit as gorc_instance event - this will replicate to nearby clients
                            let events_clone = events_for_chat.clone();
                            let object_id_str = gorc_event.object_id.clone(); 
                            luminal_handle_chat.spawn(async move {
                                if let Ok(gorc_id) = horizon_event_system::GorcObjectId::from_str(&object_id_str) {
                                    if let Err(e) = events_clone.emit_gorc_instance(gorc_id, 2, "space_communication", &chat_broadcast, horizon_event_system::Dest::Client).await {
                                        println!("📡 GORC: ❌ Failed to broadcast communication: {}", e);
                                    } else {
                                        println!("📡 GORC: ✅ Broadcasting communication from ship {} (auto-replicated to nearby ships)", chat_data.player_id);
                                    }
                                }
                            });
                        } else {
                            println!("📡 GORC: ❌ Failed to parse PlayerChatRequest from event data");
                        }
                    } else {
                        println!("📡 GORC: ❌ Failed to parse JSON from GORC event data");
                    }
                    
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Handle GORC client ship scan events (channel 3) - Detailed metadata
        let luminal_handle_scan = context.luminal_handle();
        let events_for_scan = Arc::clone(&events);
        events
            .on_gorc_client(
                luminal_handle.clone(),
                "GorcPlayer",
                3,
                "ship_scan",
                move |gorc_event: horizon_event_system::GorcEvent, client_player: horizon_event_system::PlayerId, _connection: horizon_event_system::ClientConnectionRef, _object_instance: &mut horizon_event_system::ObjectInstance| {
                    println!("🔍 GORC: Received client ship scan request from {}: {:?}", client_player, gorc_event);
                    
                    // Parse scan data from GORC event
                    if let Ok(event_data) = serde_json::from_slice::<serde_json::Value>(&gorc_event.data) {
                        if let Some(player_id) = event_data.get("player_id") {
                            println!("🔍 GORC: Ship {} requesting detailed scan", player_id);
                            
                            // Validate the player owns this ship
                            if let Ok(request_player) = serde_json::from_value::<horizon_event_system::PlayerId>(player_id.clone()) {
                                if request_player != client_player {
                                    println!("🔍 GORC: ❌ Security violation: Player {} tried to scan as {}", client_player, request_player);
                                    return Err(horizon_event_system::EventError::HandlerExecution("Unauthorized scan request".to_string()));
                                }
                            }
                            
                            // Extract scan data values  
                            let player_id_value = player_id.clone();
                            let ship_class = event_data.get("ship_class").cloned().unwrap_or(serde_json::Value::String("Unknown".to_string()));
                            let hull_integrity = event_data.get("hull_integrity").cloned().unwrap_or(serde_json::Value::Number(serde_json::Number::from(100)));
                            let shield_strength = event_data.get("shield_strength").cloned().unwrap_or(serde_json::Value::Number(serde_json::Number::from(85)));
                            let cargo_manifest = event_data.get("cargo_manifest").cloned().unwrap_or(serde_json::Value::Array(vec![]));
                            let pilot_level = event_data.get("pilot_level").cloned().unwrap_or(serde_json::Value::Number(serde_json::Number::from(1)));
                            
                            // Create scan broadcast for replication to very close ships (100m range)
                            let scan_broadcast = serde_json::json!({
                                "scanner_ship": player_id_value,
                                "scan_data": {
                                    "ship_class": ship_class,
                                    "hull_integrity": hull_integrity,
                                    "shield_strength": shield_strength,
                                    "cargo_manifest": cargo_manifest,
                                    "pilot_level": pilot_level
                                },
                                "scan_timestamp": chrono::Utc::now()
                            });
                            
                            // Emit as gorc_instance event - this will replicate to very close ships (100m range)
                            let events_clone = events_for_scan.clone();
                            let object_id_str = gorc_event.object_id.clone();
                            luminal_handle_scan.spawn(async move {
                                if let Ok(gorc_id) = horizon_event_system::GorcObjectId::from_str(&object_id_str) {
                                    if let Err(e) = events_clone.emit_gorc_instance(gorc_id, 3, "scan_results", &scan_broadcast, horizon_event_system::Dest::Client).await {
                                        println!("🔍 GORC: ❌ Failed to broadcast scan results: {}", e);
                                    } else {
                                        println!("🔍 GORC: ✅ Broadcasting scan results from ship {} (auto-replicated to ships within 100m)", player_id_value);
                                    }
                                }
                            });
                        } else {
                            println!("🔍 GORC: ❌ Ship scan event missing player_id");
                        }
                    } else {
                        println!("🔍 GORC: ❌ Failed to parse JSON from ship scan event data");
                    }
                    
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        context.log(
            LogLevel::Info,
            "🎮 PlayerPlugin: ✅ GORC player handlers registered successfully!",
        );
        Ok(())
    }

    async fn on_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            "🎮 PlayerPlugin: GORC player system activated!",
        );
        Ok(())
    }

    async fn on_shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            &format!(
                "🎮 PlayerPlugin: Shutting down. Managed {} players",
                self.players.len()
            ),
        );
        
        self.players.clear();
        Ok(())
    }
}

// Create the plugin using our macro - zero unsafe code!
create_simple_plugin!(PlayerPlugin);
