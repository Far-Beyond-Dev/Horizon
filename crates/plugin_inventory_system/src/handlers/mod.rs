// Core inventory operations
pub mod check_item;
pub mod clear_inventory;
pub mod consume_item;
pub mod drop_item;
pub mod get_inventory;
pub mod pickup_item;
pub mod setup_inventory;
pub mod transfer_item;

// Equipment system
pub mod equip_item;
pub mod unequip_item;

// Trading system
pub mod trade_management;

// Crafting system
mod craft_item;

// Search and organization
mod search_inventory;
mod sort_inventory;

// Container system
mod container_management;

// Item enhancement
mod enchant_item;
mod repair_item;

// Utility handlers
mod inventory_validation;
mod item_management;

// Export all handlers
pub use check_item::check_item_handler;
pub use clear_inventory::clear_inventory_handler;
pub use consume_item::consume_item_handler;
pub use drop_item::drop_item_handler;
pub use get_inventory::get_inventory_handler;
pub use pickup_item::pickup_item_handler;
pub use setup_inventory::setup_inventory_handler;
pub use transfer_item::transfer_item_handler;

pub use equip_item::equip_item_handler;
pub use unequip_item::unequip_item_handler;

pub use trade_management::{create_trade_handler, get_trades_handler, trade_action_handler};

pub use craft_item::craft_item_handler;

pub use search_inventory::search_inventory_handler;
pub use sort_inventory::sort_inventory_handler;

pub use container_management::{
    access_container_handler, create_container_handler, delete_container_handler,
    get_container_handler,
};

pub use enchant_item::enchant_item_handler;
pub use repair_item::repair_item_handler;

pub use inventory_validation::*;
pub use item_management::*;
