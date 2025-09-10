//! # GORC Player Test Client
//!
//! Simulates realistic player clients connecting to the server and sending
//! GORC-formatted events to test the complete event chain and zone-based replication.

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

#[derive(Parser, Debug)]
#[command(name = "player-simulator")]
#[command(about = "GORC Player Simulation Client")]
struct Args {
    /// Server WebSocket URL
    #[arg(short, long, default_value = "ws://localhost:8081/ws")]
    url: String,
    
    /// Number of simultaneous players to simulate
    #[arg(short, long, default_value = "5")]
    players: u32,
    
    /// Movement frequency in Hz
    #[arg(short, long, default_value = "10.0")]
    move_freq: f64,
    
    /// Chat frequency in messages per minute
    #[arg(short, long, default_value = "2.0")]
    chat_freq: f64,
    
    /// Attack frequency per minute
    #[arg(short, long, default_value = "5.0")]
    attack_freq: f64,
    
    /// Simulation duration in seconds
    #[arg(short, long, default_value = "60")]
    duration: u64,
    
    /// World size (square area)
    #[arg(short, long, default_value = "1000.0")]
    world_size: f32,
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
    fn is_in_range(&self, player1: PlayerId, player2: PlayerId, channel: u8) -> bool {
        if let (Some(pos1), Some(pos2)) = (self.player_positions.get(&player1), self.player_positions.get(&player2)) {
            let distance = pos1.distance(*pos2);
            match channel {
                0 => distance <= 25.0,  // Critical: 25m range
                1 => distance <= 100.0, // Detailed: 100m range  
                2 => distance <= 200.0, // Social: 200m range
                3 => distance <= 1000.0, // Metadata: 1000m range
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

    /// Create a GORC movement message (returns None if no server instance ID yet)
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
            }, // 1 = moving
            client_timestamp: chrono::Utc::now(),
        };

        Some(GorcClientMessage {
            msg_type: "gorc_event".to_string(),
            object_id: format!("{:?}", instance_id),
            channel: 0, // Critical channel for movement
            event: "move".to_string(),
            data: serde_json::to_value(&move_request).unwrap(),
            player_id: format!("{}", self.player_id),
        })
    }

    /// Create a GORC attack message (returns None if no server instance ID yet)
    fn create_attack_message(&self) -> Option<GorcClientMessage> {
        let instance_id = self.server_gorc_instance_id?;
        use rand::Rng;
        let (offset_x, offset_z) = {
            let mut rng = rand::thread_rng();
            (
                rng.gen_range(-10.0_f64..10.0_f64),
                rng.gen_range(-10.0_f64..10.0_f64),
            )
        };
        let target_offset = Vec3::new(offset_x, 0.0, offset_z);

        let attack_request = PlayerAttackRequest {
            player_id: self.player_id,
            target_position: Vec3::new(
                self.position.x + target_offset.x,
                self.position.y + target_offset.y,
                self.position.z + target_offset.z,
            ),
            attack_type: "sword_slash".to_string(),
            client_timestamp: chrono::Utc::now(),
        };

        Some(GorcClientMessage {
            msg_type: "gorc_event".to_string(),
            object_id: format!("{:?}", instance_id),
            channel: 1, // Detailed channel for combat
            event: "attack".to_string(),
            data: serde_json::to_value(&attack_request).unwrap(),
            player_id: format!("{}", self.player_id),
        })
    }

    /// Create a GORC chat message (returns None if no server instance ID yet)
    fn create_chat_message(&self, message: &str) -> Option<GorcClientMessage> {
        let instance_id = self.server_gorc_instance_id?;
        let chat_request = PlayerChatRequest {
            player_id: self.player_id,
            message: message.to_string(),
            channel: "local".to_string(),
            target_player: None,
        };

        Some(GorcClientMessage {
            msg_type: "gorc_event".to_string(),
            object_id: format!("{:?}", instance_id),
            channel: 2, // Social channel for chat
            event: "chat".to_string(),
            data: serde_json::to_value(&chat_request).unwrap(),
            player_id: format!("{}", self.player_id),
        })
    }

    /// Create a level up message (returns None if no server instance ID yet)
    fn create_level_up_message(&mut self) -> Option<GorcClientMessage> {
        let instance_id = self.server_gorc_instance_id?;
        self.level += 1;

        Some(GorcClientMessage {
            msg_type: "gorc_event".to_string(),
            object_id: format!("{:?}", instance_id),
            channel: 3, // Metadata channel for progression
            event: "level_up".to_string(),
            data: serde_json::json!({
                "player_id": self.player_id,
                "level": self.level,
                "experience": self.level * 1000
            }),
            player_id: format!("{}", self.player_id),
        })
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
    
    let chat_messages = [
        "Hello everyone!",
        "Anyone want to team up?",
        "Great weather today!",
        "Found a nice spot here",
        "Watch out for monsters",
        "GG everyone",
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
                        if sent_events == 0 {
                            info!("⏳ Player {} waiting for GORC zone enter message from server...", player_id);
                        }
                    }
                }
            }
            
            // Send chat messages
            _ = chat_timer.tick() => {
                use rand::Rng;
                let message_idx = {
                    let mut rng = rand::thread_rng();
                    rng.gen_range(0..chat_messages.len())
                };
                let message = chat_messages[message_idx];
                if let Some(chat_msg) = player.create_chat_message(message) {
                    let json = serde_json::to_string(&chat_msg)?;
                    
                    if let Err(e) = ws_sender.send(Message::Text(json)).await {
                        error!("❌ Player {} failed to send chat: {}", player_id, e);
                        break;
                    }
                    sent_events += 1;
                    info!("💬 Player {} says: '{}'", player_id, message);
                }
            }
            
            // Send attack actions
            _ = attack_timer.tick() => {
                if let Some(attack_msg) = player.create_attack_message() {
                    let json = serde_json::to_string(&attack_msg)?;
                    
                    if let Err(e) = ws_sender.send(Message::Text(json)).await {
                        error!("❌ Player {} failed to send attack: {}", player_id, e);
                        break;
                    }
                    sent_events += 1;
                    info!("⚔️ Player {} attacks at {:?}", player_id, player.position);
                }
            }
            
            // Send level up events
            _ = level_timer.tick() => {
                if let Some(level_msg) = player.create_level_up_message() {
                    let json = serde_json::to_string(&level_msg)?;
                    
                    if let Err(e) = ws_sender.send(Message::Text(json)).await {
                        error!("❌ Player {} failed to send level up: {}", player_id, e);
                        break;
                    }
                    sent_events += 1;
                    info!("⭐ Player {} leveled up to {}", player_id, player.level);
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
    
    info!("🚀 Starting GORC Player Simulation");
    info!("📊 Configuration:");
    info!("   • Players: {}", args.players);
    info!("   • World Size: {}x{}", args.world_size, args.world_size);
    info!("   • Movement Freq: {:.1} Hz", args.move_freq);
    info!("   • Chat Freq: {:.1} msg/min", args.chat_freq);
    info!("   • Attack Freq: {:.1} attacks/min", args.attack_freq);
    info!("   • Duration: {} seconds", args.duration);
    info!("   • Server URL: {}", args.url);

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
        };
        
        let handle = tokio::spawn(async move {
            if let Err(e) = simulate_player(player_id, ws_url, args_clone, spawn_pos).await {
                error!("❌ Player {} simulation failed: {}", player_id, e);
            }
        });
        
        handles.push(handle);
        
        // Stagger connections to avoid overwhelming server
        sleep(Duration::from_millis(100)).await;
    }
    
    info!("🎮 All {} players started", args.players);
    
    // Wait for all simulations to complete
    for handle in handles {
        let _ = handle.await;
    }
    
    info!("✅ GORC Player Simulation complete!");
    
    // Summary of what should have happened:
    info!("");
    info!("📋 Expected GORC Behavior Summary:");
    info!("🌐 Zone-based Event Distribution:");
    info!("   • Channel 0 (25m): Position updates at {} Hz", args.move_freq);
    info!("   • Channel 1 (100m): Combat events at {:.1}/min", args.attack_freq);
    info!("   • Channel 2 (200m): Chat messages at {:.1}/min", args.chat_freq);
    info!("   • Channel 3 (1000m): Level updates every 30s");
    info!("");
    info!("📡 Replication Logic:");
    info!("   • Players within 25m should see smooth position updates");
    info!("   • Players within 100m should see combat animations");
    info!("   • Players within 200m should receive chat messages");
    info!("   • Players within 1000m should see level/status changes");
    info!("   • Players outside ranges should NOT receive those events");
    
    Ok(())
}