//! # Core Type Definitions
//!
//! This module contains the fundamental types used throughout the Horizon Event System.
//! These types provide the building blocks for game world representation, player management,
//! and spatial organization.
//!
//! ## Key Types
//!
//! - [`PlayerId`] - Unique identifier for players in the game world
//! - [`RegionId`] - Unique identifier for game regions
//! - [`Position`] - 3D position representation with double precision
//! - [`RegionBounds`] - Spatial boundaries for game regions
//!
//! ## Design Principles
//!
//! - **Type Safety**: Wrapper types prevent ID confusion (PlayerId vs RegionId)
//! - **Precision**: Double-precision floats for accurate large-world positioning
//! - **Serialization**: All types support JSON serialization for network transmission
//! - **Performance**: Efficient memory layout and fast comparison operations

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Core Types (Minimal set)
// ============================================================================

/// Unique identifier for a player in the game world.
/// 
/// This is a wrapper around UUID that provides type safety and ensures
/// player IDs cannot be confused with other types of IDs in the system.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::PlayerId;
/// 
/// // Create a new random player ID
/// let player_id = PlayerId::new();
/// 
/// // Parse from string
/// let player_id = PlayerId::from_str("550e8400-e29b-41d4-a716-446655440000")?;
/// 
/// // Convert to string for logging/display
/// println!("Player ID: {}", player_id);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub Uuid);

impl PlayerId {
    /// Creates a new random player ID using UUID v4.
    /// 
    /// This method is cryptographically secure and provides sufficient
    /// entropy to avoid collisions in practical use.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parses a player ID from a string representation.
    /// 
    /// # Arguments
    /// 
    /// * `s` - A string slice containing a valid UUID
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(PlayerId)` if the string is a valid UUID, otherwise returns
    /// `Err(uuid::Error)` with details about the parsing failure.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use horizon_event_system::PlayerId;
    /// 
    /// let player_id = PlayerId::from_str("550e8400-e29b-41d4-a716-446655440000")?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_str(s: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(s).map(Self)
    }
}

impl std::str::FromStr for PlayerId {
    type Err = uuid::Error;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str(s)
    }
}

impl Default for PlayerId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PlayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a game region.
/// 
/// Regions are logical areas of the game world that can be managed independently.
/// Each region has its own event processing and can be started/stopped dynamically.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::RegionId;
/// 
/// let region_id = RegionId::new();
/// println!("Region: {}", region_id.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegionId(pub Uuid);

impl RegionId {
    /// Creates a new random region ID using UUID v4.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RegionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a 3D position in the game world.
/// 
/// Uses double-precision floating point for maximum accuracy in position calculations.
/// This is essential for large game worlds where single-precision might introduce
/// noticeable errors.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::Position;
/// 
/// let spawn_point = Position::new(0.0, 0.0, 0.0);
/// let player_pos = Position::new(100.5, 64.0, -200.25);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// X coordinate (typically east-west axis)
    pub x: f64,
    /// Y coordinate (typically vertical axis)
    pub y: f64,
    /// Z coordinate (typically north-south axis)
    pub z: f64,
}

impl Position {
    /// Creates a new position with the specified coordinates.
    /// 
    /// # Arguments
    /// 
    /// * `x` - X coordinate
    /// * `y` - Y coordinate  
    /// * `z` - Z coordinate
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Calculates the distance to another position.
    /// 
    /// # Arguments
    /// 
    /// * `other` - The other position to calculate distance to
    /// 
    /// # Returns
    /// 
    /// Returns the Euclidean distance between the two positions
    pub fn distance(&self, other: Position) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        ((dx * dx + dy * dy + dz * dz) as f64).sqrt()
    }
}

/// Represents a 3D vector with single-precision floating point components.
/// 
/// This type is used for game objects that need 3D positioning with single-precision
/// for better performance in scenarios where double precision is not required.
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::Vec3;
/// 
/// let velocity = Vec3::new(10.0, 0.0, -5.0);
/// let position = Vec3::new(100.5, 64.0, -200.25);
/// let distance = position.distance(Vec3::new(0.0, 0.0, 0.0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    /// X coordinate (typically east-west axis)
    pub x: f64,
    /// Y coordinate (typically vertical axis)
    pub y: f64,
    /// Z coordinate (typically north-south axis)
    pub z: f64,
}

impl Vec3 {
    /// Creates a new Vec3 with the specified coordinates.
    /// 
    /// # Arguments
    /// 
    /// * `x` - X coordinate
    /// * `y` - Y coordinate  
    /// * `z` - Z coordinate
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Calculates the distance to another Vec3.
    /// 
    /// # Arguments
    /// 
    /// * `other` - The other vector to calculate distance to
    /// 
    /// # Returns
    /// 
    /// Returns the Euclidean distance between the two vectors
    pub fn distance(&self, other: Vec3) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Creates a zero vector (0, 0, 0).
    pub fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    /// Creates a unit vector along the X axis (1, 0, 0).
    pub fn unit_x() -> Self {
        Self::new(1.0, 0.0, 0.0)
    }

    /// Creates a unit vector along the Y axis (0, 1, 0).
    pub fn unit_y() -> Self {
        Self::new(0.0, 1.0, 0.0)
    }

    /// Creates a unit vector along the Z axis (0, 0, 1).
    pub fn unit_z() -> Self {
        Self::new(0.0, 0.0, 1.0)
    }
}

impl Default for Vec3 {
    fn default() -> Self {
        Self::zero()
    }
}

impl From<Position> for Vec3 {
    fn from(pos: Position) -> Self {
        Self::new(pos.x as f64, pos.y as f64, pos.z as f64)
    }
}

impl From<Vec3> for Position {
    fn from(vec: Vec3) -> Self {
        Self::new(vec.x as f64, vec.y as f64, vec.z as f64)
    }
}

/// Defines the spatial boundaries of a game region.
/// 
/// This structure defines a 3D bounding box that encompasses all
/// the space within a game region. It's used for:
/// - Determining which region a player is in
/// - Spatial partitioning of game logic
/// - Collision detection boundaries
/// - Resource allocation planning
/// 
/// # Examples
/// 
/// ```rust
/// use horizon_event_system::RegionBounds;
/// 
/// let region_bounds = RegionBounds {
///     min_x: -500.0, max_x: 500.0,    // 1km wide
///     min_y: 0.0, max_y: 128.0,       // 128 units tall
///     min_z: -500.0, max_z: 500.0,    // 1km deep
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionBounds {
    /// Minimum X coordinate (western boundary)
    pub min_x: f64,
    /// Maximum X coordinate (eastern boundary)
    pub max_x: f64,
    /// Minimum Y coordinate (bottom boundary)
    pub min_y: f64,
    /// Maximum Y coordinate (top boundary)
    pub max_y: f64,
    /// Minimum Z coordinate (southern boundary)
    pub min_z: f64,
    /// Maximum Z coordinate (northern boundary)
    pub max_z: f64,
}

impl Default for RegionBounds {
    fn default() -> Self {
        Self {
            min_x: -1000.0,
            max_x: 1000.0,
            min_y: -1000.0,
            max_y: 1000.0,
            min_z: -100.0,
            max_z: 100.0,
        }
    }
}

/// Enumeration of possible disconnection reasons.
/// 
/// This provides structured information about why a player disconnected,
/// which is useful for debugging, logging, and handling different disconnect
/// scenarios appropriately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisconnectReason {
    /// Player initiated disconnection (normal logout)
    ClientDisconnect,
    /// Connection timed out due to inactivity or network issues
    Timeout,
    /// Server is shutting down gracefully
    ServerShutdown,
    /// An error occurred that forced disconnection
    Error(String),
}

/// Represents the authentication status of a player.
/// 
/// This enum defines the possible authentication states that a player
/// can be in, allowing plugins to query and respond to authentication
/// status appropriately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthenticationStatus {
    /// Player is not authenticated
    Unauthenticated,
    /// Player is in the process of authenticating
    Authenticating,
    /// Player is successfully authenticated
    Authenticated,
    /// Player authentication failed
    AuthenticationFailed,
}

impl Default for AuthenticationStatus {
    fn default() -> Self {
        Self::Unauthenticated
    }
}