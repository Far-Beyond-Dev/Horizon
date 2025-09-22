//! # Horizon Game Server - Realistic Player Client Demonstration
//!
//! This client demonstrates how a real game would use Horizon's GORC replication system.
//! It implements the complete event flow described in EVENT_SYSTEM_GUIDE.md, including:
//! - Zone-based event replication (4 channels with different ranges)
//! - Proper client-to-server GORC event format
//! - Realistic game scenarios: movement, combat, chat, progression
//! - Distance-based replication validation

use clap::Parser;
use futures::{SinkExt, StreamExt};
use horizon_event_system::{PlayerId, Vec3, GorcObjectId};
use plugin_player::events::{PlayerMoveRequest, PlayerAttackRequest, PlayerChatRequest};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::{interval, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn, error};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

#[derive(Parser, Debug)]
#[command(name = "horizon-space-client")]
#[command(about = "Horizon Space MMO - Realistic GORC Client Demonstration")]
struct Args {
    /// Server WebSocket URL
    #[arg(short, long, default_value = "ws://localhost:8081/ws")]
    url: String,
    
    /// Number of simultaneous space ships to simulate
    #[arg(short, long, default_value = "5")]
    players: u32,
    
    /// Ship movement frequency in Hz (position updates)
    #[arg(short, long, default_value = "10.0")]
    move_freq: f64,
    
    /// Communication frequency in messages per minute
    #[arg(short, long, default_value = "2.0")]
    chat_freq: f64,
    
    /// Weapon fire frequency per minute
    #[arg(short, long, default_value = "5.0")]
    attack_freq: f64,
    
    /// Simulation duration in seconds
    #[arg(short, long, default_value = "60")]
    duration: u64,
    
    /// Space sector size (square area in meters)
    #[arg(short, long, default_value = "2000.0")]
    world_size: f32,
    
    /// Enable JSON message logging to file
    #[arg(long, default_value = "true")]
    log_messages: bool,
    
    /// Log file path for JSON messages
    #[arg(long, default_value = "horizon_messages.log")]
    log_file: String,
}

/// GORC event message format for client-to-server communication
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GorcClientMessage {
    /// Type of message
    #[serde(rename = "type")]
    msg_type: String,
    /// Target GORC object ID
    object_id: String,
    /// GORC channel (0=critical, 1=detailed, 2=social, 3=metadata)
    channel: u8,
    /// Event name within the channel
    event: String,
    /// Event payload
    data: serde_json::Value,
    /// Player ID sending the event
    player_id: String,
}

/// GORC replication validation tracker
#[derive(Debug, Clone)]
struct GorcReplicationValidator {
    /// Expected events based on GORC zone ranges
    expected_events: std::collections::HashMap<String, u32>,
    /// Actually received events
    received_events: std::collections::HashMap<String, u32>,
    /// Player positions for distance calculations
    player_positions: std::collections::HashMap<PlayerId, Vec3>,
    /// Events that should have been received but weren't
    missing_events: Vec<String>,
    /// Events that were received but shouldn't have been
    extra_events: Vec<String>,
}

impl GorcReplicationValidator {
    fn new() -> Self {
        Self {
            expected_events: std::collections::HashMap::new(),
            received_events: std::collections::HashMap::new(),
            player_positions: std::collections::HashMap::new(),
            missing_events: Vec::new(),
            extra_events: Vec::new(),
        }
    }

    /// Update a player's position for distance-based validation
    fn update_player_position(&mut self, player_id: PlayerId, position: Vec3) {
        self.player_positions.insert(player_id, position);
    }

    /// Calculate if two players should be in range for a given GORC channel
    /// Based on EVENT_SYSTEM_GUIDE.md replication layer configuration
    fn is_in_range(&self, player1: PlayerId, player2: PlayerId, channel: u8) -> bool {
        if let (Some(pos1), Some(pos2)) = (self.player_positions.get(&player1), self.player_positions.get(&player2)) {
            let distance = pos1.distance(*pos2);
            match channel {
                0 => distance <= 1000.0, // Critical: 1km - Basic presence (SpaceShip example)
                1 => distance <= 500.0,  // Detailed: 500m - Combat details
                2 => distance <= 300.0,  // Social: 300m - Chat/social interactions  
                3 => distance <= 100.0,  // Metadata: 100m - Detailed scans
                _ => false,
            }
        } else {
            false
        }
    }

    /// Record that we expect to receive an event from another player
    fn expect_event(&mut self, from_player: PlayerId, to_player: PlayerId, channel: u8, event_type: &str) {
        if self.is_in_range(from_player, to_player, channel) {
            let key = format!("{}->{}:{}:{}", from_player, to_player, channel, event_type);
            *self.expected_events.entry(key).or_insert(0) += 1;
        }
    }

    /// Record that we actually received an event
    fn record_received_event(&mut self, from_player: PlayerId, to_player: PlayerId, channel: u8, event_type: &str) {
        let key = format!("{}->{}:{}:{}", from_player, to_player, channel, event_type);
        *self.received_events.entry(key.clone()).or_insert(0) += 1;
        
        // Check if this was expected
        if !self.expected_events.contains_key(&key) {
            self.extra_events.push(key.clone());
        }
    }

    /// Generate final validation report
    fn generate_report(&mut self, player_id: PlayerId) -> String {
        // Find missing events
        for (expected_key, expected_count) in &self.expected_events {
            let received_count = self.received_events.get(expected_key).unwrap_or(&0);
            if received_count < expected_count {
                self.missing_events.push(format!("{} (expected: {}, got: {})", expected_key, expected_count, received_count));
            }
        }

        let total_expected = self.expected_events.values().sum::<u32>();
        let total_received = self.received_events.values().sum::<u32>();
        let missing_count = self.missing_events.len();
        let extra_count = self.extra_events.len();

        format!(
            "🧪 GORC Replication Test Results for Player {}:\n\
             📊 Total Expected: {}, Total Received: {}\n\
             ❌ Missing Events: {} | ➕ Extra Events: {}\n\
             📋 Missing Details: {:#?}\n\
             📋 Extra Details: {:#?}",
            player_id, total_expected, total_received, missing_count, extra_count,
            self.missing_events, self.extra_events
        )
    }
}

/// Simulated player client
#[derive(Debug)]
struct SimulatedPlayer {
    player_id: PlayerId,
    position: Vec3,
    velocity: Vec3,
    last_chat: std::time::Instant,
    last_attack: std::time::Instant,
    move_target: Vec3,
    health: f32,
    level: u32,
    /// GORC instance ID received from server (None until server registers the player)
    server_gorc_instance_id: Option<GorcObjectId>,
    /// GORC replication validation tracker
    replication_validator: GorcReplicationValidator,
}

impl SimulatedPlayer {
    fn new(player_id: PlayerId, spawn_pos: Vec3) -> Self {
        Self {
            player_id,
            position: spawn_pos,
            velocity: Vec3::zero(),
            last_chat: std::time::Instant::now(),
            last_attack: std::time::Instant::now(),
            move_target: spawn_pos,
            health: 100.0,
            level: 1,
            server_gorc_instance_id: None, // Will be set when server sends registration
            replication_validator: GorcReplicationValidator::new(),
        }
    }

    /// Update player position with simple AI movement
    fn update_movement(&mut self, delta_time: f32, world_size: f32) -> bool {
        let distance_to_target = self.position.distance(self.move_target);
        
        // Pick new random target when close to current target
        if distance_to_target < 5.0 {
            let mut rng = rand::thread_rng();
            self.move_target = Vec3::new(
                rng.gen_range((-world_size/2.0) as f64..(world_size/2.0) as f64),
                0.0, // Keep on ground plane
                rng.gen_range((-world_size/2.0) as f64..(world_size/2.0) as f64),
            );
        }
        
        // Move towards target
        let dx = self.move_target.x - self.position.x;
        let dz = self.move_target.z - self.position.z;
        let distance = (dx * dx + dz * dz).sqrt();
        
        if distance > 0.01 {
            let direction_x = dx / distance;
            let direction_z = dz / distance;
            let speed = 8.0; // meters per second
            
            let old_position = self.position;
            self.velocity = Vec3::new(direction_x * speed, 0.0, direction_z * speed);
            self.position = Vec3::new(
                self.position.x + self.velocity.x * delta_time as f64,
                self.position.y,
                self.position.z + self.velocity.z * delta_time as f64,
            );
            
            // Return true if position changed significantly
            return old_position.distance(self.position) > 0.1;
        }
        
        false
    }

    /// Create a GORC movement message using proper client event format
    /// Follows EVENT_SYSTEM_GUIDE.md client-to-server communication pattern
    fn create_move_message(&self) -> Option<GorcClientMessage> {
        let instance_id = self.server_gorc_instance_id?;
        let move_request = PlayerMoveRequest {
            player_id: self.player_id,
            new_position: self.position,
            velocity: self.velocity,
            movement_state: {
                let vel_mag = (self.velocity.x * self.velocity.x + 
                              self.velocity.z * self.velocity.z).sqrt();
                if vel_mag > 0.1 { 1 } else { 0 }
            },
            client_timestamp: chrono::Utc::now(),
        };

        let msg = GorcClientMessage {
            msg_type: "gorc_event".to_string(),
            object_id: format!("{:?}", instance_id),
            channel: 0, // Critical channel: position updates (1000m range, 10Hz per guide)
            event: "move".to_string(),
            data: serde_json::to_value(&move_request).unwrap(),
            player_id: format!("{}", self.player_id),
        };
        // Print the JSON representation for debugging
        if let Ok(json) = serde_json::to_string(&msg) {
            println!("Move message JSON: {}", json);
        }
        Some(msg)
    }

    /// Create a GORC combat message - demonstrates space combat interaction
    /// Based on EVENT_SYSTEM_GUIDE.md Player vs Player Combat Sequence
    fn create_attack_message(&self) -> Option<GorcClientMessage> {
        let instance_id = self.server_gorc_instance_id?;
        use rand::Rng;
        let (offset_x, offset_z) = {
            let mut rng = rand::thread_rng();
            (
                rng.gen_range(-50.0_f64..50.0_f64), // Larger range for space combat
                rng.gen_range(-50.0_f64..50.0_f64),
            )
        };

        let attack_request = PlayerAttackRequest {
            player_id: self.player_id,
            target_position: Vec3::new(
                self.position.x + offset_x,
                self.position.y,
                self.position.z + offset_z,
            ),
            attack_type: "plasma_cannon".to_string(), // Space weapon
            client_timestamp: chrono::Utc::now(),
        };

        Some(GorcClientMessage {
            msg_type: "gorc_event".to_string(),
            object_id: format!("{:?}", instance_id),
            channel: 1, // Detailed channel: combat events (500m range, 30Hz per guide)
            event: "attack".to_string(),
            data: serde_json::to_value(&attack_request).unwrap(),
            player_id: format!("{}", self.player_id),
        })
    }

    /// Create a GORC chat message for social interaction
    /// Demonstrates EVENT_SYSTEM_GUIDE.md social layer (300m range)
    fn create_chat_message(&self, message: &str) -> Option<GorcClientMessage> {
        let instance_id = self.server_gorc_instance_id?;
        let chat_request = PlayerChatRequest {
            player_id: self.player_id,
            message: message.to_string(),
            channel: "local_space".to_string(), // Space MMO context
            target_player: None,
        };

        Some(GorcClientMessage {
            msg_type: "gorc_event".to_string(),
            object_id: format!("{:?}", instance_id),
            channel: 2, // Social channel: chat/emotes (300m range, 5Hz per guide)
            event: "chat".to_string(),
            data: serde_json::to_value(&chat_request).unwrap(),
            player_id: format!("{}", self.player_id),
        })
    }

    /// Create a detailed scan message - demonstrates metadata channel
    /// Based on EVENT_SYSTEM_GUIDE.md detailed scans (100m range)
    fn create_scan_message(&mut self) -> Option<GorcClientMessage> {
        let instance_id = self.server_gorc_instance_id?;
        self.level += 1;

        Some(GorcClientMessage {
            msg_type: "gorc_event".to_string(),
            object_id: format!("{:?}", instance_id),
            channel: 3, // Metadata channel: detailed scans (100m range, 60Hz per guide)
            event: "ship_scan".to_string(),
            data: serde_json::json!({
                "player_id": self.player_id,
                "ship_class": "Interceptor",
                "hull_integrity": self.health,
                "shield_strength": 85.0,
                "cargo_manifest": ["quantum_fuel", "rare_minerals"],
                "pilot_level": self.level,
                "scan_timestamp": chrono::Utc::now()
            }),
            player_id: format!("{}", self.player_id),
        })
    }
}

/// JSON Message Logger for debugging and analysis
#[derive(Debug, Clone)]
struct MessageLogger {
    log_file: Arc<Mutex<Option<tokio::fs::File>>>,
    enabled: bool,
}

impl MessageLogger {
    async fn new(log_file_path: &str, enabled: bool) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let file = if enabled {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_file_path)
                .await?;
            Some(file)
        } else {
            None
        };
        
        Ok(Self {
            log_file: Arc::new(Mutex::new(file)),
            enabled,
        })
    }
    
    async fn log_received_message(&self, player_id: PlayerId, message: &str) {
        if !self.enabled {
            return;
        }
        
        let timestamp = chrono::Utc::now().to_rfc3339();
        let log_entry = format!(
            "[{}] RECEIVED by Player {}: {}\n",
            timestamp, player_id, message
        );
        
        if let Some(ref mut file) = *self.log_file.lock().await {
            if let Err(e) = file.write_all(log_entry.as_bytes()).await {
                error!("Failed to write to log file: {}", e);
            } else if let Err(e) = file.flush().await {
                error!("Failed to flush log file: {}", e);
            }
        }
    }
    
    async fn log_sent_message(&self, player_id: PlayerId, message: &str) {
        if !self.enabled {
            return;
        }
        
        let timestamp = chrono::Utc::now().to_rfc3339();
        let log_entry = format!(
            "[{}] SENT by Player {}: {}\n",
            timestamp, player_id, message
        );
        
        if let Some(ref mut file) = *self.log_file.lock().await {
            if let Err(e) = file.write_all(log_entry.as_bytes()).await {
                error!("Failed to write to log file: {}", e);
            } else if let Err(e) = file.flush().await {
                error!("Failed to flush log file: {}", e);
            }
        }
    }
}

/// Handles received events from the server
#[derive(Debug, Deserialize)]
struct ServerEvent {
    event_type: String,
    player_id: Option<String>,
    data: serde_json::Value,
    channel: Option<u8>,
}

/// Run a single player simulation
async fn simulate_player(
    player_id: PlayerId,
    ws_url: String,
    args: Args,
    spawn_position: Vec3,
    message_logger: MessageLogger,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("🎮 Player {} starting simulation at {:?}", player_id, spawn_position);
    
    // Connect to WebSocket server
    let (ws_stream, _) = connect_async(&ws_url).await?;
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    
    let mut player = SimulatedPlayer::new(player_id, spawn_position);
    let mut move_timer = interval(Duration::from_secs_f64(1.0 / args.move_freq));
    let mut chat_timer = interval(Duration::from_secs_f64(60.0 / args.chat_freq));
    let mut attack_timer = interval(Duration::from_secs_f64(60.0 / args.attack_freq));
    let mut level_timer = interval(Duration::from_secs(30)); // Level up every 30 seconds
    
    let start_time = std::time::Instant::now();
    let simulation_duration = Duration::from_secs(args.duration);
    
    let space_chat_messages = [
        "Contact established, standing by",
        "Forming up for patrol mission",
        "Asteroid field looks promising here",
        "Sensors detecting hostile contacts",
        "Request permission to dock",
        "All systems nominal, ready for jump",
        "Mining operation successful",
        "Pirates in sector 7, stay alert!",
    ];
    
    let mut received_events = 0;
    let mut sent_events = 0;
    
    info!("🎮 Player {} connected and ready", player_id);

    loop {
        tokio::select! {
            // Handle incoming messages from server
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(message)) => {
                        // Log the variant and content where possible
                        match &message {
                            Message::Text(text) => {
                                // Log all received JSON messages to file
                                message_logger.log_received_message(player_id, text).await;
                                
                                // Raw text received
                                info!("🔍 Player {} received RAW message (length: {}): {}", player_id, text.len(), text);

                                // Try to parse as different message types (preserve existing behavior)
                                if text.starts_with("{") {
                                    // Try parsing as JSON
                                    match serde_json::from_str::<serde_json::Value>(&text) {
                                        Ok(json) => {
                                            info!("📋 Player {} parsed JSON structure: {:#}", player_id, json);

                                            // Check message type
                                            if let Some(msg_type) = json.get("type").and_then(|v| v.as_str()) {
                                                match msg_type {
                                                    "gorc_zone_enter" => {
                                                        info!("🎯 Player {} received GORC ZONE ENTER: {:#}", player_id, json);

                                                        // Extract GORC instance ID from zone enter message
                                                        if let Some(instance_id_str) = json.get("object_id").and_then(|v| v.as_str()) {
                                                            match GorcObjectId::from_str(instance_id_str) {
                                                                Ok(instance_id) => {
                                                                    player.server_gorc_instance_id = Some(instance_id);
                                                                    let channel = json.get("channel").and_then(|v| v.as_u64()).unwrap_or(0);
                                                                    let object_type = json.get("object_type").and_then(|v| v.as_str()).unwrap_or("Unknown");
                                                                    info!("✅ Player {} entered GORC zone {} for {} (ID: {})", player_id, channel, object_type, instance_id);
                                                                }
                                                                Err(e) => {
                                                                    error!("❌ Player {} failed to parse GORC instance ID '{}': {}", player_id, instance_id_str, e);
                                                                }
                                                            }
                                                        } else {
                                                            error!("❌ Player {} received GORC zone enter without instance ID", player_id);
                                                        }
                                                        received_events += 1;
                                                    }
                                                    "gorc_zone_exit" => {
                                                        info!("🎯 Player {} received GORC ZONE EXIT: {:#}", player_id, json);
                                                        received_events += 1;
                                                    }
                                                    "gorc_event" => {
                                                        info!("🎯 Player {} received GORC EVENT: {:#}", player_id, json);
                                                        received_events += 1;
                                                    }
                                                    _ => {
                                                        // Other message types handled below
                                                    }
                                                }
                                            }

                                            // Try parsing as ServerEvent
                                            if let Ok(server_event) = serde_json::from_str::<ServerEvent>(&text) {
                                                received_events += 1;
                                                info!("✅ Player {} parsed valid ServerEvent: {:?}", player_id, server_event);

                                                // Log different types of received events
                                                match server_event.event_type.as_str() {
                                                    "position_update" => {
                                                        if let Some(other_player) = server_event.player_id.as_ref() {
                                                            if *other_player != format!("{}", player_id) {
                                                                info!("📍 Player {} sees {} moved", player_id, other_player);
                                                            }
                                                        }
                                                    }
                                                    "combat_event" => {
                                                        info!("⚔️ Player {} sees combat event", player_id);
                                                    }
                                                    "chat_message" => {
                                                        if let Some(msg) = server_event.data.get("message") {
                                                            info!("💬 Player {} received chat: {}", player_id, msg);
                                                        }
                                                    }
                                                    "level_update" => {
                                                        info!("⭐ Player {} sees level update", player_id);
                                                    }
                                                    "test_event" => {
                                                        info!("🧪 Player {} received test event from server!", player_id);
                                                    }
                                                    _ => {
                                                        info!("📨 Player {} received: {}", player_id, server_event.event_type);
                                                    }
                                                }
                                            } else {
                                                info!("⚠️ Player {} received JSON but not ServerEvent format", player_id);
                                            }
                                        }
                                        Err(e) => {
                                            info!("❌ Player {} failed to parse JSON: {}", player_id, e);
                                        }
                                    }
                                } else {
                                    info!("📝 Player {} received non-JSON message: {}", player_id, text);
                                }
                            }
                            Message::Binary(bin) => {
                                // Try UTF-8 first, otherwise present a truncated hex snippet
                                if let Ok(s) = std::str::from_utf8(&bin) {
                                    info!("📦 Player {} received BINARY (as UTF-8) length {}: {}", player_id, bin.len(), s);
                                } else {
                                    // Truncate long binary payloads in logs
                                    let display_len = 256.min(bin.len());
                                    let hex_snippet: String = bin.iter().take(display_len).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join("");
                                    if bin.len() > display_len {
                                        info!("📦 Player {} received BINARY length {} hex (first {} bytes): {}...", player_id, bin.len(), display_len, hex_snippet);
                                    } else {
                                        info!("📦 Player {} received BINARY length {} hex: {}", player_id, bin.len(), hex_snippet);
                                    }
                                }
                                received_events += 1;
                            }
                            Message::Ping(payload) => {
                                let payload_str = std::str::from_utf8(payload).unwrap_or("<non-utf8>");
                                info!("🔔 Player {} received PING (len {}): {}", player_id, payload.len(), payload_str);
                                received_events += 1;
                            }
                            Message::Pong(payload) => {
                                let payload_str = std::str::from_utf8(payload).unwrap_or("<non-utf8>");
                                info!("🔔 Player {} received PONG (len {}): {}", player_id, payload.len(), payload_str);
                                received_events += 1;
                            }
                            Message::Close(frame) => {
                                info!("🔌 Player {} received CLOSE: {:?}", player_id, frame);
                                // Do not increment received_events for close; we'll break below
                            }
                            _ => {
                                info!("📨 Player {} received unhandled message variant: {:?}", player_id, message);
                                received_events += 1;
                            }
                        }

                        // If the message was a Close, stop the loop
                        if let Message::Close(_) = message {
                            info!("🔌 Player {} connection closed by server", player_id);
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        warn!("⚠️ Player {} WebSocket error: {}", player_id, e);
                        break;
                    }
                    None => {
                        info!("🔌 Player {} connection closed (stream ended)", player_id);
                        break;
                    }
                }
            }
            
            // Send movement updates
            _ = move_timer.tick() => {
                let delta_time = 1.0 / args.move_freq as f32;
                if player.update_movement(delta_time, args.world_size) {
                    if let Some(move_msg) = player.create_move_message() {
                        let json = serde_json::to_string(&move_msg)?;
                        
                        // Log outgoing message to file
                        message_logger.log_sent_message(player_id, &json).await;
                        
                        // Log outgoing message details  
                        info!("📤 Player {} sending movement (event #{}) to server: {}", player_id, sent_events + 1, json);
                        
                        if let Err(e) = ws_sender.send(Message::Text(json)).await {
                            error!("❌ Player {} failed to send movement: {}", player_id, e);
                            break;
                        }
                        sent_events += 1;
                        
                        if sent_events % 50 == 0 {
                            info!("📊 Player {} has sent {} events so far", player_id, sent_events);
                        }
                    } else {
                        // No server instance ID yet, skip sending movement
                        if sent_events % 50 == 0 {
                            info!("⏳ Player {} waiting for GORC zone enter message from server... (attempt #{})", player_id, sent_events + 1);
                        }
                    }
                }
            }
            
            // Send chat messages
            _ = chat_timer.tick() => {
                use rand::Rng;
                let message_idx = {
                    let mut rng = rand::thread_rng();
                    rng.gen_range(0..space_chat_messages.len())
                };
                let message = space_chat_messages[message_idx];
                if let Some(chat_msg) = player.create_chat_message(message) {
                    let json = serde_json::to_string(&chat_msg)?;
                    
                    // Log outgoing message to file
                    message_logger.log_sent_message(player_id, &json).await;
                    
                    if let Err(e) = ws_sender.send(Message::Text(json)).await {
                        error!("❌ Player {} failed to send chat: {}", player_id, e);
                        break;
                    }
                    sent_events += 1;
                    info!("📡 Player {} transmits: '{}'", player_id, message);
                }
            }
            
            // Send combat actions - space weapons fire
            _ = attack_timer.tick() => {
                if let Some(attack_msg) = player.create_attack_message() {
                    let json = serde_json::to_string(&attack_msg)?;
                    
                    // Log outgoing message to file
                    message_logger.log_sent_message(player_id, &json).await;
                    
                    if let Err(e) = ws_sender.send(Message::Text(json)).await {
                        error!("❌ Player {} failed to send combat action: {}", player_id, e);
                        break;
                    }
                    sent_events += 1;
                    info!("⚡ Player {} fires plasma weapons from {:?}", player_id, player.position);
                }
            }
            
            // Send detailed scans - metadata channel
            _ = level_timer.tick() => {
                if let Some(scan_msg) = player.create_scan_message() {
                    let json = serde_json::to_string(&scan_msg)?;
                    
                    // Log outgoing message to file
                    message_logger.log_sent_message(player_id, &json).await;
                    
                    if let Err(e) = ws_sender.send(Message::Text(json)).await {
                        error!("❌ Player {} failed to send ship scan: {}", player_id, e);
                        break;
                    }
                    sent_events += 1;
                    info!("🔍 Player {} performs detailed ship scan (level {})", player_id, player.level);
                }
            }
            
            // Check simulation duration
            _ = sleep(Duration::from_millis(100)) => {
                if start_time.elapsed() >= simulation_duration {
                    info!("⏰ Player {} simulation complete", player_id);
                    break;
                }
            }
        }
    }
    
    info!(
        "📊 Player {} final stats: sent {} events, received {} events",
        player_id, sent_events, received_events
    );
    
    Ok(())
}

/// Calculate spawn positions in a circular formation
fn calculate_spawn_positions(num_players: u32, world_size: f32) -> Vec<Vec3> {
    let mut positions = Vec::new();
    let spawn_radius = world_size / 4.0; // Keep spawns in center area
    
    for i in 0..num_players {
        let angle = 2.0 * std::f32::consts::PI * (i as f32) / (num_players as f32);
        let x = spawn_radius * angle.cos();
        let z = spawn_radius * angle.sin();
        positions.push(Vec3::new(x as f64, 0.0, z as f64));
    }
    
    positions
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args = Args::parse();
    
    info!("🚀 Starting Horizon Space MMO Client Demonstration");
    info!("📊 Space Sector Configuration:");
    info!("   • Space Ships: {}", args.players);
    info!("   • Sector Size: {}x{} meters", args.world_size, args.world_size);
    info!("   • Ship Movement: {:.1} Hz", args.move_freq);
    info!("   • Communications: {:.1} msg/min", args.chat_freq);
    info!("   • Weapon Fire: {:.1} shots/min", args.attack_freq);
    info!("   • Mission Duration: {} seconds", args.duration);
    info!("   • Control Server: {}", args.url);
    
    if args.log_messages {
        info!("📄 JSON Message logging enabled: {}", args.log_file);
    }
    
    // Create message logger
    let message_logger = MessageLogger::new(&args.log_file, args.log_messages).await?;

    // Calculate spawn positions
    let spawn_positions = calculate_spawn_positions(args.players, args.world_size);
    
    // Start all player simulations concurrently
    let mut handles = Vec::new();
    
    for i in 0..args.players {
        let player_id = PlayerId::new();
        let spawn_pos = spawn_positions[i as usize];
        let ws_url = args.url.clone();
        let args_clone = Args {
            url: args.url.clone(),
            players: args.players,
            move_freq: args.move_freq,
            chat_freq: args.chat_freq,
            attack_freq: args.attack_freq,
            duration: args.duration,
            world_size: args.world_size,
            log_messages: args.log_messages,
            log_file: args.log_file.clone(),
        };
        
        let logger_clone = message_logger.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = simulate_player(player_id, ws_url, args_clone, spawn_pos, logger_clone).await {
                error!("❌ Player {} simulation failed: {}", player_id, e);
            }
        });
        
        handles.push(handle);
        
        // Stagger connections to avoid overwhelming server
        sleep(Duration::from_millis(100)).await;
    }
    
    info!("🛸 All {} space ships deployed to sector", args.players);
    
    // Wait for all simulations to complete
    for handle in handles {
        let _ = handle.await;
    }
    
    info!("✅ Horizon Space MMO Client Simulation Complete!");
    
    // Summary based on EVENT_SYSTEM_GUIDE.md
    info!("");
    info!("📋 Horizon GORC Replication System Demonstration:");
    info!("🌌 Zone-based Event Distribution (Space MMO Scale):");
    info!("   • Channel 0 (1000m): Ship position updates at {} Hz", args.move_freq);
    info!("   • Channel 1 (500m): Plasma weapon fire at {:.1}/min", args.attack_freq);
    info!("   • Channel 2 (300m): Space communications at {:.1}/min", args.chat_freq);
    info!("   • Channel 3 (100m): Detailed ship scans every 30s");
    info!("");
    info!("🚀 Real Game Scenario Validation:");
    info!("   • Ships within 1km should track each other's movement");
    info!("   • Ships within 500m should see combat effects and weapon fire");
    info!("   • Ships within 300m should receive local space communications");
    info!("   • Ships within 100m should get detailed scan results and cargo data");
    info!("   • Ships outside ranges should NOT receive those events (bandwidth optimization)");
    info!("");
    info!("🛸 This demonstrates how Horizon would handle a real space MMO with:");
    info!("   • Thousands of players in space sectors");
    info!("   • Distance-based event filtering");
    info!("   • Efficient bandwidth usage");
    info!("   • Proper client-server GORC event routing");
    info!("");
    if args.log_messages {
        info!("📄 All JSON messages logged to: {}", args.log_file);
        info!("   Use this file to analyze message content and improve the system!");
    }
    
    Ok(())
}