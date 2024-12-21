use anyhow::{Context, Result};
use horizon_logger::log_warn;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    sync::{Arc, OnceLock},
};

use crate::LOGGER;

lazy_static! {
    pub static ref SERVER_CONFIG: OnceLock<Arc<ServerConfig>> = OnceLock::new();
}

pub fn server_config() -> Result<Arc<ServerConfig>> {
    let config_path = std::env::var("SERVER_CONFIG_PATH").unwrap_or_else(|_| "./server_config.json".to_string());
    let config = match fs::read_to_string(&config_path) {
        Ok(config_str) => serde_json::from_str(&config_str).with_context(|| format!("Failed to parse {}", config_path))?,
        Err(e) => {
            log_warn!(LOGGER, "SERVER", "Failed to read {}: {}", config_path, e);
            ServerConfig::new()
        }
    };
    Ok(SERVER_CONFIG.get_or_init(|| Arc::new(config)).clone())
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub struct ServerConfig {
    pub players_per_pool: u32,
    pub num_thread_pools: u32,
}

#[allow(dead_code)]
impl ServerConfig {
    fn new() -> Self {
        Self {
            players_per_pool: 5000,
            num_thread_pools: 60,
        }
    }
    fn log_level() -> String {
        String::from("info")
    }
    
}