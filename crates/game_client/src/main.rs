// test-client/src/main.rs - Enhanced test client for horizon plugin

use anyhow::Result;
use clap::Parser;
use futures::{SinkExt, StreamExt};
use shared_types::{NetworkMessage, Position};
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info};
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Server WebSocket URL
    #[arg(short, long, default_value = "ws://127.0.0.1:8080")]
    url: String,
    
    /// Test scenario to run
    #[arg(short, long, default_value = "basic")]
    scenario: String,
    
    /// Player name
    #[arg(short, long, default_value = "TestPlayer")]
    name: String,
    
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    
    info!("Starting test client for distributed games server");
    info!("Server URL: {}", args.url);
    info!("Scenario: {}", args.scenario);
    
    match args.scenario.as_str() {
        "basic" => run_basic_test(&args).await?,
        "movement" => run_movement_test(&args).await?,
        "plugin" => run_plugin_test(&args).await?,
        "horizon" => run_horizon_test(&args).await?,
        "stress" => run_stress_test(&args).await?,
        _ => {
            error!("Unknown scenario: {}", args.scenario);
            return Ok(());
        }
    }
    
    Ok(())
}

async fn run_basic_test(args: &Args) -> Result<()> {
    info!("Running basic connection test...");
    
    let url = Url::parse(&args.url)?;
    let (ws_stream, _) = connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();
    
    // Start message handler task
    let message_handler = tokio::spawn(async move {
        let mut count = 0;
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    count += 1;
                    info!("Received message {}: {}", count, text);
                    
                    // Parse and display the response
                    if let Ok(network_msg) = serde_json::from_str::<NetworkMessage>(&text) {
                        match network_msg {
                            NetworkMessage::GameData { data } => {
                                info!("Game data received: {}", serde_json::to_string_pretty(&data).unwrap_or_default());
                            }
                            _ => {
                                info!("Other message type received: {:?}", network_msg);
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Server closed connection");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
        count
    });
    
    // Send join message
    let join_msg = NetworkMessage::PlayerJoin { 
        name: args.name.clone() 
    };
    let join_text = serde_json::to_string(&join_msg)?;
    write.send(Message::Text(join_text)).await?;
    info!("Sent join message for player: {}", args.name);
    
    // Wait a bit to receive responses
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Send some test data
    let test_data = NetworkMessage::GameData {
        data: serde_json::json!({
            "test": "Hello from client",
            "timestamp": chrono::Utc::now().to_rfc3339()
        })
    };
    let test_text = serde_json::to_string(&test_data)?;
    write.send(Message::Text(test_text)).await?;
    info!("Sent test data message");
    
    // Wait for responses
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Send leave message
    let leave_msg = NetworkMessage::PlayerLeave;
    let leave_text = serde_json::to_string(&leave_msg)?;
    write.send(Message::Text(leave_text)).await?;
    info!("Sent leave message");
    
    // Wait for final responses
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Close connection gracefully
    write.send(Message::Close(None)).await?;
    
    // Wait for message handler to finish
    let final_count = timeout(Duration::from_secs(5), message_handler).await??;
    
    info!("Total messages received: {}", final_count);
    info!("Basic test completed successfully");
    
    Ok(())
}

async fn run_movement_test(args: &Args) -> Result<()> {
    info!("Running movement test...");
    
    let url = Url::parse(&args.url)?;
    let (ws_stream, _) = connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();
    
    // Start message handler
    let message_handler = tokio::spawn(async move {
        let mut count = 0;
        while let Some(msg) = read.next().await {
            if let Ok(Message::Text(text)) = msg {
                count += 1;
                info!("Movement response {}: {}", count, text);
            }
        }
        count
    });
    
    // Join first
    let join_msg = NetworkMessage::PlayerJoin { 
        name: args.name.clone() 
    };
    write.send(Message::Text(serde_json::to_string(&join_msg)?)).await?;
    info!("Joined as {}", args.name);
    
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Send movement commands
    let positions = vec![
        Position::new(10.0, 0.0, 0.0),
        Position::new(20.0, 10.0, 0.0),
        Position::new(0.0, 20.0, 0.0),
        Position::new(-10.0, 0.0, 0.0),
        Position::new(0.0, 0.0, 0.0),
    ];
    
    for (i, pos) in positions.iter().enumerate() {
        let move_msg = NetworkMessage::PlayerMove { position: *pos };
        write.send(Message::Text(serde_json::to_string(&move_msg)?)).await?;
        info!("Sent movement {} to position {:?}", i + 1, pos);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    // Leave
    let leave_msg = NetworkMessage::PlayerLeave;
    write.send(Message::Text(serde_json::to_string(&leave_msg)?)).await?;
    write.send(Message::Close(None)).await?;
    
    let final_count = timeout(Duration::from_secs(5), message_handler).await??;
    info!("Movement test completed. Received {} responses", final_count);
    
    Ok(())
}

async fn run_plugin_test(args: &Args) -> Result<()> {
    info!("Running plugin interaction test...");
    
    let url = Url::parse(&args.url)?;
    let (ws_stream, _) = connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();
    
    // Start message handler
    let message_handler = tokio::spawn(async move {
        let mut count = 0;
        while let Some(msg) = read.next().await {
            if let Ok(Message::Text(text)) = msg {
                count += 1;
                info!("Plugin response {}: {}", count, text);
            }
        }
        count
    });
    
    // Join first
    let join_msg = NetworkMessage::PlayerJoin { 
        name: args.name.clone() 
    };
    write.send(Message::Text(serde_json::to_string(&join_msg)?)).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Send plugin messages
    let horizon_msg = NetworkMessage::PluginMessage {
        plugin: "horizon".to_string(),
        data: serde_json::json!({
            "type": "SetHorizonDistance",
            "data": {
                "player_id": "test-player-id",
                "distance": 150.0
            }
        })
    };
    write.send(Message::Text(serde_json::to_string(&horizon_msg)?)).await?;
    info!("Sent horizon plugin message");
    
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    let info_msg = NetworkMessage::PluginMessage {
        plugin: "horizon".to_string(),
        data: serde_json::json!({
            "type": "GetHorizonInfo",
            "data": {
                "player_id": "test-player-id"
            }
        })
    };
    write.send(Message::Text(serde_json::to_string(&info_msg)?)).await?;
    info!("Sent horizon info request");
    
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Leave
    let leave_msg = NetworkMessage::PlayerLeave;
    write.send(Message::Text(serde_json::to_string(&leave_msg)?)).await?;
    write.send(Message::Close(None)).await?;
    
    let final_count = timeout(Duration::from_secs(5), message_handler).await??;
    info!("Plugin test completed. Received {} responses", final_count);
    
    Ok(())
}

async fn run_horizon_test(args: &Args) -> Result<()> {
    info!("Running horizon plugin test...");
    
    let url = Url::parse(&args.url)?;
    let (ws_stream, _) = connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();
    
    let mut player_id: Option<String> = None;
    
    // Start message handler task with player ID extraction
    let (id_sender, mut id_receiver) = tokio::sync::mpsc::channel::<String>(1);
    
    let message_handler = tokio::spawn(async move {
        let mut count = 0;
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    count += 1;
                    info!("Horizon response {}: {}", count, text);
                    
                    // Try to extract player ID from join success response
                    if let Ok(network_msg) = serde_json::from_str::<NetworkMessage>(&text) {
                        if let NetworkMessage::GameData { data } = network_msg {
                            if let Some(msg_type) = data.get("type").and_then(|v| v.as_str()) {
                                if msg_type == "join_success" {
                                    if let Some(pid) = data.get("player_id").and_then(|v| v.as_str()) {
                                        let _ = id_sender.send(pid.to_string()).await;
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Server closed connection");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
        count
    });
    
    // Send join message - this will trigger PlayerJoined event and horizon plugin initialization
    let join_msg = NetworkMessage::PlayerJoin { 
        name: args.name.clone() 
    };
    write.send(Message::Text(serde_json::to_string(&join_msg)?)).await?;
    info!("Sent join message for player: {} (this triggers horizon plugin)", args.name);
    
    // Wait for join response and extract player ID
    tokio::time::sleep(Duration::from_millis(500)).await;
    if let Ok(pid) = id_receiver.try_recv() {
        player_id = Some(pid);
        info!("Extracted player ID: {}", player_id.as_ref().unwrap());
    }
    
    let pid = player_id.as_ref().unwrap_or(&"fallback-id".to_string()).clone();
    
    // Move around a bit to trigger player movement events (horizon visibility updates)
    info!("Sending movement commands to trigger horizon visibility updates...");
    let positions = vec![
        Position::new(50.0, 0.0, 0.0),
        Position::new(75.0, 25.0, 0.0),
        Position::new(100.0, 50.0, 0.0),
    ];
    
    for (i, pos) in positions.iter().enumerate() {
        let move_msg = NetworkMessage::PlayerMove { position: *pos };
        write.send(Message::Text(serde_json::to_string(&move_msg)?)).await?;
        info!("Movement {}: {:?} (triggers horizon visibility update)", i + 1, pos);
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    
    // Send horizon-specific messages as GameData (these get processed as custom_message events)
    info!("Sending horizon plugin commands...");
    
    // Set horizon distance - this will trigger the horizon plugin
    let set_horizon_msg = NetworkMessage::GameData {
        data: serde_json::json!({
            "type": "SetHorizonDistance",
            "data": {
                "player_id": pid,
                "distance": 200.0
            }
        })
    };
    write.send(Message::Text(serde_json::to_string(&set_horizon_msg)?)).await?;
    info!("Sent SetHorizonDistance command (triggers horizon plugin)");
    
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Get horizon info
    let get_horizon_msg = NetworkMessage::GameData {
        data: serde_json::json!({
            "type": "GetHorizonInfo", 
            "data": {
                "player_id": pid
            }
        })
    };
    write.send(Message::Text(serde_json::to_string(&get_horizon_msg)?)).await?;
    info!("Sent GetHorizonInfo command (triggers horizon plugin)");
    
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Move again to trigger another visibility update with new horizon distance
    let final_move = NetworkMessage::PlayerMove { 
        position: Position::new(150.0, 100.0, 0.0) 
    };
    write.send(Message::Text(serde_json::to_string(&final_move)?)).await?;
    info!("Final movement to test new horizon distance");
    
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Send leave message - this will trigger PlayerLeft event
    let leave_msg = NetworkMessage::PlayerLeave;
    write.send(Message::Text(serde_json::to_string(&leave_msg)?)).await?;
    info!("Sent leave message (triggers horizon cleanup)");
    
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Close connection gracefully
    write.send(Message::Close(None)).await?;
    
    // Wait for message handler to finish
    let final_count = timeout(Duration::from_secs(5), message_handler).await??;
    
    info!("Horizon test completed. Received {} responses", final_count);
    info!("Events triggered:");
    info!("  - PlayerJoined (sets up horizon for player)");
    info!("  - PlayerMoved (updates visibility calculations)");
    info!("  - SetHorizonDistance (changes horizon distance)");
    info!("  - GetHorizonInfo (requests horizon information)");
    info!("  - PlayerLeft (cleans up horizon data)");
    
    Ok(())
}

async fn run_stress_test(args: &Args) -> Result<()> {
    info!("Running stress test with multiple rapid messages...");
    
    let url = Url::parse(&args.url)?;
    let (ws_stream, _) = connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();
    
    // Start message handler
    let message_handler = tokio::spawn(async move {
        let mut count = 0;
        while let Some(msg) = read.next().await {
            if let Ok(Message::Text(_)) = msg {
                count += 1;
                if count % 10 == 0 {
                    info!("Received {} messages so far...", count);
                }
            }
        }
        count
    });
    
    // Join
    let join_msg = NetworkMessage::PlayerJoin { 
        name: args.name.clone() 
    };
    write.send(Message::Text(serde_json::to_string(&join_msg)?)).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Send rapid messages
    for i in 0..50 {
        let test_msg = NetworkMessage::GameData {
            data: serde_json::json!({
                "message_id": i,
                "data": format!("Stress test message {}", i)
            })
        };
        write.send(Message::Text(serde_json::to_string(&test_msg)?)).await?;
        
        if i % 10 == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    
    info!("Sent 50 rapid messages, waiting for responses...");
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Leave
    let leave_msg = NetworkMessage::PlayerLeave;
    write.send(Message::Text(serde_json::to_string(&leave_msg)?)).await?;
    write.send(Message::Close(None)).await?;
    
    let final_count = timeout(Duration::from_secs(10), message_handler).await??;
    info!("Stress test completed. Received {} responses", final_count);
    
    Ok(())
}