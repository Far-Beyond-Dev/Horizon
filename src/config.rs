use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::net::SocketAddr;
use std::time::Duration;

/// Complete Horizon server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HorizonConfig {
    pub server: ServerConfig,
    pub network: NetworkConfig,
    pub world: WorldConfig,
    pub plugins: PluginConfig,
    pub monitoring: MonitoringConfig,
    pub sharding: ShardingConfig,
    pub logging: LoggingConfig,
}

impl HorizonConfig {
    /// Load configuration from file with environment variable overrides
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config_str = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.as_ref().display()))?;
        
        let mut config: HorizonConfig = toml::from_str(&config_str)
            .context("Failed to parse configuration file")?;
        
        // Apply environment variable overrides
        config.apply_env_overrides();
        
        // Validate configuration
        config.validate()?;
        
        Ok(config)
    }
    
    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        if let Ok(node_id) = std::env::var("HORIZON_NODE_ID") {
            self.server.node_id = node_id;
        }
        
        if let Ok(bind) = std::env::var("HORIZON_BIND_ADDRESS") {
            self.server.bind_address = bind;
        }
        
        if let Ok(cluster_mode) = std::env::var("HORIZON_CLUSTER_MODE") {
            self.server.cluster_mode = cluster_mode.parse().unwrap_or(false);
        }
        
        if let Ok(workers) = std::env::var("HORIZON_WORKER_THREADS") {
            if let Ok(count) = workers.parse::<usize>() {
                self.server.worker_threads = count;
            }
        }
    }
    
    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Validate server config
        if self.server.node_id.is_empty() {
            return Err(anyhow::anyhow!("Server node_id cannot be empty"));
        }
        
        if self.server.worker_threads == 0 {
            return Err(anyhow::anyhow!("worker_threads must be greater than 0"));
        }
        
        if self.server.max_players == 0 {
            return Err(anyhow::anyhow!("max_players must be greater than 0"));
        }
        
        // Validate bind address
        self.server.bind_address.parse::<SocketAddr>()
            .context("Invalid bind_address format")?;
        
        // Validate world config
        if self.world.default_region_size <= 0.0 {
            return Err(anyhow::anyhow!("default_region_size must be greater than 0"));
        }
        
        // Validate plugin config
        if !self.plugins.directory.exists() {
            return Err(anyhow::anyhow!("Plugin directory does not exist: {}", 
                self.plugins.directory.display()));
        }
        
        Ok(())
    }
    
    /// Create default configuration
    pub fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            network: NetworkConfig::default(),
            world: WorldConfig::default(),
            plugins: PluginConfig::default(),
            monitoring: MonitoringConfig::default(),
            sharding: ShardingConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Unique identifier for this server node
    pub node_id: String,
    
    /// Address to bind the server to
    pub bind_address: String,
    
    /// Number of worker threads
    pub worker_threads: usize,
    
    /// Maximum number of concurrent players
    pub max_players: usize,
    
    /// Enable cluster mode
    pub cluster_mode: bool,
    
    /// Server tick rate (Hz)
    pub tick_rate: u32,
    
    /// Enable graceful shutdown
    pub graceful_shutdown_timeout: Option<i64>, // in seconds
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            node_id: uuid::Uuid::new_v4().to_string(),
            bind_address: "0.0.0.0:3000".to_string(),
            worker_threads: num_cpus::get(),
            max_players: 10000,
            cluster_mode: false,
            tick_rate: 60,
            graceful_shutdown_timeout: Some(30),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Socket.IO configuration
    pub socketio: SocketIOConfig,
    
    /// WebSocket configuration
    pub websocket: WebSocketConfig,
    
    /// HTTP server configuration
    pub http: HttpConfig,
    
    /// Rate limiting configuration
    pub rate_limiting: RateLimitingConfig,
    
    /// Connection pool settings
    pub connection_pool: ConnectionPoolConfig,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            socketio: SocketIOConfig::default(),
            websocket: WebSocketConfig::default(),
            http: HttpConfig::default(),
            rate_limiting: RateLimitingConfig::default(),
            connection_pool: ConnectionPoolConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketIOConfig {
    /// Enable Socket.IO
    pub enabled: bool,
    
    /// Socket.IO path
    pub path: String,
    
    /// Connection timeout
    pub connection_timeout: i64, // in seconds
    
    /// Heartbeat interval
    pub heartbeat_interval: i64,
    
    /// Heartbeat timeout
    pub heartbeat_timeout: i64,
    
    /// Maximum payload size
    pub max_payload_size: usize,
}

impl Default for SocketIOConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: "/socket.io".to_string(),
            connection_timeout: 60, // 60 seconds
            heartbeat_interval: 25, // 25 seconds
            heartbeat_timeout: 60, // 60 seconds
            max_payload_size: 1024 * 1024, // 1MB
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// Enable raw WebSocket
    pub enabled: bool,
    
    /// WebSocket path
    pub path: String,
    
    /// Maximum frame size
    pub max_frame_size: usize,
    
    /// Maximum message size
    pub max_message_size: usize,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: "/ws".to_string(),
            max_frame_size: 16 * 1024, // 16KB
            max_message_size: 1024 * 1024, // 1MB
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// Enable HTTP API
    pub enabled: bool,
    
    /// API path prefix
    pub api_prefix: String,
    
    /// Request timeout
    pub request_timeout: i64,
    
    /// Maximum request size
    pub max_request_size: usize,
    
    /// Enable CORS
    pub cors_enabled: bool,
    
    /// CORS allowed origins
    pub cors_origins: Vec<String>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_prefix: "/api/v1".to_string(),
            request_timeout: 60, // 60 seconds
            max_request_size: 10 * 1024 * 1024, // 10MB
            cors_enabled: true,
            cors_origins: vec!["*".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitingConfig {
    /// Enable rate limiting
    pub enabled: bool,
    
    /// Requests per minute per IP
    pub requests_per_minute: u32,
    
    /// Burst size
    pub burst_size: u32,
    
    /// Whitelist IPs (no rate limiting)
    pub whitelist: Vec<String>,
}

impl Default for RateLimitingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_minute: 600,
            burst_size: 100,
            whitelist: vec!["127.0.0.1".to_string(), "::1".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoolConfig {
    /// Initial pool size
    pub initial_size: usize,
    
    /// Maximum pool size
    pub max_size: usize,
    
    /// Connection idle timeout
    pub idle_timeout: i64,
    
    /// Connection lifetime
    pub max_lifetime: i64,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            initial_size: 10,
            max_size: 100,
            idle_timeout: 300,
            max_lifetime: 3600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldConfig {
    /// Default region size
    pub default_region_size: f64,
    
    /// Maximum objects per region
    pub max_objects_per_region: usize,
    
    /// Simulation tick rate
    pub simulation_tick_rate: u32,
    
    /// Enable physics simulation
    pub physics_enabled: bool,
    
    /// Physics update rate
    pub physics_tick_rate: u32,
    
    /// Spatial indexing configuration
    pub spatial_index: SpatialIndexConfig,
    
    /// Region transfer configuration
    pub region_transfer: RegionTransferConfig,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            default_region_size: 1000.0,
            max_objects_per_region: 10000,
            simulation_tick_rate: 60,
            physics_enabled: true,
            physics_tick_rate: 60,
            spatial_index: SpatialIndexConfig::default(),
            region_transfer: RegionTransferConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpatialIndexConfig {
    /// R-tree node capacity
    pub rtree_node_capacity: usize,
    
    /// Enable spatial caching
    pub spatial_caching: bool,
    
    /// Cache size
    pub cache_size: usize,
    
    /// Cache TTL
    pub cache_ttl: i64,
}

impl Default for SpatialIndexConfig {
    fn default() -> Self {
        Self {
            rtree_node_capacity: 16,
            spatial_caching: true,
            cache_size: 10000,
            cache_ttl: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionTransferConfig {
    /// Enable region transfers
    pub enabled: bool,
    
    /// Transfer timeout
    pub timeout: i64,
    
    /// Maximum concurrent transfers
    pub max_concurrent: usize,
    
    /// Retry attempts
    pub retry_attempts: u32,
}

impl Default for RegionTransferConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout: 60, // seconds
            max_concurrent: 5,
            retry_attempts: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Plugin directory
    pub directory: PathBuf,
    
    /// Enable hot reload
    pub hot_reload: bool,
    
    /// Plugin scan interval
    pub scan_interval: i64,
    
    /// Plugin timeout
    pub timeout: i64,
    
    /// Maximum plugin memory usage
    pub max_memory_usage: u64,
    
    /// Sandbox configuration
    pub sandbox: SandboxConfig,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("./plugins"),
            hot_reload: true,
            scan_interval: 5, // 5 seconds
            timeout: 30, // 30 seconds
            max_memory_usage: 512 * 1024 * 1024, // 512MB
            sandbox: SandboxConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Enable sandboxing
    pub enabled: bool,
    
    /// Allowed system calls
    pub allowed_syscalls: Vec<String>,
    
    /// Network access
    pub network_access: bool,
    
    /// File system access
    pub filesystem_access: Vec<PathBuf>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_syscalls: vec![],
            network_access: true,
            filesystem_access: vec![PathBuf::from("./data")],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable monitoring
    pub enabled: bool,
    
    /// Metrics collection interval
    pub metrics_interval: i64,
    
    /// Prometheus metrics
    pub prometheus: PrometheusConfig,
    
    /// Jaeger tracing
    pub jaeger: JaegerConfig,
    
    /// Health check configuration
    pub health_check: HealthCheckConfig,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            metrics_interval: 10, // 10 seconds
            prometheus: PrometheusConfig::default(),
            jaeger: JaegerConfig::default(),
            health_check: HealthCheckConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusConfig {
    /// Enable Prometheus metrics
    pub enabled: bool,
    
    /// Metrics endpoint
    pub endpoint: String,
    
    /// Metrics port
    pub port: u16,
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: "/metrics".to_string(),
            port: 9090,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JaegerConfig {
    /// Enable Jaeger tracing
    pub enabled: bool,
    
    /// Jaeger agent endpoint
    pub agent_endpoint: String,
    
    /// Service name
    pub service_name: String,
    
    /// Sampling rate
    pub sampling_rate: f64,
}

impl Default for JaegerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            agent_endpoint: "http://localhost:14268/api/traces".to_string(),
            service_name: "horizon2".to_string(),
            sampling_rate: 0.1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check endpoint
    pub endpoint: String,
    
    /// Health check interval
    pub interval: i64,
    
    /// Health check timeout
    pub timeout: i64,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            endpoint: "/health".to_string(),
            interval: 30, // 30 seconds
            timeout: 5, // 5 seconds
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardingConfig {
    /// Enable sharding
    pub enabled: bool,
    
    /// Sharding strategy
    pub strategy: ShardingStrategy,
    
    /// Auto-scaling configuration
    pub auto_scaling: AutoScalingConfig,
    
    /// Shard replication factor
    pub replication_factor: u32,
    
    /// Consistent hashing configuration
    pub consistent_hashing: ConsistentHashingConfig,
}

impl Default for ShardingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            strategy: ShardingStrategy::Spatial,
            auto_scaling: AutoScalingConfig::default(),
            replication_factor: 1,
            consistent_hashing: ConsistentHashingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShardingStrategy {
    Spatial,
    Hash,
    Range,
    Custom { algorithm: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoScalingConfig {
    /// Enable auto-scaling
    pub enabled: bool,
    
    /// Scale up threshold (CPU %)
    pub scale_up_threshold: f64,
    
    /// Scale down threshold (CPU %)
    pub scale_down_threshold: f64,
    
    /// Minimum nodes
    pub min_nodes: u32,
    
    /// Maximum nodes
    pub max_nodes: u32,
    
    /// Cooldown period
    pub cooldown: i64,
}

impl Default for AutoScalingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            scale_up_threshold: 80.0,
            scale_down_threshold: 20.0,
            min_nodes: 1,
            max_nodes: 10,
            cooldown: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistentHashingConfig {
    /// Number of virtual nodes per physical node
    pub virtual_nodes: u32,
    
    /// Hash function
    pub hash_function: String,
}

impl Default for ConsistentHashingConfig {
    fn default() -> Self {
        Self {
            virtual_nodes: 150,
            hash_function: "sha256".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,
    
    /// Log format
    pub format: LogFormat,
    
    /// Log output
    pub output: LogOutput,
    
    /// Enable structured logging
    pub structured: bool,
    
    /// Log rotation
    pub rotation: LogRotationConfig,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: LogFormat::Pretty,
            output: LogOutput::Stdout,
            structured: true,
            rotation: LogRotationConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    Pretty,
    Json,
    Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LogOutput {
    Stdout,
    File { path: PathBuf },
    Syslog,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotationConfig {
    /// Enable log rotation
    pub enabled: bool,
    
    /// Maximum file size
    pub max_size: u64,
    
    /// Maximum age
    pub max_age: i64,
    
    /// Maximum files to keep
    pub max_files: u32,
    
    /// Compress rotated files
    pub compress: bool,
}

impl Default for LogRotationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size: 100 * 1024 * 1024, // 100MB
            max_age: 7 * 24 * 3600, // 7 days
            max_files: 10,
            compress: true,
        }
    }
}