pub use async_trait::async_trait;
pub use event_system::{
    create_simple_plugin, current_timestamp, EventSystem, LogLevel, PlayerId, PluginError,
    ServerContext, SimplePlugin,
};
pub use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    vec::Vec,
};

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct InventorySlot {
    pub item: u64,
    pub stack: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct PickupItemRequest {
    pub id: PlayerId,
    pub item_count: u32,
    pub item_id: u32,
}

pub struct Player {
    pub id: PlayerId,
    pub item_count: u32,
    pub inventory: HashMap<u64, InventorySlot>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct InventorySettingRequest {
    pub slot_count: Option<u32>,
    pub inventory_count: Option<u8>,
}

pub struct InventorySystem {
    pub players: Arc<Mutex<Option<HashMap<PlayerId, Player>>>>,
    pub player_count: Arc<Mutex<Option<u32>>>,
}
