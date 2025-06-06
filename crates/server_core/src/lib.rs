use async_trait::async_trait;
use dashmap::DashMap;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use libloading::{Library, Symbol};
use shared_types::*;
use std::ops::Deref;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tracing::{error, info, warn, debug};

type WsStream = WebSocketStream<TcpStream>;
type WsSink = SplitSink<WsStream, Message>;
type WsReceiver = SplitStream<WsStream>;

/// Main game server that manages plugins, players, and events
pub struct GameServer {
    region_id: RegionId,
    region_bounds: RegionBounds,
    players: Arc<DashMap<PlayerId, Player>>,
    connections: Arc<DashMap<PlayerId, WsSink>>,
    plugins: Arc<RwLock<Vec<Arc<RwLock<Box<dyn Plugin>>>>>>,
    plugin_libraries: Vec<Library>,
    event_subscriptions: Arc<DashMap<EventId, Vec<usize>>>, // plugin index
    event_sender: broadcast::Sender<(EventId, Arc<dyn GameEvent + Send + Sync>)>,
    shutdown_signal: tokio::sync::watch::Sender<bool>,
}

impl GameServer {
    /// Create a new game server for the specified region
    pub fn new(region_bounds: RegionBounds) -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        let (shutdown_signal, _) = tokio::sync::watch::channel(false);
        
        Self {
            region_id: RegionId::new(),
            region_bounds,
            players: Arc::new(DashMap::new()),
            connections: Arc::new(DashMap::new()),
            plugins: Arc::new(RwLock::new(Vec::new())),
            plugin_libraries: Vec::new(),
            event_subscriptions: Arc::new(DashMap::new()),
            event_sender,
            shutdown_signal,
        }
    }
    
    /// Load a plugin from a dynamic library
    pub async fn load_plugin(&mut self, library_path: impl AsRef<Path>) -> Result<(), ServerError> {
        let lib_path = library_path.as_ref();
        info!("Loading plugin from: {}", lib_path.display());
        
        unsafe {
            let lib = Library::new(lib_path)
                .map_err(|e| ServerError::Plugin(PluginError::InitializationFailed(
                    format!("Failed to load library: {}", e)
                )))?;
            
            let create_plugin: Symbol<PluginCreateFn> = lib.get(b"create_plugin")
                .map_err(|e| ServerError::Plugin(PluginError::InitializationFailed(
                    format!("Failed to find create_plugin function: {}", e)
                )))?;
            
            let plugin_ptr = create_plugin();
            if plugin_ptr.is_null() {
                return Err(ServerError::Plugin(PluginError::InitializationFailed(
                    "Plugin creation returned null pointer".to_string()
                )));
            }
            
            let plugin = Box::from_raw(plugin_ptr);
            let plugin_name = plugin.name();
            let plugin_version = plugin.version();
            
            info!("Loaded plugin: {} v{}", plugin_name, plugin_version);
            
            // Get subscribed events before we move the plugin
            let subscribed_events = plugin.subscribed_events();
            
            // Store the plugin
            let plugin_index = {
                let mut plugins = self.plugins.write().await;
                let index = plugins.len();
                plugins.push(Arc::new(RwLock::new(plugin)));
                index
            };
            
            // Register event subscriptions
            for event_id in subscribed_events {
                self.event_subscriptions
                    .entry(event_id)
                    .or_insert_with(Vec::new)
                    .push(plugin_index);
            }
            
            // Initialize the plugin
            {
                let plugins = self.plugins.read().await;
                let plugin_arc = plugins[plugin_index].clone();
                let mut plugin = plugin_arc.write().await;
                let context = ServerContextImpl::new(
                    self.region_id,
                    self.players.clone(),
                    self.connections.clone(),
                    self.event_sender.clone()
                );
                plugin.initialize(&context).await?;
            }
            
            self.plugin_libraries.push(lib);
            
            info!("Successfully initialized plugin: {}", plugin_name);
        }
        
        Ok(())
    }
    
    /// Start the server on the specified address
    pub async fn start(&self, addr: impl Into<SocketAddr>) -> Result<(), ServerError> {
        let addr = addr.into();
        let listener = TcpListener::bind(addr).await
            .map_err(|e| ServerError::Network(format!("Failed to bind to {}: {}", addr, e)))?;
        
        info!("Game server starting on {}", addr);
        info!("Region ID: {:?}", self.region_id);
        info!("Region bounds: {:?}", self.region_bounds);
        
        // Start event processing task
        self.start_event_processor().await;
        
        // Accept connections
        loop {
            let mut shutdown_rx = self.shutdown_signal.subscribe();
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            info!("New connection from: {}", addr);
                            self.handle_connection(stream, addr).await;
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("Shutdown signal received, stopping server");
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    /// Handle a new WebSocket connection
    async fn handle_connection(&self, stream: TcpStream, addr: SocketAddr) {
        let ws_stream = match accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                error!("WebSocket handshake failed for {}: {}", addr, e);
                return;
            }
        };
        
        let (ws_sink, mut ws_stream) = ws_stream.split();
        let player_id = PlayerId::new();
        
        // Store connection
        self.connections.insert(player_id, ws_sink);
        
        let players = self.players.clone();
        let connections = self.connections.clone();
        let event_sender = self.event_sender.clone();
        let region_bounds = self.region_bounds.clone();
        
        // Handle messages from this connection
        tokio::spawn(async move {
            while let Some(msg) = ws_stream.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Err(e) = Self::handle_message(
                            &text, 
                            player_id, 
                            &players, 
                            &connections,
                            &event_sender,
                            &region_bounds
                        ).await {
                            error!("Error handling message from {}: {}", player_id, e);
                            
                            // Send error response to client using connections map
                            let error_response = NetworkMessage::GameData { 
                                data: serde_json::json!({
                                    "type": "error",
                                    "message": e.to_string()
                                })
                            };
                            if let Ok(response_text) = serde_json::to_string(&error_response) {
                                if let Some(mut sink) = connections.get_mut(&player_id) {
                                    let _ = sink.send(Message::Text(response_text)).await;
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("Player {} disconnected", player_id);
                        break;
                    }
                    Err(e) => {
                        error!("WebSocket error for player {}: {}", player_id, e);
                        break;
                    }
                    _ => {} // Ignore other message types
                }
            }
            
            // Clean up on disconnect
            if let Some(player) = players.remove(&player_id) {
                let _ = event_sender.send((
                    EventId::new(EventNamespace::new("core"), "player_left"),
                    Arc::new(CoreEvent::PlayerLeft { player_id: player.0 })
                ));
            }
            connections.remove(&player_id);
        });
    }
    
    /// Handle a message from a client
    async fn handle_message(
        text: &str,
        player_id: PlayerId,
        players: &DashMap<PlayerId, Player>,
        connections: &DashMap<PlayerId, WsSink>,
        event_sender: &broadcast::Sender<(EventId, Arc<dyn GameEvent + Send + Sync>)>,
        region_bounds: &RegionBounds,
    ) -> Result<(), ServerError> {
        let message: NetworkMessage = serde_json::from_str(text)
            .map_err(|e| ServerError::Serialization(format!("Invalid JSON: {}", e)))?;
        
        match message {
            NetworkMessage::PlayerJoin { name } => {
                let player = Player::new(name.clone(), Position::new(0.0, 0.0, 0.0));
                let player_copy = player.clone();
                players.insert(player_id, player);
                
                // Send success response to the joining player
                let response = NetworkMessage::GameData { 
                    data: serde_json::json!({
                        "type": "join_success",
                        "player_id": player_id,
                        "name": name,
                        "position": {"x": 0.0, "y": 0.0, "z": 0.0}
                    })
                };
                
                if let Some(mut sink) = connections.get_mut(&player_id) {
                    if let Ok(response_text) = serde_json::to_string(&response) {
                        let _ = sink.send(Message::Text(response_text)).await;
                    }
                }
                
                // Emit event for plugins
                let _ = event_sender.send((
                    EventId::new(EventNamespace::new("core"), "player_joined"),
                    Arc::new(CoreEvent::PlayerJoined { player: player_copy })
                ));
                
                info!("Player {} ({}) joined successfully", name, player_id);
            }
            
            NetworkMessage::PlayerMove { position } => {
                if !region_bounds.contains(&position) {
                    return Err(ServerError::Region("Position outside region bounds".to_string()));
                }
                
                if let Some(mut player) = players.get_mut(&player_id) {
                    let old_position = player.position;
                    player.position = position;
                    
                    // Send movement confirmation to player
                    let response = NetworkMessage::GameData { 
                        data: serde_json::json!({
                            "type": "move_success",
                            "position": position
                        })
                    };
                    
                    if let Some(mut sink) = connections.get_mut(&player_id) {
                        if let Ok(response_text) = serde_json::to_string(&response) {
                            let _ = sink.send(Message::Text(response_text)).await;
                        }
                    }
                    
                    let _ = event_sender.send((
                        EventId::new(EventNamespace::new("core"), "player_moved"),
                        Arc::new(CoreEvent::PlayerMoved { 
                            player_id, 
                            old_position, 
                            new_position: position 
                        })
                    ));
                    
                    debug!("Player {} moved to {:?}", player_id, position);
                }
            }
            
            NetworkMessage::PlayerLeave => {
                if let Some(player) = players.remove(&player_id) {
                    // Send goodbye message
                    let response = NetworkMessage::GameData { 
                        data: serde_json::json!({
                            "type": "leave_success"
                        })
                    };
                    
                    if let Some(mut sink) = connections.get_mut(&player_id) {
                        if let Ok(response_text) = serde_json::to_string(&response) {
                            let _ = sink.send(Message::Text(response_text)).await;
                        }
                    }
                    
                    let _ = event_sender.send((
                        EventId::new(EventNamespace::new("core"), "player_left"),
                        Arc::new(CoreEvent::PlayerLeft { player_id: player.0 })
                    ));
                    
                    info!("Player {} left", player_id);
                }
                connections.remove(&player_id);
            }
            
            NetworkMessage::GameData { data } => {
                // Echo the data back for testing
                let response = NetworkMessage::GameData { 
                    data: serde_json::json!({
                        "type": "echo",
                        "original": data
                    })
                };
                
                if let Some(mut sink) = connections.get_mut(&player_id) {
                    if let Ok(response_text) = serde_json::to_string(&response) {
                        let _ = sink.send(Message::Text(response_text)).await;
                    }
                }
                
                let _ = event_sender.send((
                    EventId::new(EventNamespace::new("core"), "custom_message"),
                    Arc::new(CoreEvent::CustomMessage { data })
                ));
            }
            
            NetworkMessage::PluginMessage { plugin, data } => {
                // Forward to plugin and send response
                let response = NetworkMessage::GameData { 
                    data: serde_json::json!({
                        "type": "plugin_message_received",
                        "plugin": plugin,
                        "data": data
                    })
                };
                
                if let Some(mut sink) = connections.get_mut(&player_id) {
                    if let Ok(response_text) = serde_json::to_string(&response) {
                        let _ = sink.send(Message::Text(response_text)).await;
                    }
                }
                
                let _ = event_sender.send((
                    EventId::new(EventNamespace::plugin_default(&plugin), "message"),
                    Arc::new(CoreEvent::CustomMessage { data })
                ));
            }
        }
        
        Ok(())
    }
    
    pub async fn start_event_processor(&self) {
        let mut event_receiver = self.event_sender.subscribe();
        let plugins = self.plugins.clone();
        let event_subscriptions = self.event_subscriptions.clone();
        let players = self.players.clone();
        let connections = self.connections.clone();
        let region_id = self.region_id;
        let event_sender = self.event_sender.clone();
        
        tokio::spawn(async move {
            while let Ok((event_id, event)) = event_receiver.recv().await {
                if let Some(subscriber_indices) = event_subscriptions.get(&event_id) {
                    let plugins_guard = plugins.read().await;
                    
                    for &plugin_index in subscriber_indices.iter() {
                        if let Some(plugin_arc) = plugins_guard.get(plugin_index) {
                            let plugin_arc = plugin_arc.clone();
                            let event_id = event_id.clone();
                            let event = event.clone();
                            let players = players.clone();
                            let connections = connections.clone();
                            let event_sender = event_sender.clone();
                            
                            tokio::spawn(async move {
                                let mut plugin = plugin_arc.write().await;
                                let context = ServerContextImpl::new(
                                    region_id,
                                    players,
                                    connections,
                                    event_sender
                                );
                            
                                if let Err(e) = plugin.handle_event(&event_id, event.as_ref(), &context).await {
                                    error!("Plugin error handling event {}: {}", event_id, e);
                                }
                            });
                        }
                    }
                }
            }
        });
    }
    
    /// Shutdown the server gracefully
    pub async fn shutdown(&self) -> Result<(), ServerError> {
        info!("Shutting down game server...");
        
        // Signal shutdown
        let _ = self.shutdown_signal.send(true);
        
        // Shutdown all plugins
        let plugins = self.plugins.read().await;
        for plugin_arc in plugins.iter() {
            let mut plugin = plugin_arc.write().await;
            let context = ServerContextImpl::new(
                self.region_id,
                self.players.clone(),
                self.connections.clone(),
                self.event_sender.clone()
            );
            if let Err(e) = plugin.shutdown(&context).await {
                error!("Error shutting down plugin: {}", e);
            }
        }
        
        info!("Game server shutdown complete");
        Ok(())
    }
}

/// Implementation of ServerContext for plugins
struct ServerContextImpl {
    region_id: RegionId,
    players: Arc<DashMap<PlayerId, Player>>,
    connections: Arc<DashMap<PlayerId, WsSink>>,
    event_sender: broadcast::Sender<(EventId, Arc<dyn GameEvent + Send + Sync>)>,
}

impl ServerContextImpl {
    fn new(
        region_id: RegionId,
        players: Arc<DashMap<PlayerId, Player>>,
        connections: Arc<DashMap<PlayerId, WsSink>>,
        event_sender: broadcast::Sender<(EventId, Arc<dyn GameEvent + Send + Sync>)>,
    ) -> Self {
        Self {
            region_id,
            players,
            connections,
            event_sender,
        }
    }
}

#[async_trait]
impl ServerContext for ServerContextImpl {
    async fn emit_event(&self, namespace: EventNamespace, event: Box<dyn GameEvent>) -> Result<(), ServerError> {
        let event_id = EventId::new(namespace, event.event_type());
        self.event_sender.send((event_id, event.into()))
            .map_err(|e| ServerError::Internal(format!("Failed to emit event: {}", e)))?;
        
        Ok(())
    }
    
    fn region_id(&self) -> RegionId {
        self.region_id
    }
    
    async fn get_players(&self) -> Result<Vec<Player>, ServerError> {
        Ok(self.players.iter().map(|entry| entry.value().clone()).collect())
    }
    
    async fn get_player(&self, id: PlayerId) -> Result<Option<Player>, ServerError> {
        Ok(self.players.get(&id).map(|entry| entry.value().clone()))
    }
    
    async fn send_to_player(&self, player_id: PlayerId, message: &[u8]) -> Result<(), ServerError> {
        if let Some(mut sink) = self.connections.get_mut(&player_id) {
            let message_text = String::from_utf8_lossy(message);
            sink.send(Message::Text(message_text.to_string())).await
                .map_err(|e| ServerError::Network(format!("Failed to send message: {}", e)))?;
        }
        Ok(())
    }
    
    async fn broadcast_to_region(&self, message: &[u8]) -> Result<(), ServerError> {
        let message_text = String::from_utf8_lossy(message);
        let msg = Message::Text(message_text.to_string());
        
        for mut entry in self.connections.iter_mut() {
            let _ = entry.value_mut().send(msg.clone()).await;
        }
        
        Ok(())
    }
    
    fn log(&self, level: LogLevel, message: &str) {
        match level {
            LogLevel::Error => error!("{}", message),
            LogLevel::Warn => warn!("{}", message),
            LogLevel::Info => info!("{}", message),
            LogLevel::Debug => debug!("{}", message),
            LogLevel::Trace => tracing::trace!("{}", message),
        }
    }
}

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub listen_addr: SocketAddr,
    pub region_bounds: RegionBounds,
    pub plugin_directory: String,
    pub max_players: usize,
    pub tick_rate: u64, // milliseconds
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:8080".parse().unwrap(),
            region_bounds: RegionBounds {
                min_x: -1000.0,
                max_x: 1000.0,
                min_y: -1000.0,
                max_y: 1000.0,
                min_z: -100.0,
                max_z: 100.0,
            },
            plugin_directory: "plugins".to_string(),
            max_players: 1000,
            tick_rate: 50, // 20 FPS
        }
    }
}