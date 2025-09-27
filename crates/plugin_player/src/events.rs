//! # Player Event Data Structures
//!
//! This module defines the event data structures used for communication between
//! clients and the server in the GORC (Game Object Replication and Communication)
//! system. These structures represent the various types of player actions and
//! requests that are transmitted over the network.
//!
//! ## Event Categories
//!
//! The player plugin handles four main categories of client events:
//!
//! ### Movement Events (Channel 0)
//! High-frequency position and velocity updates for real-time movement:
//! - [`PlayerMoveRequest`] - Player movement and position updates
//!
//! ### Combat Events (Channel 1)  
//! Weapon firing and attack coordination:
//! - [`PlayerAttackRequest`] - Weapon fire and combat actions
//!
//! ### Communication Events (Channel 2)
//! Chat and social interaction:
//! - [`PlayerChatRequest`] - Chat messages and communication
//!
//! ### Scanning Events (Channel 3)
//! Ship information and metadata sharing:
//! - Ship scanning requests (handled via JSON parsing in handlers)
//!
//! ## Serialization
//!
//! All event structures use `serde` for JSON serialization and are designed for:
//! - **Efficient Encoding**: Minimal serialization overhead for network transmission
//! - **Forward Compatibility**: Fields can be added without breaking existing clients
//! - **Type Safety**: Strong typing prevents malformed requests at compile time
//! - **Validation Ready**: Structured for easy validation and sanitization
//!
//! ## Network Protocol
//!
//! Events are transmitted via GORC's binary protocol with JSON payloads:
//! 1. Client creates event structure and serializes to JSON
//! 2. JSON is embedded in GORC event with channel and action identifiers
//! 3. Server receives event, validates, and processes through handlers
//! 4. Server broadcasts results to nearby players via spatial replication
//!
//! ## Example Usage
//!
//! ```rust
//! use plugin_player::events::*;
//! use horizon_event_system::{PlayerId, Vec3};
//! use chrono::Utc;
//!
//! // Create a movement request
//! let move_request = PlayerMoveRequest {
//!     player_id: PlayerId(42),
//!     new_position: Vec3::new(100.0, 0.0, 50.0),
//!     velocity: Vec3::new(5.0, 0.0, 2.0),
//!     movement_state: 1, // Running
//!     client_timestamp: Utc::now(),
//! };
//!
//! // Serialize for network transmission
//! let json = serde_json::to_string(&move_request).unwrap();
//!
//! // Create a combat request
//! let attack_request = PlayerAttackRequest {
//!     player_id: PlayerId(42),
//!     target_position: Vec3::new(120.0, 0.0, 60.0),
//!     attack_type: "laser".to_string(),
//!     client_timestamp: Utc::now(),
//! };
//!
//! // Create a communication request
//! let chat_request = PlayerChatRequest {
//!     player_id: PlayerId(42),
//!     message: "Hello, fellow pilots!".to_string(),
//!     channel: "general".to_string(),
//!     target_player: None,
//! };
//! ```

use serde::{Deserialize, Serialize};
use horizon_event_system::{PlayerId, Vec3};
use chrono::{DateTime, Utc};

/// Player movement request event for GORC channel 0.
///
/// This structure represents a client request to update a player's position and
/// movement state. It contains all the information needed for real-time movement
/// processing and validation on the server side.
///
/// ## Network Characteristics
/// - **Channel**: 0 (Critical movement data)
/// - **Frequency**: Up to 60Hz for smooth movement
/// - **Range**: 25m replication radius
/// - **Priority**: Highest (critical for gameplay)
///
/// ## Movement States
/// The `movement_state` field uses integer encoding for efficiency:
/// - `0`: Idle/stationary
/// - `1`: Walking
/// - `2`: Running  
/// - `3`: Sprinting
/// - `4`: Crouching
/// - `5`: Jumping/airborne
///
/// ## Validation
/// Servers perform extensive validation on movement requests:
/// - Position delta checking (prevents teleportation)
/// - Velocity bounds validation (prevents speed hacking)
/// - Timestamp verification (prevents replay attacks)
/// - Player ownership validation (prevents unauthorized control)
///
/// ## Example Usage
///
/// ```rust
/// use plugin_player::events::PlayerMoveRequest;
/// use horizon_event_system::{PlayerId, Vec3};
/// use chrono::Utc;
///
/// let move_request = PlayerMoveRequest {
///     player_id: PlayerId(42),
///     new_position: Vec3::new(100.5, 0.0, 50.3),
///     velocity: Vec3::new(8.0, 0.0, 4.0),
///     movement_state: 2, // Running
///     client_timestamp: Utc::now(),
/// };
///
/// // This request would move player 42 to the new position
/// // with a running animation state and specified velocity
/// ```
///
/// ## Network Optimization
/// - Uses `i32` for movement state (compact encoding)
/// - Timestamp allows for client-side prediction validation
/// - All fields are optimized for minimal serialization overhead
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerMoveRequest {
    /// ID of the player requesting the movement
    pub player_id: PlayerId,
    /// Requested new position in world coordinates  
    pub new_position: Vec3,
    /// Current velocity vector for prediction
    pub velocity: Vec3,
    /// Current movement state (0=idle, 1=walking, 2=running, etc.)
    pub movement_state: i32,
    /// Client-side timestamp for validation and prediction
    pub client_timestamp: DateTime<Utc>,
}

/// Player attack request event for GORC channel 1.
///
/// This structure represents a client request to perform a combat action, such as
/// firing weapons or initiating an attack. It contains targeting information and
/// weapon specifications for server-side validation and replication.
///
/// ## Network Characteristics
/// - **Channel**: 1 (Combat events)
/// - **Frequency**: Event-driven (not continuous)
/// - **Range**: 500m replication radius
/// - **Priority**: High (important for tactical awareness)
///
/// ## Attack Types
/// The `attack_type` field specifies the weapon or attack being used:
/// - `"laser"`: High-precision energy beam weapons
/// - `"missile"`: Guided projectile weapons with area damage
/// - `"plasma"`: Energy projectiles with travel time
/// - `"kinetic"`: Physical projectile weapons
/// - `"melee"`: Close-quarters combat attacks
///
/// ## Security and Validation
/// Servers perform strict validation on attack requests:
/// - Player ownership verification (only own ships can attack)
/// - Weapon availability checking (player must have the weapon)
/// - Range validation (target must be within weapon range)
/// - Rate limiting (prevents rapid-fire exploits)
/// - Ammunition tracking (future enhancement)
///
/// ## Replication Behavior
/// Successful attacks are automatically replicated to nearby players:
/// - Visual effects are synchronized across clients
/// - Damage calculations are server-authoritative  
/// - Sound effects are triggered on receiving clients
/// - Combat logs are maintained for analysis
///
/// ## Example Usage
///
/// ```rust
/// use plugin_player::events::PlayerAttackRequest;
/// use horizon_event_system::{PlayerId, Vec3};
/// use chrono::Utc;
///
/// let attack_request = PlayerAttackRequest {
///     player_id: PlayerId(42),
///     target_position: Vec3::new(150.0, 0.0, 75.0),
///     attack_type: "laser".to_string(),
///     client_timestamp: Utc::now(),
/// };
///
/// // This request would fire a laser weapon at the specified coordinates
/// // The server validates the attack and replicates to nearby players
/// ```
///
/// ## Combat Mechanics Integration
/// - Damage calculations use server-side weapon statistics
/// - Critical hits and modifiers are applied server-side
/// - Line-of-sight checking prevents shooting through walls
/// - Energy/ammunition costs are deducted from player resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerAttackRequest {
    /// ID of the player performing the attack
    pub player_id: PlayerId,
    /// World coordinates of the attack target
    pub target_position: Vec3,
    /// Type of weapon or attack being used
    pub attack_type: String,
    /// Client-side timestamp for attack timing validation
    pub client_timestamp: DateTime<Utc>,
}

/// Player communication request event for GORC channel 2.
///
/// This structure represents a client request to send a chat message or other
/// communication. It supports multiple communication channels and both broadcast
/// and direct messaging capabilities.
///
/// ## Network Characteristics  
/// - **Channel**: 2 (Communication events)
/// - **Frequency**: Event-driven (as needed)
/// - **Range**: 300m replication radius
/// - **Priority**: Medium (social interaction)
///
/// ## Communication Channels
/// The `channel` field specifies the communication channel:
/// - `"general"`: General purpose chat (default)
/// - `"emergency"`: Emergency distress signals (extended range)
/// - `"trade"`: Commercial and trading communication
/// - `"fleet"`: Fleet coordination and tactical communication
/// - `"private"`: Direct player-to-player messaging
///
/// ## Message Types
/// The system supports various message types:
/// - **Broadcast**: Messages sent to all nearby players
/// - **Direct**: Messages sent to a specific target player
/// - **Channel**: Messages sent on specific communication channels
/// - **Emergency**: Priority messages with extended range
///
/// ## Content Moderation
/// The server performs content validation and moderation:
/// - Message length limits (500 characters maximum)
/// - Rate limiting to prevent spam (1 message per second)
/// - Profanity filtering (configurable)
/// - Abuse reporting and player muting
///
/// ## Spatial Communication
/// Communication follows realistic space communication patterns:
/// - Local range communication (300m standard range)
/// - Emergency channels have extended range (1000m)
/// - Direct messages bypass range restrictions
/// - Signal quality may degrade with distance (future feature)
///
/// ## Example Usage
///
/// ```rust
/// use plugin_player::events::PlayerChatRequest;
/// use horizon_event_system::PlayerId;
///
/// // General broadcast message
/// let broadcast_msg = PlayerChatRequest {
///     player_id: PlayerId(42),
///     message: "Looking for trading partners near Station Alpha".to_string(),
///     channel: "trade".to_string(),
///     target_player: None,
/// };
///
/// // Direct private message
/// let private_msg = PlayerChatRequest {
///     player_id: PlayerId(42),
///     message: "Meet me at the asteroid belt".to_string(),
///     channel: "private".to_string(),
///     target_player: Some(PlayerId(17)),
/// };
///
/// // Emergency distress signal
/// let emergency_msg = PlayerChatRequest {
///     player_id: PlayerId(42),
///     message: "MAYDAY! Under attack at coordinates 120,50,30!".to_string(),
///     channel: "emergency".to_string(),
///     target_player: None,
/// };
/// ```
///
/// ## Privacy and Security
/// - Private messages are only delivered to intended recipients
/// - Players can mute or block other players
/// - Message history is logged for moderation purposes
/// - Encryption may be used for sensitive communications (future)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerChatRequest {
    /// ID of the player sending the message
    pub player_id: PlayerId,
    /// The chat message content (max 500 characters)
    pub message: String,
    /// Communication channel ("general", "emergency", "trade", "fleet", "private")
    pub channel: String,
    /// Target player for direct messages (None for broadcast)
    pub target_player: Option<PlayerId>,
}

/// Player block change request event for GORC channel 1.
///
/// This structure represents a client request to modify the game world by
/// breaking or placing blocks. It contains all the information needed for
/// world state synchronization between players.
///
/// ## Network Characteristics
/// - **Channel**: 1 (Combat/World events)
/// - **Frequency**: Event-driven (when blocks are modified)
/// - **Range**: 500m replication radius
/// - **Priority**: High (important for world state consistency)
///
/// ## Block Operations
/// The `newTile` field determines the operation type:
/// - `0` (AIR): Block breaking operation
/// - `1-7`: Block placing operation with specific tile type
///
/// ## Block Types
/// Standard tile types used in Terraria-like games:
/// - `0`: AIR (empty space)
/// - `1`: GRASS (surface terrain)
/// - `2`: DIRT (common soil blocks)
/// - `3`: STONE (underground rock)
/// - `4`: COAL (mineral ore)
/// - `5`: IRON (metal ore)
/// - `6`: TREE (wood trunk)
/// - `7`: LEAVES (tree foliage)
///
/// ## Security and Validation
/// Servers perform validation on block change requests:
/// - Player range checking (must be within reach)
/// - Block ownership rules (some blocks may be protected)
/// - Rate limiting (prevents block spam)
/// - World bounds validation (coordinates must be valid)
/// - Physics validation (can't place blocks inside players)
///
/// ## Replication Behavior
/// Block changes are automatically replicated to nearby players:
/// - World state is synchronized across all clients
/// - Visual effects (breaking particles) are triggered
/// - Sound effects are played on receiving clients
/// - Block change history is maintained for rollback
///
/// ## Example Usage
///
/// ```rust
/// use plugin_player::events::PlayerBlockChangeRequest;
/// use horizon_event_system::PlayerId;
/// use chrono::Utc;
///
/// // Breaking a block (place AIR)
/// let break_request = PlayerBlockChangeRequest {
///     player_id: PlayerId(42),
///     x: 150,
///     y: 75,
///     old_tile: 3, // STONE
///     new_tile: 0, // AIR
///     client_timestamp: Utc::now(),
/// };
///
/// // Placing a block
/// let place_request = PlayerBlockChangeRequest {
///     player_id: PlayerId(42),
///     x: 150,
///     y: 75,
///     old_tile: 0, // AIR
///     new_tile: 2, // DIRT
///     client_timestamp: Utc::now(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerBlockChangeRequest {
    /// ID of the player making the block change
    pub player_id: PlayerId,
    /// X coordinate of the block being changed
    pub x: i32,
    /// Y coordinate of the block being changed
    pub y: i32,
    /// Previous tile type at this position
    pub old_tile: u8,
    /// New tile type to place at this position
    pub new_tile: u8,
    /// Client-side timestamp when the change was initiated
    pub client_timestamp: DateTime<Utc>,
}