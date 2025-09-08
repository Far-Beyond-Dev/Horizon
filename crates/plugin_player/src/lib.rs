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
                        if let Ok(handle) = tokio::runtime::Handle::try_current() {
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
                                
                                println!("🎮 GORC: ✅ Player {} registered with REAL GORC ID {:?} at position {:?}", 
                                    event.player_id, gorc_id, spawn_position);
                                    
                                // Test: Try to emit a GORC event immediately
                                println!("🎮 GORC: Testing immediate GORC event emission for player {}", event.player_id);
                                #[derive(Debug, serde::Serialize, serde::Deserialize)]
                                struct TestEvent {
                                    message: String,
                                    player_id: horizon_event_system::PlayerId,
                                }
                                let test_event = TestEvent {
                                    message: "Hello from GORC!".to_string(),
                                    player_id: event.player_id,
                                };
                                
                                if let Err(e) = events_clone.emit_gorc_instance(gorc_id, 0, "test_event", &test_event, horizon_event_system::Dest::Both).await {
                                    println!("🎮 GORC: ❌ Failed to emit test GORC event: {}", e);
                                } else {
                                    println!("🎮 GORC: ✅ Test GORC event emitted successfully");
                                }
                            });
                        } else {
                            println!("🎮 GORC: ❌ Failed to get tokio runtime handle");
                        }
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
