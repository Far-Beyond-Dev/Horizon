//! Integration tests for authentication core events with game server.

#[cfg(test)]
mod tests {
    use crate::connection::ConnectionManager;
    use horizon_event_system::{PlayerId, AuthenticationStatus, AuthenticationStatusSetEvent, current_timestamp};
    use std::net::SocketAddr;
    use std::sync::Arc;
    
    #[tokio::test]
    async fn test_connection_manager_auth_status() {
        let connection_manager = Arc::new(ConnectionManager::new());
        
        // Simulate a client connection
        let remote_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let connection_id = connection_manager.add_connection(remote_addr).await;
        
        // Initially, auth status should be default (Unauthenticated)
        let initial_status = connection_manager.get_auth_status(connection_id).await;
        assert_eq!(initial_status, Some(AuthenticationStatus::Unauthenticated));
        
        // Set authentication status to Authenticating
        connection_manager.set_auth_status(connection_id, AuthenticationStatus::Authenticating).await;
        let status = connection_manager.get_auth_status(connection_id).await;
        assert_eq!(status, Some(AuthenticationStatus::Authenticating));
        
        // Set authentication status to Authenticated
        connection_manager.set_auth_status(connection_id, AuthenticationStatus::Authenticated).await;
        let status = connection_manager.get_auth_status(connection_id).await;
        assert_eq!(status, Some(AuthenticationStatus::Authenticated));
        
        // Assign a player ID and test player-based auth status queries
        let player_id = PlayerId::new();
        connection_manager.set_player_id(connection_id, player_id).await;
        
        // Test getting auth status by player ID
        let player_status = connection_manager.get_auth_status_by_player(player_id).await;
        assert_eq!(player_status, Some(AuthenticationStatus::Authenticated));
        
        // Test setting auth status by player ID
        let success = connection_manager.set_auth_status_by_player(player_id, AuthenticationStatus::AuthenticationFailed).await;
        assert!(success);
        
        let player_status = connection_manager.get_auth_status_by_player(player_id).await;
        assert_eq!(player_status, Some(AuthenticationStatus::AuthenticationFailed));
    }
    
    #[tokio::test]
    async fn test_auth_status_workflow_simulation() {
        let connection_manager = Arc::new(ConnectionManager::new());
        
        // Simulate authentication workflow
        let remote_addr: SocketAddr = "127.0.0.1:54321".parse().unwrap();
        let connection_id = connection_manager.add_connection(remote_addr).await;
        let player_id = PlayerId::new();
        
        // Step 1: Connection established, player not yet assigned
        let initial_status = connection_manager.get_auth_status(connection_id).await;
        assert_eq!(initial_status, Some(AuthenticationStatus::Unauthenticated));
        
        // Step 2: Player ID assigned, start authentication process
        connection_manager.set_player_id(connection_id, player_id).await;
        connection_manager.set_auth_status_by_player(player_id, AuthenticationStatus::Authenticating).await;
        
        let status = connection_manager.get_auth_status_by_player(player_id).await;
        assert_eq!(status, Some(AuthenticationStatus::Authenticating));
        
        // Step 3: Authentication successful
        connection_manager.set_auth_status_by_player(player_id, AuthenticationStatus::Authenticated).await;
        
        let final_status = connection_manager.get_auth_status_by_player(player_id).await;
        assert_eq!(final_status, Some(AuthenticationStatus::Authenticated));
        
        // Step 4: Verify connection still exists and has correct status
        let connection_exists = connection_manager.get_connection_id_by_player(player_id).await;
        assert!(connection_exists.is_some());
        
        let status_via_connection = connection_manager.get_auth_status(connection_exists.unwrap()).await;
        assert_eq!(status_via_connection, Some(AuthenticationStatus::Authenticated));
    }
    
    #[tokio::test]
    async fn test_auth_status_nonexistent_player() {
        let connection_manager = Arc::new(ConnectionManager::new());
        let nonexistent_player = PlayerId::new();
        
        // Querying auth status for nonexistent player should return None
        let status = connection_manager.get_auth_status_by_player(nonexistent_player).await;
        assert_eq!(status, None);
        
        // Setting auth status for nonexistent player should return false
        let success = connection_manager.set_auth_status_by_player(nonexistent_player, AuthenticationStatus::Authenticated).await;
        assert!(!success);
    }
}