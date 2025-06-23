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

pub use chrono::prelude::*;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct MessageSystemRequest {
    id: PlayerId,
    name: String,
    time: DateTime<Utc>,
}

pub struct Guild {
    chat: Option<Vec<MessageSystemRequest>>, 
}