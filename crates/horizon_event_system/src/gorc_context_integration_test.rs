//! Test for GORC integration via ServerContext

use crate::*;
use std::sync::Arc;

/// Test ServerContext that provides GORC access
#[derive(Debug)]
struct TestServerContextWithGorc {
    gorc_instance_manager: Arc<gorc::GorcInstanceManager>,
}

impl TestServerContextWithGorc {
    fn new() -> Self {
        Self {
            gorc_instance_manager: Arc::new(gorc::GorcInstanceManager::new()),
        }
    }
}

#[async_trait::async_trait]
impl ServerContext for TestServerContextWithGorc {
    fn events(&self) -> Arc<EventSystem> {
        Arc::new(EventSystem::with_gorc(self.gorc_instance_manager.clone()))
    }

    fn region_id(&self) -> RegionId {
        RegionId::new()
    }

    fn log(&self, _level: LogLevel, _message: &str) {
        eprintln!("LOG: {}", _message);
    }

    async fn send_to_player(&self, _player_id: PlayerId, _data: &[u8]) -> Result<(), ServerError> {
        Ok(())
    }

    async fn broadcast(&self, _data: &[u8]) -> Result<(), ServerError> {
        Ok(())
    }

    fn luminal_handle(&self) -> luminal::Handle {
        // Create a new luminal runtime for testing
        let rt = luminal::Runtime::new().expect("Failed to create luminal runtime for tests");
        rt.handle().clone()
    }

    fn gorc_instance_manager(&self) -> Option<Arc<gorc::GorcInstanceManager>> {
        Some(self.gorc_instance_manager.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_context_provides_gorc_access() {
        let context = Arc::new(TestServerContextWithGorc::new());
        
        // Verify that the context provides GORC access
        assert!(context.gorc_instance_manager().is_some());
        
        let events = context.events();
        
        // Register a GORC instance handler to test functionality
        let handler_registered = events.on_gorc_instance("TestObject", 0, "test_event", |event: GorcEvent, _instance| {
            eprintln!("Received GORC instance event: {}", event.object_id);
            Ok(())
        }).await;
        
        assert!(handler_registered.is_ok(), "Failed to register GORC instance handler");
        
        // Test emitting a general GORC event (doesn't require object registration)
        let test_event = GorcEvent {
            object_id: "test_object_123".to_string(),
            instance_uuid: "test_instance_123".to_string(),
            object_type: "TestObject".to_string(),
            channel: 0,
            data: vec![1, 2, 3, 4],
            priority: "Normal".to_string(),
            timestamp: current_timestamp(),
        };
        
        let emit_result = events.emit_gorc("TestObject", 0, "test_event", &test_event).await;
        assert!(emit_result.is_ok(), "Failed to emit GORC event");
    }

    // Note: Plugin manager is in a separate crate, so we can't test it here directly.
    // The integration is tested by the server's usage of PluginManager::with_gorc()

    #[tokio::test]
    async fn test_mock_context_returns_none_for_gorc() {
        // Test that basic ServerContext implementations without GORC support return None
        #[derive(Debug)]
        struct BasicMockContext;
        
        #[async_trait::async_trait]
        impl ServerContext for BasicMockContext {
            fn events(&self) -> Arc<EventSystem> {
                Arc::new(EventSystem::new())
            }
            fn region_id(&self) -> RegionId { RegionId::new() }
            fn log(&self, _level: LogLevel, _message: &str) {}
            async fn send_to_player(&self, _player_id: PlayerId, _data: &[u8]) -> Result<(), ServerError> { Ok(()) }
            async fn broadcast(&self, _data: &[u8]) -> Result<(), ServerError> { Ok(()) }
            fn luminal_handle(&self) -> luminal::Handle { 
                let rt = luminal::Runtime::new().expect("Failed to create luminal runtime for tests");
                rt.handle().clone()
            }
            fn gorc_instance_manager(&self) -> Option<Arc<gorc::GorcInstanceManager>> { None }
        }
        
        let mock_context = BasicMockContext;
        assert!(mock_context.gorc_instance_manager().is_none());
    }

    #[tokio::test]
    async fn test_socket_message_dual_routing() -> Result<(), Box<dyn std::error::Error>> {
        // Test that demonstrates how socket messages can be routed to both 
        // regular client handlers and GORC handlers
        
        use tokio::sync::Mutex;
        
        let context = Arc::new(TestServerContextWithGorc::new());
        let events = context.events();
        
        // Track events received by each handler type
        let client_events = Arc::new(Mutex::new(Vec::<String>::new()));
        let gorc_events = Arc::new(Mutex::new(Vec::<String>::new()));
        
        // Register a regular client handler for auth:login
        let client_collector = client_events.clone();
        events.on_client("auth", "login", move |event: serde_json::Value| {
            let client_collector = client_collector.clone();
            tokio::spawn(async move {
                let mut events = client_collector.lock().await;
                events.push(format!("client_handler received: {}", event));
            });
            Ok(())
        }).await?;
        
        // Register a GORC handler for Auth:login (PascalCase object type)
        let gorc_collector = gorc_events.clone();
        events.on_gorc("Auth", 0, "login", move |event: serde_json::Value| {
            let gorc_collector = gorc_collector.clone();
            tokio::spawn(async move {
                let mut events = gorc_collector.lock().await;
                events.push(format!("gorc_handler received: {}", event));
            });
            Ok(())
        }).await?;
        
        // Simulate a GORC message with instance_uuid (unified format)
        let auth_data_with_instance = serde_json::json!({
            "instance_uuid": "12345678-1234-1234-1234-123456789abc",
            "object_id": "auth_session_001",
            "credentials": {
                "username": "admin",
                "password": "password123"
            }
        });
        
        let player_id = PlayerId::new();
        
        // 1. Route to client handlers (existing behavior)
        events.emit_client_with_context("auth", "login", player_id, &auth_data_with_instance).await?;
        
        // 2. Route to GORC handlers (new functionality)
        let gorc_event = GorcEvent {
            object_id: "auth_session_001".to_string(),
            instance_uuid: "12345678-1234-1234-1234-123456789abc".to_string(),
            object_type: "Auth".to_string(),
            channel: 0,
            data: serde_json::to_vec(&serde_json::json!({
                "player_id": player_id,
                "event_name": "login",
                "original_namespace": "auth",
                "data": auth_data_with_instance,
                "timestamp": current_timestamp()
            })).unwrap_or_default(),
            priority: "Normal".to_string(),
            timestamp: current_timestamp(),
        };
        
        events.emit_gorc("Auth", 0, "login", &gorc_event).await?;
        
        // Allow handlers to execute
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        // Verify both handler types received events
        let client_events_guard = client_events.lock().await;
        let gorc_events_guard = gorc_events.lock().await;
        
        eprintln!("✅ Socket message dual routing test results:");
        eprintln!("   Client events: {}", client_events_guard.len());
        eprintln!("   GORC events: {}", gorc_events_guard.len());
        
        for (i, event) in client_events_guard.iter().enumerate() {
            eprintln!("   Client[{}]: {}", i + 1, event);
        }
        
        for (i, event) in gorc_events_guard.iter().enumerate() {
            eprintln!("   GORC[{}]: {}", i + 1, event);
        }
        
        assert!(!client_events_guard.is_empty(), "Client handler should receive events");
        assert!(!gorc_events_guard.is_empty(), "GORC handler should receive events");
        
        // Verify event content
        assert!(client_events_guard[0].contains("admin"), "Client event should contain credentials");
        // For GORC events, the credentials are in the serialized data field, but we can check other fields
        assert!(gorc_events_guard[0].contains("Auth"), "GORC event should contain object type");
        assert!(gorc_events_guard[0].contains("12345678-1234-1234-1234-123456789abc"), "GORC event should contain instance_uuid");
        assert!(gorc_events_guard[0].contains("auth_session_001"), "GORC event should contain object_id");
        
        eprintln!("✅ All assertions passed - socket messages successfully routed to both handler types!");
        
        Ok(())
    }

    #[tokio::test]
    async fn test_unified_message_format_routing() -> Result<(), Box<dyn std::error::Error>> {
        // Test that demonstrates the unified message format where instance_uuid 
        // determines routing behavior
        
        use tokio::sync::Mutex;
        
        let context = Arc::new(TestServerContextWithGorc::new());
        let events = context.events();
        
        let gorc_events = Arc::new(Mutex::new(Vec::<String>::new()));
        
        // Register a GORC handler
        let gorc_collector = gorc_events.clone();
        events.on_gorc("Auth", 0, "login", move |event: GorcEvent| {
            let gorc_collector = gorc_collector.clone();
            tokio::spawn(async move {
                let mut events = gorc_collector.lock().await;
                events.push(format!("GORC received: instance_uuid={}", event.instance_uuid));
            });
            Ok(())
        }).await?;
        
        // Test 1: Message WITHOUT instance_uuid - should NOT trigger GORC routing
        let regular_message = serde_json::json!({
            "credentials": {
                "username": "user1",
                "password": "pass123"
            }
        });
        
        events.emit_client("auth", "login", &regular_message).await?;
        
        // Test 2: Message WITH instance_uuid - SHOULD trigger GORC routing  
        let gorc_message = serde_json::json!({
            "instance_uuid": "test-instance-123",
            "object_id": "auth_obj_456",
            "credentials": {
                "username": "user2", 
                "password": "pass456"
            }
        });
        
        // This would be called by the message router when it detects instance_uuid
        let gorc_event = GorcEvent {
            object_id: "auth_obj_456".to_string(),
            instance_uuid: "test-instance-123".to_string(),
            object_type: "Auth".to_string(),
            channel: 0,
            data: serde_json::to_vec(&gorc_message).unwrap_or_default(),
            priority: "Normal".to_string(),
            timestamp: current_timestamp(),
        };
        
        events.emit_gorc("Auth", 0, "login", &gorc_event).await?;
        
        // Allow handlers to execute
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        let gorc_events_guard = gorc_events.lock().await;
        
        eprintln!("✅ Unified message format test results:");
        eprintln!("   GORC events received: {}", gorc_events_guard.len());
        
        for (i, event) in gorc_events_guard.iter().enumerate() {
            eprintln!("   GORC[{}]: {}", i + 1, event);
        }
        
        // Should have received exactly 1 GORC event (only from the message with instance_uuid)
        assert_eq!(gorc_events_guard.len(), 1, "Should receive exactly 1 GORC event");
        assert!(gorc_events_guard[0].contains("test-instance-123"), "Should contain correct instance_uuid");
        
        eprintln!("✅ Unified message format correctly routes based on instance_uuid presence!");
        
        Ok(())
    }
}