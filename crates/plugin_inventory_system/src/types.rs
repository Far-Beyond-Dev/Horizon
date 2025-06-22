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

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
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

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct DropItemRequest {
    pub id: PlayerId,
    pub item_count: u32,
    pub item_id: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct GetInventoryRequest {
    pub id: PlayerId,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CheckItemRequest {
    pub id: PlayerId,
    pub item_id: u32,
    pub required_amount: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TransferItemRequest {
    pub from_player: PlayerId,
    pub to_player: PlayerId,
    pub item_id: u32,
    pub amount: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ConsumeItemRequest {
    pub id: PlayerId,
    pub item_id: u32,
    pub amount: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct MoveItemRequest {
    pub player_id: PlayerId,
    pub original_slot: u8,
    pub new_slot: u8,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ClearInventoryRequest {
    pub id: PlayerId,
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
