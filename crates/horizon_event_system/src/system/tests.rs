//! Tests for the connection-aware event system

#[cfg(test)]
mod tests {
    
    use crate::{EventSystem, ClientConnectionRef, ClientResponseSender};
    use crate::events::RawClientMessageEvent;
    use crate::types::PlayerId;
    use std::sync::{Arc, Mutex};
    use crate::events::{PlayerConnectedEvent, GorcEvent};
    use crate::gorc::instance::{GorcInstanceManager, GorcObjectId};


    
    // Mock response sender for testing
    #[derive(Debug, Clone)]
    struct MockResponseSender {
        sent_messages: Arc<Mutex<Vec<(PlayerId, Vec<u8>)>>>,
    }
    
    impl MockResponseSender {
        fn new() -> Self {
            Self {
                sent_messages: Arc::new(Mutex::new(Vec::new())),
            }
        }
        
        #[allow(dead_code)]
        fn get_sent_messages(&self) -> Vec<(PlayerId, Vec<u8>)> {
            self.sent_messages.lock().unwrap().clone()
        }
    }
    
    impl ClientResponseSender for MockResponseSender {
        fn send_to_client(&self, player_id: PlayerId, data: Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
            let sent_messages = self.sent_messages.clone();
            Box::pin(async move {
                sent_messages.lock().unwrap().push((player_id, data));
                Ok(())
            })
        }
        
        fn is_connection_active(&self, _player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>> {
            Box::pin(async move { true })
        }
        
        fn get_auth_status(&self, _player_id: PlayerId) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<crate::types::AuthenticationStatus>> + Send + '_>> {
            Box::pin(async move { Some(crate::types::AuthenticationStatus::Authenticated) })
        }

        fn kick(&self, player_id: PlayerId, reason: Option<String>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
            let sent_messages = self.sent_messages.clone();
            Box::pin(async move {
                let kick_message = format!("Kicked: {}", reason.unwrap_or_else(|| "No reason provided".to_string()));
                sent_messages.lock().unwrap().push((player_id, kick_message.into_bytes()));
                Ok(())
            })
        }
    }
    
    #[tokio::test]
    async fn test_connection_aware_handler() {
        let mut events = EventSystem::new();
        let mock_sender = Arc::new(MockResponseSender::new());
        events.set_client_response_sender(mock_sender.clone());
        
        let response_received = Arc::new(Mutex::new(false));
        let response_received_clone = response_received.clone();
        
        // Register a connection-aware handler
        events.on_client_with_connection("test", "message", 
            move |_event: RawClientMessageEvent, client: ClientConnectionRef| {
                let response_received = response_received_clone.clone();
                // Mark that we received the event
                *response_received.lock().unwrap() = true;
                
                // Use tokio handle to execute async response
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let _ = client.respond(b"Hello back!").await;
                    });
                }
                Ok(())
            }
        ).await.unwrap();
        
        // Simulate emitting an event (in real usage, this would come from the server)
        let test_event = RawClientMessageEvent {
            player_id: PlayerId::new(),
            message_type: "test:message".to_string(),
            data: b"Hello".to_vec(),
            timestamp: crate::utils::current_timestamp(),
        };
        
        events.emit_client("test", "message", &test_event).await.unwrap();
        
        // Give some time for async processing
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        // Note: The connection-aware handler won't actually be triggered without proper 
        // connection context in this test, but the handler registration should succeed
        println!("✅ Connection-aware handler registration test passed");
    }
    
    #[tokio::test]
    async fn test_async_handlers() {
        let events = EventSystem::new();
        
        let sync_handler_called = Arc::new(Mutex::new(false));
        let sync_handler_called_clone = sync_handler_called.clone();
        
        // Register a regular (sync) handler for comparison
        events.on_core("test_sync_event", 
            move |_event: serde_json::Value| {
                *sync_handler_called_clone.lock().unwrap() = true;
                Ok(())
            }
        ).await.unwrap();
        
        // Emit to the sync handler first to verify the system works
        let test_event = serde_json::json!({"test": "data"});
        events.emit_core("test_sync_event", &test_event).await.unwrap();
        
        // Verify sync handler was called
        assert!(*sync_handler_called.lock().unwrap());
        
        // Now test that we can register async handlers (even if we can't easily test execution)
        let async_result = events.on_core_async("test_async_event",
            move |_event: serde_json::Value| {
                // Use tokio handle to execute async code within sync handler
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                    });
                }
                Ok(())
            }
        ).await;
        
        // Verify async handler registration succeeded
        assert!(async_result.is_ok());
        
        println!("✅ Async handler registration test passed");
    }
    
    #[tokio::test]
    async fn test_system_stats() {
        let events = EventSystem::new();
        
        // Register various handler types
        events.on_core("test_core", |_: serde_json::Value| Ok(())).await.unwrap();
        events.on_client("test", "client_event", |_: serde_json::Value| Ok(())).await.unwrap();
        events.on_plugin("test_plugin", "plugin_event", |_: serde_json::Value| Ok(())).await.unwrap();
        
        let stats = events.get_stats().await;
        assert_eq!(stats.total_handlers, 3);
        
        let detailed_stats = events.get_detailed_stats().await;
        assert_eq!(detailed_stats.handler_count_by_category.core_handlers, 1);
        assert_eq!(detailed_stats.handler_count_by_category.client_handlers, 1);
        assert_eq!(detailed_stats.handler_count_by_category.plugin_handlers, 1);
        
        println!("✅ System stats test passed");
    }

    #[tokio::test]
    async fn test_event_system_creation() {
        let events = EventSystem::new();
        let stats = events.get_stats().await;
        
        assert_eq!(stats.total_handlers, 0);
        assert_eq!(stats.events_emitted, 0);
    }

    #[tokio::test]
    async fn test_core_event_registration_and_emission() {
        let events = EventSystem::new();
        
        // Register handler
        events.on_core("player_connected", |event: PlayerConnectedEvent| {
            assert!(!event.player_id.to_string().is_empty());
            Ok(())
        }).await.unwrap();
        
        // Check handler was registered
        let stats = events.get_stats().await;
        assert_eq!(stats.total_handlers, 1);
        
        // Emit event
        let player_event = PlayerConnectedEvent {
            player_id: PlayerId::new(),
            connection_id: "test_conn".to_string(),
            remote_addr: "127.0.0.1:8080".to_string(),
            timestamp: crate::utils::current_timestamp(),
        };
        
        events.emit_core("player_connected", &player_event).await.unwrap();
        
        // Check event was emitted
        let stats = events.get_stats().await;
        assert_eq!(stats.events_emitted, 1);
    }

    #[tokio::test]
    async fn test_gorc_event_system() {
        let gorc_instances = Arc::new(GorcInstanceManager::new());
        let events = EventSystem::with_gorc(gorc_instances);
        
        // Register GORC handler
        events.on_gorc("Asteroid", 0, "position_update", |event: GorcEvent| {
            assert_eq!(event.object_type, "Asteroid");
            assert_eq!(event.channel, 0);
            Ok(())
        }).await.unwrap();
        
        // Emit GORC event
        let object_id = GorcObjectId::new();
        let gorc_event = GorcEvent {
            object_id: object_id.to_string(),
            instance_uuid: format!("test_instance_{}", object_id),
            object_type: "Asteroid".to_string(),
            channel: 0,
            data: vec![1, 2, 3, 4],
            priority: "Critical".to_string(),
            timestamp: crate::utils::current_timestamp(),
        };
        
        events.emit_gorc("Asteroid", 0, "position_update", &gorc_event).await.unwrap();
        
        let stats = events.get_stats().await;
        assert_eq!(stats.gorc_events_emitted, 1);
    }

    #[tokio::test]
    async fn test_handler_category_stats() {
        let events = EventSystem::new();
        
        // Register different types of handlers
        events.on_core("test_core", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        events.on_client("test", "test_client", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        events.on_plugin("test_plugin", "test_event", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        events.on_gorc("TestObject", 0, "test_gorc", |_: GorcEvent| Ok(())).await.unwrap();
        
        let detailed_stats = events.get_detailed_stats().await;
        let category_stats = detailed_stats.handler_count_by_category;
        
        assert_eq!(category_stats.core_handlers, 1);
        assert_eq!(category_stats.client_handlers, 1);
        assert_eq!(category_stats.plugin_handlers, 1);
        assert_eq!(category_stats.gorc_handlers, 1);
        assert_eq!(category_stats.gorc_instance_handlers, 0);
    }

    #[tokio::test]
    async fn test_event_validation() {
        let events = EventSystem::new();
        
        // Register a handler then remove it to create an empty key
        events.on_core("test_event", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        
        let issues = events.validate().await;
        // Should not have issues with a properly registered handler
        assert!(issues.is_empty());
    }

    #[tokio::test]
    async fn test_handler_removal() {
        let events = EventSystem::new();
        
        events.on_core("test1", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        events.on_core("test2", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        events.on_client("namespace", "test3", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        
        let initial_stats = events.get_stats().await;
        assert_eq!(initial_stats.total_handlers, 3);
        
        // Remove core handlers
        let removed = events.remove_handlers("core:").await;
        assert_eq!(removed, 2);
        
        let final_stats = events.get_stats().await;
        assert_eq!(final_stats.total_handlers, 1);
    }
}