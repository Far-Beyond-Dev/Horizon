use async_trait::async_trait;
use shared_types::*;
use std::collections::HashMap;

/// Horizon plugin - example plugin that manages player horizons and visibility
/// 
/// This plugin demonstrates the callback-based event system by registering
/// for specific game events and handling them through direct callbacks.
pub struct HorizonPlugin {
    name: &'static str,
    version: &'static str,
    visibility_ranges: HashMap<PlayerId, f64>,
    default_horizon_distance: f64,
    initialized: bool,
}

impl HorizonPlugin {
    pub fn new() -> Self {
        Self {
            name: "horizon",
            version: "1.0.0", 
            visibility_ranges: HashMap::new(),
            default_horizon_distance: 100.0,
            initialized: false,
        }
    }
    
    /// Update player visibility based on horizon rules
    async fn update_player_visibility(&mut self, player_id: PlayerId, context: &dyn ServerContext) -> Result<(), PluginError> {
        let players = context.get_players().await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to get players: {}", e)))?;
        
        let current_player = players.iter()
            .find(|p| p.id == player_id)
            .ok_or_else(|| PluginError::ExecutionError("Player not found".to_string()))?;
        
        let horizon_distance = self.visibility_ranges
            .get(&player_id)
            .copied()
            .unwrap_or(self.default_horizon_distance);
        
        // Find players within horizon distance
        let mut visible_players = Vec::new();
        for other_player in &players {
            if other_player.id != player_id {
                let distance = current_player.position.distance_to(&other_player.position);
                if distance <= horizon_distance {
                    visible_players.push(other_player.clone());
                }
            }
        }
        let visible_players_count = visible_players.len();
        
        // Create visibility update event
        let visibility_event = HorizonEvent::VisibilityUpdated {
            player_id,
            visible_players,
            horizon_distance,
        };
        
        // Emit the event through callback system
        let namespace = EventNamespace::plugin_default(self.name);
        context.emit_event(namespace, Box::new(visibility_event)).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to emit event through callback system: {}", e)))?;
        
        context.log(LogLevel::Debug, &format!("Updated visibility for player {} via callback system: {} visible players", player_id, visible_players_count));
        
        Ok(())
    }
    
    /// Handle player horizon distance changes
    async fn handle_horizon_change(&mut self, player_id: PlayerId, new_distance: f64, context: &dyn ServerContext) -> Result<(), PluginError> {
        // Validate horizon distance
        let clamped_distance = new_distance.clamp(10.0, 500.0);
        if clamped_distance != new_distance {
            context.log(LogLevel::Warn, &format!("Clamped horizon distance for player {} from {} to {}", player_id, new_distance, clamped_distance));
        }
        
        self.visibility_ranges.insert(player_id, clamped_distance);
        
        // Update visibility immediately
        self.update_player_visibility(player_id, context).await?;
        
        // Emit horizon changed event
        let horizon_event = HorizonEvent::HorizonDistanceChanged {
            player_id,
            old_distance: new_distance,
            new_distance: clamped_distance,
        };
        
        let namespace = EventNamespace::plugin_default(self.name);
        context.emit_event(namespace, Box::new(horizon_event)).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to emit horizon change event through callback system: {}", e)))?;
        
        Ok(())
    }
}

impl Default for HorizonPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for HorizonPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    
    fn version(&self) -> &'static str {
        self.version
    }
    
    async fn initialize(&mut self, context: &dyn ServerContext) -> Result<(), PluginError> {
        if self.initialized {
            return Err(PluginError::InitializationFailed("Plugin already initialized".to_string()));
        }
        
        context.log(LogLevel::Info, &format!("Initializing Horizon plugin v{} with callback-based events", self.version));
        println!("Horizon plugin initializing with callback-based event system");
        println!("Default horizon distance: {}", self.default_horizon_distance);

        // Emit initialization event through callback system
        let init_event = HorizonEvent::PluginInitialized {
            default_horizon_distance: self.default_horizon_distance,
        };

        println!("Emitting plugin initialized event through callback system");
        
        let namespace = EventNamespace::plugin_default(self.name);
        context.emit_event(namespace, Box::new(init_event)).await
            .map_err(|e| PluginError::InitializationFailed(format!("Failed to emit init event through callback system: {}", e)))?;
        
        context.log(LogLevel::Info, "Horizon plugin initialization complete using callback-based events");

        self.initialized = true;
        context.log(LogLevel::Info, "Horizon plugin initialized successfully with callback dispatch");

        println!("Horizon plugin initialized successfully with callback-based event system");

        Ok(())
    }
    
    /// Handle events through callback-based dispatch
    /// 
    /// This method is called directly by the event processor when events
    /// that this plugin has subscribed to are emitted. No polling required!
    async fn handle_event(&mut self, event_id: &EventId, event: &dyn GameEvent, context: &dyn ServerContext) -> Result<(), PluginError> {
        if !self.initialized {
            return Err(PluginError::ExecutionError("Plugin not initialized".to_string()));
        }
        
        context.log(LogLevel::Debug, &format!("Horizon plugin handling event {} via callback dispatch", event_id));
        
        // Handle core events
        if event_id.namespace.0 == "core" {
            if let Some(core_event) = event.as_any().downcast_ref::<CoreEvent>() {
                match core_event {
                    CoreEvent::PlayerJoined { player } => {
                        context.log(LogLevel::Info, &format!("Player {} joined (callback dispatch), setting up horizon", player.name));
                        self.visibility_ranges.insert(player.id, self.default_horizon_distance);
                        self.update_player_visibility(player.id, context).await?;
                    }
                    
                    CoreEvent::PlayerLeft { player_id } => {
                        context.log(LogLevel::Info, &format!("Player {} left (callback dispatch), cleaning up horizon data", player_id));
                        self.visibility_ranges.remove(player_id);
                    }
                    
                    CoreEvent::PlayerMoved { player_id, old_position: _, new_position: _ } => {
                        // Update visibility when player moves (called via callback)
                        context.log(LogLevel::Debug, &format!("Player {} moved (callback dispatch), updating visibility", player_id));
                        self.update_player_visibility(*player_id, context).await?;
                    }
                    
                    CoreEvent::CustomMessage { data } => {
                        // Check if this is a horizon-related message
                        if let Ok(horizon_msg) = serde_json::from_value::<HorizonMessage>(data.clone()) {
                            context.log(LogLevel::Debug, "Processing horizon message via callback dispatch");
                            match horizon_msg {
                                HorizonMessage::SetHorizonDistance { player_id, distance } => {
                                    context.log(LogLevel::Info, &format!("Setting horizon distance {} for player {} (callback dispatch)", distance, player_id));
                                    self.handle_horizon_change(player_id, distance, context).await?;
                                }
                                HorizonMessage::GetHorizonInfo { player_id } => {
                                    let current_distance = self.visibility_ranges
                                        .get(&player_id)
                                        .copied()
                                        .unwrap_or(self.default_horizon_distance);
                                    
                                    context.log(LogLevel::Info, &format!("Getting horizon info for player {} (callback dispatch): distance {}", player_id, current_distance));
                                    
                                    let info_event = HorizonEvent::HorizonInfo {
                                        player_id,
                                        current_distance,
                                        max_distance: 500.0,
                                        min_distance: 10.0,
                                    };
                                    
                                    let namespace = EventNamespace::plugin_default(self.name);
                                    context.emit_event(namespace, Box::new(info_event)).await
                                        .map_err(|e| PluginError::ExecutionError(format!("Failed to emit info event through callback system: {}", e)))?;
                                }
                            }
                        }
                    }
                    
                    CoreEvent::RegionChanged { region_id } => {
                        context.log(LogLevel::Info, &format!("Region changed to {:?} (callback dispatch), recalculating all horizons", region_id));
                        // Recalculate visibility for all players
                        let player_ids: Vec<PlayerId> = self.visibility_ranges.keys().copied().collect();
                        for player_id in player_ids {
                            self.update_player_visibility(player_id, context).await?;
                        }
                    }
                }
            }
        }
        
        // Handle plugin-specific events (also via callback dispatch)
        if event_id.namespace == EventNamespace::plugin_default(self.name) {
            if let Some(horizon_event) = event.as_any().downcast_ref::<HorizonEvent>() {
                match horizon_event {
                    HorizonEvent::VisibilityUpdated { player_id, visible_players, horizon_distance } => {
                        context.log(LogLevel::Debug, &format!(
                            "Visibility updated for {} via callback dispatch: {} players visible within {} units", 
                            player_id, visible_players.len(), horizon_distance
                        ));
                    }
                    HorizonEvent::PluginInitialized { default_horizon_distance } => {
                        context.log(LogLevel::Info, &format!("Horizon plugin initialization event processed via callback (default distance: {})", default_horizon_distance));
                    }
                    _ => {
                        context.log(LogLevel::Debug, "Other horizon event processed via callback dispatch");
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Return events this plugin wants to receive via callback dispatch
    /// 
    /// The event processor will register callbacks for these events,
    /// eliminating the need for polling.
    fn subscribed_events(&self) -> Vec<EventId> {
        vec![
            // Core events - will be delivered via callback dispatch
            EventId::new(EventNamespace::new("core"), "player_joined"),
            EventId::new(EventNamespace::new("core"), "player_left"),
            EventId::new(EventNamespace::new("core"), "player_moved"),
            EventId::new(EventNamespace::new("core"), "region_changed"),
            EventId::new(EventNamespace::new("core"), "custom_message"),
            
            // Plugin-specific events - also delivered via callback dispatch
            EventId::new(EventNamespace::plugin_default(self.name), "visibility_updated"),
            EventId::new(EventNamespace::plugin_default(self.name), "horizon_distance_changed"),
            EventId::new(EventNamespace::plugin_default(self.name), "plugin_initialized"),
        ]
    }
    
    async fn shutdown(&mut self, context: &dyn ServerContext) -> Result<(), PluginError> {
        if !self.initialized {
            return Ok(()); // Already shut down
        }
        
        context.log(LogLevel::Info, "Shutting down Horizon plugin (callback-based events)");
        
        // Clear all data
        self.visibility_ranges.clear();
        self.initialized = false;
        
        // Emit shutdown event through callback system
        let shutdown_event = HorizonEvent::PluginShutdown;
        let namespace = EventNamespace::plugin_default(self.name);
        context.emit_event(namespace, Box::new(shutdown_event)).await
            .map_err(|e| PluginError::ExecutionError(format!("Failed to emit shutdown event through callback system: {}", e)))?;
        
        context.log(LogLevel::Info, "Horizon plugin shutdown complete (callbacks will be automatically unregistered)");
        
        Ok(())
    }
}

/// Events specific to the Horizon plugin
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum HorizonEvent {
    PluginInitialized { default_horizon_distance: f64 },
    PluginShutdown,
    VisibilityUpdated { 
        player_id: PlayerId, 
        visible_players: Vec<Player>,
        horizon_distance: f64,
    },
    HorizonDistanceChanged {
        player_id: PlayerId,
        old_distance: f64,
        new_distance: f64,
    },
    HorizonInfo {
        player_id: PlayerId,
        current_distance: f64,
        max_distance: f64,
        min_distance: f64,
    },
}

impl GameEvent for HorizonEvent {
    fn event_type(&self) -> &'static str {
        match self {
            HorizonEvent::PluginInitialized { .. } => "plugin_initialized",
            HorizonEvent::PluginShutdown => "plugin_shutdown",
            HorizonEvent::VisibilityUpdated { .. } => "visibility_updated",
            HorizonEvent::HorizonDistanceChanged { .. } => "horizon_distance_changed",
            HorizonEvent::HorizonInfo { .. } => "horizon_info",
        }
    }
    
    fn serialize(&self) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(serde_json::to_vec(self)?)
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Messages that can be sent to the Horizon plugin
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum HorizonMessage {
    SetHorizonDistance { player_id: PlayerId, distance: f64 },
    GetHorizonInfo { player_id: PlayerId },
}

/// CORRECT FFI EXPORTS - These return trait objects directly for safe Rust-to-Rust FFI

/// Required export for plugin creation - returns *mut dyn Plugin directly
#[no_mangle]
pub unsafe extern "C" fn create_plugin() -> *mut dyn Plugin {
    let plugin = Box::new(HorizonPlugin::new());
    // Convert concrete plugin to trait object
    let trait_object: Box<dyn Plugin> = plugin;
    // Return the raw pointer to the trait object
    Box::into_raw(trait_object)
}

/// Required export for plugin destruction - takes *mut dyn Plugin directly
#[no_mangle]
pub unsafe extern "C" fn destroy_plugin(plugin: *mut dyn Plugin) {
    if !plugin.is_null() {
        // Convert back to Box and let it drop
        let _ = Box::from_raw(plugin);
    }
}