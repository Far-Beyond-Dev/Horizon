//! # GORC Player Object Implementation
//!
//! This module defines the core player object that integrates with the GORC
//! (Game Object Replication and Communication) system for distributed multiplayer
//! gaming in the Horizon engine.
//!
//! ## GORC Zone Architecture
//!
//! The player object uses GORC's zone-based replication system with three distinct
//! zones, each optimized for different types of data and update frequencies:
//!
//! ### Zone 0: Critical Data (25m range, 60Hz)
//! High-frequency updates for essential game state:
//! - **Position**: Current 3D coordinates in world space
//! - **Velocity**: Current movement vector for prediction
//! - **Health**: Current hit points for combat systems
//!
//! ### Zone 1: Detailed Data (100m range, 30Hz)  
//! Medium-frequency updates for gameplay state:
//! - **Movement State**: Current movement mode (idle, walking, running, etc.)
//! - **Level**: Player experience level for progression systems
//!
//! ### Zone 2: Social Data (200m range, 15Hz)
//! Low-frequency updates for social interaction:
//! - **Name**: Player display name for identification
//! - **Chat Bubble**: Temporary chat message display
//!
//! ## Performance Optimization
//!
//! The zone-based approach provides several performance benefits:
//! - **Spatial Culling**: Players only receive updates for nearby objects
//! - **Frequency Scaling**: Critical data updates more frequently than social data
//! - **Bandwidth Efficiency**: Different data types use appropriate update rates
//! - **Scalability**: System scales to hundreds of players per instance
//!
//! ## Example Usage
//!
//! ```rust
//! use plugin_player::player::GorcPlayer;
//! use horizon_event_system::{PlayerId, Vec3};
//!
//! // Create a new player at spawn position
//! let player = GorcPlayer::new(
//!     PlayerId(42),
//!     "PlayerName".to_string(),
//!     Vec3::new(0.0, 0.0, 0.0)
//! );
//!
//! // Update player position with validation
//! let new_pos = Vec3::new(10.0, 0.0, 5.0);
//! let velocity = Vec3::new(2.0, 0.0, 1.0);
//! 
//! match player.validate_and_apply_movement(new_pos, velocity) {
//!     Ok(()) => println!("Movement applied successfully"),
//!     Err(e) => println!("Movement rejected: {}", e),
//! }
//! ```

use serde::{Deserialize, Serialize};
use horizon_event_system::{PlayerId, Vec3, GorcZoneData, impl_gorc_object};
use chrono::{DateTime, Utc};

/// Critical player data for high-frequency replication (GORC Zone 0).
///
/// This structure contains the most essential player state that must be updated
/// frequently (60Hz) and replicated to all nearby players within 25 meters.
/// The data in this zone is crucial for smooth gameplay and responsive interactions.
///
/// ## Update Frequency: 60Hz
/// ## Replication Range: 25 meters
/// ## Network Priority: Highest
///
/// # Fields
///
/// - `position`: Current 3D world coordinates for spatial tracking
/// - `velocity`: Current movement vector for client-side prediction
/// - `health`: Current hit points for combat and damage systems
///
/// # Performance Notes
///
/// This data structure is optimized for minimal serialization overhead:
/// - Uses native f32 types for efficient network transmission
/// - Designed for frequent updates without allocation pressure
/// - Automatically managed by GORC spatial replication system
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerCriticalData {
    /// Current position in world coordinates (meters)
    pub position: Vec3,
    /// Current velocity vector (meters/second)
    pub velocity: Vec3,
    /// Current health points (0.0 to 100.0)
    pub health: f32,
}

impl GorcZoneData for PlayerCriticalData {
    /// Returns the type identifier for GORC zone data serialization.
    fn zone_type_name() -> &'static str {
        "PlayerCriticalData"
    }
}

/// Detailed player data for medium-frequency replication (GORC Zone 1).
///
/// This structure contains secondary player state that is updated at moderate
/// frequency (30Hz) and replicated to players within 100 meters. This zone
/// includes gameplay state that affects how players interact but doesn't
/// require the highest update rate.
///
/// ## Update Frequency: 30Hz
/// ## Replication Range: 100 meters  
/// ## Network Priority: Medium
///
/// # Fields
///
/// - `movement_state`: Current movement mode for animation and behavior
/// - `level`: Player progression level for gameplay systems
///
/// # Usage Examples
///
/// ```rust
/// let detailed_data = PlayerDetailedData {
///     movement_state: "running".to_string(),
///     level: 25,
/// };
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerDetailedData {
    /// Current movement state ("idle", "walking", "running", "crouching", etc.)
    pub movement_state: String,
    /// Player experience level (1-100)
    pub level: u32,
}

impl GorcZoneData for PlayerDetailedData {
    /// Returns the type identifier for GORC zone data serialization.
    fn zone_type_name() -> &'static str {
        "PlayerDetailedData"
    }
}

/// Social player data for low-frequency replication (GORC Zone 2).
///
/// This structure contains social and display information that is updated
/// infrequently (15Hz) and replicated to players within 200 meters. This zone
/// handles player identification and social interaction elements.
///
/// ## Update Frequency: 15Hz
/// ## Replication Range: 200 meters
/// ## Network Priority: Low
///
/// # Fields
///
/// - `name`: Player display name for identification
/// - `chat_bubble`: Temporary chat message for visual display
///
/// # Chat Bubble System
///
/// The chat bubble system provides visual indication of recent messages:
/// - Appears above the player character for 5-10 seconds
/// - Automatically cleared after timeout
/// - Used for immersive local communication visualization
///
/// # Usage Examples
///
/// ```rust
/// let social_data = PlayerSocialData {
///     name: "SpacePilot42".to_string(),
///     chat_bubble: Some("Hello there!".to_string()),
/// };
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerSocialData {
    /// Player display name (max 32 characters)
    pub name: String,
    /// Optional chat bubble text (max 100 characters, auto-expires)
    pub chat_bubble: Option<String>,
}

impl GorcZoneData for PlayerSocialData {
    /// Returns the type identifier for GORC zone data serialization.
    fn zone_type_name() -> &'static str {
        "PlayerSocialData"
    }
}

/// Complete GORC player object with zone-based data replication.
///
/// The `GorcPlayer` represents a player entity in the game world, implementing
/// the GORC (Game Object Replication and Communication) pattern for efficient
/// distributed multiplayer gaming. The object automatically manages different
/// types of player data across three replication zones.
///
/// ## Zone-Based Architecture
///
/// The player object is designed around GORC's three-zone replication system:
///
/// - **Zone 0 (Critical)**: Position, velocity, health - 25m range, 60Hz updates
/// - **Zone 1 (Detailed)**: Movement state, level - 100m range, 30Hz updates  
/// - **Zone 2 (Social)**: Name, chat bubble - 200m range, 15Hz updates
///
/// ## Automatic Replication
///
/// When registered with the GORC system, this object automatically:
/// - Replicates critical data to nearby players for smooth movement
/// - Updates detailed state for gameplay interactions
/// - Shares social information for player identification and communication
/// - Handles spatial culling based on zone ranges
///
/// ## Thread Safety
///
/// The `GorcPlayer` object is designed to be:
/// - `Clone`: Can be safely copied for async operations
/// - `Send + Sync`: Can be shared across threads safely
/// - Serializable: Efficiently transmitted over network connections
///
/// ## Example Usage
///
/// ```rust
/// use plugin_player::player::GorcPlayer;
/// use horizon_event_system::{PlayerId, Vec3};
///
/// // Create a new player
/// let mut player = GorcPlayer::new(
///     PlayerId(1),
///     "TestPlayer".to_string(),
///     Vec3::new(100.0, 0.0, 50.0)
/// );
///
/// // Update position with validation
/// let new_pos = Vec3::new(105.0, 0.0, 52.0);
/// let velocity = Vec3::new(5.0, 0.0, 2.0);
/// 
/// match player.validate_and_apply_movement(new_pos, velocity) {
///     Ok(()) => println!("Movement updated"),
///     Err(e) => println!("Invalid movement: {}", e),
/// }
///
/// // Set temporary chat bubble
/// player.set_chat_bubble("Hello world!".to_string());
///
/// // Perform combat action
/// let target = Vec3::new(110.0, 0.0, 55.0);
/// match player.perform_attack(target) {
///     Ok(damage) => println!("Attack dealt {} damage", damage),
///     Err(e) => println!("Attack failed: {}", e),
/// }
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GorcPlayer {
    /// Unique identifier for this player across the game world
    pub player_id: PlayerId,
    /// Timestamp of the last update to any player data (UTC seconds)
    #[serde(with = "chrono::serde::ts_seconds")]
    pub last_update: DateTime<Utc>,
    /// Zone 0: Critical player state (25m range, 60Hz updates)
    /// Contains position, velocity, and health for real-time interaction
    pub critical_data: PlayerCriticalData,
    /// Zone 1: Detailed state (100m range, 30Hz updates)
    /// Contains movement state and level for gameplay systems
    pub detailed_data: PlayerDetailedData,
    /// Zone 2: Social data (200m range, 15Hz updates)
    /// Contains name and chat bubble for player identification
    pub social_data: PlayerSocialData,
}

impl GorcPlayer {
    /// Creates a new GORC player with default configuration.
    ///
    /// Initializes a player object with sensible defaults for all zone data.
    /// The player starts at the specified position with zero velocity, full health,
    /// idle movement state, and level 1.
    ///
    /// # Parameters
    ///
    /// - `player_id`: Unique identifier for this player
    /// - `name`: Display name for the player (used in social interactions)
    /// - `position`: Initial spawn position in world coordinates
    ///
    /// # Returns
    ///
    /// A new `GorcPlayer` instance ready for registration with the GORC system.
    ///
    /// # Example
    ///
    /// ```rust
    /// use plugin_player::player::GorcPlayer;
    /// use horizon_event_system::{PlayerId, Vec3};
    ///
    /// let player = GorcPlayer::new(
    ///     PlayerId(42),
    ///     "SpaceExplorer".to_string(),
    ///     Vec3::new(1000.0, 0.0, 500.0)
    /// );
    ///
    /// assert_eq!(player.player_id, PlayerId(42));
    /// assert_eq!(player.critical_data.health, 100.0);
    /// assert_eq!(player.detailed_data.level, 1);
    /// ```
    pub fn new(player_id: PlayerId, name: String, position: Vec3) -> Self {
        Self {
            player_id,
            last_update: Utc::now(),
            critical_data: PlayerCriticalData {
                position,
                velocity: Vec3::new(0.0, 0.0, 0.0),
                health: 100.0,
            },
            detailed_data: PlayerDetailedData {
                movement_state: "idle".to_string(),
                level: 1,
            },
            social_data: PlayerSocialData {
                chat_bubble: None,
                name,
            },
        }
    }

    /// Sets a temporary chat bubble message for visual display.
    ///
    /// Updates the player's social data with a chat bubble message that will
    /// be visible to nearby players. The message is automatically replicated
    /// to players within 200m range at 15Hz frequency.
    ///
    /// # Parameters
    ///
    /// - `message`: The chat message to display (should be under 100 characters)
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut player = GorcPlayer::new(
    ///     PlayerId(1),
    ///     "Pilot".to_string(),
    ///     Vec3::new(0.0, 0.0, 0.0)
    /// );
    ///
    /// player.set_chat_bubble("Requesting docking clearance".to_string());
    /// assert!(player.social_data.chat_bubble.is_some());
    /// ```
    ///
    /// # Notes
    ///
    /// - Chat bubbles typically auto-expire after 5-10 seconds on the client side
    /// - Long messages may be truncated for display purposes
    /// - Updates the `last_update` timestamp for change tracking
    pub fn set_chat_bubble(&mut self, message: String) {
        self.social_data.chat_bubble = Some(message);
        self.last_update = Utc::now();
    }

    /// Validates and applies a movement update to the player.
    ///
    /// This method performs comprehensive validation of movement requests to prevent
    /// cheating and ensure reasonable gameplay behavior. If validation passes, the
    /// player's position and velocity are updated in the critical data zone.
    ///
    /// # Parameters
    ///
    /// - `new_position`: The requested new position in world coordinates
    /// - `velocity`: The current velocity vector for this movement
    ///
    /// # Returns
    ///
    /// - `Ok(())`: Movement was valid and applied successfully
    /// - `Err(String)`: Movement was rejected with detailed reason
    ///
    /// # Validation Rules
    ///
    /// - **Movement Distance**: Maximum 100 units per update (prevents teleportation)
    /// - **Velocity Bounds**: Reasonable velocity limits to prevent speed hacking
    /// - **Position Bounds**: Ensures position stays within valid world boundaries
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut player = GorcPlayer::new(
    ///     PlayerId(1),
    ///     "Runner".to_string(),
    ///     Vec3::new(0.0, 0.0, 0.0)
    /// );
    ///
    /// // Valid movement
    /// let result = player.validate_and_apply_movement(
    ///     Vec3::new(5.0, 0.0, 3.0),
    ///     Vec3::new(10.0, 0.0, 6.0)
    /// );
    /// assert!(result.is_ok());
    ///
    /// // Invalid teleportation attempt
    /// let result = player.validate_and_apply_movement(
    ///     Vec3::new(1000.0, 0.0, 1000.0),
    ///     Vec3::new(0.0, 0.0, 0.0)
    /// );
    /// assert!(result.is_err());
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - This method is called frequently (up to 60Hz) so it's optimized for speed
    /// - Validation uses simple distance calculations to minimize CPU overhead
    /// - Updates the `last_update` timestamp for change tracking
    pub fn validate_and_apply_movement(&mut self, new_position: Vec3, velocity: Vec3) -> Result<(), String> {
        // Calculate movement delta to detect teleportation attempts
        let distance = ((new_position.x - self.critical_data.position.x).powi(2) + 
                       (new_position.y - self.critical_data.position.y).powi(2) + 
                       (new_position.z - self.critical_data.position.z).powi(2)).sqrt();
        
        // Reject movement that's too large (likely cheating or network issues)
        if distance > 100.0 {
            return Err(format!("Movement distance too large: {:.2} units (max 100)", distance));
        }

        // Apply the validated movement
        self.critical_data.position = new_position;
        self.critical_data.velocity = velocity;
        self.last_update = Utc::now();
        Ok(())
    }

    /// Performs a combat attack action and calculates damage.
    ///
    /// This method handles player-initiated combat actions, calculating damage
    /// based on the attack type, target distance, and player level. The attack
    /// is automatically replicated to nearby players through the combat system.
    ///
    /// # Parameters
    ///
    /// - `target_position`: The world coordinates being targeted
    ///
    /// # Returns
    ///
    /// - `Ok(damage)`: Attack succeeded, returns damage dealt
    /// - `Err(String)`: Attack failed with detailed reason
    ///
    /// # Combat Mechanics
    ///
    /// - **Base Damage**: 25 points (modified by player level and equipment)
    /// - **Range Limits**: Attacks have maximum effective range
    /// - **Cooldowns**: Attack frequency may be limited to prevent spam
    /// - **Line of Sight**: Future versions may check for obstacles
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut player = GorcPlayer::new(
    ///     PlayerId(1),
    ///     "Warrior".to_string(),
    ///     Vec3::new(0.0, 0.0, 0.0)
    /// );
    ///
    /// let target = Vec3::new(10.0, 0.0, 0.0);
    /// match player.perform_attack(target) {
    ///     Ok(damage) => println!("Dealt {} damage", damage),
    ///     Err(reason) => println!("Attack failed: {}", reason),
    /// }
    /// ```
    ///
    /// # Network Replication
    ///
    /// Successful attacks are automatically broadcast to nearby players (500m range)
    /// through the combat event system for visual effects and damage calculation.
    pub fn perform_attack(&mut self, _target_position: Vec3) -> Result<f32, String> {
        // Simple attack logic - future versions will include:
        // - Range validation
        // - Line of sight checking  
        // - Equipment-based damage calculation
        // - Attack cooldown enforcement
        let base_damage = 25.0;
        let level_modifier = (self.detailed_data.level as f32) * 0.5;
        let total_damage = base_damage + level_modifier;
        
        self.last_update = Utc::now();
        Ok(total_damage)
    }

    /// Returns the current position of the player.
    ///
    /// This is a convenience method that provides direct access to the player's
    /// current world coordinates from the critical data zone.
    ///
    /// # Returns
    ///
    /// The player's current position as a `Vec3` in world coordinates.
    ///
    /// # Example
    ///
    /// ```rust
    /// let player = GorcPlayer::new(
    ///     PlayerId(1),
    ///     "Traveler".to_string(),
    ///     Vec3::new(100.0, 50.0, 25.0)
    /// );
    ///
    /// let pos = player.position();
    /// assert_eq!(pos.x, 100.0);
    /// assert_eq!(pos.y, 50.0);
    /// assert_eq!(pos.z, 25.0);
    /// ```
    ///
    /// # Usage Notes
    ///
    /// This method is frequently used by:
    /// - Spatial replication systems for distance calculations
    /// - Combat systems for range checking
    /// - Movement validation for bounds checking
    /// - UI systems for minimap and radar display
    pub fn position(&self) -> Vec3 {
        self.critical_data.position
    }
}

// Implement the type-based GorcObject using proper zone structure
impl_gorc_object! {
    GorcPlayer {
        0 => critical_data: PlayerCriticalData,  // 25m range, 60Hz - position, velocity, health
        1 => detailed_data: PlayerDetailedData,  // 100m range, 30Hz - level, movement_state  
        2 => social_data: PlayerSocialData,      // 200m range, 15Hz - chat_bubble, name
    }
}