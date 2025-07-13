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
/// ```rust
/// let server_context = Arc::new(MyServerContext::new());
/// let (events, gorc_system) = create_complete_horizon_system(server_context)?;
/// 
/// // Use the event system for traditional events
/// events.on_core("server_started", |event: ServerStartedEvent| {
///     println!("Server online!");
///     Ok(())
/// }).await?;
/// 
/// // Use the GORC system for object replication
/// let asteroid_id = gorc_system.register_object(my_asteroid, position).await;
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
/// ```rust
/// let events = create_simple_horizon_system();
/// 
/// events.on_core("player_connected", |event: PlayerConnectedEvent| {
///     println!("Player {} connected", event.player_id);
///     Ok(())
/// }).await?;
/// ```
pub fn create_simple_horizon_system() -> Arc<EventSystem> {
    create_horizon_event_system()
}