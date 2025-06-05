use anyhow::{Context, Result};
use dashmap::DashMap;
use ring::digest::{Context as DigestContext, SHA256};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, BTreeMap};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock, mpsc};
use tracing::{info, warn, error, debug, instrument};
use uuid::Uuid;

use crate::config::ShardingConfig;
use crate::core::ServerEvent;
use crate::world::WorldManager;

/// Shard manager handles distributed world partitioning and load balancing
pub struct ShardManager {
    config: ShardingConfig,
    node_id: Uuid,
    world_manager: Arc<WorldManager>,
    event_sender: broadcast::Sender<ServerEvent>,
    
    // Shard management
    local_shards: Arc<DashMap<Uuid, LocalShard>>,
    shard_assignments: Arc<RwLock<HashMap<Uuid, ShardAssignment>>>,
    cluster_nodes: Arc<RwLock<HashMap<Uuid, ClusterNode>>>,
    
    // Consistent hashing
    hash_ring: Arc<RwLock<ConsistentHashRing>>,
    
    // Communication
    shard_communicator: Arc<ShardCommunicator>,
    
    // Load balancing
    load_balancer: Arc<LoadBalancer>,
    
    // Auto-scaling
    auto_scaler: Option<Arc<AutoScaler>>,
    
    running: Arc<RwLock<bool>>,
}

/// Local shard managed by this node
#[derive(Debug, Clone)]
pub struct LocalShard {
    pub id: Uuid,
    pub region_ids: Vec<Uuid>,
    pub bounds: ShardBounds,
    pub state: ShardState,
    pub players: Vec<Uuid>,
    pub object_count: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
    pub metrics: ShardMetrics,
}

/// Shard assignment information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardAssignment {
    pub shard_id: Uuid,
    pub node_id: Uuid,
    pub region_ids: Vec<Uuid>,
    pub priority: u32,
    pub assigned_at: chrono::DateTime<chrono::Utc>,
    pub status: ShardStatus,
}

/// Cluster node information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterNode {
    pub node_id: Uuid,
    pub address: String,
    pub port: u16,
    pub region: String,
    pub capacity: NodeCapacity,
    pub current_load: NodeLoad,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    pub status: NodeStatus,
    pub metadata: HashMap<String, String>,
}

/// Node capacity information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapacity {
    pub max_players: usize,
    pub max_regions: usize,
    pub max_objects: usize,
    pub cpu_cores: u32,
    pub memory_gb: u32,
    pub network_bandwidth_mbps: u32,
}

/// Current node load
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeLoad {
    pub current_players: usize,
    pub current_regions: usize,
    pub current_objects: usize,
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub network_usage: f64,
}

/// Shard bounds in 3D space
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardBounds {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
    pub min_z: f64,
    pub max_z: f64,
}

/// Shard state
#[derive(Debug, Clone, PartialEq)]
pub enum ShardState {
    Initializing,
    Active,
    Transferring,
    Draining,
    Inactive,
}

/// Shard status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShardStatus {
    Assigned,
    Migrating,
    Failed,
}

/// Node status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeStatus {
    Healthy,
    Degraded,
    Unavailable,
}

/// Shard metrics
#[derive(Debug, Clone, Default)]
pub struct ShardMetrics {
    pub tps: f64, // Transactions per second
    pub latency_ms: f64,
    pub memory_usage: u64,
    pub cpu_usage: f64,
    pub network_in_bps: u64,
    pub network_out_bps: u64,
}

/// Consistent hash ring for shard distribution
#[derive(Debug)]
pub struct ConsistentHashRing {
    ring: BTreeMap<u64, Uuid>, // hash -> node_id
    virtual_nodes: HashMap<Uuid, Vec<u64>>, // node_id -> hash values
    nodes: HashMap<Uuid, ClusterNode>,
}

/// Shard communication system
pub struct ShardCommunicator {
    message_queue: Arc<RwLock<Vec<ShardMessage>>>,
    pending_transfers: Arc<DashMap<Uuid, ShardTransfer>>,
}

/// Load balancer for shard distribution
pub struct LoadBalancer {
    strategy: LoadBalancingStrategy,
    thresholds: LoadBalancingThresholds,
}

/// Auto-scaler for dynamic shard management
pub struct AutoScaler {
    config: crate::config::AutoScalingConfig,
    last_scale_event: Arc<RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
}

/// Shard message for inter-node communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardMessage {
    pub id: Uuid,
    pub from_node: Uuid,
    pub to_node: Uuid,
    pub message_type: ShardMessageType,
    pub payload: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Types of shard messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShardMessageType {
    Heartbeat,
    ShardTransferRequest,
    ShardTransferResponse,
    PlayerMigration,
    StateSync,
    LoadReport,
    Custom(String),
}

/// Shard transfer request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardTransfer {
    pub transfer_id: Uuid,
    pub shard_id: Uuid,
    pub from_node: Uuid,
    pub to_node: Uuid,
    pub region_ids: Vec<Uuid>,
    pub player_ids: Vec<Uuid>,
    pub status: TransferStatus,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub estimated_completion: Option<chrono::DateTime<chrono::Utc>>,
}

/// Transfer status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransferStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

/// Load balancing strategy
#[derive(Debug, Clone)]
pub enum LoadBalancingStrategy {
    RoundRobin,
    LeastConnections,
    ResourceBased,
    GeographicProximity,
    Custom,
}

/// Load balancing thresholds
#[derive(Debug, Clone)]
pub struct LoadBalancingThresholds {
    pub max_cpu_usage: f64,
    pub max_memory_usage: f64,
    pub max_players_per_node: usize,
    pub max_regions_per_node: usize,
}

impl Default for LoadBalancingThresholds {
    fn default() -> Self {
        Self {
            max_cpu_usage: 80.0,
            max_memory_usage: 85.0,
            max_players_per_node: 5000,
            max_regions_per_node: 100,
        }
    }
}

impl ShardManager {
    /// Create new shard manager
    #[instrument(skip_all)]
    pub async fn new(
        config: &ShardingConfig,
        node_id: Uuid,
        world_manager: Arc<WorldManager>,
        event_sender: broadcast::Sender<ServerEvent>,
    ) -> Result<Self> {
        info!("⚡ Initializing shard manager for node: {}", node_id);
        
        let shard_communicator = Arc::new(ShardCommunicator::new());
        let load_balancer = Arc::new(LoadBalancer::new(&config));
        let auto_scaler = if config.auto_scaling.enabled {
            Some(Arc::new(AutoScaler::new(&config.auto_scaling)))
        } else {
            None
        };
        
        let manager = Self {
            config: config.clone(),
            node_id,
            world_manager,
            event_sender,
            local_shards: Arc::new(DashMap::new()),
            shard_assignments: Arc::new(RwLock::new(HashMap::new())),
            cluster_nodes: Arc::new(RwLock::new(HashMap::new())),
            hash_ring: Arc::new(RwLock::new(ConsistentHashRing::new())),
            shard_communicator,
            load_balancer,
            auto_scaler,
            running: Arc::new(RwLock::new(false)),
        };
        
        // Initialize local node in cluster
        manager.register_local_node().await?;
        
        // Load existing shard assignments
        manager.load_shard_assignments().await?;
        
        info!("✅ Shard manager initialized");
        Ok(manager)
    }
    
    /// Register this node in the cluster
    async fn register_local_node(&self) -> Result<()> {
        let local_node = ClusterNode {
            node_id: self.node_id,
            address: "127.0.0.1".to_string(), // Would get actual address
            port: 3000, // Would get actual port
            region: "default".to_string(),
            capacity: NodeCapacity {
                max_players: 10000,
                max_regions: 1000,
                max_objects: 100000,
                cpu_cores: num_cpus::get() as u32,
                memory_gb: 16, // Would detect actual memory
                network_bandwidth_mbps: 1000,
            },
            current_load: NodeLoad {
                current_players: 0,
                current_regions: 0,
                current_objects: 0,
                cpu_usage: 0.0,
                memory_usage: 0.0,
                network_usage: 0.0,
            },
            last_heartbeat: chrono::Utc::now(),
            status: NodeStatus::Healthy,
            metadata: HashMap::new(),
        };
        
        self.cluster_nodes.write().await.insert(self.node_id, local_node.clone());
        self.hash_ring.write().await.add_node(local_node);
        
        info!("📡 Registered local node in cluster");
        Ok(())
    }
    
    /// Load existing shard assignments from storage
    async fn load_shard_assignments(&self) -> Result<()> {
        debug!("Loading shard assignments from storage");
        
        // In a real implementation, this would load from the database
        // For now, we'll create a default shard if none exist
        
        if self.local_shards.is_empty() {
            self.create_initial_shard().await?;
        }
        
        Ok(())
    }
    
    /// Create initial shard for this node
    async fn create_initial_shard(&self) -> Result<()> {
        let shard_id = Uuid::new_v4();
        
        let shard = LocalShard {
            id: shard_id,
            region_ids: vec![], // Will be populated as regions are assigned
            bounds: ShardBounds {
                min_x: -1000.0,
                max_x: 1000.0,
                min_y: -1000.0,
                max_y: 1000.0,
                min_z: -1000.0,
                max_z: 1000.0,
            },
            state: ShardState::Active,
            players: vec![],
            object_count: 0,
            created_at: chrono::Utc::now(),
            last_updated: chrono::Utc::now(),
            metrics: ShardMetrics::default(),
        };
        
        self.local_shards.insert(shard_id, shard);
        
        let assignment = ShardAssignment {
            shard_id,
            node_id: self.node_id,
            region_ids: vec![],
            priority: 100,
            assigned_at: chrono::Utc::now(),
            status: ShardStatus::Assigned,
        };
        
        self.shard_assignments.write().await.insert(shard_id, assignment);
        
        info!("🎯 Created initial shard: {}", shard_id);
        Ok(())
    }
    
    /// Run the shard manager
    pub async fn run(&self) -> Result<()> {
        info!("⚡ Starting shard manager");
        *self.running.write().await = true;
        
        // Start subsystem tasks
        let heartbeat_task = self.start_heartbeat_system();
        let load_monitoring_task = self.start_load_monitoring();
        let shard_maintenance_task = self.start_shard_maintenance();
        let communication_task = self.start_communication_system();
        
        let auto_scaling_task = if let Some(ref auto_scaler) = self.auto_scaler {
            Some(self.start_auto_scaling(auto_scaler.clone()))
        } else {
            None
        };
        
        // Wait for shutdown signal
        tokio::select! {
            _ = heartbeat_task => info!("Heartbeat system stopped"),
            _ = load_monitoring_task => info!("Load monitoring stopped"),
            _ = shard_maintenance_task => info!("Shard maintenance stopped"),
            _ = communication_task => info!("Communication system stopped"),
            _ = async {
                if let Some(task) = auto_scaling_task {
                    task.await
                } else {
                    std::future::pending().await
                }
            } => info!("Auto-scaling stopped"),
        }
        
        Ok(())
    }
    
    /// Start heartbeat system
    fn start_heartbeat_system(&self) -> tokio::task::JoinHandle<()> {
        let cluster_nodes = self.cluster_nodes.clone();
        let node_id = self.node_id;
        let running = self.running.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            
            while *running.read().await {
                interval.tick().await;
                
                // Update local node heartbeat
                if let Some(mut node) = cluster_nodes.write().await.get_mut(&node_id) {
                    node.last_heartbeat = chrono::Utc::now();
                }
                
                // Check for dead nodes
                let now = chrono::Utc::now();
                let mut dead_nodes = Vec::new();
                
                {
                    let nodes = cluster_nodes.read().await;
                    for (node_id, node) in nodes.iter() {
                        let time_since_heartbeat = now - node.last_heartbeat;
                        if time_since_heartbeat > chrono::Duration::seconds(120) {
                            dead_nodes.push(*node_id);
                        }
                    }
                }
                
                // Remove dead nodes
                if !dead_nodes.is_empty() {
                    let mut nodes = cluster_nodes.write().await;
                    for dead_node in dead_nodes {
                        if let Some(node) = nodes.remove(&dead_node) {
                            warn!("💀 Removed dead node from cluster: {}", node.node_id);
                        }
                    }
                }
            }
        })
    }
    
    /// Start load monitoring
    fn start_load_monitoring(&self) -> tokio::task::JoinHandle<()> {
        let cluster_nodes = self.cluster_nodes.clone();
        let local_shards = self.local_shards.clone();
        let node_id = self.node_id;
        let running = self.running.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            
            while *running.read().await {
                interval.tick().await;
                
                // Update local node load
                let current_load = calculate_current_load(&local_shards).await;
                
                if let Some(mut node) = cluster_nodes.write().await.get_mut(&node_id) {
                    node.current_load = current_load;
                }
            }
        })
    }
    
    /// Start shard maintenance
    fn start_shard_maintenance(&self) -> tokio::task::JoinHandle<()> {
        let local_shards = self.local_shards.clone();
        let running = self.running.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            
            while *running.read().await {
                interval.tick().await;
                
                // Update shard metrics
                for mut shard_ref in local_shards.iter_mut() {
                    update_shard_metrics(&mut shard_ref).await;
                }
                
                // Cleanup inactive shards
                let mut inactive_shards = Vec::new();
                for shard_ref in local_shards.iter() {
                    if shard_ref.state == ShardState::Inactive {
                        inactive_shards.push(shard_ref.id);
                    }
                }
                
                for shard_id in inactive_shards {
                    local_shards.remove(&shard_id);
                    debug!("🧹 Cleaned up inactive shard: {}", shard_id);
                }
            }
        })
    }
    
    /// Start communication system
    fn start_communication_system(&self) -> tokio::task::JoinHandle<()> {
        let communicator = self.shard_communicator.clone();
        let running = self.running.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
            
            while *running.read().await {
                interval.tick().await;
                
                // Process pending messages
                communicator.process_messages().await;
            }
        })
    }
    
    /// Start auto-scaling system
    fn start_auto_scaling(&self, auto_scaler: Arc<AutoScaler>) -> tokio::task::JoinHandle<()> {
        let cluster_nodes = self.cluster_nodes.clone();
        let running = self.running.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            
            while *running.read().await {
                interval.tick().await;
                
                auto_scaler.evaluate_scaling(&cluster_nodes).await;
            }
        })
    }
    
    /// Assign a region to a shard
    #[instrument(skip(self))]
    pub async fn assign_region_to_shard(&self, region_id: Uuid, shard_id: Uuid) -> Result<()> {
        if let Some(mut shard) = self.local_shards.get_mut(&shard_id) {
            if !shard.region_ids.contains(&region_id) {
                shard.region_ids.push(region_id);
                shard.last_updated = chrono::Utc::now();
                
                debug!("🎯 Assigned region {} to shard {}", region_id, shard_id);
            }
        }
        
        Ok(())
    }
    
    /// Find the best shard for a new region
    pub async fn find_best_shard_for_region(&self, region_bounds: &ShardBounds) -> Option<Uuid> {
        let shards = self.local_shards.clone();
        let mut best_shard = None;
        let mut best_score = f64::MIN;
        
        for shard_ref in shards.iter() {
            let score = calculate_shard_affinity(&shard_ref, region_bounds);
            if score > best_score {
                best_score = score;
                best_shard = Some(shard_ref.id);
            }
        }
        
        best_shard
    }
    
    /// Transfer a shard to another node
    #[instrument(skip(self))]
    pub async fn transfer_shard(&self, shard_id: Uuid, target_node: Uuid) -> Result<()> {
        if let Some(shard) = self.local_shards.get(&shard_id) {
            let transfer = ShardTransfer {
                transfer_id: Uuid::new_v4(),
                shard_id,
                from_node: self.node_id,
                to_node: target_node,
                region_ids: shard.region_ids.clone(),
                player_ids: shard.players.clone(),
                status: TransferStatus::Pending,
                started_at: chrono::Utc::now(),
                estimated_completion: None,
            };
            
            self.shard_communicator.pending_transfers.insert(transfer.transfer_id, transfer.clone());
            
            // Send transfer request message
            let message = ShardMessage {
                id: Uuid::new_v4(),
                from_node: self.node_id,
                to_node: target_node,
                message_type: ShardMessageType::ShardTransferRequest,
                payload: serde_json::to_value(&transfer)?,
                timestamp: chrono::Utc::now(),
            };
            
            self.shard_communicator.send_message(message).await;
            
            info!("📦 Initiated shard transfer: {} -> {}", shard_id, target_node);
        }
        
        Ok(())
    }
    
    /// Get shard statistics
    pub async fn get_shard_stats(&self) -> ShardStats {
        let local_shard_count = self.local_shards.len();
        let total_assignments = self.shard_assignments.read().await.len();
        let cluster_node_count = self.cluster_nodes.read().await.len();
        
        let mut total_players = 0;
        let mut total_regions = 0;
        
        for shard_ref in self.local_shards.iter() {
            total_players += shard_ref.players.len();
            total_regions += shard_ref.region_ids.len();
        }
        
        ShardStats {
            local_shards: local_shard_count,
            total_assignments,
            cluster_nodes: cluster_node_count,
            total_players,
            total_regions,
            pending_transfers: self.shard_communicator.pending_transfers.len(),
        }
    }
    
    /// Shutdown shard manager
    #[instrument(skip(self))]
    pub async fn shutdown(&self) -> Result<()> {
        info!("🛑 Shutting down shard manager");
        *self.running.write().await = false;
        
        // Save current shard state
        for shard_ref in self.local_shards.iter() {
            debug!("💾 Saving shard state: {}", shard_ref.id);
            // In a real implementation, save shard state to storage
        }
        
        // Notify other nodes of shutdown
        let shutdown_message = ShardMessage {
            id: Uuid::new_v4(),
            from_node: self.node_id,
            to_node: Uuid::nil(), // Broadcast
            message_type: ShardMessageType::Custom("node_shutdown".to_string()),
            payload: serde_json::json!({"node_id": self.node_id}),
            timestamp: chrono::Utc::now(),
        };
        
        self.shard_communicator.send_message(shutdown_message).await;
        
        info!("✅ Shard manager shutdown complete");
        Ok(())
    }
}

/// Shard statistics
#[derive(Debug, Clone, Serialize)]
pub struct ShardStats {
    pub local_shards: usize,
    pub total_assignments: usize,
    pub cluster_nodes: usize,
    pub total_players: usize,
    pub total_regions: usize,
    pub pending_transfers: usize,
}

impl ConsistentHashRing {
    fn new() -> Self {
        Self {
            ring: BTreeMap::new(),
            virtual_nodes: HashMap::new(),
            nodes: HashMap::new(),
        }
    }
    
    fn add_node(&mut self, node: ClusterNode) {
        let virtual_node_count = 150; // Default from config
        let mut hashes = Vec::new();
        
        for i in 0..virtual_node_count {
            let key = format!("{}:{}", node.node_id, i);
            let hash = self.hash_key(&key);
            
            self.ring.insert(hash, node.node_id);
            hashes.push(hash);
        }
        
        self.virtual_nodes.insert(node.node_id, hashes);
        self.nodes.insert(node.node_id, node);
    }
    
    fn remove_node(&mut self, node_id: Uuid) {
        if let Some(hashes) = self.virtual_nodes.remove(&node_id) {
            for hash in hashes {
                self.ring.remove(&hash);
            }
        }
        self.nodes.remove(&node_id);
    }
    
    fn get_node(&self, key: &str) -> Option<Uuid> {
        if self.ring.is_empty() {
            return None;
        }
        
        let hash = self.hash_key(key);
        
        // Find the first node with hash >= our hash
        if let Some((_, &node_id)) = self.ring.range(hash..).next() {
            Some(node_id)
        } else {
            // Wrap around to the first node
            self.ring.values().next().copied()
        }
    }
    
    fn hash_key(&self, key: &str) -> u64 {
        let mut context = DigestContext::new(&SHA256);
        context.update(key.as_bytes());
        let digest = context.finish();
        
        // Convert first 8 bytes to u64
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&digest.as_ref()[..8]);
        u64::from_be_bytes(bytes)
    }
}

impl ShardCommunicator {
    fn new() -> Self {
        Self {
            message_queue: Arc::new(RwLock::new(Vec::new())),
            pending_transfers: Arc::new(DashMap::new()),
        }
    }
    
    async fn send_message(&self, message: ShardMessage) {
        self.message_queue.write().await.push(message);
    }
    
    async fn process_messages(&self) {
        let mut queue = self.message_queue.write().await;
        let messages = std::mem::take(&mut *queue);
        drop(queue);
        
        for message in messages {
            self.handle_message(message).await;
        }
    }
    
    async fn handle_message(&self, message: ShardMessage) {
        match message.message_type {
            ShardMessageType::Heartbeat => {
                debug!("💓 Received heartbeat from {}", message.from_node);
            }
            
            ShardMessageType::ShardTransferRequest => {
                debug!("📦 Received shard transfer request from {}", message.from_node);
                // Handle incoming transfer request
            }
            
            ShardMessageType::ShardTransferResponse => {
                debug!("📬 Received shard transfer response from {}", message.from_node);
                // Handle transfer response
            }
            
            ShardMessageType::PlayerMigration => {
                debug!("🏃 Received player migration from {}", message.from_node);
                // Handle player migration
            }
            
            ShardMessageType::StateSync => {
                debug!("🔄 Received state sync from {}", message.from_node);
                // Handle state synchronization
            }
            
            ShardMessageType::LoadReport => {
                debug!("📊 Received load report from {}", message.from_node);
                // Update node load information
            }
            
            ShardMessageType::Custom(ref msg_type) => {
                debug!("🔧 Received custom message '{}' from {}", msg_type, message.from_node);
                // Handle custom message types
            }
        }
    }
}

impl LoadBalancer {
    fn new(config: &ShardingConfig) -> Self {
        Self {
            strategy: LoadBalancingStrategy::ResourceBased,
            thresholds: LoadBalancingThresholds::default(),
        }
    }
}

impl AutoScaler {
    fn new(config: &crate::config::AutoScalingConfig) -> Self {
        Self {
            config: config.clone(),
            last_scale_event: Arc::new(RwLock::new(None)),
        }
    }
    
    async fn evaluate_scaling(&self, cluster_nodes: &Arc<RwLock<HashMap<Uuid, ClusterNode>>>) {
        let nodes = cluster_nodes.read().await;
        
        if nodes.is_empty() {
            return;
        }
        
        let mut total_cpu = 0.0;
        let mut node_count = 0;
        
        for node in nodes.values() {
            total_cpu += node.current_load.cpu_usage;
            node_count += 1;
        }
        
        let avg_cpu = total_cpu / node_count as f64;
        
        // Check if we need to scale up
        if avg_cpu > self.config.scale_up_threshold && node_count < self.config.max_nodes as usize {
            if self.can_scale().await {
                info!("📈 Auto-scaling up: CPU usage {:.1}%", avg_cpu);
                self.scale_up().await;
            }
        }
        
        // Check if we can scale down
        if avg_cpu < self.config.scale_down_threshold && node_count > self.config.min_nodes as usize {
            if self.can_scale().await {
                info!("📉 Auto-scaling down: CPU usage {:.1}%", avg_cpu);
                self.scale_down().await;
            }
        }
    }
    
    async fn can_scale(&self) -> bool {
        let last_scale = self.last_scale_event.read().await;
        
        if let Some(last_time) = *last_scale {
            let std_duration = std::time::Duration::from_secs(self.config.cooldown as u64);
            let cooldown_end = last_time + chrono::Duration::from_std(std_duration).unwrap();
            chrono::Utc::now() > cooldown_end
        } else {
            true
        }
    }
    
    async fn scale_up(&self) {
        *self.last_scale_event.write().await = Some(chrono::Utc::now());
        // In a real implementation, this would create new server instances
        debug!("🆙 Scaling up cluster");
    }
    
    async fn scale_down(&self) {
        *self.last_scale_event.write().await = Some(chrono::Utc::now());
        // In a real implementation, this would remove server instances
        debug!("⬇️ Scaling down cluster");
    }
}

/// Helper functions

async fn calculate_current_load(local_shards: &DashMap<Uuid, LocalShard>) -> NodeLoad {
    let mut total_players = 0;
    let mut total_regions = 0;
    let mut total_objects = 0;
    
    for shard_ref in local_shards.iter() {
        total_players += shard_ref.players.len();
        total_regions += shard_ref.region_ids.len();
        total_objects += shard_ref.object_count;
    }
    
    NodeLoad {
        current_players: total_players,
        current_regions: total_regions,
        current_objects: total_objects,
        cpu_usage: 0.0, // Would get from system monitor
        memory_usage: 0.0,
        network_usage: 0.0,
    }
}

async fn update_shard_metrics(shard: &mut LocalShard) {
    // Update shard metrics
    shard.metrics.tps = 0.0; // Would calculate from actual transactions
    shard.metrics.latency_ms = 0.0;
    shard.metrics.memory_usage = 0;
    shard.metrics.cpu_usage = 0.0;
    shard.last_updated = chrono::Utc::now();
}

fn calculate_shard_affinity(shard: &LocalShard, region_bounds: &ShardBounds) -> f64 {
    // Calculate spatial affinity between shard and region
    // This is a simplified calculation - real implementation would be more sophisticated
    
    let shard_center_x = (shard.bounds.min_x + shard.bounds.max_x) / 2.0;
    let shard_center_y = (shard.bounds.min_y + shard.bounds.max_y) / 2.0;
    let shard_center_z = (shard.bounds.min_z + shard.bounds.max_z) / 2.0;
    
    let region_center_x = (region_bounds.min_x + region_bounds.max_x) / 2.0;
    let region_center_y = (region_bounds.min_y + region_bounds.max_y) / 2.0;
    let region_center_z = (region_bounds.min_z + region_bounds.max_z) / 2.0;
    
    let distance = ((shard_center_x - region_center_x).powi(2) +
                   (shard_center_y - region_center_y).powi(2) +
                   (shard_center_z - region_center_z).powi(2)).sqrt();
    
    // Closer regions have higher affinity
    1.0 / (1.0 + distance)
}