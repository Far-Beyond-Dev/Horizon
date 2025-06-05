use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::{info, warn, error, debug, instrument};
use uuid::Uuid;

use crate::config::HorizonConfig;
use crate::network::NetworkManager;
use crate::plugins::PluginManager;
use crate::world::WorldManager;
use crate::sharding::ShardManager;

/// Main server instance that coordinates all subsystems
pub struct HorizonServer {
    config: HorizonConfig,
    node_id: Uuid,
    state: Arc<RwLock<ServerState>>,
    
    // Core managers
    network_manager: Arc<NetworkManager>,
    plugin_manager: Arc<PluginManager>,
    world_manager: Arc<WorldManager>,
    shard_manager: Arc<ShardManager>,
    
    // Communication channels
    event_sender: broadcast::Sender<ServerEvent>,
    command_sender: mpsc::UnboundedSender<ServerCommand>,
    command_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<ServerCommand>>>>,
}

#[derive(Debug, Clone)]
pub enum ServerState {
    Starting,
    Running,
    Degraded { reason: String },
    Stopping,
    Stopped,
}

#[derive(Debug, Clone)]
pub enum ServerEvent {
    PlayerConnected { player_id: Uuid, session_id: String },
    PlayerDisconnected { player_id: Uuid, reason: String },
    WorldUpdate { region_id: Uuid, update_type: String },
    PluginLoaded { plugin_name: String },
    PluginUnloaded { plugin_name: String },
    ShardTransfer { player_id: Uuid, from_shard: Uuid, to_shard: Uuid },
    SystemAlert { level: AlertLevel, message: String },
}

#[derive(Debug, Clone)]
pub enum AlertLevel {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug)]
pub enum ServerCommand {
    Shutdown,
    ReloadConfig,
    ReloadPlugin { name: String },
    ScaleWorkers { count: usize },
    CreateShard { region_id: Uuid },
    TransferShard { shard_id: Uuid, target_node: Uuid },
}

impl HorizonServer {
    /// Create a new Horizon server instance
    #[instrument(skip(config))]
    pub async fn new(config: HorizonConfig) -> Result<Self> {
        info!("🔧 Initializing Horizon Server...");
        
        let node_id = Uuid::parse_str(&config.server.node_id)
            .unwrap_or_else(|_| Uuid::new_v4());
        
        info!("🏷️  Node ID: {}", node_id);
        
        // Create communication channels
        let (event_sender, _) = broadcast::channel(10000);
        let (command_sender, command_receiver) = mpsc::unbounded_channel();
        
        // Initialize plugin system first (provides core services)
        info!("🔌 Initializing plugin manager...");
        let plugin_manager = Arc::new(
            PluginManager::new(&config.plugins).await
                .context("Failed to initialize plugin manager")?
        );
        
        // Initialize world management
        info!("🌍 Initializing world manager...");
        let world_manager = Arc::new(
            WorldManager::new(&config.world).await
                .context("Failed to initialize world manager")?
        );
        
        // Initialize shard management
        info!("⚡ Initializing shard manager...");
        let shard_manager = Arc::new(
            ShardManager::new(
                &config.sharding,
                node_id,
                world_manager.clone(),
                event_sender.clone(),
            ).await
                .context("Failed to initialize shard manager")?
        );
        
        // Initialize network layer (depends on all other systems)
        info!("🌐 Initializing network manager...");
        let network_manager = Arc::new(
            NetworkManager::new(
                &config.network,
                world_manager.clone(),
                plugin_manager.clone(),
                shard_manager.clone(),
                event_sender.clone(),
            ).await
                .context("Failed to initialize network manager")?
        );
        
        let server = Self {
            config,
            node_id,
            state: Arc::new(RwLock::new(ServerState::Starting)),
            network_manager,
            plugin_manager,
            world_manager,
            shard_manager,
            event_sender,
            command_sender,
            command_receiver: Arc::new(RwLock::new(Some(command_receiver))),
        };
        
        info!("✅ Horizon Server initialized successfully");
        Ok(server)
    }
    
    /// Run the server
    #[instrument(skip(self))]
    pub async fn run(self) -> Result<()> {
        info!("🚀 Starting Horizon Server...");
        
        // Update state to running
        *self.state.write().await = ServerState::Running;
        
        // Load and start plugins first (they provide core services)
        self.plugin_manager.load_all_plugins().await
            .context("Failed to load plugins")?;
        
        // Check if essential services are available
        self.check_essential_services().await?;
        
        // Start all subsystem tasks
        let tasks = self.start_subsystems().await?;
        
        // Start the command handler
        let command_receiver = self.command_receiver.write().await.take()
            .expect("Command receiver should be available");
        // let command_task = self.spawn_command_handler(command_receiver);

        
        info!("✅ Horizon Server is now running on {}", self.config.server.bind_address);
        
        // Wait for any task to complete (which would indicate shutdown or error)
        tokio::select! {
            _ = futures::future::join_all(tasks) => {
                info!("All subsystem tasks completed");
            }
//            _ = event_task => {
//                info!("Event monitor completed");
//            }
        }
        
        // Update state to stopping
        *self.state.write().await = ServerState::Stopping;
        
        // Graceful shutdown
        self.shutdown().await?;
        
        // Update state to stopped
        *self.state.write().await = ServerState::Stopped;
        
        info!("👋 Horizon Server stopped gracefully");
        Ok(())
    }
    
    /// Check if essential services are available from plugins
    async fn check_essential_services(&self) -> Result<()> {
        let storage_service = self.plugin_manager.get_storage_service().await;
        let auth_service = self.plugin_manager.get_auth_service().await;
        
        if storage_service.is_none() {
            warn!("⚠️  No storage service plugin loaded - world persistence disabled");
        } else {
            info!("📦 Storage service available from plugins");
        }
        
        if auth_service.is_none() {
            warn!("⚠️  No auth service plugin loaded - authentication disabled");
        } else {
            info!("🔐 Auth service available from plugins");
        }
        
        Ok(())
    }
    
    /// Start all subsystem tasks
    async fn start_subsystems(&self) -> Result<Vec<tokio::task::JoinHandle<()>>> {
        let mut tasks = Vec::new();
        
        // Start network manager
        tasks.push(tokio::spawn({
            let network_manager = self.network_manager.clone();
            async move {
                if let Err(e) = network_manager.run().await {
                    error!("Network manager error: {}", e);
                }
            }
        }));
        
        // Start world manager
        tasks.push(tokio::spawn({
            let world_manager = self.world_manager.clone();
            async move {
                if let Err(e) = world_manager.run().await {
                    error!("World manager error: {}", e);
                }
            }
        }));
        
        // Start shard manager
        tasks.push(tokio::spawn({
            let shard_manager = self.shard_manager.clone();
            async move {
                if let Err(e) = shard_manager.run().await {
                    error!("Shard manager error: {}", e);
                }
            }
        }));

        Ok(tasks)
    }
    
    /// Spawn command handler task
    // fn spawn_command_handler(
    //     &self, 
    //     mut receiver: mpsc::UnboundedReceiver<ServerCommand>
    // ) -> tokio::task::JoinHandle<()> {
    //     let plugin_manager = self.plugin_manager.clone();
    //     let state = self.state.clone();
    //     
    //     tokio::spawn(async move {
    //         while let Some(command) = receiver.recv().await {
    //             match command {
    //                 ServerCommand::Shutdown => {
    //                     info!("Received shutdown command");
    //                     break;
    //                 }
                    
    //                 ServerCommand::ReloadConfig => {
    //                     info!("Received reload config command");
    //                     // Implementation would reload configuration
    //                 }
                    
    //                 ServerCommand::ReloadPlugin { name } => {
    //                     info!("Received reload plugin command: {}", name);
    //                     if let Err(e) = plugin_manager.reload_plugin(&name).await {
    //                         error!("Failed to reload plugin {}: {}", name, e);
    //                     }
    //                 }
                    
    //                 ServerCommand::ScaleWorkers { count } => {
    //                     info!("Received scale workers command: {}", count);
    //                     // Implementation would scale worker threads
    //                 }
                    
    //                 ServerCommand::CreateShard { region_id } => {
    //                     info!("Received create shard command: {}", region_id);
    //                     // Implementation would create new shard
    //                 }
                    
    //                 ServerCommand::TransferShard { shard_id, target_node } => {
    //                     info!("Received transfer shard command: {} -> {}", shard_id, target_node);
    //                     // Implementation would transfer shard to another node
    //                 }
    //             }
    //         }
            
    //         info!("Command handler shutting down");
    //     })
    // }
    
    /// Graceful shutdown
    #[instrument(skip(self))]
    async fn shutdown(&self) -> Result<()> {
        info!("🛑 Starting graceful shutdown...");
        
        // Stop accepting new connections
        self.network_manager.stop_accepting().await?;
        
        // Notify all players of shutdown
        self.network_manager.broadcast_shutdown_notice().await?;
        
        // Give players time to disconnect gracefully
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        
        // Stop world manager (will persist data if storage service available)
        self.world_manager.shutdown().await?;
        
        // Stop shard manager
        self.shard_manager.shutdown().await?;
        
        // Stop plugin manager last (it provides services to other systems)
        self.plugin_manager.shutdown().await?;
                
        info!("✅ Graceful shutdown completed");
        Ok(())
    }
    
    /// Get the current server state
    pub async fn get_state(&self) -> ServerState {
        self.state.read().await.clone()
    }
    
    /// Send a command to the server
    pub fn send_command(&self, command: ServerCommand) -> Result<()> {
        self.command_sender.send(command)
            .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))
    }
    
    /// Subscribe to server events
    pub fn subscribe_events(&self) -> broadcast::Receiver<ServerEvent> {
        self.event_sender.subscribe()
    }
    
    /// Get server metrics
    pub async fn get_metrics(&self) -> Result<ServerMetrics> {
        Ok(ServerMetrics {
            node_id: self.node_id,
            state: self.get_state().await,
            connected_players: self.network_manager.get_player_count().await,
            active_regions: self.world_manager.get_region_count().await,
            loaded_plugins: self.plugin_manager.get_plugin_count().await,
            services_available: ServiceStatus {
                storage: self.plugin_manager.get_storage_service().await.is_some(),
                auth: self.plugin_manager.get_auth_service().await.is_some(),
            },
        })
    }
}

#[derive(Debug, Clone)]
pub struct ServerMetrics {
    pub node_id: Uuid,
    pub state: ServerState,
    pub connected_players: usize,
    pub active_regions: usize,
    pub loaded_plugins: usize,
    pub services_available: ServiceStatus,
}

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub storage: bool,
    pub auth: bool,
}