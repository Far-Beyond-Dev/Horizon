use anyhow::{Context, Result};
use axum::{
    extract::{ConnectInfo, State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use dashmap::DashMap;
use futures::TryFutureExt;
use serde::{Deserialize, Serialize};
use socketioxide::{
    extract::{Data, SocketRef},
    layer::SocketIoLayer,
    SocketIo,
};
use std::net::SocketAddr;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use tokio::sync::{broadcast, RwLock};
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    trace::TraceLayer,
    compression::CompressionLayer,
};
use tracing::{info, warn, error, debug, instrument};
use uuid::Uuid;

use crate::config::NetworkConfig;
use crate::plugins::{PluginManager, PluginEvent, AuthCredentials, UserRegistration};
use crate::world::WorldManager;
use crate::sharding::ShardManager;
use crate::core::ServerEvent;

/// Network manager handles all client connections and communications
pub struct NetworkManager {
    config: NetworkConfig,
    socket_io: SocketIo,
    players: Arc<DashMap<Uuid, ConnectedPlayer>>,
    sessions: Arc<DashMap<String, PlayerSession>>,
    connection_count: Arc<AtomicUsize>,
    world_manager: Arc<WorldManager>,
    plugin_manager: Arc<PluginManager>,
    shard_manager: Arc<ShardManager>,
    event_sender: broadcast::Sender<ServerEvent>,
    rate_limiter: Arc<RateLimiter>,
    accepting_connections: Arc<RwLock<bool>>,
}

/// Connected player information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedPlayer {
    pub id: Uuid,
    pub session_id: String,
    pub socket_id: String,
    pub username: String,
    pub ip_address: String, // Changed from SocketAddr to String for serialization
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub region_id: Option<Uuid>,
    pub shard_id: Option<Uuid>,
    pub authenticated: bool,
    pub permissions: PlayerPermissions,
    pub metadata: PlayerMetadata,
}

/// Player session data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSession {
    pub session_id: String,
    pub player_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub data: serde_json::Value,
}

/// Player permissions (local to network module)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerPermissions {
    pub admin: bool,
    pub moderator: bool,
    pub can_build: bool,
    pub can_chat: bool,
    pub custom_permissions: Vec<String>,
}

impl Default for PlayerPermissions {
    fn default() -> Self {
        Self {
            admin: false,
            moderator: false,
            can_build: true,
            can_chat: true,
            custom_permissions: Vec::new(),
        }
    }
}

/// Convert from plugin PlayerPermissions to network PlayerPermissions
impl From<crate::plugins::PlayerPermissions> for PlayerPermissions {
    fn from(perms: crate::plugins::PlayerPermissions) -> Self {
        Self {
            admin: perms.admin,
            moderator: perms.moderator,
            can_build: perms.can_build,
            can_chat: perms.can_chat,
            custom_permissions: perms.custom_permissions,
        }
    }
}

/// Player metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerMetadata {
    pub level: u32,
    pub experience: u64,
    pub character_data: serde_json::Value,
    pub preferences: PlayerPreferences,
    pub statistics: PlayerStatistics,
}

impl Default for PlayerMetadata {
    fn default() -> Self {
        Self {
            level: 1,
            experience: 0,
            character_data: serde_json::Value::Null,
            preferences: PlayerPreferences::default(),
            statistics: PlayerStatistics::default(),
        }
    }
}

/// Player preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerPreferences {
    pub language: String,
    pub ui_scale: f32,
    pub graphics_quality: String,
    pub audio_volume: f32,
    pub custom_settings: serde_json::Value,
}

impl Default for PlayerPreferences {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            ui_scale: 1.0,
            graphics_quality: "medium".to_string(),
            audio_volume: 1.0,
            custom_settings: serde_json::Value::Null,
        }
    }
}

/// Player statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStatistics {
    pub total_play_time: std::time::Duration,
    pub sessions_count: u64,
    pub messages_sent: u64,
    pub distance_traveled: f64,
    pub custom_stats: serde_json::Value,
}

impl Default for PlayerStatistics {
    fn default() -> Self {
        Self {
            total_play_time: std::time::Duration::ZERO,
            sessions_count: 0,
            messages_sent: 0,
            distance_traveled: 0.0,
            custom_stats: serde_json::Value::Null,
        }
    }
}

/// Rate limiter for connections
pub struct RateLimiter {
    requests: Arc<DashMap<SocketAddr, Vec<chrono::DateTime<chrono::Utc>>>>,
    config: crate::config::RateLimitingConfig,
}

impl RateLimiter {
    fn new(config: crate::config::RateLimitingConfig) -> Self {
        Self {
            requests: Arc::new(DashMap::new()),
            config,
        }
    }
    
    fn check_rate_limit(&self, addr: SocketAddr) -> bool {
        if !self.config.enabled {
            return true;
        }
        
        // Check whitelist
        if self.config.whitelist.iter().any(|ip| ip.parse::<SocketAddr>().map(|a| a.ip()) == Ok(addr.ip())) {
            return true;
        }
        
        let now = chrono::Utc::now();
        let window_start = now - chrono::Duration::minutes(1);
        
        let mut entry = self.requests.entry(addr).or_default();
        
        // Remove old requests
        entry.retain(|time| *time > window_start);
        
        // Check if under limit
        if entry.len() < self.config.requests_per_minute as usize {
            entry.push(now);
            true
        } else {
            false
        }
    }
}

/// Player message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PlayerMessage {
    Chat { message: String, channel: Option<String> },
    Movement { position: [f64; 3], rotation: [f64; 3], velocity: [f64; 3] },
    Interaction { target_id: Uuid, action: String, data: serde_json::Value },
    WorldAction { action: String, data: serde_json::Value },
    Custom { event_type: String, data: serde_json::Value },
}

/// Server message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    Welcome { player_id: Uuid, world_info: serde_json::Value },
    PlayerUpdate { player_id: Uuid, data: serde_json::Value },
    WorldUpdate { region_id: Uuid, objects: Vec<serde_json::Value> },
    Chat { player_id: Uuid, username: String, message: String, channel: Option<String> },
    Notification { level: String, message: String },
    Error { code: String, message: String },
    Custom { event_type: String, data: serde_json::Value },
}

/// Authentication request
#[derive(Debug, Deserialize)]
pub struct AuthRequest {
    pub username: String,
    pub password: Option<String>,
    pub token: Option<String>,
    pub provider: Option<String>,
}

/// Authentication response
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub success: bool,
    pub player_id: Option<Uuid>,
    pub session_token: Option<String>,
    pub error: Option<String>,
    pub auth_available: bool,
}

/// Registration request
#[derive(Debug, Deserialize)]
pub struct RegistrationRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

impl NetworkManager {
    /// Create new network manager
    #[instrument(skip_all)]
    pub async fn new(
        config: &NetworkConfig,
        world_manager: Arc<WorldManager>,
        plugin_manager: Arc<PluginManager>,
        shard_manager: Arc<ShardManager>,
        event_sender: broadcast::Sender<ServerEvent>,
    ) -> Result<Self> {
        info!("🌐 Initializing network manager");
        
        let (layer, socket_io) = SocketIo::new_layer();
        
        let manager = Self {
            config: config.clone(),
            socket_io,
            players: Arc::new(DashMap::new()),
            sessions: Arc::new(DashMap::new()),
            connection_count: Arc::new(AtomicUsize::new(0)),
            world_manager,
            plugin_manager,
            shard_manager,
            event_sender,
            rate_limiter: Arc::new(RateLimiter::new(config.rate_limiting.clone())),
            accepting_connections: Arc::new(RwLock::new(true)),
        };
        
        // Setup Socket.IO event handlers
        manager.setup_socket_handlers().await;
        
        info!("✅ Network manager initialized");
        Ok(manager)
    }
    
    /// Setup Socket.IO event handlers
    async fn setup_socket_handlers(&self) {
        let players = self.players.clone();
        let sessions = self.sessions.clone();
        let world_manager = self.world_manager.clone();
        let plugin_manager = self.plugin_manager.clone();
        let event_sender = self.event_sender.clone();
        let rate_limiter = self.rate_limiter.clone();
        let connection_count = self.connection_count.clone();
        let accepting = self.accepting_connections.clone();
        
        // Connection handler
        self.socket_io.ns("/", move |socket: SocketRef, Data(data): Data<serde_json::Value>| {
            let players = players.clone();
            let sessions = sessions.clone();
            let world_manager = world_manager.clone();
            let plugin_manager = plugin_manager.clone();
            let event_sender = event_sender.clone();
            let rate_limiter = rate_limiter.clone();
            let connection_count = connection_count.clone();
            let accepting = accepting.clone();
            
            async move {
                if let Err(e) = handle_connection(
                    socket,
                    data,
                    players,
                    sessions,
                    world_manager,
                    plugin_manager,
                    event_sender,
                    rate_limiter,
                    connection_count,
                    accepting,
                ).await {
                    error!("Error handling connection: {}", e);
                }
            }
        });
    }
    
    /// Run the network manager
    pub async fn run(&self) -> Result<()> {
        info!("🌐 Starting network server");
        
        let app = self.create_app().await?;
        
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
            .await
            .context("Failed to bind to address")?;
        
        info!("🚀 Network server listening on {}", listener.local_addr()?);
        
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .context("Server error")?;
        
        Ok(())
    }
    
    /// Create the Axum application
    async fn create_app(&self) -> Result<Router> {
        // Create a new layer for Socket.IO
        let (socket_layer, _) = SocketIo::new_layer();
        
        let app = Router::new()
            .route("/", get(root_handler))
            .route("/health", get(health_handler))
            .route("/api/v1/auth", post(auth_handler))
            .route("/api/v1/register", post(register_handler))
            .route("/api/v1/players", get(players_handler))
            .route("/api/v1/stats", get(stats_handler))
            .route("/ws", get(websocket_handler))
            .layer(
                ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    .layer(CompressionLayer::new())
                    .layer(CorsLayer::permissive())
                    .layer(socket_layer),
            )
            .with_state(NetworkState {
                players: self.players.clone(),
                connection_count: self.connection_count.clone(),
                plugin_manager: self.plugin_manager.clone(),
            });
        
        Ok(app)
    }
    
    /// Broadcast message to all connected players
    #[instrument(skip(self, message))]
    pub async fn broadcast_message(&self, message: ServerMessage) -> Result<()> {
        let message_json = serde_json::to_value(&message)?;
        
        for player in self.players.iter() {
            if let Err(e) = self.socket_io.to(player.socket_id.clone()).emit("message", &message_json).await {
                warn!("Failed to send message to player {}: {}", player.id, e);
            }
        }
        
        Ok(())
    }
    
    /// Send message to specific player
    #[instrument(skip(self, message))]
    pub async fn send_to_player(&self, player_id: Uuid, message: ServerMessage) -> Result<()> {
        if let Some(player) = self.players.iter().find(|p| p.id == player_id) {
            let message_json = serde_json::to_value(&message)?;
            
            self.socket_io.to(player.socket_id.clone()).emit("message", &message_json).await
                .map_err(|e| anyhow::anyhow!("Failed to send message: {}", e))?;
        }
        
        Ok(())
    }
    
    /// Send message to players in a region
    #[instrument(skip(self, message))]
    pub async fn send_to_region(&self, region_id: Uuid, message: ServerMessage) -> Result<()> {
        let message_json = serde_json::to_value(&message)?;
        
        for player in self.players.iter() {
            if player.region_id == Some(region_id) {
                if let Err(e) = self.socket_io.to(player.socket_id.clone()).emit("message", &message_json).await {
                    warn!("Failed to send message to player {} in region {}: {}", 
                          player.id, region_id, e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Broadcast shutdown notice
    pub async fn broadcast_shutdown_notice(&self) -> Result<()> {
        let message = ServerMessage::Notification {
            level: "warning".to_string(),
            message: "Server is shutting down. Please save your progress.".to_string(),
        };
        
        self.broadcast_message(message).await
    }
    
    /// Stop accepting new connections
    pub async fn stop_accepting(&self) -> Result<()> {
        *self.accepting_connections.write().await = false;
        info!("🛑 Stopped accepting new connections");
        Ok(())
    }
    
    /// Get current player count
    pub async fn get_player_count(&self) -> usize {
        self.connection_count.load(Ordering::Relaxed)
    }
    
    /// Get player by ID
    pub async fn get_player(&self, player_id: Uuid) -> Option<ConnectedPlayer> {
        self.players.iter().find(|p| p.id == player_id).map(|p| p.clone())
    }
    
    /// Disconnect player
    #[instrument(skip(self))]
    pub async fn disconnect_player(&self, player_id: Uuid, reason: &str) -> Result<()> {
        if let Some((_, player)) = self.players.remove(&player_id) {
            if let Ok(sid) = player.socket_id.parse() {
                if let Some(socket) = self.socket_io.get_socket(sid) {
                    socket.disconnect().ok();
                }
            }
            
            self.connection_count.fetch_sub(1, Ordering::Relaxed);
            
            // Send disconnect event
            let _ = self.event_sender.send(ServerEvent::PlayerDisconnected {
                player_id,
                reason: reason.to_string(),
            });
            
            info!("👋 Player {} disconnected: {}", player.username, reason);
        }
        
        Ok(())
    }
}

/// Network state for Axum handlers
#[derive(Clone)]
struct NetworkState {
    players: Arc<DashMap<Uuid, ConnectedPlayer>>,
    connection_count: Arc<AtomicUsize>,
    plugin_manager: Arc<PluginManager>,
}

/// Handle new connection
#[instrument(skip_all)]
async fn handle_connection(
    socket: SocketRef,
    _data: serde_json::Value,
    players: Arc<DashMap<Uuid, ConnectedPlayer>>,
    _sessions: Arc<DashMap<String, PlayerSession>>,
    _world_manager: Arc<WorldManager>,
    plugin_manager: Arc<PluginManager>,
    event_sender: broadcast::Sender<ServerEvent>,
    rate_limiter: Arc<RateLimiter>,
    connection_count: Arc<AtomicUsize>,
    accepting: Arc<RwLock<bool>>,
) -> Result<()> {
    // Check if accepting connections
    if !*accepting.read().await {
        socket.emit("error", "Server not accepting connections").ok();
        socket.disconnect().ok();
        return Ok(());
    }
    
    // Get client IP - we'll use a placeholder since SocketRef doesn't expose remote address
    let client_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    
    // Rate limiting
    if !rate_limiter.check_rate_limit(client_addr) {
        warn!("Rate limit exceeded for {}", client_addr);
        socket.emit("error", "Rate limit exceeded").ok();
        socket.disconnect().ok();
        return Ok(());
    }
    
    info!("🔌 New connection from {}", client_addr);
    
    let player_id = Uuid::new_v4();
    let session_id = format!("session_{}", Uuid::new_v4());
    let socket_id = socket.id.to_string();
    
    // Create initial player data
    let player = ConnectedPlayer {
        id: player_id,
        session_id: session_id.clone(),
        socket_id: socket_id.clone(),
        username: format!("Guest_{}", player_id.to_string()[..8].to_uppercase()),
        ip_address: client_addr.to_string(),
        connected_at: chrono::Utc::now(),
        last_activity: chrono::Utc::now(),
        region_id: None,
        shard_id: None,
        authenticated: false,
        permissions: PlayerPermissions::default(),
        metadata: PlayerMetadata::default(),
    };
    
    // Store player
    players.insert(player_id, player.clone());
    connection_count.fetch_add(1, Ordering::Relaxed);
    
    // Setup event handlers for this socket
    setup_player_handlers(
        socket.clone(),
        player_id,
        players.clone(),
        plugin_manager.clone(),
        event_sender.clone(),
    ).await;
    
    // Send welcome message
    let auth_available = plugin_manager.get_auth_service().await.is_some();
    let welcome = ServerMessage::Welcome {
        player_id,
        world_info: serde_json::json!({
            "server_name": "Horizon 2",
            "version": env!("CARGO_PKG_VERSION"),
            "max_players": 10000,
            "current_players": connection_count.load(Ordering::Relaxed),
            "auth_available": auth_available
        }),
    };
    
    socket.emit("welcome", &serde_json::to_value(&welcome)?).ok();
    
    // Send connection event
    let _ = event_sender.send(ServerEvent::PlayerConnected {
        player_id,
        session_id,
    });
    
    // Notify plugins
    let _ = plugin_manager.dispatch_event(PluginEvent::PlayerConnected {
        player_id,
        data: serde_json::to_value(&player)?,
    }).await;
    
    Ok(())
}

/// Setup event handlers for a player socket
async fn setup_player_handlers(
    socket: SocketRef,
    player_id: Uuid,
    players: Arc<DashMap<Uuid, ConnectedPlayer>>,
    plugin_manager: Arc<PluginManager>,
    event_sender: broadcast::Sender<ServerEvent>,
) {
    // Authentication handler
    {
        let players = players.clone();
        let plugin_manager = plugin_manager.clone();
        socket.on("auth", move |socket: SocketRef, Data(auth_req): Data<AuthRequest>| {
            let players = players.clone();
            let plugin_manager = plugin_manager.clone();
            async move {
                handle_auth(socket, player_id, auth_req, players, plugin_manager).await;
            }
        });
    }
    
    // Registration handler
    {
        let players = players.clone();
        let plugin_manager = plugin_manager.clone();
        socket.on("register", move |socket: SocketRef, Data(reg_req): Data<RegistrationRequest>| {
            let players = players.clone();
            let plugin_manager = plugin_manager.clone();
            async move {
                handle_registration(socket, player_id, reg_req, players, plugin_manager).await;
            }
        });
    }
    
    // Message handler
    {
        let players = players.clone();
        let plugin_manager = plugin_manager.clone();
        socket.on("message", move |_socket: SocketRef, Data(msg): Data<PlayerMessage>| {
            let players = players.clone();
            let plugin_manager = plugin_manager.clone();
            async move {
                handle_player_message(player_id, msg, players, plugin_manager).await;
            }
        });
    }
    
    // Ping/Pong for keepalive
    socket.on("ping", |socket: SocketRef, Data(data): Data<serde_json::Value>| async move {
        socket.emit("pong", &data).ok();
    });
    
    // Disconnect handler
    {
        let players = players.clone();
        let event_sender = event_sender.clone();
        socket.on_disconnect(move |_socket: SocketRef| {
            let players = players.clone();
            let event_sender = event_sender.clone();
            async move {
                handle_disconnect(player_id, players, event_sender).await;
            }
        });
    }
}

/// Handle authentication
async fn handle_auth(
    socket: SocketRef,
    player_id: Uuid,
    auth_req: AuthRequest,
    players: Arc<DashMap<Uuid, ConnectedPlayer>>,
    plugin_manager: Arc<PluginManager>,
) {
    // Check if auth service is available
    let auth_service = match plugin_manager.get_auth_service().await {
        Some(service) => service,
        None => {
            let response = AuthResponse {
                success: false,
                player_id: None,
                session_token: None,
                error: Some("Authentication service not available".to_string()),
                auth_available: false,
            };
            socket.emit("auth_response", &serde_json::to_value(&response).unwrap()).ok();
            return;
        }
    };
    
    // Convert auth request to credentials
    let credentials = if let Some(password) = auth_req.password {
        AuthCredentials::Password(password)
    } else if let Some(token) = auth_req.token {
        AuthCredentials::Token(token)
    } else if let Some(provider) = auth_req.provider {
        AuthCredentials::OAuth { provider, token: "".to_string() }
    } else {
        let response = AuthResponse {
            success: false,
            player_id: None,
            session_token: None,
            error: Some("No valid credentials provided".to_string()),
            auth_available: true,
        };
        socket.emit("auth_response", &serde_json::to_value(&response).unwrap()).ok();
        return;
    };
    
    // Attempt authentication
    match auth_service.authenticate(&auth_req.username, &credentials).await {
        Ok(Some(user)) => {
            // Create session
            match auth_service.create_session(&user).await {
                Ok(session_token) => {
                    // Update player with authenticated info
                    if let Some(mut player) = players.get_mut(&player_id) {
                        player.username = user.username.clone();
                        player.authenticated = true;
                        player.permissions = PlayerPermissions::from(user.permissions.clone());
                        player.metadata.character_data = user.metadata.clone();
                    }
                    
                    let response = AuthResponse {
                        success: true,
                        player_id: Some(player_id),
                        session_token: Some(session_token),
                        error: None,
                        auth_available: true,
                    };
                    
                    socket.emit("auth_response", &serde_json::to_value(&response).unwrap()).ok();
                    info!("✅ Player {} authenticated as {}", player_id, auth_req.username);
                }
                Err(e) => {
                    let response = AuthResponse {
                        success: false,
                        player_id: None,
                        session_token: None,
                        error: Some(format!("Failed to create session: {}", e)),
                        auth_available: true,
                    };
                    socket.emit("auth_response", &serde_json::to_value(&response).unwrap()).ok();
                }
            }
        }
        Ok(None) => {
            let response = AuthResponse {
                success: false,
                player_id: None,
                session_token: None,
                error: Some("Invalid credentials".to_string()),
                auth_available: true,
            };
            socket.emit("auth_response", &serde_json::to_value(&response).unwrap()).ok();
            warn!("❌ Authentication failed for {}", auth_req.username);
        }
        Err(e) => {
            let response = AuthResponse {
                success: false,
                player_id: None,
                session_token: None,
                error: Some(format!("Authentication error: {}", e)),
                auth_available: true,
            };
            socket.emit("auth_response", &serde_json::to_value(&response).unwrap()).ok();
            error!("Authentication error for {}: {}", auth_req.username, e);
        }
    }
}

/// Handle user registration
async fn handle_registration(
    socket: SocketRef,
    player_id: Uuid,
    reg_req: RegistrationRequest,
    players: Arc<DashMap<Uuid, ConnectedPlayer>>,
    plugin_manager: Arc<PluginManager>,
) {
    // Check if auth service is available
    let auth_service = match plugin_manager.get_auth_service().await {
        Some(service) => service,
        None => {
            let response = AuthResponse {
                success: false,
                player_id: None,
                session_token: None,
                error: Some("Authentication service not available".to_string()),
                auth_available: false,
            };
            socket.emit("register_response", &serde_json::to_value(&response).unwrap()).ok();
            return;
        }
    };
    
    // Convert registration request
    let registration = UserRegistration {
        username: reg_req.username.clone(),
        email: reg_req.email.clone(),
        password: reg_req.password,
        display_name: reg_req.display_name,
        metadata: None,
    };
    
    // Attempt registration
    match auth_service.register_user(&registration).await {
        Ok(user) => {
            info!("👤 Registered new user: {}", user.username);
            
            // Create session for new user
            match auth_service.create_session(&user).await {
                Ok(session_token) => {
                    // Update player with new user info
                    if let Some(mut player) = players.get_mut(&player_id) {
                        player.username = user.username.clone();
                        player.authenticated = true;
                        player.permissions = PlayerPermissions::from(user.permissions.clone());
                        player.metadata.character_data = user.metadata.clone();
                    }
                    
                    let response = AuthResponse {
                        success: true,
                        player_id: Some(player_id),
                        session_token: Some(session_token),
                        error: None,
                        auth_available: true,
                    };
                    
                    socket.emit("register_response", &serde_json::to_value(&response).unwrap()).ok();
                }
                Err(e) => {
                    let response = AuthResponse {
                        success: false,
                        player_id: None,
                        session_token: None,
                        error: Some(format!("Failed to create session: {}", e)),
                        auth_available: true,
                    };
                    socket.emit("register_response", &serde_json::to_value(&response).unwrap()).ok();
                }
            }
        }
        Err(e) => {
            let response = AuthResponse {
                success: false,
                player_id: None,
                session_token: None,
                error: Some(format!("Registration failed: {}", e)),
                auth_available: true,
            };
            socket.emit("register_response", &serde_json::to_value(&response).unwrap()).ok();
            warn!("❌ Registration failed for {}: {}", reg_req.username, e);
        }
    }
}

/// Handle player message
async fn handle_player_message(
    player_id: Uuid,
    message: PlayerMessage,
    players: Arc<DashMap<Uuid, ConnectedPlayer>>,
    plugin_manager: Arc<PluginManager>,
) {
    // Update last activity
    if let Some(mut player) = players.get_mut(&player_id) {
        player.last_activity = chrono::Utc::now();
    }
    
    // Handle different message types
    match message {
        PlayerMessage::Chat { message, channel: _ } => {
            info!("💬 Chat from {}: {}", player_id, message);
            
            // Broadcast chat message
            // Implementation would handle different channels, permissions, etc.
        }
        
        PlayerMessage::Movement { position, rotation: _, velocity: _ } => {
            debug!("🏃 Movement from {}: pos={:?}", player_id, position);
            
            // Update player position in world
            // Implementation would validate movement and update world state
        }
        
        PlayerMessage::Interaction { target_id, action, data: _ } => {
            debug!("🤝 Interaction from {}: target={}, action={}", player_id, target_id, action);
            
            // Handle player interactions
            // Implementation would validate and process interactions
        }
        
        PlayerMessage::WorldAction { action, data: _ } => {
            debug!("🌍 World action from {}: action={}", player_id, action);
            
            // Handle world actions
            // Implementation would validate permissions and process actions
        }
        
        PlayerMessage::Custom { event_type, data } => {
            debug!("🔧 Custom event from {}: type={}", player_id, event_type);
            
            // Forward to plugins
            let _ = plugin_manager.dispatch_event(PluginEvent::MessageReceived {
                player_id,
                message: serde_json::json!({
                    "event_type": event_type,
                    "data": data
                }),
            }).await;
        }
    }
}

/// Handle player disconnect
async fn handle_disconnect(
    player_id: Uuid,
    players: Arc<DashMap<Uuid, ConnectedPlayer>>,
    event_sender: broadcast::Sender<ServerEvent>,
) {
    if let Some((_, player)) = players.remove(&player_id) {
        info!("👋 Player {} ({}) disconnected", player.username, player_id);
        
        // Send disconnect event
        let _ = event_sender.send(ServerEvent::PlayerDisconnected {
            player_id,
            reason: "Client disconnected".to_string(),
        });
    }
}

/// HTTP Handlers
async fn root_handler() -> &'static str {
    "Horizon 2 Game Server"
}

async fn health_handler() -> StatusCode {
    StatusCode::OK
}

async fn auth_handler(
    State(_state): State<NetworkState>,
    axum::Json(_auth_req): axum::Json<AuthRequest>,
) -> Result<Response, StatusCode> {
    // HTTP authentication endpoint
    Err(StatusCode::NOT_IMPLEMENTED)
}

async fn register_handler(
    State(_state): State<NetworkState>,
    axum::Json(_reg_req): axum::Json<RegistrationRequest>,
) -> Result<Response, StatusCode> {
    // HTTP registration endpoint
    Err(StatusCode::NOT_IMPLEMENTED)
}

async fn players_handler(State(state): State<NetworkState>) -> impl IntoResponse {
    let count = state.connection_count.load(Ordering::Relaxed);
    let auth_available = state.plugin_manager.get_auth_service().await.is_some();
    
    serde_json::json!({
        "online_players": count,
        "auth_available": auth_available,
        "players": []  // Would include player list if needed
    }).to_string()
}

async fn stats_handler(State(state): State<NetworkState>) -> impl IntoResponse {
    let count = state.connection_count.load(Ordering::Relaxed);
    let auth_available = state.plugin_manager.get_auth_service().await.is_some();
    let storage_available = state.plugin_manager.get_storage_service().await.is_some();
    
    serde_json::json!({
        "server": "Horizon 2",
        "version": env!("CARGO_PKG_VERSION"),
        "online_players": count,
        "uptime": "N/A", // Would calculate actual uptime
        "memory_usage": "N/A", // Would get actual memory usage
        "services": {
            "auth": auth_available,
            "storage": storage_available
        }
    }).to_string()
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Response {
    ws.on_upgrade(move |_socket| handle_websocket(addr))
}

async fn handle_websocket(addr: SocketAddr) {
    info!("WebSocket connection from {}", addr);
    // WebSocket handler implementation would go here
}