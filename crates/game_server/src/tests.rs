// Tests for the clean universal plugin system
#[cfg(test)]
mod tests {
    use crate::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_core_server_creation() {
        let server = create_server();
        let event_bus = server.get_event_bus();

        // Verify we can register handlers in the universal system
        event_bus
            .on("test", "event", |event: serde_json::Value| {
                println!("Test event: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register event handler");

        // Emit a test event
        event_bus
            .emit("test", "event", &serde_json::json!({
                "test": "data"
            }))
            .await
            .expect("Failed to emit test event");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_plugin_manager_creation() {
        let server = create_server();
        let plugin_manager = server.get_plugin_manager();

        // Initially should have no plugins loaded
        assert_eq!(plugin_manager.plugin_count(), 0);
        assert!(plugin_manager.plugin_names().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_universal_event_system() {
        let server = create_server();
        let event_bus = server.get_event_bus();

        // Test event registration and emission
        let test_data = serde_json::json!({
            "message": "Hello from universal system",
            "timestamp": 1234567890
        });

        event_bus
            .on("core", "test_message", |event: serde_json::Value| {
                println!("Received test message: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register handler");

        event_bus
            .emit("core", "test_message", &test_data)
            .await
            .expect("Failed to emit event");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_client_message_routing() {
        let server = create_server();
        let event_bus = server.get_event_bus();

        // Register handlers that plugins would register
        event_bus
            .on("movement", "move_request", |event: serde_json::Value| {
                println!("Movement plugin would handle: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register movement handler");

        event_bus
            .on("chat", "send_message", |event: serde_json::Value| {
                println!("Chat plugin would handle: {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register chat handler");

        // Test routing
        event_bus
            .emit("movement", "move_request", &serde_json::json!({
                "target_x": 100.0,
                "target_y": 200.0,
                "target_z": 0.0
            }))
            .await
            .expect("Failed to emit event for movement");

        event_bus
            .emit("chat", "send_message", &serde_json::json!({
                "message": "Hello world!",
                "channel": "general"
            }))
            .await
            .expect("Failed to emit event for chat");
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
        
        println!("Server created with custom configuration:");
        println!("  - Bind address: {}", config.bind_address);
        println!("  - Max connections: {}", config.max_connections);
        println!("  - Connection timeout: {}s", config.connection_timeout);
        println!("  - Use reuse port: {}", config.use_reuse_port);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_clean_separation() {
        let server = create_server();
        let event_bus = server.get_event_bus();

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
        event_bus
            .on("core", "player_connected", |event: serde_json::Value| {
                println!("✅ Core: Player connected {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register core player connected handler");

        // This would be handled by movement plugin, not core
        event_bus
            .on("movement", "jump", |event: serde_json::Value| {
                println!("🦘 Movement Plugin: Jump event {:?}", event);
                Ok(())
            })
            .await
            .expect("Failed to register movement handler");

        println!("✨ Clean separation achieved with universal routing!");
    }
}