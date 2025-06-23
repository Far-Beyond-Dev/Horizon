pub use async_trait::async_trait;
pub use chrono::prelude::*;
pub use event_system::{
    create_simple_plugin, current_timestamp, EventSystem, LogLevel, PlayerId, PluginError,
    ServerContext, SimplePlugin,
};
pub use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    vec::Vec,
};
use uuid::Uuid;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Dimensions {
    pub x: u128,
    pub y: u128,
    pub z: u128,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Room {
    pub room_id: Option<Uuid>,
    pub room_name: Option<String>,
    pub dimensions: Option<Dimensions>,
    pub room_type: Option<RoomType>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum RoomType {
    Bedroom,
    Kitchen,
    LivingRoom,
    Bathroom,
    Storage,
    Workshop,
    Custom(String),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub world: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct House {
    pub house_id: Option<Uuid>,
    pub owner_id: Option<PlayerId>,
    pub house_name: Option<String>,
    pub dimensions: Option<Dimensions>,
    pub rooms: Option<Vec<Room>>,
    pub location: Option<Position>,
    pub created_at: Option<DateTime<Utc>>,
    pub last_modified: Option<DateTime<Utc>>,
}
