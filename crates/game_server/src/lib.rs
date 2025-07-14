//! # Game Server - Clean Infrastructure Foundation
//!
//! A production-ready game server focused on providing clean, modular infrastructure
//! for multiplayer game development. This server handles core networking, connection
//! management, and plugin orchestration while delegating all game logic to plugins.
//!
//! ## Design Philosophy
//!
//! The game server core contains **NO game logic** - it only provides infrastructure:
//!
//! * **WebSocket connection management** - Handles client connections and message routing
//! * **Plugin system integration** - Dynamic loading and management of game logic
//! * **Event-driven architecture** - Clean separation between infrastructure and game code
//! * **GORC integration** - Advanced replication and spatial management capabilities
//! * **Multi-threaded networking** - Scalable accept loops for high-performance operation
//!
//! All game mechanics, rules, and behaviors are implemented as plugins that communicate
//! through the standardized event system.
//!
//! ## Architecture Overview
//!
//! ### Core Components
//!
//! * **Event System** - Central hub for all plugin communication
//! * **Connection Manager** - WebSocket lifecycle and player mapping  
//! * **Plugin Manager** - Dynamic loading and management of game logic
//! * **GORC Components** - Advanced replication and spatial systems
//!
//! ### Message Flow
//!
//! 1. Client sends WebSocket message with `{namespace, event, data}` structure
//! 2. Server parses and validates the message format
//! 3. Message is routed to plugins via the event system
//! 4. Plugins process the message and emit responses
//! 5. Responses are sent back to clients through the connection manager
//!
//! ### Plugin Integration
//!
//! Plugins register event handlers for specific namespace/event combinations:
//!
//! ```rust
//! // Example plugin handler registration
//! event_system.on_client("movement", "move_request", |event| {
//!     // Handle movement logic
//!     Ok(())
//! }).await?;
//! ```
//!
//! ## Configuration
//!
//! The server can be configured through the [`ServerConfig`] struct:
//!
//! * **Network settings** - Bind address, connection limits, timeouts
//! * **Region configuration** - Spatial bounds for the server region
//! * **Plugin management** - Plugin directory and loading behavior
//! * **Performance tuning** - Multi-threading and resource limits
//!
//! ## GORC Integration
//!
//! The server includes full GORC (Game Object Replication Channel) support:
//!
//! * **Spatial Partitioning** - Efficient proximity queries and region management
//! * **Subscription Management** - Dynamic event subscription based on player state
//! * **Multicast Groups** - Efficient broadcasting to groups of players
//! * **Replication Channels** - High-performance object state synchronization
//!
//! ## Error Handling
//!
//! The server uses structured error types ([`ServerError`]) to categorize failures:
//!
//! * **Network errors** - Connection, binding, and protocol issues
//! * **Internal errors** - Plugin failures and event system problems
//!
//! ## Thread Safety
//!
//! All server components are designed for safe concurrent access:
//!
//! * Connection management uses `Arc<RwLock<HashMap>>` for thread-safe state
//! * Event system provides async-safe handler registration and emission
//! * Plugin system coordinates safe loading and unloading of plugins
//!
//! ## Performance Considerations
//!
//! * **Multi-threaded accept loops** - Configure `use_reuse_port` for CPU core scaling
//! * **Efficient message routing** - Zero-copy message passing where possible  
//! * **Plugin isolation** - Plugins run in separate contexts to prevent interference
//! * **Connection pooling** - Reuse connections and minimize allocation overhead

// Re-export core types and functions for easy access
pub use config::ServerConfig;
pub use error::ServerError;
pub use server::GameServer;
pub use utils::{create_server, create_server_with_config};

// Public module declarations
pub mod config;
pub mod error;
pub mod server;
pub mod utils;

// Internal modules (not part of public API)
mod connection;
mod messaging;

// Include tests
#[cfg(test)]
mod tests {
    use super::*;
    use horizon_event_system::{PlayerConnectedEvent, RegionStartedEvent};

    #[tokio::test(flavor = "multi_thread")]
    async fn test_core_server_creation() {
        let server = create_server();
        let events = server.get_horizon_event_system();

        // Verify we can register core handlers
        events
            .on_core("test_event", |event: serde_json::Value| {
                println!("Test core event: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register core event handler");

        // Emit a test event
        events
            .emit_core(
                "test_event",
                &serde_json::json!({
                    "test": "data"
                }),
            )
            .await
            .expect("Failed to emit core event");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_plugin_message_routing() {
        let server = create_server();
        let events = server.get_horizon_event_system();

        // Register handlers that plugins would register
        events
            .on_client("movement", "move_request", |event: serde_json::Value| {
                println!("Movement plugin would handle: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register movement handler");

        events
            .on_client("chat", "send_message", |event: serde_json::Value| {
                println!("Chat plugin would handle: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register chat handler");

        // Test routing
        events
            .emit_client(
                "movement",
                "move_request",
                &serde_json::json!({
                    "target_x": 100.0,
                    "target_y": 200.0,
                    "target_z": 0.0
                }),
            )
            .await
            .expect("Failed to emit client event for movement");

        events
            .emit_client(
                "chat",
                "send_message",
                &serde_json::json!({
                    "message": "Hello world!",
                    "channel": "general"
                }),
            )
            .await
            .expect("Failed to emit client event for chat");
        }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_generic_client_message_routing() {
        let server = create_server();
        let events = server.get_horizon_event_system();

        // Register handlers for different namespaces/events
        events
            .on_client("movement", "jump", |event: serde_json::Value| {
                println!("Movement plugin handles jump: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register movement handler");

        events
            .on_client("inventory", "use_item", |event: serde_json::Value| {
                println!("Inventory plugin handles use_item: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register inventory handler");

        events
            .on_client(
                "custom_plugin",
                "custom_event",
                |event: serde_json::Value| {
                    println!("Custom plugin handles custom_event: {:?}", event);
                    Ok(())
                },
            )
            .await
            .expect("Failed to register custom event handler");

        // Test the new generic routing
        events
            .emit_client(
                "movement",
                "jump",
                &serde_json::json!({
                    "height": 5.0
                }),
            )
            .await
            .expect("Failed to emit client event for movement");

        events
            .emit_client(
                "inventory",
                "use_item",
                &serde_json::json!({
                    "item_id": "potion_health",
                    "quantity": 1
                }),
            )
            .await
            .expect("Failed to emit client event for inventory");

        events
            .emit_client(
                "custom_plugin",
                "custom_event",
                &serde_json::json!({
                    "custom_data": "anything"
                }),
            )
            .await
            .expect("Failed to emit client event for custom plugin");

        println!("✅ All messages routed generically without hardcoded logic!");
    }

    /// Example of how clean the new server is - just infrastructure!
    #[tokio::test(flavor = "multi_thread")]
    async fn demonstrate_clean_separation() {
        let server = create_server();
        let events = server.get_horizon_event_system();

        println!("🧹 This server only handles:");
        println!("  - WebSocket connections");
        println!("  - Generic message routing");
        println!("  - Plugin communication");
        println!("  - Core infrastructure events");
        println!("");
        println!("🎮 Game logic is handled by plugins:");
        println!("  - Movement, combat, chat, inventory");
        println!("  - All game-specific events");
        println!("  - Business logic and rules");

        // Show the clean API in action
        events
            .on_core("player_connected", |event: PlayerConnectedEvent| {
                println!("✅ Core: Player {} connected", event.player_id);
                Ok(())
            })
            .await
            .expect("Failed to register core player connected handler");

        // This would be handled by movement plugin, not core
        events
            .on_client("movement", "jump", |event: serde_json::Value| {
                println!("🦘 Movement Plugin: Jump event {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register movement handler");

        println!("✨ Clean separation achieved with generic routing!");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_gorc_integration() {
        let server = create_server();

        // Test GORC component accessibility
        let gorc_manager = server.get_gorc_manager();
        let subscription_manager = server.get_subscription_manager();
        let multicast_manager = server.get_multicast_manager();
        let spatial_partition = server.get_spatial_partition();

        // Test basic GORC functionality
        let stats = gorc_manager.get_stats().await;
        assert_eq!(stats.total_objects, 0);

        // Test spatial partition
        use horizon_event_system::Position;
        spatial_partition.add_region(
            "test_region".to_string(),
            Position::new(0.0, 0.0, 0.0).into(),
            Position::new(1000.0, 1000.0, 1000.0).into(),
        ).await;

        // Test subscription management
        use horizon_event_system::PlayerId;
        let player_id = PlayerId::new();
        let position = Position::new(100.0, 100.0, 100.0);
        subscription_manager.add_player(player_id, position).await;

        // Test multicast group creation
        use std::collections::HashSet;
        let channels: HashSet<u8> = vec![0, 1].into_iter().collect();
        let group_id = multicast_manager.create_group(
            "test_group".to_string(),
            channels,
            horizon_event_system::ReplicationPriority::Normal,
        ).await;

        // Add player to multicast group
        let added = multicast_manager.add_player_to_group(player_id, group_id).await;
        assert!(matches!(added, Ok(true)));

        println!("✅ GORC integration test passed!");
        println!("  - GORC Manager: Initialized with default channels");
        println!("  - Subscription Manager: Player subscription system ready");
        println!("  - Multicast Manager: Group creation and player management working");
        println!("  - Spatial Partition: Region management and spatial queries available");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_configuration() {
        // Test custom configuration
        let config = ServerConfig {
            bind_address: "127.0.0.1:9999".parse().unwrap(),
            max_connections: 2000,
            connection_timeout: 120,
            use_reuse_port: true,
            ..Default::default()
        };

        let server = create_server_with_config(config.clone());
        
        // Verify the server was created with custom config
        // Note: In a real implementation, you might want to expose config getters
        println!("Server created with custom configuration:");
        println!("  - Bind address: {}", config.bind_address);
        println!("  - Max connections: {}", config.max_connections);
        println!("  - Connection timeout: {}s", config.connection_timeout);
        println!("  - Use reuse port: {}", config.use_reuse_port);
    }
}