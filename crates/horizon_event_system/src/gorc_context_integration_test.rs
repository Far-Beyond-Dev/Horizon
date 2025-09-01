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

    fn tokio_handle(&self) -> tokio::runtime::Handle {
        tokio::runtime::Handle::current()
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
            fn tokio_handle(&self) -> tokio::runtime::Handle { tokio::runtime::Handle::current() }
            fn gorc_instance_manager(&self) -> Option<Arc<gorc::GorcInstanceManager>> { None }
        }
        
        let mock_context = BasicMockContext;
        assert!(mock_context.gorc_instance_manager().is_none());
    }
}