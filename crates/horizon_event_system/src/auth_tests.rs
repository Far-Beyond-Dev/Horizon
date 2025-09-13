//! Tests for authentication-related core events.

#[cfg(test)]
mod tests {
    use crate::{
        AuthenticationStatus, AuthenticationStatusSetEvent, AuthenticationStatusGetEvent, 
        AuthenticationStatusGetResponseEvent, AuthenticationStatusChangedEvent, 
        create_horizon_event_system, PlayerId, current_timestamp
    };
    use tracing::debug;
    
    #[tokio::test]
    async fn test_auth_status_set_event() {
        let events = create_horizon_event_system();
        let player_id = PlayerId::new();
        
        // Test setting authentication status
        let auth_event = AuthenticationStatusSetEvent {
            player_id,
            status: AuthenticationStatus::Authenticated,
            timestamp: current_timestamp(),
        };
        
        // This should not panic and should serialize correctly
        let result = events.emit_core("auth_status_set", &auth_event).await;
        assert!(result.is_ok(), "Failed to emit auth status set event: {:?}", result);
    }
    
    #[tokio::test]
    async fn test_auth_status_get_event() {
        let events = create_horizon_event_system();
        let player_id = PlayerId::new();
        
        // Test querying authentication status
        let auth_query = AuthenticationStatusGetEvent {
            player_id,
            request_id: "test_request_123".to_string(),
            timestamp: current_timestamp(),
        };
        
        // This should not panic and should serialize correctly
        let result = events.emit_core("auth_status_get", &auth_query).await;
        assert!(result.is_ok(), "Failed to emit auth status get event: {:?}", result);
    }
    
    #[tokio::test]
    async fn test_auth_status_changed_event() {
        let events = create_horizon_event_system();
        let player_id = PlayerId::new();
        
        // Test authentication status change notification
        let auth_changed = AuthenticationStatusChangedEvent {
            player_id,
            old_status: AuthenticationStatus::Authenticating,
            new_status: AuthenticationStatus::Authenticated,
            timestamp: current_timestamp(),
        };
        
        // This should not panic and should serialize correctly
        let result = events.emit_core("auth_status_changed", &auth_changed).await;
        assert!(result.is_ok(), "Failed to emit auth status changed event: {:?}", result);
    }
    
    #[tokio::test]
    async fn test_auth_status_default() {
        // Test that default authentication status is Unauthenticated
        let default_status = AuthenticationStatus::default();
        assert_eq!(default_status, AuthenticationStatus::Unauthenticated);
    }
    
    #[tokio::test]
    async fn test_auth_status_serialization() {
        // Test serialization and deserialization of authentication status
        let status = AuthenticationStatus::Authenticated;
        let serialized = serde_json::to_string(&status).unwrap();
        let deserialized: AuthenticationStatus = serde_json::from_str(&serialized).unwrap();
        assert_eq!(status, deserialized);
    }
    
    #[tokio::test]
    async fn test_auth_status_get_response_event() {
        let events = create_horizon_event_system();
        let player_id = PlayerId::new();
        
        // Test authentication status query response
        let auth_response = AuthenticationStatusGetResponseEvent {
            player_id,
            request_id: "test_request_123".to_string(),
            status: Some(AuthenticationStatus::Authenticated),
            timestamp: current_timestamp(),
        };
        
        // This should not panic and should serialize correctly
        let result = events.emit_core("auth_status_get_response", &auth_response).await;
        assert!(result.is_ok(), "Failed to emit auth status get response event: {:?}", result);
    }

    #[tokio::test]
    async fn test_auth_event_handler_registration() {
        let events = create_horizon_event_system();
        
        // Test that we can register handlers for authentication events
        let result = events.on_core("auth_status_set", |event: AuthenticationStatusSetEvent| {
            debug!("Received auth status set event for player: {}", event.player_id);
            Ok(())
        }).await;
        
        assert!(result.is_ok(), "Failed to register auth status set handler: {:?}", result);
    }
}