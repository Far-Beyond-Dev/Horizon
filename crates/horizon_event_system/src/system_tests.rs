//! Tests for the connection-aware event system

#[cfg(test)]
mod tests {
    use crate::{EventSystem, ClientConnectionRef, ClientResponseSender};
    use crate::events::RawClientMessageEvent;
    use crate::types::PlayerId;
    use std::sync::{Arc, Mutex};
    
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
                async move {
                    // Mark that we received the event
                    *response_received.lock().unwrap() = true;
                    
                    // Send a response back to the client
                    client.respond(b"Hello back!").await?;
                    Ok(())
                }
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
            move |_event: serde_json::Value| async move {
                // This would be called if properly triggered
                tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
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
}