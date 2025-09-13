/// High-level API functions for creating and managing Horizon systems
use crate::*;

/// Creates a complete event system with full GORC integration
/// 
/// This is the recommended way to create an event system for games that need
/// object replication capabilities.
/// 
/// # Arguments
/// 
/// * `server_context` - Server context providing access to core services
/// 
/// # Returns
/// 
/// Returns a tuple of (EventSystem, CompleteGorcSystem) ready for use
/// 
/// # Examples
/// 
/// ```rust,no_run
/// use horizon_event_system::{create_complete_horizon_system, ServerContext, PlayerId, Vec3};
/// use std::sync::Arc;
/// use async_trait::async_trait;
/// 
/// struct MyServerContext;
/// 
/// #[async_trait]
/// impl ServerContext for MyServerContext {
///     async fn get_player_count(&self) -> u32 { 0 }
///     async fn broadcast_message(&self, _message: &str) {}
///     async fn get_player_ids(&self) -> Vec<PlayerId> { vec![] }
///     async fn get_player_position(&self, _player_id: PlayerId) -> Option<Vec3> { None }
/// }
/// 
/// impl MyServerContext {
///     fn new() -> Self { Self }
/// }
/// 
/// async fn example() -> Result<(), Box<dyn std::error::Error>> {
///     let server_context = Arc::new(MyServerContext::new());
///     let (events, _gorc_system) = create_complete_horizon_system(server_context)?;
///     
///     // Use the event system for traditional events
///     // events.on_core("server_started", |event: ServerStartedEvent| {
///     //     println!("Server online!");
///     //     Ok(())
///     // }).await?;
///     Ok(())
/// }
/// ```
pub fn create_complete_horizon_system(
    server_context: Arc<dyn ServerContext>
) -> Result<(Arc<EventSystem>, CompleteGorcSystem), gorc::GorcError> {
    let gorc_system = gorc::utils::create_complete_gorc_system(server_context)?;
    let event_system = Arc::new(EventSystem::with_gorc(gorc_system.instance_manager.clone()));

    Ok((event_system, gorc_system))
}

/// Creates a lightweight event system without GORC for simple use cases
/// 
/// This creates just the basic event system without object replication capabilities.
/// Use this for simpler applications that don't need advanced replication features.
/// 
/// # Returns
/// 
/// Returns an Arc<EventSystem> ready for basic event handling
/// 
/// # Examples
/// 
/// ```rust,no_run
/// use horizon_event_system::{create_simple_horizon_system, PlayerConnectedEvent};
/// 
/// async fn example() -> Result<(), Box<dyn std::error::Error>> {
///     let events = create_simple_horizon_system();
///     
///     events.on_core("player_connected", |event: PlayerConnectedEvent| {
///         println!("Player {} connected", event.player_id);
///         Ok(())
///     }).await?;
///     Ok(())
/// }
/// ```
pub fn create_simple_horizon_system() -> Arc<EventSystem> {
    create_horizon_event_system()
}