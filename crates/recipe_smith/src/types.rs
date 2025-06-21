// crates/recipe_smith/src/types.rs

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type RecipeId = Uuid;
pub type ItemId = Uuid;
pub type CraftId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemStack {
    pub item_id: ItemId,
    pub quantity: u32,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: RecipeId,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub skill_requirements: HashMap<String, u32>,
    pub ingredients: Vec<ItemStack>,
    pub products: Vec<ItemStack>,
    pub craft_time: Option<u64>,
    pub tool_requirements: Option<Vec<ItemId>>,
    pub station_requirements: Option<Vec<String>>,
    pub is_learned: bool,
    pub unlock_conditions: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveCraft {
    pub craft_id: CraftId,
    pub player_id: PlayerId,
    pub recipe_id: RecipeId,
    pub started_at: u64,
    pub duration: u64,
    pub ingredients: Vec<ItemStack>,
}

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

// API request/response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartCraftRequest {
    pub player_id: PlayerId,
    pub recipe_id: RecipeId,
    pub ingredients: Vec<ItemStack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartCraftResponse {
    pub craft_id: CraftId,
    pub recipe_id: RecipeId,
    pub duration: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelCraftRequest {
    pub player_id: PlayerId,
    pub craft_id: CraftId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCraftingRequest {
    pub player_id: PlayerId,
    pub recipe_id: RecipeId,
    pub ingredient_sources: Vec<InventorySlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySlot {
    pub slot_id: u32,
    pub item_id: ItemId,
    pub quantity: u32,
}