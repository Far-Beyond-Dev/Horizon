pub use async_trait::async_trait;
pub use horizon_event_system::{
    create_simple_plugin, current_timestamp, EventSystem, LogLevel, PlayerId, PluginError,
    ServerContext, SimplePlugin,
};
pub use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    vec::Vec,
};
pub use uuid::Uuid;
pub use lru::LruCache;
pub use tracing::{error, info, warn};
pub use indexmap::IndexMap;

// Core Item Types
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ItemDefinition {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub category: ItemCategory,
    pub rarity: ItemRarity,
    pub max_stack: u32,
    pub weight: f32,
    pub size: (u32, u32), // width, height for grid inventories
    pub tradeable: bool,
    pub consumable: bool,
    pub durability_max: Option<u32>,
    pub icon_path: String,
    pub value: u32,
    pub level_requirement: Option<u32>,
    pub custom_properties: HashMap<String, serde_json::Value>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ItemInstance {
    pub definition_id: u64,
    pub instance_id: String,
    pub stack: u32,
    pub durability: Option<u32>,
    pub enchantments: Vec<Enchantment>,
    pub bound_to_player: Option<PlayerId>,
    pub acquired_timestamp: u64,
    pub custom_data: HashMap<String, serde_json::Value>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct InventorySlot {
    pub slot_id: u32,
    pub item: Option<ItemInstance>,
    pub locked: bool,
}

// Categories and Classifications
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum ItemCategory {
    Weapon(WeaponType),
    Armor(ArmorType),
    Consumable(ConsumableType),
    Material,
    Quest,
    Currency,
    Tool,
    Container,
    Enchantment,
    Custom(String),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum WeaponType {
    Sword,
    Bow,
    Staff,
    Dagger,
    Axe,
    Hammer,
    Spear,
    Custom(String),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum ArmorType {
    Helmet,
    Chest,
    Legs,
    Boots,
    Gloves,
    Shield,
    Ring,
    Necklace,
    Custom(String),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum ConsumableType {
    Potion,
    Food,
    Scroll,
    Ammo,
    Custom(String),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum ItemRarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
    Mythic,
    Custom(String),
}

// Inventory System Types
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum InventoryType {
    General,
    Equipment,
    Hotbar,
    Bank,
    Container(String),
    Crafting,
    Trade,
    Quest,
    Custom(String),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct InventoryConstraints {
    pub max_slots: Option<u32>,
    pub max_weight: Option<f32>,
    pub allowed_categories: Option<Vec<ItemCategory>>,
    pub grid_size: Option<(u32, u32)>,
    pub auto_sort: bool,
    pub allow_overflow: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Inventory {
    pub inventory_type: InventoryType,
    pub slots: HashMap<u32, InventorySlot>,
    pub constraints: InventoryConstraints,
    pub current_weight: f32,
    pub last_modified: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct PlayerInventories {
    pub player_id: PlayerId,
    pub inventories: HashMap<String, Inventory>, // inventory_name -> inventory
    pub equipped_items: EquipmentSlots,
    pub active_effects: Vec<ItemEffect>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct EquipmentSlots {
    pub helmet: Option<ItemInstance>,
    pub chest: Option<ItemInstance>,
    pub legs: Option<ItemInstance>,
    pub boots: Option<ItemInstance>,
    pub gloves: Option<ItemInstance>,
    pub weapon_main: Option<ItemInstance>,
    pub weapon_off: Option<ItemInstance>,
    pub ring_1: Option<ItemInstance>,
    pub ring_2: Option<ItemInstance>,
    pub necklace: Option<ItemInstance>,
    pub custom_slots: HashMap<String, ItemInstance>,
}

// Enchantment System
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Enchantment {
    pub enchantment_id: String,
    pub name: String,
    pub level: u32,
    pub effects: Vec<EnchantmentEffect>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct EnchantmentEffect {
    pub effect_type: String,
    pub value: f32,
    pub duration: Option<u32>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ItemEffect {
    pub effect_id: String,
    pub source_item: u64,
    pub effect_type: String,
    pub value: f32,
    pub duration: Option<u64>,
    pub applied_at: u64,
}

// Trading System
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TradeOffer {
    pub trade_id: String,
    pub from_player: PlayerId,
    pub to_player: PlayerId,
    pub offered_items: Vec<TradeItem>,
    pub requested_items: Vec<TradeItem>,
    pub status: TradeStatus,
    pub created_at: u64,
    pub expires_at: Option<u64>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TradeItem {
    pub item_instance: ItemInstance,
    pub quantity: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum TradeStatus {
    Pending,
    Accepted,
    Declined,
    Completed,
    Cancelled,
    Expired,
}

// Container System
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Container {
    pub container_id: String,
    pub container_type: ContainerType,
    pub name: String,
    pub inventory: Inventory,
    pub location: Option<WorldPosition>,
    pub access_permissions: Vec<PlayerId>,
    pub password_protected: bool,
    pub public_access: bool,
    pub created_at: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum ContainerType {
    Chest,
    Barrel,
    Bag,
    Vault,
    Shop,
    Custom(String),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct WorldPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub world: String,
}

// Crafting System
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CraftingRecipe {
    pub recipe_id: String,
    pub name: String,
    pub category: String,
    pub required_items: Vec<RecipeItem>,
    pub required_tools: Vec<u64>,
    pub output_items: Vec<RecipeItem>,
    pub skill_requirements: HashMap<String, u32>,
    pub crafting_time: u32,
    pub experience_reward: u32,
    pub unlock_level: Option<u32>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct RecipeItem {
    pub item_id: u64,
    pub quantity: u32,
    pub consumed: bool, // false for tools that aren't consumed
}

// Search and Sorting
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct InventoryQuery {
    pub player_id: PlayerId,
    pub inventory_types: Vec<String>,
    pub category_filter: Option<ItemCategory>,
    pub name_search: Option<String>,
    pub rarity_filter: Option<ItemRarity>,
    pub sort_by: SortCriteria,
    pub sort_order: SortOrder,
    pub pagination: Pagination,
    pub include_equipped: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum SortCriteria {
    Name,
    Category,
    Rarity,
    Quantity,
    Value,
    DateAcquired,
    Weight,
    Durability,
    Custom(String),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Pagination {
    pub page: u32,
    pub per_page: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct InventorySearchResult {
    pub items: Vec<ItemInstance>,
    pub total_count: u32,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}

// Player Structure
pub struct Player {
    pub id: PlayerId,
    pub inventories: PlayerInventories,
    pub online: bool,
    pub last_activity: u64,
}

// Main Plugin Structure
pub struct InventorySystem {
    pub players: Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    pub player_count: Arc<Mutex<Option<u32>>>,
    pub item_definitions: Arc<Mutex<HashMap<u64, ItemDefinition>>>,
    pub crafting_recipes: Arc<Mutex<HashMap<String, CraftingRecipe>>>,
    pub containers: Arc<Mutex<HashMap<String, Container>>>,
    pub active_trades: Arc<Mutex<HashMap<String, TradeOffer>>>,
    pub config: Arc<Mutex<InventoryConfig>>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct InventoryConfig {
    pub default_inventory_size: u32,
    pub max_player_inventories: u32,
    pub enable_weight_system: bool,
    pub enable_durability: bool,
    pub allow_item_stacking: bool,
    pub auto_stack_items: bool,
    pub currency_items: Vec<u64>,
    pub trade_timeout_seconds: u64,
    pub container_access_distance: f64,
    pub custom_rules: HashMap<String, serde_json::Value>,
}

// Request/Response Types for all operations
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct PickupItemRequest {
    pub id: PlayerId,
    pub item_count: u32,
    pub item_id: u32,
    pub inventory_name: Option<String>,
    pub target_slot: Option<u32>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct DropItemRequest {
    pub id: PlayerId,
    pub item_count: u32,
    pub item_id: u32,
    pub inventory_name: Option<String>,
    pub slot_id: Option<u32>,
    pub position: Option<WorldPosition>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct GetInventoryRequest {
    pub id: PlayerId,
    pub inventory_name: Option<String>,
    pub include_equipment: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CheckItemRequest {
    pub id: PlayerId,
    pub item_id: u32,
    pub required_amount: u32,
    pub inventory_name: Option<String>,
    pub include_equipped: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TransferItemRequest {
    pub from_player: PlayerId,
    pub to_player: PlayerId,
    pub item_id: u32,
    pub amount: u32,
    pub from_inventory: Option<String>,
    pub to_inventory: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ConsumeItemRequest {
    pub id: PlayerId,
    pub item_id: u32,
    pub amount: u32,
    pub inventory_name: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct MoveItemRequest {
    pub player_id: PlayerId,
    pub original_slot: u8,
    pub new_slot: u8,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ClearInventoryRequest {
    pub id: PlayerId,
    pub inventory_name: Option<String>,
    pub confirm: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct EquipItemRequest {
    pub id: PlayerId,
    pub item_instance_id: String,
    pub equipment_slot: String,
    pub from_inventory: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct UnequipItemRequest {
    pub id: PlayerId,
    pub equipment_slot: String,
    pub to_inventory: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CreateTradeRequest {
    pub from_player: PlayerId,
    pub to_player: PlayerId,
    pub offered_items: Vec<TradeItem>,
    pub requested_items: Vec<TradeItem>,
    pub expires_in_seconds: Option<u64>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TradeActionRequest {
    pub player_id: PlayerId,
    pub trade_id: String,
    pub action: TradeAction,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum TradeAction {
    Accept,
    Decline,
    Cancel,
    ModifyOffer(Vec<TradeItem>),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CraftItemRequest {
    pub player_id: PlayerId,
    pub recipe_id: String,
    pub quantity: u32,
    pub use_inventory: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SearchInventoryRequest {
    pub query: InventoryQuery,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SortInventoryRequest {
    pub player_id: PlayerId,
    pub inventory_name: Option<String>,
    pub sort_criteria: SortCriteria,
    pub sort_order: SortOrder,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CreateContainerRequest {
    pub container_id: String,
    pub container_type: ContainerType,
    pub name: String,
    pub constraints: InventoryConstraints,
    pub position: Option<WorldPosition>,
    pub public_access: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct AccessContainerRequest {
    pub player_id: PlayerId,
    pub container_id: String,
    pub action: ContainerAction,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum ContainerAction {
    Open,
    Close,
    AddItem { item_instance_id: String, quantity: u32 },
    RemoveItem { item_instance_id: String, quantity: u32 },
    SetPermissions { permissions: Vec<PlayerId> },
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct EnchantItemRequest {
    pub player_id: PlayerId,
    pub item_instance_id: String,
    pub enchantment: Enchantment,
    pub enchantment_materials: Vec<TradeItem>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct RepairItemRequest {
    pub player_id: PlayerId,
    pub item_instance_id: String,
    pub repair_materials: Option<Vec<TradeItem>>,
    pub use_currency: Option<u32>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct InventorySettingRequest {
    pub slot_count: Option<u32>,
    pub inventory_count: Option<u8>,
    pub enable_weight_system: Option<bool>,
    pub enable_durability: Option<bool>,
    pub auto_stack_items: Option<bool>,
}

// Event Types
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum InventoryEvent {
    ItemAdded {
        player_id: PlayerId,
        item: ItemInstance,
        inventory: String,
        source: ItemSource,
    },
    ItemRemoved {
        player_id: PlayerId,
        item: ItemInstance,
        inventory: String,
        reason: RemovalReason,
    },
    ItemMoved {
        player_id: PlayerId,
        item_instance_id: String,
        from_inventory: String,
        to_inventory: String,
        from_slot: u32,
        to_slot: u32,
    },
    ItemEquipped {
        player_id: PlayerId,
        item: ItemInstance,
        equipment_slot: String,
    },
    ItemUnequipped {
        player_id: PlayerId,
        item: ItemInstance,
        equipment_slot: String,
    },
    ItemCrafted {
        player_id: PlayerId,
        recipe_id: String,
        crafted_items: Vec<ItemInstance>,
        consumed_items: Vec<ItemInstance>,
    },
    TradeCreated {
        trade: TradeOffer,
    },
    TradeCompleted {
        trade: TradeOffer,
    },
    ContainerAccessed {
        player_id: PlayerId,
        container_id: String,
        action: ContainerAction,
    },
    ItemEnchanted {
        player_id: PlayerId,
        item: ItemInstance,
        enchantment: Enchantment,
    },
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum ItemSource {
    Pickup,
    Craft,
    Trade,
    Drop,
    Container,
    Admin,
    Quest,
    Purchase,
    Custom(String),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum RemovalReason {
    Drop,
    Consume,
    Trade,
    Craft,
    Destroy,
    Transfer,
    Admin,
    Custom(String),
}

// Utility Types
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SlotId {
    pub inventory_name: String,
    pub slot_number: u32,
}

// Error Types
#[derive(thiserror::Error, Debug, Clone)]
pub enum InventoryError {
    #[error("Player not found: {0:?}")]
    PlayerNotFound(PlayerId),
    #[error("Item not found: {0}")]
    ItemNotFound(u64),
    #[error("Insufficient items: need {needed}, have {available}")]
    InsufficientItems { needed: u32, available: u32 },
    #[error("Inventory full")]
    InventoryFull,
    #[error("Invalid slot: {0}")]
    InvalidSlot(u32),
    #[error("Item not stackable")]
    ItemNotStackable,
    #[error("Weight limit exceeded")]
    WeightLimitExceeded,
    #[error("Item not tradeable")]
    ItemNotTradeable,
    #[error("Container not found: {0}")]
    ContainerNotFound(String),
    #[error("Access denied")]
    AccessDenied,
    #[error("Trade not found: {0}")]
    TradeNotFound(String),
    #[error("Recipe not found: {0}")]
    RecipeNotFound(String),
    #[error("Durability too low")]
    DurabilityTooLow,
    #[error("Custom error: {0}")]
    Custom(String),
}
