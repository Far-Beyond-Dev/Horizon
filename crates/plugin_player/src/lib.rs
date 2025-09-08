use async_trait::async_trait;
use horizon_event_system::{
    create_simple_plugin, EventSystem, LogLevel, PlayerId, PluginError, 
    ServerContext, SimplePlugin, Vec3, GorcObjectId,
    PlayerConnectedEvent, PlayerDisconnectedEvent,
};
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

        // Register player connection handler exactly like LoggerPlugin
        let players_conn = Arc::clone(&self.players);
        let events_for_conn = Arc::clone(&events);
        let context_for_conn = Arc::clone(&context);
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
                        
                        println!("🎮 GORC: Created GorcPlayer: {:?}", player);
                        
                        // Use async block to handle the real GORC registration
                        let players_conn_clone = players_conn.clone();
                        let events_clone = Arc::clone(&events_for_conn);
                        let context_clone = Arc::clone(&context_for_conn);
                        let handle = context_clone.tokio_handle();
                        handle.block_on(async move {
                            println!("🎮 GORC: About to register player {} with GORC instances", event.player_id);

                            // Register the player object with GORC
                            let gorc_id = gorc_instances.register_object(player, spawn_position).await;
                            println!("🎮 GORC: register_object returned GORC ID: {:?}", gorc_id);
                            
                            // Also add player to GORC position tracking  
                            gorc_instances.add_player(event.player_id, spawn_position).await;
                            println!("🎮 GORC: add_player completed for player {}", event.player_id);
                            
                            // Store the GORC ID for cleanup later
                            players_conn_clone.insert(event.player_id, gorc_id);
                            
                            println!("🎮 GORC: ✅ Player {} registered with REAL GORC instance ID {:?} at position {:?}", 
                                event.player_id, gorc_id, spawn_position);

                            // GORC will automatically handle zone entry messages when players move
                            println!("🎮 GORC: ✅ Player registered - GORC system will send zone messages as needed");
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
        let players_gorc = Arc::clone(&self.players);
        let events_for_gorc = Arc::clone(&events);
        
        // Handle GORC movement events (channel 0)
        events
            .on_gorc_instance(
                "GorcPlayer",
                0,
                "move",
                move |gorc_event: horizon_event_system::GorcEvent, object_instance: &mut horizon_event_system::ObjectInstance| {
                    println!("🎮 GORC: Received movement event for instance {}: {:?}", gorc_event.object_id, gorc_event);
                    
                    // Parse movement data from the GORC event
                    if let Ok(event_data) = serde_json::from_slice::<serde_json::Value>(&gorc_event.data) {
                        if let Ok(move_data) = serde_json::from_value::<events::PlayerMoveRequest>(event_data) {
                            println!("🎮 GORC: Processing movement for player {} to position {:?}", move_data.player_id, move_data.new_position);
                            
                            // Update the object instance position directly
                            object_instance.object.update_position(move_data.new_position);
                            println!("🎮 GORC: ✅ Updated position for instance {} to {:?}", gorc_event.object_id, move_data.new_position);
                        } else {
                            println!("🎮 GORC: ❌ Failed to parse PlayerMoveRequest from event data");
                        }
                    } else {
                        println!("🎮 GORC: ❌ Failed to parse JSON from GORC event data");
                    }
                    
                    Ok(())
                },
            )
            .await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Handle GORC chat events (channel 2)  
        let events_for_chat = Arc::clone(&events);
        events
            .on_gorc_instance(
                "GorcPlayer", 
                2,
                "chat",
                move |gorc_event: horizon_event_system::GorcEvent, _object_instance: &mut horizon_event_system::ObjectInstance| {
                    println!("🎮 GORC: Received chat event for instance {}: {:?}", gorc_event.object_id, gorc_event);
                    
                    // Parse chat data from GORC event
                    if let Ok(event_data) = serde_json::from_slice::<serde_json::Value>(&gorc_event.data) {
                        if let Ok(chat_data) = serde_json::from_value::<events::PlayerChatRequest>(event_data) {
                            println!("🎮 GORC: Player {} says: '{}'", chat_data.player_id, chat_data.message);
                            
                            // Create chat response for nearby players
                            let chat_response = serde_json::json!({
                                "player_id": chat_data.player_id,
                                "message": chat_data.message,
                                "timestamp": chrono::Utc::now()
                            });
                            
                            // Broadcast to nearby players via GORC (using Client destination for replication)
                            let events_clone = events_for_chat.clone();
                            let object_id_str = gorc_event.object_id.clone(); 
                            tokio::spawn(async move {
                                if let Ok(gorc_id) = horizon_event_system::GorcObjectId::from_str(&object_id_str) {
                                    if let Err(e) = events_clone.emit_gorc_instance(gorc_id, 2, "chat_message", &chat_response, horizon_event_system::Dest::Client).await {
                                        println!("🎮 GORC: ❌ Failed to broadcast chat: {}", e);
                                    } else {
                                        println!("🎮 GORC: ✅ Broadcasted chat from player {}", chat_data.player_id);
                                    }
                                }
                            });
                        } else {
                            println!("🎮 GORC: ❌ Failed to parse PlayerChatRequest from event data");
                        }
                    } else {
                        println!("🎮 GORC: ❌ Failed to parse JSON from GORC event data");
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
