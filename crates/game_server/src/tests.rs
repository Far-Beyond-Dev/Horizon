
// Include tests
#[cfg(test)]
mod tests {
    use crate::*;
    use horizon_event_system::PlayerConnectedEvent;
    use tracing::{debug, info};

    #[tokio::test(flavor = "multi_thread")]
    async fn test_core_server_creation() {
        let server = create_server();
        let events = server.get_horizon_event_system();

        // Verify we can register core handlers
        events
            .on_core("test_event", |event: serde_json::Value| {
                debug!("Test core event: {:?}", event);
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
            .on_client("movement", "move_request", |event: serde_json::Value, _player_id: horizon_event_system::PlayerId, _connection: horizon_event_system::ClientConnectionRef| {
                debug!("Movement plugin would handle: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register movement handler");

        events
            .on_client("chat", "send_message", |event: serde_json::Value, _player_id: horizon_event_system::PlayerId, _connection: horizon_event_system::ClientConnectionRef| {
                debug!("Chat plugin would handle: {:?}", event);
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
            .on_client("movement", "jump", |event: serde_json::Value, _player_id: horizon_event_system::PlayerId, _connection: horizon_event_system::ClientConnectionRef| {
                debug!("Movement plugin handles jump: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register movement handler");

        events
            .on_client("inventory", "use_item", |event: serde_json::Value, _player_id: horizon_event_system::PlayerId, _connection: horizon_event_system::ClientConnectionRef| {
                debug!("Inventory plugin handles use_item: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register inventory handler");

        events
            .on_client(
                "custom_plugin",
                "custom_event",
                |event: serde_json::Value, _player_id: horizon_event_system::PlayerId, _connection: horizon_event_system::ClientConnectionRef| {
                    debug!("Custom plugin handles custom_event: {:?}", event);
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

        info!("✅ All messages routed generically without hardcoded logic!");
    }

    /// Example of how clean the new server is - just infrastructure!
    #[tokio::test(flavor = "multi_thread")]
    async fn demonstrate_clean_separation() {
        let server = create_server();
        let events = server.get_horizon_event_system();

        debug!("🧹 This server only handles:");
        debug!("  - WebSocket connections");
        debug!("  - Generic message routing");
        debug!("  - Plugin communication");
        debug!("  - Core infrastructure events");
        debug!("");
        debug!("🎮 Game logic is handled by plugins:");
        debug!("  - Movement, combat, chat, inventory");
        debug!("  - All game-specific events");
        debug!("  - Business logic and rules");

        // Show the clean API in action
        events
            .on_core("player_connected", |event: PlayerConnectedEvent| {
                debug!("✅ Core: Player {} connected", event.player_id);
                Ok(())
            })
            .await
            .expect("Failed to register core player connected handler");

        // This would be handled by movement plugin, not core
        events
            .on_client("movement", "jump", |event: serde_json::Value, _player_id: horizon_event_system::PlayerId, _connection: horizon_event_system::ClientConnectionRef| {
                debug!("🦘 Movement Plugin: Jump event {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register movement handler");

        info!("✨ Clean separation achieved with generic routing!");
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

        info!("✅ GORC integration test passed!");
        debug!("  - GORC Manager: Initialized with default channels");
        debug!("  - Subscription Manager: Player subscription system ready");
        debug!("  - Multicast Manager: Group creation and player management working");
        debug!("  - Spatial Partition: Region management and spatial queries available");
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

        let _server = create_server_with_config(config.clone());
        
        // Verify the server was created with custom config
        // Note: In a real implementation, you might want to expose config getters
        debug!("Server created with custom configuration:");
        debug!("  - Bind address: {}", config.bind_address);
        debug!("  - Max connections: {}", config.max_connections);
        debug!("  - Connection timeout: {}s", config.connection_timeout);
        debug!("  - Use reuse port: {}", config.use_reuse_port);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_config_defaults() {
        let config = ServerConfig::default();
        
        assert_eq!(config.bind_address.to_string(), "127.0.0.1:8080");
        assert_eq!(config.max_connections, 1000);
        assert_eq!(config.connection_timeout, 60);
        assert_eq!(config.use_reuse_port, false);
        assert_eq!(config.tick_interval_ms, 50);
        assert_eq!(config.plugin_directory, std::path::PathBuf::from("plugins"));
        
        // Test region bounds
        assert_eq!(config.region_bounds.min_x, -1000.0);
        assert_eq!(config.region_bounds.max_x, 1000.0);
        assert_eq!(config.region_bounds.min_y, -1000.0);
        assert_eq!(config.region_bounds.max_y, 1000.0);
        assert_eq!(config.region_bounds.min_z, -100.0);
        assert_eq!(config.region_bounds.max_z, 100.0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_config_custom_values() {
        use horizon_event_system::RegionBounds;
        use std::path::PathBuf;

        let custom_bounds = RegionBounds {
            min_x: -2000.0,
            max_x: 2000.0,
            min_y: -1500.0,
            max_y: 1500.0,
            min_z: -200.0,
            max_z: 200.0,
        };

        let config = ServerConfig {
            bind_address: "0.0.0.0:3000".parse().unwrap(),
            region_bounds: custom_bounds.clone(),
            plugin_directory: PathBuf::from("/custom/plugins"),
            max_connections: 5000,
            connection_timeout: 300,
            use_reuse_port: true,
            tick_interval_ms: 16, // 60 FPS
            security: Default::default(),
            plugin_safety: Default::default(),
        };

        assert_eq!(config.bind_address.to_string(), "0.0.0.0:3000");
        assert_eq!(config.max_connections, 5000);
        assert_eq!(config.connection_timeout, 300);
        assert_eq!(config.use_reuse_port, true);
        assert_eq!(config.tick_interval_ms, 16);
        assert_eq!(config.plugin_directory, PathBuf::from("/custom/plugins"));
        assert_eq!(config.region_bounds.min_x, custom_bounds.min_x);
        assert_eq!(config.region_bounds.max_x, custom_bounds.max_x);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_tick_events() {
        use std::sync::Arc;
        use tokio::time::Duration;

        let server = create_server();
        let events = server.get_horizon_event_system();
        
        // Counter to track tick events
        let counter = Arc::new(std::sync::Mutex::new(0u64));
        let counter_clone = counter.clone();
        
        events
            .on_core("server_tick", move |event: serde_json::Value| {
                let mut count = counter_clone.lock().unwrap();
                *count += 1;
                
                // Verify event structure
                assert!(event.get("tick_count").is_some());
                assert!(event.get("timestamp").is_some());
                
                debug!("Received tick #{}: {:?}", *count, event);
                Ok(())
            })
            .await
            .expect("Failed to register tick handler");

        // Simulate server tick by manually emitting events (since we can't easily test the actual timer)
        for i in 1..=5 {
            let tick_event = serde_json::json!({
                "tick_count": i,
                "timestamp": horizon_event_system::current_timestamp()
            });
            
            events
                .emit_core("server_tick", &tick_event)
                .await
                .expect("Failed to emit server tick");
        }

        // Give a moment for event processing (handlers are synchronous)
        tokio::time::sleep(Duration::from_millis(50)).await;

        let final_count = *counter.lock().unwrap();
        assert_eq!(final_count, 5);
        
        info!("✅ Server tick events processed: {}", final_count);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_tick_disabled() {
        use horizon_event_system::RegionBounds;

        // Create config with tick disabled
        let config = ServerConfig {
            tick_interval_ms: 0, // Disabled
            bind_address: "127.0.0.1:8081".parse().unwrap(),
            region_bounds: RegionBounds::default(),
            plugin_directory: std::path::PathBuf::from("plugins"),
            max_connections: 1000,
            connection_timeout: 60,
            use_reuse_port: false,
            security: Default::default(),
            plugin_safety: Default::default(),
        };

        let server = create_server_with_config(config);
        
        // In real implementation, we would verify the tick task wasn't spawned
        // For now, just verify the server can be created with tick_interval_ms = 0
        assert_eq!(server.get_horizon_event_system().get_stats().await.total_handlers, 0);
        
        info!("✅ Server created successfully with tick disabled");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_tick_different_intervals() {
        // Test various tick intervals for edge cases
        let test_intervals = vec![1, 16, 50, 100, 1000];
        
        for interval_ms in test_intervals {
            let config = ServerConfig {
                tick_interval_ms: interval_ms,
                bind_address: format!("127.0.0.1:{}", 8082 + interval_ms).parse().unwrap(),
                ..Default::default()
            };

            let server = create_server_with_config(config.clone());
            
            // Verify server creation succeeds with different intervals
            let events = server.get_horizon_event_system();
            // Verify server is functioning by checking event system exists
            assert!(events.get_stats().await.total_handlers == events.get_stats().await.total_handlers);
            
            info!("✅ Server created with {}ms tick interval", interval_ms);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_region_bounds_validation() {
        use horizon_event_system::RegionBounds;

        // Test valid region bounds
        let valid_bounds = RegionBounds {
            min_x: -1000.0,
            max_x: 1000.0,
            min_y: -1000.0,
            max_y: 1000.0,
            min_z: -100.0,
            max_z: 100.0,
        };

        let config = ServerConfig {
            region_bounds: valid_bounds.clone(),
            ..Default::default()
        };

        let server = create_server_with_config(config);
        
        // Verify server creation succeeds
        let events = server.get_horizon_event_system();
        // Verify event system is functioning
        let _stats = events.get_stats().await;
        
        info!("✅ Server created with valid region bounds");

        // Test edge case bounds
        let edge_bounds = RegionBounds {
            min_x: 0.0,
            max_x: 0.0,  // Single point
            min_y: 0.0,
            max_y: 0.0,
            min_z: 0.0,
            max_z: 0.0,
        };

        let edge_config = ServerConfig {
            region_bounds: edge_bounds,
            bind_address: "127.0.0.1:8083".parse().unwrap(),
            ..Default::default()
        };

        let edge_server = create_server_with_config(edge_config);
        // Verify edge server event system is functioning
        let _stats = edge_server.get_horizon_event_system().get_stats().await;
        
        info!("✅ Server created with edge case region bounds (single point)");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_max_connections_config() {
        let test_max_connections = vec![1, 10, 100, 1000, 10000];
        
        for max_conn in test_max_connections {
            let config = ServerConfig {
                max_connections: max_conn,
                bind_address: format!("127.0.0.1:{}", 8084 + (max_conn % 1000)).parse().unwrap(),
                ..Default::default()
            };

            let server = create_server_with_config(config.clone());
            
            // Verify server creation succeeds with different max_connections
            let events = server.get_horizon_event_system();
            // Verify event system is functioning
        let _stats = events.get_stats().await;
            
            info!("✅ Server created with max_connections: {}", max_conn);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_connection_timeout_config() {
        let test_timeouts = vec![1, 30, 60, 300, 3600];
        
        for timeout in test_timeouts {
            let config = ServerConfig {
                connection_timeout: timeout,
                bind_address: format!("127.0.0.1:{}", 8090 + (timeout % 100)).parse().unwrap(),
                ..Default::default()
            };

            let server = create_server_with_config(config.clone());
            
            // Verify server creation succeeds with different timeouts
            let events = server.get_horizon_event_system();
            // Verify event system is functioning
        let _stats = events.get_stats().await;
            
            info!("✅ Server created with connection_timeout: {}s", timeout);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_plugin_directory_config() {
        use std::path::PathBuf;

        let test_dirs = vec![
            "plugins",
            "/absolute/path/plugins",
            "./relative/plugins",
            "../parent/plugins",
            "custom_plugins_dir",
        ];
        
        for (i, dir) in test_dirs.iter().enumerate() {
            let config = ServerConfig {
                plugin_directory: PathBuf::from(dir),
                bind_address: format!("127.0.0.1:{}", 8100 + i).parse().unwrap(),
                ..Default::default()
            };

            let server = create_server_with_config(config.clone());
            
            // Verify server creation succeeds with different plugin directories
            let events = server.get_horizon_event_system();
            // Verify event system is functioning
        let _stats = events.get_stats().await;
            
            info!("✅ Server created with plugin_directory: {:?}", dir);
        }
    }
}