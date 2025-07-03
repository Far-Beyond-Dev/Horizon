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
/// use horizon_events::PlayerId;
/// 
/// // Create a new random player ID
/// let player_id = PlayerId::new();
/// 
/// // Parse from string
/// let player_id = PlayerId::from_str("550e8400-e29b-41d4-a716-446655440000")?;
/// 
/// // Convert to string for logging/display
/// println!("Player ID: {}", player_id);
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
    /// let player_id = PlayerId::from_str("550e8400-e29b-41d4-a716-446655440000")?;
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
/// use horizon_events::RegionId;
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
/// use horizon_events::Position;
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