//! Consolidated type definitions for the RecipeSmith crafting system
//! 
//! This module contains all custom types used across the RecipeSmith plugin,
//! organized by functional area:
//! - Core recipe and crafting types
//! - Player state and preferences
//! - Queue management types
//! - Validation system types
//! - Integration and plugin types
//! - Event types for communication
//! - Client integration types (feature-gated)
//! - Error types
//! - Configuration types

use event_system::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

// ============================================================================
// Core Identifiers
// ============================================================================

/// Unique identifier for recipes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
pub struct RecipeId(pub u32);

impl RecipeId {
    pub fn new() -> Self {
        // In practice, this would be from a sequence or configuration
        Self(rand::random())
    }
}

impl std::fmt::Display for RecipeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Recipe({})", self.0)
    }
}

/// Unique identifier for crafting operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CraftingOperationId(pub Uuid);

impl CraftingOperationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for CraftingOperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// Core Recipe System Types
// ============================================================================

/// Complete recipe definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    /// Unique recipe identifier
    pub id: RecipeId,
    /// Human-readable recipe name
    pub name: String,
    /// Recipe description
    pub description: String,
    /// Recipe category for organization
    pub category: String,
    
    /// Required ingredients
    pub ingredients: Vec<Ingredient>,
    /// Main crafted product
    pub main_product: CraftingResult,
    /// Optional byproducts
    pub byproducts: Vec<CraftingResult>,
    
    /// Various requirements to execute this recipe
    pub requirements: CraftingRequirements,
    /// Timing and duration information
    pub timing: CraftingTiming,
    
    /// Conditions required to unlock this recipe
    pub unlock_conditions: Vec<UnlockCondition>,
    /// Experience rewards per skill
    pub experience_rewards: HashMap<String, u32>,
    
    /// Chance of failure (0.0 to 1.0)
    pub failure_chance: f32,
    /// Chance of critical success (0.0 to 1.0)
    pub critical_success_chance: f32,
}

/// Ingredient requirement for a recipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ingredient {
    /// Item ID required
    pub item_id: u32,
    /// Quantity required
    pub quantity: u32,
    /// Minimum quality required (if quality system is enabled)
    pub quality_requirement: Option<f32>,
}

/// Result of crafting (product or byproduct)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingResult {
    /// Item ID produced
    pub item_id: u32,
    /// Base quantity produced
    pub quantity: u32,
    /// Quality bonus multiplier (if quality system is enabled)
    pub quality_bonus: Option<f32>,
}

/// Requirements to execute a recipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingRequirements {
    /// Required skill levels (skill_name -> level)
    pub skill_requirements: HashMap<String, u32>,
    /// Required crafting station
    pub station_requirement: Option<String>,
    /// Required tools
    pub tool_requirements: Vec<String>,
    /// Environmental requirements (near fire, in water, etc.)
    pub environmental_requirements: Vec<String>,
}

/// Timing and duration information for recipes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingTiming {
    /// Base crafting duration
    #[serde(with = "duration_serde")]
    pub base_duration: Duration,
    /// Speed bonus per skill level (percentage)
    pub skill_speed_bonus: Option<f32>,
    /// Speed bonus from stations (percentage)
    pub station_speed_bonus: Option<f32>,
}

/// Conditions required to unlock a recipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnlockCondition {
    /// Player must have a specific skill level
    SkillLevel(String, u32),
    /// Player must know another recipe
    RecipeKnown(RecipeId),
    /// Player must have completed a quest
    QuestCompleted(String),
    /// Player must have discovered an item
    ItemDiscovered(u32),
    /// Player must be in a specific location
    LocationRequirement(String),
    /// Custom condition handled by other plugins
    CustomCondition(String, serde_json::Value),
}

// ============================================================================
// Crafting Operation Types
// ============================================================================

/// Current status of a crafting operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
pub enum CraftingStatus {
    /// Operation is waiting to start
    Queued,
    /// Operation is currently in progress
    InProgress,
    /// Operation completed successfully
    Completed,
    /// Operation failed
    Failed,
    /// Operation was cancelled
    Cancelled,
    /// Operation is paused (e.g., missing ingredients)
    Paused,
}

impl std::fmt::Display for CraftingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queued => write!(f, "Queued"),
            Self::InProgress => write!(f, "In Progress"),
            Self::Completed => write!(f, "Completed"),
            Self::Failed => write!(f, "Failed"),
            Self::Cancelled => write!(f, "Cancelled"),
            Self::Paused => write!(f, "Paused"),
        }
    }
}

/// A single crafting operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingOperation {
    /// Unique operation identifier
    pub id: CraftingOperationId,
    /// Player performing the crafting
    pub player_id: PlayerId,
    /// Recipe being crafted
    pub recipe: Recipe,
    /// Quantity being crafted
    pub quantity: u32,
    /// Source slots for ingredients
    pub ingredient_sources: Vec<InventorySlot>,
    
    /// When the operation started
    pub started_at: SystemTime,
    /// Estimated completion time
    pub estimated_completion: SystemTime,
    /// Current progress (0.0 to 1.0)
    pub progress: f32,
    /// Current status
    pub status: CraftingStatus,
    
    /// Quality modifiers applied to this operation
    pub quality_modifiers: Vec<QualityModifier>,
    /// Experience multiplier for this operation
    pub experience_multiplier: f32,
}

impl CraftingOperation {
    /// Check if the operation is complete
    pub fn is_complete(&self) -> bool {
        SystemTime::now() >= self.estimated_completion || self.progress >= 1.0
    }
    
    /// Get remaining time
    pub fn remaining_time(&self) -> Duration {
        self.estimated_completion
            .duration_since(SystemTime::now())
            .unwrap_or_default()
    }
}

/// Quality modifier that affects crafting results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityModifier {
    /// Source of the modifier (skill, station, tool, etc.)
    pub source: String,
    /// Modifier value (multiplier)
    pub modifier: f32,
    /// Type of modifier
    pub modifier_type: QualityModifierType,
}

/// Type of quality modifier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QualityModifierType {
    /// Affects base quality
    BaseQuality,
    /// Affects critical success chance
    CriticalChance,
    /// Affects failure chance
    FailureReduction,
    /// Affects experience gain
    ExperienceBonus,
    /// Affects crafting speed
    SpeedBonus,
}

// ============================================================================
// Player State Types
// ============================================================================

/// Player's crafting state and progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerCraftingState {
    /// Player identifier
    pub player_id: PlayerId,
    /// Recipes this player knows
    pub known_recipes: Vec<RecipeId>,
    /// Currently active crafting operations
    pub active_operations: Vec<CraftingOperationId>,
    
    /// Statistics
    pub total_crafting_attempts: u64,
    pub successful_crafts: u64,
    
    /// Overall crafting level
    pub crafting_level: u32,
    /// Experience points per skill
    pub experience_points: HashMap<String, u32>,
    
    /// Unlocked recipe categories
    pub unlocked_categories: Vec<String>,
    
    /// Player crafting preferences
    pub preferences: CraftingPreferences,
}

/// Player preferences for crafting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingPreferences {
    /// Auto-queue similar items when ingredients are available
    pub auto_queue_enabled: bool,
    /// Preferred ingredient sources (inventory, storage, etc.)
    pub preferred_ingredient_sources: Vec<String>,
    /// Quality threshold for auto-selection
    pub minimum_quality_threshold: Option<f32>,
    /// Automatically use highest quality ingredients
    pub use_best_quality_ingredients: bool,
    /// Send progress notifications
    pub progress_notifications: bool,
}

impl Default for CraftingPreferences {
    fn default() -> Self {
        Self {
            auto_queue_enabled: false,
            preferred_ingredient_sources: vec!["inventory".to_string()],
            minimum_quality_threshold: None,
            use_best_quality_ingredients: true,
            progress_notifications: true,
        }
    }
}

// ============================================================================
// Queue Management Types
// ============================================================================

/// Player-specific crafting queue
#[derive(Debug, Clone)]
pub struct PlayerQueue {
    /// Player ID
    pub player_id: PlayerId,
    /// Currently active operations
    pub active_operations: Vec<CraftingOperationId>,
    /// Queued operations waiting to start
    pub queued_operations: VecDeque<CraftingOperationId>,
    /// Maximum concurrent operations for this player
    pub max_concurrent_operations: u32,
    /// Queue creation time
    pub created_at: SystemTime,
    /// Last activity time
    pub last_activity: SystemTime,
    /// Player queue statistics
    pub stats: PlayerQueueStats,
}

/// Statistics for a player's queue
#[derive(Debug, Default, Clone)]
pub struct PlayerQueueStats {
    pub total_operations_started: u64,
    pub total_operations_completed: u64,
    pub total_operations_failed: u64,
    pub total_operations_cancelled: u64,
    pub total_time_crafting: Duration,
    pub average_operation_time: Duration,
    pub longest_operation_time: Duration,
    pub shortest_operation_time: Option<Duration>,
}

/// Global queue statistics
#[derive(Debug, Default, Clone)]
pub struct QueueStats {
    pub active_operations_count: u64,
    pub total_operations_processed: u64,
    pub operations_per_second: f64,
    pub average_queue_size: f64,
    pub peak_concurrent_operations: u64,
    pub system_load_factor: f32, // 0.0 to 1.0
}

/// Completed operation record for history
#[derive(Debug, Clone)]
pub struct CompletedOperation {
    pub operation_id: CraftingOperationId,
    pub player_id: PlayerId,
    pub recipe_id: RecipeId,
    pub status: CraftingStatus,
    pub started_at: SystemTime,
    pub completed_at: SystemTime,
    pub duration: Duration,
    pub quality_achieved: Option<f32>,
    pub experience_gained: HashMap<String, u32>,
}

/// Performance metrics for monitoring and alerting
#[derive(Debug, Clone)]
pub struct QueuePerformanceMetrics {
    pub active_operations: u64,
    pub queued_operations: u64,
    pub completed_operations: u64,
    pub failed_operations: u64,
    pub cancelled_operations: u64,
    pub paused_operations: u64,
    pub system_load_factor: f32,
    pub average_queue_size: f64,
    pub peak_concurrent_operations: u64,
    pub total_players_with_queues: u64,
}

// ============================================================================
// Validation System Types
// ============================================================================

/// Result of crafting request validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the request is valid
    pub is_valid: bool,
    /// Reason for failure (if invalid)
    pub failure_reason: Option<String>,
    /// Quality modifiers that apply
    pub quality_modifiers: Vec<QualityModifier>,
    /// Experience multiplier
    pub experience_multiplier: f32,
    /// Warnings (non-blocking issues)
    pub warnings: Vec<String>,
}

/// Configuration for validation behavior
#[derive(Debug, Clone)]
pub struct ValidationSettings {
    /// Allow crafting with insufficient quality ingredients (lower success rate)
    pub allow_low_quality_substitution: bool,
    /// Require exact ingredient quantities or allow excess
    pub allow_excess_ingredients: bool,
    /// Check tool durability before allowing crafting
    pub check_tool_durability: bool,
    /// Validate environmental conditions
    pub check_environmental_conditions: bool,
    /// Use cached results for performance (time-based expiry)
    pub cache_validation_results: bool,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
}

impl Default for ValidationSettings {
    fn default() -> Self {
        Self {
            allow_low_quality_substitution: true,
            allow_excess_ingredients: true,
            check_tool_durability: true,
            check_environmental_conditions: true,
            cache_validation_results: true,
            cache_ttl_seconds: 300, // 5 minutes
        }
    }
}

/// Validation rule for caching
#[derive(Debug, Clone)]
pub struct ValidationRule {
    pub rule_id: String,
    pub cached_result: ValidationResult,
    pub created_at: SystemTime,
    pub expires_at: SystemTime,
}

/// Result from integration validation
#[derive(Debug, Clone)]
pub struct IntegrationValidationResult {
    pub is_valid: bool,
    pub failure_reason: Option<String>,
    pub quality_modifiers: Vec<QualityModifier>,
    pub experience_multiplier: f32,
    pub warnings: Vec<String>,
}

/// Information about crafting station access
#[derive(Debug, Clone)]
pub struct StationAccessInfo {
    pub has_access: bool,
    pub quality_modifier: Option<f32>,
    pub speed_modifier: Option<f32>,
}

/// Status of environmental conditions
#[derive(Debug, Clone)]
pub struct EnvironmentalStatus {
    pub condition_met: bool,
    pub quality_modifier: Option<f32>,
}

/// Validation statistics
#[derive(Debug, Clone)]
pub struct ValidationStats {
    pub cached_rules_count: usize,
    pub integration_handlers_count: usize,
    pub cache_hit_rate: f64,
    pub average_validation_time_ms: f64,
    pub total_validations_performed: u64,
}

// ============================================================================
// Recipe Management Types
// ============================================================================

/// Statistics for a recipe
#[derive(Debug, Default, Clone)]
pub struct RecipeStats {
    pub times_crafted: u64,
    pub total_successes: u64,
    pub total_failures: u64,
    pub average_quality: f32,
    pub fastest_craft_time: Option<Duration>,
    pub players_who_know: u64,
}

/// Criteria for searching recipes
#[derive(Debug, Clone)]
pub struct RecipeSearchCriteria {
    pub name_pattern: Option<String>,
    pub category: Option<String>,
    pub max_skill_level: Option<HashMap<String, u32>>,
    pub available_ingredients: Option<HashMap<u32, u32>>,
    pub max_craft_time: Option<Duration>,
    pub min_success_rate: Option<f32>,
}

// ============================================================================
// Configuration Types
// ============================================================================

/// Configuration for the crafting system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingConfig {
    /// Maximum concurrent operations per player
    pub max_operations_per_player: u32,
    /// Global crafting speed multiplier
    pub global_speed_multiplier: f32,
    /// Enable quality system integration
    pub quality_system_enabled: bool,
    /// Enable skill system integration
    pub skill_system_enabled: bool,
    /// Enable station system integration
    pub station_system_enabled: bool,
    /// Recipe discovery enabled
    pub recipe_discovery_enabled: bool,
    /// Crafting failure causes ingredient loss
    pub failure_consumes_ingredients: bool,
    /// Critical success bonus multiplier
    pub critical_success_bonus: f32,
}

impl Default for CraftingConfig {
    fn default() -> Self {
        Self {
            max_operations_per_player: 5,
            global_speed_multiplier: 1.0,
            quality_system_enabled: true,
            skill_system_enabled: true,
            station_system_enabled: true,
            recipe_discovery_enabled: true,
            failure_consumes_ingredients: true,
            critical_success_bonus: 1.5,
        }
    }
}

// ============================================================================
// Plugin Integration Types
// ============================================================================

/// Integration point with another plugin
#[derive(Debug, Clone)]
pub struct PluginIntegration {
    /// Name of the integrated plugin
    pub plugin_name: String,
    /// Available capabilities
    pub capabilities: Vec<String>,
    /// Integration configuration
    pub config: HashMap<String, serde_json::Value>,
}

/// Statistics for the crafting system
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CraftingStats {
    /// Total crafting operations started
    pub total_crafting_started: u64,
    /// Total crafting operations completed
    pub total_crafting_completed: u64,
    /// Total crafting operations failed
    pub total_crafting_failed: u64,
    /// Total crafting operations cancelled
    pub total_crafting_cancelled: u64,
    /// Total critical successes
    pub total_critical_successes: u64,
    
    /// Recipes attempted (recipe_name -> count)
    pub recipes_attempted: HashMap<String, u64>,
    /// Most popular recipes
    pub popular_recipes: Vec<(String, u64)>,
    
    /// Average crafting time per recipe
    pub average_crafting_times: HashMap<String, Duration>,
    /// Success rates per recipe
    pub success_rates: HashMap<String, f32>,
}

// ============================================================================
// Client Integration Types (Feature Gated)
// ============================================================================

#[cfg(feature = "client_events")]
/// Configuration for client integration
#[derive(Debug, Clone)]
pub struct ClientIntegrationConfig {
    /// Send real-time progress updates
    pub enable_progress_updates: bool,
    /// Update frequency for progress (milliseconds)
    pub progress_update_frequency_ms: u64,
    /// Enable client-side recipe validation
    pub enable_client_validation: bool,
    /// Cache recipe list on client
    pub cache_recipes_on_client: bool,
    /// Maximum client message size
    pub max_client_message_size: usize,
    /// Rate limiting for client requests
    pub client_request_limit_per_minute: u32,
    /// Enable debug mode for client communication
    pub debug_client_communication: bool,
}

#[cfg(feature = "client_events")]
impl Default for ClientIntegrationConfig {
    fn default() -> Self {
        Self {
            enable_progress_updates: true,
            progress_update_frequency_ms: 1000, // 1 second
            enable_client_validation: true,
            cache_recipes_on_client: true,
            max_client_message_size: 1024 * 64, // 64KB
            client_request_limit_per_minute: 60,
            debug_client_communication: false,
        }
    }
}

#[cfg(feature = "client_events")]
/// Active client session information
#[derive(Debug, Clone)]
pub struct ClientSession {
    pub player_id: PlayerId,
    pub connected_at: SystemTime,
    pub last_activity: SystemTime,
    pub client_version: Option<String>,
    pub supported_features: Vec<String>,
    pub request_count: u32,
    pub last_request_time: SystemTime,
}

#[cfg(feature = "client_events")]
/// Client event statistics
#[derive(Debug, Default, Clone)]
pub struct ClientEventStats {
    pub total_client_requests: u64,
    pub crafting_requests: u64,
    pub cancel_requests: u64,
    pub recipe_list_requests: u64,
    pub discovery_requests: u64,
    pub progress_updates_sent: u64,
    pub validation_failures: u64,
    pub rate_limited_requests: u64,
    pub average_response_time_ms: f64,
}

#[cfg(feature = "client_events")]
/// Client subscriptions to various events
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ClientSubscriptions {
    pub progress_updates: bool,
    pub recipe_unlocks: bool,
    pub crafting_completions: bool,
    pub system_notifications: bool,
    pub discovery_results: bool,
}

#[cfg(feature = "client_events")]
/// Client information provided on connection
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub version: Option<String>,
    pub supported_features: Vec<String>,
    pub platform: Option<String>,
}

#[cfg(feature = "client_events")]
/// Simplified recipe info for clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRecipeInfo {
    pub recipe_id: RecipeId,
    pub name: String,
    pub description: String,
    pub category: String,
    pub ingredients: Vec<Ingredient>,
    pub main_product: CraftingResult,
    #[serde(with = "duration_serde")]
    pub crafting_time: Duration,
    pub skill_requirements: HashMap<String, u32>,
    pub can_craft: bool,
    pub missing_requirements: Vec<String>,
}

// ============================================================================
// Event Types
// ============================================================================

/// Event emitted when crafting starts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingStartedEvent {
    pub operation_id: CraftingOperationId,
    pub player_id: PlayerId,
    pub recipe_id: RecipeId,
    pub quantity: u32,
    #[serde(with = "duration_serde")]
    pub estimated_duration: Duration,
    pub timestamp: u64,
}

/// Event emitted during crafting progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingProgressEvent {
    pub operation_id: CraftingOperationId,
    pub player_id: PlayerId,
    pub recipe_id: RecipeId,
    pub progress_percentage: u32,
    pub estimated_completion: SystemTime,
    pub timestamp: u64,
}

/// Event emitted when crafting completes successfully
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingCompletedEvent {
    pub operation_id: CraftingOperationId,
    pub player_id: PlayerId,
    pub recipe_id: RecipeId,
    pub quantity_produced: u32,
    pub quality_achieved: Option<f32>,
    pub experience_gained: HashMap<String, u32>,
    pub byproducts: Vec<CraftingResult>,
    pub critical_success: bool,
    pub timestamp: u64,
}

/// Event emitted when crafting fails
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingFailedEvent {
    pub operation_id: CraftingOperationId,
    pub player_id: PlayerId,
    pub recipe_id: RecipeId,
    pub reason: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySlot {
    pub slot_id: u32,
    pub item_id: u32,
    pub quantity: u32,
    pub quality: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingRequestEvent {
    pub player_id: PlayerId,
    pub recipe_id: RecipeId,
    pub quantity: u32,
    pub ingredient_sources: Vec<InventorySlot>,
}

/// Event emitted when crafting is cancelled
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingCancelledEvent {
    pub operation_id: CraftingOperationId,
    pub player_id: PlayerId,
    pub recipe_id: RecipeId,
    pub progress_lost: f32,
    pub timestamp: u64,
}

/// Event emitted when a recipe is unlocked
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeUnlockedEvent {
    pub player_id: PlayerId,
    pub recipe_id: RecipeId,
    pub unlock_reason: String,
    pub timestamp: u64,
}

/// Event emitted when recipes are loaded
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipesLoadedEvent {
    pub recipe_count: usize,
    pub timestamp: u64,
}

/// Event emitted when player crafting state is initialized
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerCraftingInitializedEvent {
    pub player_id: PlayerId,
    pub initial_recipes: Vec<RecipeId>,
    pub crafting_level: u32,
    pub timestamp: u64,
}

/// Event for requesting cancellation of crafting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelCraftingEvent {
    pub player_id: PlayerId,
    pub operation_id: CraftingOperationId,
    pub reason: String,
}

/// Generic plugin ready event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginReadyEvent {
    pub plugin_name: String,
    pub capabilities: Vec<String>,
    pub integration_points: Vec<String>,
    pub timestamp: u64,
}

/// Generic plugin shutdown event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginShutdownEvent {
    pub plugin_name: String,
    pub reason: String,
    pub timestamp: u64,
}

// ============================================================================
// Integration Events (for other plugins)
// ============================================================================

/// Event emitted when inventory is updated (consumed by this plugin)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryUpdatedEvent {
    pub player_id: PlayerId,
    pub item_id: u32,
    pub old_quantity: u32,
    pub new_quantity: u32,
    pub change_reason: String,
    pub timestamp: u64,
}

/// Event emitted when skill level is updated (consumed by this plugin)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLevelUpdatedEvent {
    pub player_id: PlayerId,
    pub skill_name: String,
    pub old_level: u32,
    pub new_level: u32,
    pub experience_gained: u32,
    pub timestamp: u64,
}

/// Event emitted when item quality is updated (consumed by this plugin)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemQualityUpdatedEvent {
    pub player_id: PlayerId,
    pub item_id: u32,
    pub slot_id: u32,
    pub old_quality: f32,
    pub new_quality: f32,
    pub timestamp: u64,
}

// ============================================================================
// Client Event Types (Feature Gated)
// ============================================================================

#[cfg(feature = "client_events")]
/// Client event for requesting crafting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCraftingRequestEvent {
    pub player_id: PlayerId,
    pub recipe_id: u32,
    pub quantity: u32,
    pub ingredient_sources: Vec<InventorySlot>,
}

#[cfg(feature = "client_events")]
/// Client event for cancelling crafting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCancelCraftingEvent {
    pub player_id: PlayerId,
    pub operation_id: CraftingOperationId,
}

#[cfg(feature = "client_events")]
/// Client event for recipe discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRecipeDiscoveryEvent {
    pub player_id: PlayerId,
    pub experimental_ingredients: Vec<InventorySlot>,
    pub discovery_method: String, // "experimentation", "learning", "quest_reward"
}

#[cfg(feature = "client_events")]
/// Client request for recipe list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRecipeListRequestEvent {
    pub player_id: PlayerId,
    pub category_filter: Option<String>,
    pub craftable_only: bool,
    pub include_locked: bool,
}

#[cfg(feature = "client_events")]
/// Client subscription event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSubscriptionEvent {
    pub player_id: PlayerId,
    pub subscriptions: ClientSubscriptions,
}

#[cfg(feature = "client_events")]
/// Client progress update event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientProgressUpdateEvent {
    pub operation_id: CraftingOperationId,
    pub progress_percentage: u32,
    pub estimated_completion: SystemTime,
    pub timestamp: u64,
}

#[cfg(feature = "client_events")]
/// Client crafting completion event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCraftingCompletedEvent {
    pub operation_id: CraftingOperationId,
    pub recipe_id: RecipeId,
    pub quantity_produced: u32,
    pub quality_achieved: Option<f32>,
    pub experience_gained: HashMap<String, u32>,
    pub critical_success: bool,
    pub timestamp: u64,
}

#[cfg(feature = "client_events")]
/// Client recipe unlock event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRecipeUnlockedEvent {
    pub recipe_id: RecipeId,
    pub unlock_reason: String,
    pub timestamp: u64,
}

#[cfg(feature = "client_events")]
/// Client response for available recipes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRecipeListResponse {
    pub player_id: PlayerId,
    pub available_recipes: Vec<ClientRecipeInfo>,
    pub unlockable_recipes: Vec<ClientRecipeInfo>,
}

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur in the crafting system
#[derive(Debug, thiserror::Error)]
pub enum CraftingError {
    #[error("Recipe not found: {0}")]
    RecipeNotFound(u32),
    
    #[error("Crafting operation not found: {0}")]
    OperationNotFound(CraftingOperationId),
    
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Insufficient ingredients: {0}")]
    InsufficientIngredients(String),
    
    #[error("Requirements not met: {0}")]
    RequirementsNotMet(String),
    
    #[error("Permission denied")]
    PermissionDenied,
    
    #[error("Maximum operations reached")]
    MaxOperationsReached,
    
    #[error("Recipe locked")]
    RecipeLocked,
    
    #[error("System error: {0}")]
    SystemError(String),
    
    #[error("Integration error: {0}")]
    IntegrationError(String),
    
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
}

// ============================================================================
// Serialization Helpers
// ============================================================================

/// Helper module for Duration serialization
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

// ============================================================================
// Test Helper Functions
// ============================================================================

#[cfg(test)]
pub mod test_helpers {
    use super::*;

    /// Create a test recipe for unit tests
    pub fn create_test_recipe() -> Recipe {
        Recipe {
            id: RecipeId(1001),
            name: "Test Recipe".to_string(),
            description: "A test recipe".to_string(),
            category: "Test".to_string(),
            ingredients: vec![
                Ingredient {
                    item_id: 1,
                    quantity: 2,
                    quality_requirement: None,
                }
            ],
            main_product: CraftingResult {
                item_id: 2,
                quantity: 1,
                quality_bonus: None,
            },
            byproducts: vec![],
            requirements: CraftingRequirements {
                skill_requirements: HashMap::new(),
                station_requirement: None,
                tool_requirements: vec![],
                environmental_requirements: vec![],
            },
            timing: CraftingTiming {
                base_duration: Duration::from_secs(10),
                skill_speed_bonus: None,
                station_speed_bonus: None,
            },
            unlock_conditions: vec![],
            experience_rewards: HashMap::new(),
            failure_chance: 0.1,
            critical_success_chance: 0.05,
        }
    }

    /// Create a test player crafting state
    pub fn create_test_player_state() -> PlayerCraftingState {
        PlayerCraftingState {
            player_id: PlayerId::new(),
            known_recipes: vec![RecipeId(1001)],
            active_operations: vec![],
            total_crafting_attempts: 0,
            successful_crafts: 0,
            crafting_level: 10,
            experience_points: HashMap::new(),
            unlocked_categories: vec!["Test".to_string()],
            preferences: CraftingPreferences::default(),
        }
    }

    /// Create a test crafting operation
    pub fn create_test_operation(player_id: PlayerId) -> CraftingOperation {
        CraftingOperation {
            id: CraftingOperationId::new(),
            player_id,
            recipe: create_test_recipe(),
            quantity: 1,
            ingredient_sources: vec![],
            started_at: SystemTime::now(),
            estimated_completion: SystemTime::now() + Duration::from_secs(10),
            progress: 0.0,
            status: CraftingStatus::Queued,
            quality_modifiers: vec![],
            experience_multiplier: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recipe_id_creation() {
        let id1 = RecipeId::new();
        let id2 = RecipeId::new();
        // Should be different due to randomness (very high probability)
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn test_crafting_operation_id_creation() {
        let id1 = CraftingOperationId::new();
        let id2 = CraftingOperationId::new();
        assert_ne!(id1, id2); // Should be different UUIDs
    }

    #[test]
    fn test_crafting_preferences_default() {
        let prefs = CraftingPreferences::default();
        assert!(!prefs.auto_queue_enabled);
        assert!(prefs.use_best_quality_ingredients);
        assert!(prefs.progress_notifications);
    }

    #[test]
    fn test_crafting_config_default() {
        let config = CraftingConfig::default();
        assert_eq!(config.max_operations_per_player, 5);
        assert_eq!(config.global_speed_multiplier, 1.0);
        assert!(config.quality_system_enabled);
    }

    #[test]
    fn test_validation_result() {
        let result = ValidationResult {
            is_valid: true,
            failure_reason: None,
            quality_modifiers: vec![],
            experience_multiplier: 1.0,
            warnings: vec![],
        };
        assert!(result.is_valid);
        assert!(result.failure_reason.is_none());
    }

    #[test]
    fn test_crafting_status_display() {
        assert_eq!(format!("{}", CraftingStatus::InProgress), "In Progress");
        assert_eq!(format!("{}", CraftingStatus::Completed), "Completed");
        assert_eq!(format!("{}", CraftingStatus::Failed), "Failed");
    }

    #[test]
    fn test_crafting_operation_completion_check() {
        use test_helpers::*;
        
        let player_id = PlayerId::new();
        let mut operation = create_test_operation(player_id);
        
        // Should not be complete initially
        assert!(!operation.is_complete());
        
        // Set progress to 100%
        operation.progress = 1.0;
        assert!(operation.is_complete());
        
        // Reset progress and set completion time in the past
        operation.progress = 0.5;
        operation.estimated_completion = SystemTime::now() - Duration::from_secs(1);
        assert!(operation.is_complete());
    }

    #[cfg(feature = "client_events")]
    #[test]
    fn test_client_integration_config_default() {
        let config = ClientIntegrationConfig::default();
        assert!(config.enable_progress_updates);
        assert_eq!(config.progress_update_frequency_ms, 1000);
        assert_eq!(config.client_request_limit_per_minute, 60);
    }
}