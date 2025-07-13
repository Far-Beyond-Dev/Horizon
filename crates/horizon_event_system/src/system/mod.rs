/// Event system module - broken down into manageable components
mod client;
mod core;
mod emitters;
mod handlers;
mod management;
mod stats;

// Re-export all public items from submodules
pub use client::{ClientConnectionRef, ClientResponseSender};
pub use core::EventSystem;
pub use stats::{EventSystemStats, DetailedEventSystemStats, HandlerCategoryStats};

// Re-export utility functions
use crate::gorc::instance::GorcInstanceManager;
use std::sync::Arc;

/// Helper function to create an event system with GORC integration
pub fn create_event_system_with_gorc(
    gorc_instances: Arc<GorcInstanceManager>
) -> Arc<EventSystem> {
    Arc::new(EventSystem::with_gorc(gorc_instances))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{PlayerConnectedEvent, GorcEvent};
    use crate::types::PlayerId;
    use crate::gorc::instance::{GorcInstanceManager, GorcObjectId};

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
        let gorc_event = GorcEvent {
            object_id: GorcObjectId::new().to_string(),
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