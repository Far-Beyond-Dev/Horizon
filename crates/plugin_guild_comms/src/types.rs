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
use uuid::Uuid;

pub use chrono::prelude::*;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ClanSystem {
    pub clan_id: Uuid,
    pub clan_name: String,
    pub player_count: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct MessageSystemRequest {
    id: PlayerId,
    name: String,
    time: DateTime<Utc>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Roles {
    pub permission: u8,
    pub role_name: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Channels {
    pub channel_name: String,
    pub roles_with_access: Option<Vec<Roles>>,
    pub active_users_in_channel: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct GuildSystem {
    pub clans: Option<Vec<ClanSystem>>,
    pub chat: Option<Vec<MessageSystemRequest>>,
    pub roles: Option<Vec<Roles>>,
    pub channels: Option<Vec<Channels>>,
}
