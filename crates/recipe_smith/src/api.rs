//! Type-safe API definitions for RecipeSmith plugin

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifiers
pub type RecipeId = Uuid;
pub type ItemId = Uuid;
pub type CraftId = Uuid;

/// Item stack representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemStack {
    pub item_id: ItemId,
    pub quantity: u32,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Crafting recipe definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: RecipeId,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub skill_requirements: Vec<SkillRequirement>,
    pub ingredients: Vec<ItemStack>,
    pub products: Vec<ItemStack>,
    #[serde(default)]
    pub craft_time: Option<u64>,
    #[serde(default)]
    pub tool_requirements: Option<Vec<ItemId>>,
    #[serde(default)]
    pub station_requirements: Option<Vec<String>>,
    #[serde(default)]
    pub is_learned: bool,
    #[serde(default)]
    pub unlock_conditions: Option<serde_json::Value>,
}

/// Skill requirement for a recipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRequirement {
    pub skill: String,
    pub level: u32,
}

/// Active craft information
#[derive(Debug, Clone)]
pub struct ActiveCraft {
    pub craft_id: CraftId,
    pub player_id: super::PlayerId,
    pub recipe_id: RecipeId,
    pub started_at: std::time::SystemTime,
    pub duration: u64,
    pub ingredients: Vec<ItemStack>,
}

/// Craft started response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftStarted {
    pub craft_id: CraftId,
    pub recipe_id: RecipeId,
    pub duration: u64,
}

/// Craft event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CraftEvent {
    Started {
        craft_id: CraftId,
        recipe_id: RecipeId,
        duration: u64,
    },
    Completed {
        craft_id: CraftId,
        recipe_id: RecipeId,
        products: Vec<ItemStack>,
    },
    Failed {
        craft_id: CraftId,
        recipe_id: RecipeId,
        reason: String,
    },
    Cancelled {
        craft_id: CraftId,
        recipe_id: RecipeId,
        reason: String,
    },
    Progress {
        craft_id: CraftId,
        recipe_id: RecipeId,
        progress: f32,
    },
}

// Request/Response types ------------------------------------------------------

/// Request to start a new craft
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartCraftRequest {
    pub player_id: super::PlayerId,
    pub recipe_id: RecipeId,
    pub ingredients: Vec<ItemStack>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_event: Option<super::EventId>,
}

/// Response to start craft request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartCraftResponse {
    pub craft_id: CraftId,
    pub recipe_id: RecipeId,
    pub duration: u64,
    pub estimated_completion: u64,
}

/// Request to cancel a craft
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelCraftRequest {
    pub player_id: super::PlayerId,
    pub craft_id: CraftId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_event: Option<super::EventId>,
}

/// Request to get available recipes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRecipesRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_id: Option<super::PlayerId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_event: Option<super::EventId>,
}

/// Craft event notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftEventNotification {
    pub player_id: super::PlayerId,
    pub event: CraftEvent,
}

/// Plugin ready event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginReadyEvent {
    pub recipe_count: usize,
}

/// Client crafting request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCraftingRequest {
    pub player_id: super::PlayerId,
    pub recipe_id: RecipeId,
    pub ingredient_sources: Vec<InventorySlot>,
}

/// Inventory slot reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySlot {
    pub container_id: Uuid,
    pub item_id: ItemId,
    pub slot_id: u32,
    pub quantity: u32,
}

/// Recipe update event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeUpdateEvent {
    pub recipe: Recipe,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_event: Option<super::EventId>,
}

/// Recipe deletion event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeDeleteEvent {
    pub recipe_id: RecipeId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_event: Option<super::EventId>,
}