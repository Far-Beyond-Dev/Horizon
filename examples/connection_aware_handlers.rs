//! # Connection-Aware Event Handler Example
//!
//! This example demonstrates the new connection-aware event handler system
//! that allows handlers to respond directly to the client that triggered the event.
//! This enables high-performance request-response patterns with minimal latency.

use horizon_event_system::{
    create_horizon_event_system, EventSystem, ClientConnectionRef, RawClientMessageEvent, 
    current_timestamp, EventError, Event, PlayerConnectedEvent, PlayerId
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::Duration;
use tracing::info;

// Example event types that might come from clients
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessageEvent {
    pub message: String,
    pub channel: String,
    pub timestamp: u64,
}

impl Event for ChatMessageEvent {
    fn type_name() -> &'static str {
        "ChatMessageEvent"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatResponse {
    pub message_id: String,
    pub status: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoginRequestEvent {
    pub username: String,
    pub password_hash: String,
}

impl Event for LoginRequestEvent {
    fn type_name() -> &'static str {
        "LoginRequestEvent"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoginResponse {
    pub success: bool,
    pub session_token: Option<String>,
    pub error_message: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::init();
    
    let events = create_horizon_event_system();
    
    info!("🚀 Setting up connection-aware event handlers...");

    // Example 1: Chat handler with direct response capability
    events.on_client_with_connection("chat", "send_message", 
        |event: ChatMessageEvent, client: ClientConnectionRef| async move {
            info!("📨 Received chat message from {}: {}", client.player_id, event.message);
            
            // Process the message (validate, store, broadcast, etc.)
            tokio::time::sleep(Duration::from_millis(5)).await; // Simulate processing
            
            // Send immediate acknowledgment to the sender
            let response = ChatResponse {
                message_id: format!("msg_{}", current_timestamp()),
                status: "received".to_string(),
                timestamp: current_timestamp(),
            };
            
            client.respond_json(&response).await?;
            info!("✅ Sent acknowledgment to {}", client.player_id);
            
            Ok(())
        }
    ).await?;

    // Example 2: Login handler with async database operations
    events.on_client_with_connection("auth", "login",
        |event: LoginRequestEvent, client: ClientConnectionRef| async move {
            info!("🔐 Login attempt from {} for user: {}", client.player_id, event.username);
            
            // Simulate async database verification
            tokio::time::sleep(Duration::from_millis(50)).await;
            
            let response = if event.username == "admin" {
                LoginResponse {
                    success: true,
                    session_token: Some("session_abc123".to_string()),
                    error_message: None,
                }
            } else {
                LoginResponse {
                    success: false,
                    session_token: None,
                    error_message: Some("Invalid credentials".to_string()),
                }
            };
            
            client.respond_json(&response).await?;
            
            if response.success {
                info!("✅ Login successful for {}", event.username);
            } else {
                info!("❌ Login failed for {}", event.username);
            }
            
            Ok(())
        }
    ).await?;

    // Example 3: Regular async handler without connection awareness
    events.on_client_async("inventory", "use_item",
        |event: RawClientMessageEvent| async move {
            info!("🎒 Processing inventory action for {}", event.player_id);
            
            // Simulate async game logic processing
            tokio::time::sleep(Duration::from_millis(10)).await;
            
            // This handler processes the action but doesn't need to respond directly
            // It might trigger other events or update game state
            
            Ok(())
        }
    ).await?;

    // Example 4: High-performance handler for frequent events
    events.on_client_async("movement", "position_update",
        |event: RawClientMessageEvent| async move {
            // Ultra-fast processing for position updates
            // No logging or heavy processing to maintain 500k msg/sec target
            
            // Just validate and forward to game state
            // This would update spatial partitioning, notify nearby players, etc.
            
            Ok(())
        }
    ).await?;

    info!("📊 Event System Features Demonstrated:");
    info!("  ✅ Connection-aware handlers with direct client response");
    info!("  ✅ Async handlers for database/IO operations");
    info!("  ✅ High-performance handlers for frequent events");
    info!("  ✅ Parallel handler execution for maximum throughput");
    
    // Simulate some events being triggered
    info!("\n🎮 Simulating client events...");
    
    // These would normally come from actual client connections
    let chat_event = ChatMessageEvent {
        message: "Hello, everyone!".to_string(),
        channel: "general".to_string(),
        timestamp: current_timestamp(),
    };
    
    let login_event = LoginRequestEvent {
        username: "admin".to_string(),
        password_hash: "hashed_password".to_string(),
    };
    
    // Demonstrate actual connection events as they are emitted by the real server
    info!("💡 These events demonstrate the actual client connection integration:");
    
    // Emit realistic connection events like the game server does
    let player_connected = PlayerConnectedEvent {
        player_id: PlayerId::new(),
        connection_id: "conn_12345".to_string(),
        remote_addr: "192.168.1.100:45678".to_string(), 
        timestamp: current_timestamp(),
    };
    
    events.emit_core("player_connected", &player_connected).await?;
    info!("  ✅ PlayerConnectedEvent emitted (as done by game server)");
    
    // Emit client events through the namespace system
    events.emit_client("chat", "message", &chat_event).await?;
    info!("  ✅ Client chat event routed through namespace system");
    
    events.emit_client("auth", "login_request", &login_event).await?;  
    info!("  ✅ Client auth event processed by connection-aware handlers");
    
    let stats = events.get_stats().await;
    info!("\n📈 System Statistics:");
    info!("  - Total handlers: {}", stats.total_handlers);
    info!("  - Events emitted: {}", stats.events_emitted);
    
    Ok(())
}