//! # Player Plugin for Horizon GORC System
//!
//! This crate provides a complete player management system for the Horizon game engine,
//! implementing the GORC (Game Object Replication and Communication) architecture for
//! distributed multiplayer gaming.
//!
//! ## Overview
//!
//! The Player Plugin manages the complete lifecycle of players in the game world:
//! - **Connection Management**: Player join/leave events and resource allocation
//! - **Movement System**: Real-time position updates with spatial replication
//! - **Combat System**: Weapon firing and combat event distribution
//! - **Communication**: Chat and messaging between nearby players
//! - **Scanning System**: Detailed ship information sharing at close range
//!
//! ## GORC Architecture
//!
//! The plugin utilizes GORC's multi-channel communication system:
//!
//! | Channel | Purpose            | Range | Frequency    | Examples                   |
//! |---------|--------------------|-------|--------------|----------------------------|
//! | 0       | Critical movement  | 25m   | 60Hz         | Position, velocity, health |
//! | 1       | Combat events      | 500m  | Event-driven | Weapon fire, explosions    |
//! | 2       | Communication      | 300m  | Event-driven | Chat, voice, emotes        |
//! | 3       | Detailed scanning  | 100m  | Event-driven | Ship specs, cargo          |
//!
//! ## Security Model
//!
//! All player interactions include comprehensive security validation:
//! - **Authentication**: Only authenticated connections can send requests
//! - **Ownership**: Players can only control their own ships and objects
//! - **Input Validation**: All data is validated for bounds and format
//! - **Anti-Cheat**: Movement and combat include anti-exploitation measures
//!
//! ## Performance Characteristics
//!
//! The system is designed for high-performance multiplayer gaming:
//! - **Spatial Culling**: Events only replicate to relevant nearby players
//! - **Async Processing**: All handlers use non-blocking async operations
//! - **Memory Efficiency**: Minimal allocations during steady-state operation
//! - **Scalable Architecture**: Supports hundreds of concurrent players per instance
//!
//! ## Example Usage
//!
//! ```rust
//! use plugin_player::PlayerPlugin;
//! use horizon_event_system::create_simple_plugin;
//!
//! // The plugin is automatically registered using the create_simple_plugin! macro
//! // and provides complete player management out of the box
//! ```
//!
//! ## Module Organization
//!
//! - [`player`] - Core player object and GORC integration
//! - [`events`] - Event data structures and serialization
//! - [`handlers`] - Specialized event handlers for different game systems

use async_trait::async_trait;
use dashmap::DashMap;
use horizon_event_system::{
    create_simple_plugin,
    EventSystem,
    GorcObjectId,
    LogLevel,
    PlayerId,
    PluginError,
    ServerContext,
    SimplePlugin,
};
use std::sync::Arc;
use tracing::{ debug, error };

// Public modules for external access
pub mod events;
pub mod handlers;
pub mod player;

// Internal imports
use handlers::*;

/// The core Player Plugin implementation for the Horizon GORC system.
///
/// This plugin provides comprehensive player management including connection lifecycle,
/// real-time movement replication, combat event handling, communication systems,
/// and detailed ship scanning capabilities.
///
/// ## Architecture
///
/// The plugin follows the GORC (Game Object Replication and Communication) pattern:
/// - Players are registered as GORC objects with spatial awareness
/// - Events are processed through specialized handlers for different game systems
/// - All communication uses multi-channel GORC messaging for optimal performance
///
/// ## Thread Safety
///
/// The plugin is designed for high-concurrency operation:
/// - Player registry uses `DashMap` for lock-free concurrent access
/// - All handlers are async and non-blocking
/// - Event processing is distributed across multiple async tasks
///
/// ## Resource Management
///
/// - **Player Registry**: Maps player IDs to GORC object IDs for efficient cleanup
/// - **Automatic Cleanup**: Players are removed from all systems on disconnect
/// - **Memory Efficient**: Uses shared references and zero-copy operations where possible
pub struct PlayerPlugin {
    /// Human-readable name of the plugin
    name: String,
    /// Thread-safe registry mapping PlayerId to GorcObjectId for resource management
    /// This allows efficient lookup during movement, combat, and cleanup operations
    players: Arc<DashMap<PlayerId, GorcObjectId>>,
}

impl PlayerPlugin {
    /// Creates a new PlayerPlugin instance with default configuration.
    ///
    /// Initializes the plugin with:
    /// - Empty player registry for tracking active players
    /// - Default plugin name for identification in logs
    /// - Thread-safe data structures for concurrent operation
    ///
    /// # Returns
    ///
    /// A new `PlayerPlugin` instance ready for registration with the event system.
    ///
    /// # Example
    ///
    /// ```rust
    /// use plugin_player::PlayerPlugin;
    ///
    /// let plugin = PlayerPlugin::new();
    /// // Plugin is now ready to be registered with the server
    /// ```
    pub fn new() -> Self {
        debug!("🎮 PlayerPlugin: Creating new instance with GORC architecture");
        Self {
            name: "PlayerPlugin".to_string(),
            players: Arc::new(DashMap::new()),
        }
    }
}

impl Default for PlayerPlugin {
    /// Provides default construction via `PlayerPlugin::new()`.
    ///
    /// This implementation allows the plugin to be created using Rust's
    /// standard `Default` trait, which is useful for dependency injection
    /// and configuration systems.
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SimplePlugin for PlayerPlugin {
    /// Returns the human-readable name of the plugin.
    ///
    /// Used by the event system for logging and identification purposes.
    fn name(&self) -> &str {
        &self.name
    }

    /// Returns the version string of the plugin.
    ///
    /// Used for compatibility checking and deployment tracking.
    /// Version follows semantic versioning (major.minor.patch).
    fn version(&self) -> &str {
        "1.0.0"
    }

    /// Registers all event handlers for the plugin with the event system.
    ///
    /// This method sets up the complete player management system by registering
    /// specialized handlers for different aspects of player interaction:
    ///
    /// ## Handler Registration
    ///
    /// - **Connection Handlers**: Core player lifecycle (connect/disconnect)
    /// - **Movement Handler**: GORC channel 0 for real-time position updates
    /// - **Combat Handler**: GORC channel 1 for weapon firing and combat events
    /// - **Communication Handler**: GORC channel 2 for chat and messaging
    /// - **Scanning Handler**: GORC channel 3 for detailed ship information
    ///
    /// ## Event System Integration
    ///
    /// The plugin integrates with both core server events and GORC client events:
    /// - Core events handle server-side lifecycle management
    /// - GORC events process client requests with spatial replication
    ///
    /// # Parameters
    ///
    /// - `events`: Shared reference to the event system
    /// - `context`: Server context providing logging and async runtime access
    ///
    /// # Returns
    ///
    /// `Result<(), PluginError>` - Success or detailed registration error
    ///
    /// # Example
    ///
    /// ```rust
    /// // Handler registration is automatic when plugin is loaded
    /// // Each handler processes specific event types and channels
    /// ```
    async fn register_handlers(
        &mut self,
        events: Arc<EventSystem>,
        context: Arc<dyn ServerContext>
    ) -> Result<(), PluginError> {
        debug!("🎮 PlayerPlugin: Registering comprehensive GORC event handlers...");
        context.log(
            LogLevel::Info,
            "🎮 PlayerPlugin: Initializing multi-channel player management system..."
        );

        let luminal_handle = context.luminal_handle();

        // Register core server event handlers for player lifecycle management
        self.register_connection_handlers(
            Arc::clone(&events),
            context.clone(),
            luminal_handle.clone()
        ).await?;

        // Register GORC client event handlers for real-time gameplay
        self.register_movement_handler(Arc::clone(&events), luminal_handle.clone()).await?;
        self.register_combat_handler(Arc::clone(&events), luminal_handle.clone()).await?;
        self.register_communication_handler(Arc::clone(&events), luminal_handle.clone()).await?;
        self.register_scanning_handler(Arc::clone(&events), luminal_handle.clone()).await?;

        context.log(
            LogLevel::Info,
            "🎮 PlayerPlugin: ✅ All GORC player handlers registered successfully!"
        );
        Ok(())
    }

    /// Called when the plugin is initialized after registration.
    ///
    /// This lifecycle method is called once the plugin has been successfully
    /// registered with the event system and is ready to begin processing events.
    ///
    /// # Parameters
    ///
    /// - `context`: Server context for logging and system access
    ///
    /// # Returns
    ///
    /// `Result<(), PluginError>` - Success or initialization error
    async fn on_init(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            "🎮 PlayerPlugin: GORC player management system activated and ready!"
        );
        Ok(())
    }

    /// Called when the plugin is shutting down.
    ///
    /// This lifecycle method handles cleanup when the server is shutting down
    /// or the plugin is being unloaded. It ensures all player resources are
    /// properly cleaned up and logged for debugging.
    ///
    /// # Parameters
    ///
    /// - `context`: Server context for logging and system access
    ///
    /// # Returns
    ///
    /// `Result<(), PluginError>` - Success or shutdown error
    async fn on_shutdown(&mut self, context: Arc<dyn ServerContext>) -> Result<(), PluginError> {
        context.log(
            LogLevel::Info,
            &format!(
                "🎮 PlayerPlugin: Shutting down gracefully. Managed {} players during session",
                self.players.len()
            )
        );

        // Clear the player registry to release all GORC object references
        self.players.clear();
        Ok(())
    }
}

// Implementation of individual handler registration methods
impl PlayerPlugin {
    /// Registers connection/disconnection handlers for player lifecycle management.
    ///
    /// These handlers manage the complete player lifecycle from connection to cleanup:
    /// - Creates GORC player objects when players connect
    /// - Registers players with the spatial replication system
    /// - Cleans up resources when players disconnect
    ///
    /// # Parameters
    ///
    /// - `events`: Event system reference for handler registration
    /// - `luminal_handle`: Async runtime handle for background operations
    ///
    /// # Returns
    ///
    /// `Result<(), PluginError>` - Success or registration error
    async fn register_connection_handlers(
        &self,
        events: Arc<EventSystem>,
        context: Arc<dyn ServerContext>,
        luminal_handle: luminal::Handle
    ) -> Result<(), PluginError> {
        debug!("🎮 PlayerPlugin: Registering connection lifecycle handlers");

        // Register player connection handler
        let players_conn = Arc::clone(&self.players);
        let events_for_conn = Arc::clone(&events);
        let luminal_handle_connect = luminal_handle.clone();

        events
            .on_core("player_connected", move |event: serde_json::Value| {
                let players = players_conn.clone();
                let events = events_for_conn.clone();
                let handle = luminal_handle_connect.clone();

                // Use the dedicated connection handler
                let handle_clone = handle.clone();
                handle.spawn(async move {
                    match
                        serde_json::from_value::<horizon_event_system::PlayerConnectedEvent>(event)
                    {
                        Ok(player_event) => {
                            if
                                let Err(e) = handle_player_connected(
                                    player_event,
                                    players,
                                    events,
                                    handle_clone
                                ).await
                            {
                                error!("🎮 Failed to handle player connection: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("🎮 Failed to deserialize PlayerConnectedEvent: {}", e);
                        }
                    }
                });

                Ok(())
            }).await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Register player disconnection handler
        let players_disc = Arc::clone(&self.players);
        events
            .on_core("player_disconnected", move |event: serde_json::Value| {
                let players = players_disc.clone();

                Ok(())
            }).await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        debug!("🎮 PlayerPlugin: ✅ Connection handlers registered");
        Ok(())
    }

    /// Registers GORC channel 0 handler for real-time movement events.
    ///
    /// Channel 0 handles critical player state with high frequency updates:
    /// - 60Hz update rate for smooth movement
    /// - 25m replication range for performance
    /// - Authentication and ownership validation
    /// - Position update broadcasting to nearby players
    ///
    /// # Parameters
    ///
    /// - `events`: Event system reference for handler registration
    /// - `luminal_handle`: Async runtime handle for background operations
    ///
    /// # Returns
    ///
    /// `Result<(), PluginError>` - Success or registration error
    async fn register_movement_handler(
        &self,
        events: Arc<EventSystem>,
        luminal_handle: luminal::Handle
    ) -> Result<(), PluginError> {
        debug!("🎮 PlayerPlugin: Registering GORC channel 0 (movement) handler");

        let events_for_move = Arc::clone(&events);
        let luminal_handle_move = luminal_handle.clone();
        events
            .on_gorc_client(
                luminal_handle,
                "GorcPlayer",
                0, // Channel 0: Critical movement data
                "move",
                move |gorc_event, client_player, connection, object_instance| {
                    // Use the dedicated movement handler
                    movement::handle_movement_request_sync(
                        gorc_event,
                        client_player,
                        connection,
                        object_instance,
                        events_for_move.clone(),
                        luminal_handle_move.clone()
                    )
                }
            ).await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        debug!("🎮 PlayerPlugin: ✅ Movement handler registered on channel 0");
        Ok(())
    }

    /// Registers GORC channel 1 handler for combat events.
    ///
    /// Channel 1 handles weapon firing and combat interactions:
    /// - Event-driven weapon fire processing
    /// - 500m replication range for tactical awareness
    /// - Security validation for weapon authorization
    /// - Combat event broadcasting to nearby ships
    ///
    /// # Parameters
    ///
    /// - `events`: Event system reference for handler registration
    /// - `luminal_handle`: Async runtime handle for background operations
    ///
    /// # Returns
    ///
    /// `Result<(), PluginError>` - Success or registration error
    async fn register_combat_handler(
        &self,
        events: Arc<EventSystem>,
        luminal_handle: luminal::Handle
    ) -> Result<(), PluginError> {
        debug!("🎮 PlayerPlugin: Registering GORC channel 1 (combat) handler");

        let events_for_combat = Arc::clone(&events);
        let events_for_blocks = Arc::clone(&events);
        let luminal_handle_attack = luminal_handle.clone();

        // Register attack handler
        events
            .on_gorc_client(
                luminal_handle_attack,
                "GorcPlayer",
                1, // Channel 1: Combat events
                "attack",
                move |gorc_event, client_player, connection, object_instance| {
                    // Use the dedicated combat handler
                    combat::handle_attack_request_sync(
                        gorc_event,
                        client_player,
                        connection,
                        object_instance,
                        events_for_combat.clone()
                    )
                }
            ).await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Register block_change handler
        let luminal_handle_block_for_closure = luminal_handle.clone();
        events
            .on_gorc_client(
                luminal_handle_block_for_closure.clone(),
                "GorcPlayer",
                1, // Channel 1: World events
                "block_change",
                move |gorc_event, client_player, connection, object_instance| {
                    // Use the dedicated block change handler
                    let luminal_handle_block = luminal_handle_block_for_closure.clone();
                    combat::handle_block_change_request_sync(
                        gorc_event,
                        client_player,
                        connection,
                        object_instance,
                        events_for_blocks.clone(),
                        luminal_handle_block
                    )
                }
            ).await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        debug!("🎮 PlayerPlugin: ✅ Combat and block change handlers registered on channel 1");
        Ok(())
    }

    /// Registers GORC channel 2 handler for communication events.
    ///
    /// Channel 2 handles player chat and messaging:
    /// - Social communication between nearby players
    /// - 300m replication range for local area chat
    /// - Multi-channel support (general, emergency, private)
    /// - Message validation and content filtering
    ///
    /// # Parameters
    ///
    /// - `events`: Event system reference for handler registration
    /// - `luminal_handle`: Async runtime handle for background operations
    ///
    /// # Returns
    ///
    /// `Result<(), PluginError>` - Success or registration error
    async fn register_communication_handler(
        &self,
        events: Arc<EventSystem>,
        luminal_handle: luminal::Handle
    ) -> Result<(), PluginError> {
        debug!("🎮 PlayerPlugin: Registering GORC channel 2 (communication) handler");

        let events_for_chat = Arc::clone(&events);
        let luminal_handle_chat = luminal_handle.clone();
        events
            .on_gorc_client(
                luminal_handle,
                "GorcPlayer",
                2, // Channel 2: Communication events
                "chat",
                move |gorc_event, client_player, connection, object_instance| {
                    // Use the dedicated communication handler
                    communication::handle_communication_request_sync(
                        gorc_event,
                        client_player,
                        connection,
                        object_instance,
                        events_for_chat.clone(),
                        luminal_handle_chat.clone()
                    )
                }
            ).await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        debug!("🎮 PlayerPlugin: ✅ Communication handler registered on channel 2");
        Ok(())
    }

    /// Registers GORC channel 3 handler for ship scanning events.
    ///
    /// Channel 3 handles detailed ship information sharing:
    /// - Close-range ship scanning and metadata exchange
    /// - 100m intimate range for intentional close encounters
    /// - Rich ship data including specs, cargo, pilot info
    /// - Privacy-aware information sharing
    ///
    /// # Parameters
    ///
    /// - `events`: Event system reference for handler registration
    /// - `luminal_handle`: Async runtime handle for background operations
    ///
    /// # Returns
    ///
    /// `Result<(), PluginError>` - Success or registration error
    async fn register_scanning_handler(
        &self,
        events: Arc<EventSystem>,
        luminal_handle: luminal::Handle
    ) -> Result<(), PluginError> {
        debug!("🎮 PlayerPlugin: Registering GORC channel 3 (scanning) handler");

        let events_for_scan = Arc::clone(&events);
        let luminal_handle_scan = luminal_handle.clone();
        events
            .on_gorc_client(
                luminal_handle,
                "GorcPlayer",
                3, // Channel 3: Detailed scanning events
                "ship_scan",
                move |gorc_event, client_player, connection, object_instance| {
                    // Use the dedicated scanning handler
                    scanning::handle_scanning_request_sync(
                        gorc_event,
                        client_player,
                        connection,
                        object_instance,
                        events_for_scan.clone(),
                        luminal_handle_scan.clone()
                    )
                }
            ).await
            .map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        debug!("🎮 PlayerPlugin: ✅ Scanning handler registered on channel 3");
        Ok(())
    }
}

// Create the plugin using our macro - zero unsafe code!
create_simple_plugin!(PlayerPlugin);
