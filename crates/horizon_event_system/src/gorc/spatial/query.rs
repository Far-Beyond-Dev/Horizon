/// Spatial query types and utilities
use crate::types::{PlayerId, Position};
use std::collections::{HashMap, HashSet};

/// Spatial query parameters
#[derive(Debug, Clone)]
pub struct SpatialQuery {
    /// Center position of the query
    pub center: Position,
    /// Query radius
    pub radius: f32,
    /// Optional filters for the query
    pub filters: QueryFilters,
}

/// Filters that can be applied to spatial queries
#[derive(Debug, Clone, Default)]
pub struct QueryFilters {
    /// Include only specific players
    pub include_players: Option<HashSet<PlayerId>>,
    /// Exclude specific players
    pub exclude_players: Option<HashSet<PlayerId>>,
    /// Maximum number of results to return
    pub max_results: Option<usize>,
    /// Minimum distance from query center
    pub min_distance: Option<f32>,
}

/// Result of a spatial query
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Player ID
    pub player_id: PlayerId,
    /// Player position
    pub position: Position,
    /// Distance from query center
    pub distance: f32,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}