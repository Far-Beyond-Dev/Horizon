//! Atlas client for Horizon server registration and heartbeat.
//!
//! This module provides a client for Horizon game server instances to register
//! with Atlas and send periodic heartbeats. When Atlas is configured, the server
//! will register on startup and maintain connectivity.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::config::ServerConfig;

// Re-export and use types from the common crate
pub use Horizon_Network_Common::{WorldCoordinate, RegionCoordinate, RegionBounds};
pub use Horizon_Network_Common::{
    ApiServerRegistration as ServerRegistration,
    ApiRegistrationResponse as RegistrationResponse,
    ApiServerHeartbeat as ServerHeartbeat,
    ApiHeartbeatResponse as HeartbeatResponse,
    AdjacentServerInfo,
    ServerCommand,
};

/// Atlas client configuration.
#[derive(Debug, Clone)]
pub struct AtlasConfig {
    /// Atlas API address (e.g., "127.0.0.1:9001")
    pub address: String,
    /// Whether Atlas integration is enabled
    pub enabled: bool,
    /// Connection timeout
    pub timeout: Duration,
    /// Heartbeat interval in seconds
    pub heartbeat_interval_secs: u32,
}

impl Default for AtlasConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:9001".to_string(),
            enabled: false,
            timeout: Duration::from_secs(10),
            heartbeat_interval_secs: 30,
        }
    }
}

impl AtlasConfig {
    /// Creates config from environment variables.
    pub fn from_env() -> Self {
        let address = std::env::var("HORIZON_ATLAS_URL")
            .map(|url| url.trim_start_matches("http://").to_string())
            .unwrap_or_else(|_| "127.0.0.1:9001".to_string());
        
        let enabled = std::env::var("HORIZON_ATLAS_ENABLED")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        
        Self {
            address,
            enabled,
            ..Default::default()
        }
    }
}

/// Extension trait for creating ServerRegistration from config
pub trait ServerRegistrationExt {
    /// Creates registration from server config.
    fn from_config(config: &ServerConfig, region_coord: RegionCoordinate) -> ServerRegistration;
}

impl ServerRegistrationExt for ServerRegistration {
    fn from_config(config: &ServerConfig, region_coord: RegionCoordinate) -> ServerRegistration {
        let bounds = &config.region_bounds;
        let center = WorldCoordinate::new(
            (bounds.min_x + bounds.max_x) / 2.0,
            (bounds.min_y + bounds.max_y) / 2.0,
            (bounds.min_z + bounds.max_z) / 2.0,
        );
        let half_extent = (bounds.max_x - bounds.min_x) / 2.0;
        
        ServerRegistration {
            name: format!("horizon-{}-{}-{}", region_coord.x, region_coord.y, region_coord.z),
            address: config.bind_address.to_string(),
            region_coord,
            center,
            bounds: half_extent,
            capacity: config.max_connections as u32,
            version: env!("CARGO_PKG_VERSION").to_string(),
            metadata: HashMap::new(),
        }
    }
}

/// Atlas client for server registration and heartbeat.
pub struct AtlasClient {
    config: AtlasConfig,
    server_id: Arc<RwLock<Option<String>>>,
    current_connections: AtomicU32,
    capacity: u32,
    connected: AtomicBool,
}

impl AtlasClient {
    /// Creates a new Atlas client.
    pub fn new(config: AtlasConfig, capacity: u32) -> Self {
        Self {
            config,
            server_id: Arc::new(RwLock::new(None)),
            current_connections: AtomicU32::new(0),
            capacity,
            connected: AtomicBool::new(false),
        }
    }

    /// Checks if Atlas integration is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Registers this server with Atlas.
    pub async fn register(&self, registration: ServerRegistration) -> Result<RegistrationResponse, String> {
        if !self.config.enabled {
            return Err("Atlas integration is disabled".to_string());
        }

        info!("📡 Registering with Atlas at {}", self.config.address);
        
        let body = serde_json::to_string(&registration)
            .map_err(|e| format!("Failed to serialize registration: {}", e))?;
        
        let response = self.http_request("POST", "/api/v1/servers/register", Some(&body))?;
        
        let reg_response: RegistrationResponse = serde_json::from_str(&response)
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        if reg_response.success {
            let mut server_id = self.server_id.write().await;
            *server_id = Some(reg_response.server_id.clone());
            self.connected.store(true, Ordering::SeqCst);
            
            info!("✅ Registered with Atlas as {}", reg_response.server_id);
            
            if !reg_response.adjacent_servers.is_empty() {
                info!("🔗 Adjacent servers: {:?}", 
                    reg_response.adjacent_servers.iter()
                        .map(|s| &s.server_id)
                        .collect::<Vec<_>>());
            }
        } else {
            error!("❌ Registration failed: {}", reg_response.message);
        }
        
        Ok(reg_response)
    }

    /// Sends a heartbeat to Atlas.
    pub async fn heartbeat(&self) -> Result<HeartbeatResponse, String> {
        let server_id = {
            let id = self.server_id.read().await;
            id.clone().ok_or_else(|| "Not registered with Atlas".to_string())?
        };
        
        let current = self.current_connections.load(Ordering::SeqCst);
        let load = if self.capacity > 0 {
            current as f32 / self.capacity as f32
        } else {
            0.0
        };
        
        let heartbeat = ServerHeartbeat {
            server_id: server_id.clone(),
            current_connections: current,
            load,
            accepting_connections: current < self.capacity,
            avg_tick_ms: 0.0,
            memory_bytes: 0,
        };
        
        let body = serde_json::to_string(&heartbeat)
            .map_err(|e| format!("Failed to serialize heartbeat: {}", e))?;
        
        let path = format!("/api/v1/servers/{}/heartbeat", server_id);
        let response = self.http_request("PUT", &path, Some(&body))?;
        
        let hb_response: HeartbeatResponse = serde_json::from_str(&response)
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        // Process commands
        for command in &hb_response.commands {
            match command {
                ServerCommand::PrepareShutdown { deadline_secs } => {
                    warn!("⚠️ Received shutdown command, deadline: {}s", deadline_secs);
                    // TODO: Trigger graceful shutdown
                }
                ServerCommand::ConfigUpdate { config } => {
                    debug!("📋 Received config update: {:?}", config);
                    // TODO: Apply config update
                }
                ServerCommand::HealthCheck => {
                    debug!("🩺 Received health check request");
                }
            }
        }
        
        Ok(hb_response)
    }

    /// Unregisters from Atlas.
    pub async fn unregister(&self) -> Result<(), String> {
        let server_id = {
            let id = self.server_id.read().await;
            match id.clone() {
                Some(id) => id,
                None => return Ok(()), // Not registered
            }
        };
        
        info!("👋 Unregistering from Atlas");
        
        let path = format!("/api/v1/servers/{}", server_id);
        let _ = self.http_request("DELETE", &path, None);
        
        self.connected.store(false, Ordering::SeqCst);
        let mut id = self.server_id.write().await;
        *id = None;
        
        Ok(())
    }

    /// Updates connection count.
    pub fn set_connections(&self, count: u32) {
        self.current_connections.store(count, Ordering::SeqCst);
    }

    /// Increments connection count.
    pub fn increment_connections(&self) {
        self.current_connections.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrements connection count.
    pub fn decrement_connections(&self) {
        let _ = self.current_connections.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| {
            if x > 0 { Some(x - 1) } else { Some(0) }
        });
    }

    /// Checks if connected to Atlas.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Starts the heartbeat loop.
    pub fn start_heartbeat_loop(self: Arc<Self>) {
        if !self.config.enabled {
            return;
        }
        
        let interval_secs = self.config.heartbeat_interval_secs;
        
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(interval_secs as u64));
            
            loop {
                ticker.tick().await;
                
                if !self.is_connected() {
                    debug!("Heartbeat skipped: not connected to Atlas");
                    continue;
                }
                
                match self.heartbeat().await {
                    Ok(response) => {
                        if !response.success {
                            warn!("Heartbeat failed: {}", response.message);
                        }
                    }
                    Err(e) => {
                        warn!("Heartbeat error: {}", e);
                        self.connected.store(false, Ordering::SeqCst);
                    }
                }
            }
        });
    }

    /// Makes an HTTP request to Atlas.
    fn http_request(&self, method: &str, path: &str, body: Option<&str>) -> Result<String, String> {
        let mut stream = TcpStream::connect(&self.config.address)
            .map_err(|e| format!("Failed to connect to Atlas: {}", e))?;
        
        stream.set_read_timeout(Some(self.config.timeout))
            .map_err(|e| format!("Failed to set timeout: {}", e))?;
        stream.set_write_timeout(Some(self.config.timeout))
            .map_err(|e| format!("Failed to set timeout: {}", e))?;
        
        let body_str = body.unwrap_or("");
        let request = if body.is_some() {
            format!(
                "{} {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                method, path, self.config.address, body_str.len(), body_str
            )
        } else {
            format!(
                "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                method, path, self.config.address
            )
        };
        
        stream.write_all(request.as_bytes())
            .map_err(|e| format!("Failed to send request: {}", e))?;
        
        let mut response = String::new();
        stream.read_to_string(&mut response)
            .map_err(|e| format!("Failed to read response: {}", e))?;
        
        // Parse HTTP response
        let parts: Vec<&str> = response.splitn(2, "\r\n\r\n").collect();
        if parts.len() < 2 {
            return Err("Invalid HTTP response".to_string());
        }
        
        let headers = parts[0];
        let body = parts[1];
        
        // Check status code
        let status_line = headers.lines().next().unwrap_or("");
        if status_line.contains("200") || status_line.contains("201") {
            Ok(body.to_string())
        } else if status_line.contains("404") {
            Err("Not found".to_string())
        } else if status_line.contains("409") {
            Err(format!("Conflict: {}", body))
        } else {
            Err(format!("HTTP error: {}", status_line))
        }
    }
}

/// Helper function to register with Atlas on server startup.
pub async fn register_with_atlas(
    server_config: &ServerConfig,
    atlas_config: AtlasConfig,
) -> Option<Arc<AtlasClient>> {
    if !atlas_config.enabled {
        info!("📡 Atlas integration disabled");
        return None;
    }

    let region_coord = RegionCoordinate::from_env();
    let registration = ServerRegistration::from_config(server_config, region_coord);
    
    let client = Arc::new(AtlasClient::new(
        atlas_config,
        server_config.max_connections as u32,
    ));
    
    match client.register(registration).await {
        Ok(response) if response.success => {
            // Start heartbeat loop
            Arc::clone(&client).start_heartbeat_loop();
            Some(client)
        }
        Ok(response) => {
            error!("Failed to register with Atlas: {}", response.message);
            None
        }
        Err(e) => {
            error!("Failed to connect to Atlas: {}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_region_coordinate_from_env() {
        std::env::set_var("HORIZON_REGION_X", "1");
        std::env::set_var("HORIZON_REGION_Y", "2");
        std::env::set_var("HORIZON_REGION_Z", "-1");
        
        let coord = RegionCoordinate::from_env();
        assert_eq!(coord.x, 1);
        assert_eq!(coord.y, 2);
        assert_eq!(coord.z, -1);
        
        // Clean up
        std::env::remove_var("HORIZON_REGION_X");
        std::env::remove_var("HORIZON_REGION_Y");
        std::env::remove_var("HORIZON_REGION_Z");
    }

    #[test]
    fn test_registration_serialization() {
        let reg = ServerRegistration {
            name: "test-server".to_string(),
            address: "127.0.0.1:8080".to_string(),
            region_coord: RegionCoordinate::center(),
            center: WorldCoordinate::new(0.0, 0.0, 0.0),
            bounds: 1000.0,
            capacity: 100,
            version: "1.0.0".to_string(),
            metadata: HashMap::new(),
        };
        
        let json = serde_json::to_string(&reg).unwrap();
        assert!(json.contains("test-server"));
        assert!(json.contains("127.0.0.1:8080"));
    }
}
