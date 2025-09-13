#[cfg(test)]
use super::*;
#[cfg(test)]
use tracing::debug;

// Mock server context for testing
#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MockServerContext;

#[cfg(test)]
impl MockServerContext {
    fn new() -> Self {
        Self
    }
}

#[cfg(test)]
#[async_trait]
impl ServerContext for MockServerContext {
    fn events(&self) -> Arc<crate::system::EventSystem> {
        Arc::new(EventSystem::new())
    }

    fn region_id(&self) -> RegionId {
        RegionId::new()
    }

    fn log(&self, _level: LogLevel, _message: &str) {
        // Mock implementation
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

    fn gorc_instance_manager(&self) -> Option<Arc<crate::gorc::GorcInstanceManager>> {
        None
    }
}

// Helper function to create a test event system with mock client response sender
#[cfg(test)]
fn create_test_event_system() -> Arc<EventSystem> {
    let mut events = EventSystem::new();
    
    // Add mock client response sender for tests  
    #[derive(Debug, Clone)]
    struct MockResponseSender {
        sent_messages: Arc<std::sync::Mutex<Vec<(crate::types::PlayerId, Vec<u8>)>>>,
    }
    
    impl MockResponseSender {
        fn new() -> Self {
            Self {
                sent_messages: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }
    }
    
    impl crate::ClientResponseSender for MockResponseSender {
        fn send_to_client(&self, player_id: crate::types::PlayerId, data: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
            let sent_messages = self.sent_messages.clone();
            Box::pin(async move {
                sent_messages.lock().unwrap().push((player_id, data));
                Ok(())
            })
        }
        
        fn is_connection_active(&self, _player_id: crate::types::PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>> {
            Box::pin(async move { true })
        }
        
        fn get_auth_status(&self, _player_id: crate::types::PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<crate::types::AuthenticationStatus>> + Send + '_>> {
            Box::pin(async move { Some(crate::types::AuthenticationStatus::Authenticated) })
        }
        
        fn kick(&self, _player_id: crate::types::PlayerId, _reason: Option<String>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
            Box::pin(async move { Ok(()) })
        }
    }
    
    let mock_sender = Arc::new(MockResponseSender::new());
    events.set_client_response_sender(mock_sender.clone());
    Arc::new(events)
}

#[tokio::test]
async fn test_complete_system_integration() {
    let server_context = Arc::new(MockServerContext::new());
    let (events, mut gorc_system) = create_complete_horizon_system(server_context).unwrap();

    // Test event registration
    events
        .on_core("test_event", |_: PlayerConnectedEvent| Ok(()))
        .await
        .unwrap();

    // Test GORC object registration
    let asteroid = ExampleAsteroid::new(Vec3::new(100.0, 0.0, 200.0), MineralType::Platinum);
    let _asteroid_id = gorc_system
        .register_object(asteroid, Vec3::new(100.0, 0.0, 200.0))
        .await;

    // Test player management
    let player_id = PlayerId::new();
    gorc_system
        .add_player(player_id, Vec3::new(50.0, 0.0, 180.0))
        .await;

    // Test system tick
    let result = gorc_system.tick().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_monitoring_system() {
    let events = create_test_event_system();
    let mut monitor = HorizonMonitor::new(events.clone());

    // Generate initial report
    let report = monitor.generate_report().await;
    assert!(report.timestamp > 0);
    assert_eq!(report.uptime_seconds, 0); // Just started

    // Check alerts (should be none for new system)
    let alerts = monitor.should_alert().await;
    assert!(alerts.is_empty());
}

// Test types for integration testing
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TestChatEvent {
    message: String,
    channel: String,
    timestamp: u64,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TestMovementEvent {
    x: f32,
    y: f32,
    z: f32,
    velocity: f32,
}

#[tokio::test]
async fn test_debug_event_emission_and_handling() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    let events = create_test_event_system();
    
    // Test with JSON handler first to see if event emission works at all
    let json_count = Arc::new(AtomicUsize::new(0));
    let json_count_clone = json_count.clone();
    
    events
        .on_client("debug", "test", move |wrapper: serde_json::Value, _player_id: crate::types::PlayerId, _connection: crate::ClientConnectionRef| {
            json_count_clone.fetch_add(1, Ordering::SeqCst);
            debug!("JSON handler called with: {}", wrapper);
            Ok(())
        })
        .await
        .unwrap();
    
    // Create test event
    let player_id = PlayerId::new();
    let chat_event = TestChatEvent {
        message: "Debug test message".to_string(),
        channel: "debug".to_string(),
        timestamp: current_timestamp(),
    };
    
    debug!("Emitting event...");
    events
        .emit_client_with_context("debug", "test", player_id, &chat_event)
        .await
        .unwrap();
    
    debug!("Waiting for handlers...");
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    debug!("JSON handler call count: {}", json_count.load(Ordering::SeqCst));
    assert_eq!(json_count.load(Ordering::SeqCst), 1, "JSON handler should be called");
}

#[tokio::test]
async fn test_typed_client_event_handlers() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    let events = create_test_event_system();
    
    // Counter to verify handler was called
    let call_count = Arc::new(AtomicUsize::new(0));
    let call_count_clone = call_count.clone();
    
    // Register typed handler for client events
    events
        .on_client("chat", "message", move |wrapper: ClientEventWrapper<TestChatEvent>, _player_id: crate::types::PlayerId, _connection: crate::ClientConnectionRef| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            
            // Verify we can access typed data
            assert!(!wrapper.data.message.is_empty());
            assert!(!wrapper.data.channel.is_empty());
            assert!(wrapper.data.timestamp > 0);
            
            // Verify player_id is accessible
            assert_ne!(wrapper.player_id.0, uuid::Uuid::nil());
            
            Ok(())
        })
        .await
        .unwrap();
    
    // Create test event
    let player_id = PlayerId::new();
    let chat_event = TestChatEvent {
        message: "Hello from integration test!".to_string(),
        channel: "integration_test".to_string(),
        timestamp: current_timestamp(),
    };
    
    // Emit event using emit_client_with_context (the way the system actually works)
    events
        .emit_client_with_context("chat", "message", player_id, &chat_event)
        .await
        .unwrap();
    
    // Give handlers time to run
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Debug: Check how many handlers are registered
    debug!("Handler call count: {}", call_count.load(Ordering::SeqCst));
    
    // Verify handler was called
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_typed_vs_json_handlers_compatibility() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    let events = create_test_event_system();
    
    let typed_count = Arc::new(AtomicUsize::new(0));
    let json_count = Arc::new(AtomicUsize::new(0));
    
    // Register both typed and JSON handlers for the same event
    let typed_count_clone = typed_count.clone();
    events
        .on_client("movement", "update", move |wrapper: ClientEventWrapper<TestMovementEvent>, _player_id: crate::types::PlayerId, _connection: crate::ClientConnectionRef| {
            typed_count_clone.fetch_add(1, Ordering::SeqCst);
            
            // Verify typed access works
            assert!(wrapper.data.x >= 0.0);
            assert!(wrapper.data.y >= 0.0);
            assert!(wrapper.data.z >= 0.0);
            assert!(wrapper.data.velocity >= 0.0);
            
            Ok(())
        })
        .await
        .unwrap();
    
    let json_count_clone = json_count.clone();
    events
        .on_client("movement", "update", move |wrapper: serde_json::Value, _player_id: crate::types::PlayerId, _connection: crate::ClientConnectionRef| {
            json_count_clone.fetch_add(1, Ordering::SeqCst);
            
            // Verify JSON handler still works
            assert!(wrapper.get("player_id").is_some());
            assert!(wrapper.get("data").is_some());
            
            Ok(())
        })
        .await
        .unwrap();
    
    // Create test event
    let player_id = PlayerId::new();
    let movement_event = TestMovementEvent {
        x: 100.0,
        y: 50.0,
        z: 200.0,
        velocity: 25.5,
    };
    
    // Emit event
    events
        .emit_client_with_context("movement", "update", player_id, &movement_event)
        .await
        .unwrap();
    
    // Give handlers time to run
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    
    // Verify both handlers were called
    assert_eq!(typed_count.load(Ordering::SeqCst), 1);
    assert_eq!(json_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_typed_core_event_handlers() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    let events = create_test_event_system();
    
    let call_count = Arc::new(AtomicUsize::new(0));
    let call_count_clone = call_count.clone();
    let player_id_captured = Arc::new(std::sync::Mutex::new(PlayerId::new()));
    let player_id_clone = player_id_captured.clone();
    
    // Register typed handler for core events
    events
        .on_core("player_connected", move |event: PlayerConnectedEvent| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            
            // Verify we can access typed data
            assert!(!event.connection_id.is_empty());
            assert!(!event.remote_addr.is_empty());
            assert!(event.timestamp > 0);
            
            // Store player_id for verification
            *player_id_clone.lock().unwrap() = event.player_id;
            
            Ok(())
        })
        .await
        .unwrap();
    
    // Create and emit core event
    let test_player_id = PlayerId::new();
    let connect_event = PlayerConnectedEvent {
        player_id: test_player_id,
        connection_id: "integration_test_conn".to_string(),
        remote_addr: "127.0.0.1:12345".to_string(),
        timestamp: current_timestamp(),
    };
    
    events
        .emit_core("player_connected", &connect_event)
        .await
        .unwrap();
    
    // Give handler time to run
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    
    // Verify handler was called and captured correct data
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
    assert_eq!(*player_id_captured.lock().unwrap(), test_player_id);
}

#[tokio::test]
async fn test_event_handler_error_handling() {
    let events = create_test_event_system();
    
    // Register handler that intentionally fails
    events
        .on_client("error_test", "fail", move |_wrapper: ClientEventWrapper<TestChatEvent>, _player_id: crate::types::PlayerId, _connection: crate::ClientConnectionRef| {
            Err(EventError::HandlerExecution("Intentional test failure".to_string()))
        })
        .await
        .unwrap();
    
    // Create test event
    let player_id = PlayerId::new();
    let chat_event = TestChatEvent {
        message: "This should cause handler to fail".to_string(),
        channel: "error_test".to_string(),
        timestamp: current_timestamp(),
    };
    
    // Event emission should succeed even if handler fails
    let emit_result = events
        .emit_client_with_context("error_test", "fail", player_id, &chat_event)
        .await;
    
    assert!(emit_result.is_ok(), "Event emission should succeed even with failing handlers");
    
    // Give handler time to run and fail
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
}

#[tokio::test]  
async fn test_multiple_typed_handlers_same_event() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    let events = create_test_event_system();
    
    let handler1_count = Arc::new(AtomicUsize::new(0));
    let handler2_count = Arc::new(AtomicUsize::new(0));
    let handler3_count = Arc::new(AtomicUsize::new(0));
    
    // Register multiple typed handlers for the same event
    let h1_count = handler1_count.clone();
    events
        .on_client("multi_test", "event", move |wrapper: ClientEventWrapper<TestChatEvent>, _player_id: crate::types::PlayerId, _connection: crate::ClientConnectionRef| {
            h1_count.fetch_add(1, Ordering::SeqCst);
            assert_eq!(wrapper.data.channel, "multi_handler_test");
            Ok(())
        })
        .await
        .unwrap();
    
    let h2_count = handler2_count.clone();
    events
        .on_client("multi_test", "event", move |wrapper: ClientEventWrapper<TestChatEvent>, _player_id: crate::types::PlayerId, _connection: crate::ClientConnectionRef| {
            h2_count.fetch_add(1, Ordering::SeqCst);
            assert!(wrapper.data.message.contains("multi"));
            Ok(())
        })
        .await
        .unwrap();
    
    let h3_count = handler3_count.clone();
    events
        .on_client("multi_test", "event", move |wrapper: ClientEventWrapper<TestChatEvent>, _player_id: crate::types::PlayerId, _connection: crate::ClientConnectionRef| {
            h3_count.fetch_add(1, Ordering::SeqCst);
            assert!(wrapper.data.timestamp > 0);
            Ok(())
        })
        .await
        .unwrap();
    
    // Create and emit test event
    let player_id = PlayerId::new();
    let chat_event = TestChatEvent {
        message: "multi handler test message".to_string(),
        channel: "multi_handler_test".to_string(),
        timestamp: current_timestamp(),
    };
    
    events
        .emit_client_with_context("multi_test", "event", player_id, &chat_event)
        .await
        .unwrap();
    
    // Give handlers time to run
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    
    // Verify all handlers were called
    assert_eq!(handler1_count.load(Ordering::SeqCst), 1);
    assert_eq!(handler2_count.load(Ordering::SeqCst), 1);
    assert_eq!(handler3_count.load(Ordering::SeqCst), 1);
}
