//-----------------------------------------------------------------------------
// Multi-Threaded SocketIO Server Implementation
//   - Manages multiple threads for handling player connections
//   - Uses SocketIO for real-time communication with clients
//   - Configurable server settings
//   - Logging with Horizon Logger
//   - Server state management with lazy_static
//   - Horizon Server and Horizon Thread structs
//   - Socket event handlers for message and ack events
//   - Server startup with axum web framework
//   - Server configuration with config module
//
//-----------------------------------------------------------------------------
//   Written by: Tristan James Poland, and Caznix
//-----------------------------------------------------------------------------

use crate::LOGGER;
use anyhow::{Context, Result};
use axum::{routing::get, Router};
use config::ServerConfig;
use horizon_data_types::Player;
use horizon_logger::{log_debug, log_error, log_info};
use horizon_plugin_api::{LoadedPlugin};
use parking_lot::RwLock;
use socketioxide::{
    extract::{AckSender, Data, SocketRef},
    SocketIo,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use plugin_api::*;
pub mod config;
mod event_rep;
use lazy_static::lazy_static;

lazy_static! {
    static ref SERVER: Server = Server::new().unwrap();
}

// Server state management

//-----------------------------------------------------------------------------
// Horizon Server Struct
//-----------------------------------------------------------------------------

#[allow(dead_code)]
pub struct HorizonServer {
    config: ServerConfig,
    threads: RwLock<Vec<Arc<HorizonThread>>>,
}

struct Server {
    instance: Arc<RwLock<HorizonServer>>,
}

impl Server {
    fn new() -> Result<Self> {
        Ok(Self {
            instance: Arc::new(RwLock::new(HorizonServer::new()?)),
        })
    }
    fn get_instance(&self) -> Arc<RwLock<HorizonServer>> {
        Arc::clone(&self.instance)
    }
}

impl HorizonServer {
    fn new() -> Result<Self> {
        Ok(Self {
            config: *config::server_config()?,
            threads: RwLock::new(Vec::new()),
        })
    }

    fn spawn_thread(&self) -> Result<usize> {
        let thread = HorizonThread::new();
        let thread_id = {
            let mut threads = self.threads.write();
            threads.push(thread.into());
            let id = threads.len() - 1;
            id
        };

        Ok(thread_id)
    }
}

//-----------------------------------------------------------------------------
// Horizon Thread Structhorizon_plugin_api::Plugin
//-----------------------------------------------------------------------------

#[allow(dead_code)]
struct HorizonThread {
    players: Mutex<Vec<Player>>,
    plugins: HashMap<String, LoadedPlugin>,
    handle: tokio::task::JoinHandle<()>,
}

#[allow(dead_code, unused_variables)]
impl HorizonThread {
    fn new() -> Self {
        let mut plugin_manager = plugin_api::PluginManager::new();
        let plugins = plugin_manager.load_all();

        plugins.iter().for_each(|(name, plugin)| {
            log_info!(LOGGER, "PLUGIN", "Loaded plugin: {}", name);
        });
        Self {
            players: Mutex::new(Vec::new()),
            plugins,
            handle: tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }),
        }
    }

    #[allow(dead_code)]
    fn id(&self) -> usize {
        self.players
            .try_lock()
            .map(|players| players.len())
            .unwrap_or(usize::MAX)
    }

    async fn add_player(&self, player: Player) -> Result<()> {
        let mut players = self.players.lock().await;
        players.push(player);
        Ok(())
    }

    // async fn remove_player(&self, player_id: &str) -> Result<bool> {
    //     let mut players = self.players.lock().await;
    //     if let Some(pos) = players.iter().position(|p| p.id == player_id) {
    //         players.remove(pos);
    //         Ok(true)
    //     } else {
    //         Ok(false)
    //     }
    // }
}

//-----------------------------------------------------------------------------
// Socket event handlers
//-----------------------------------------------------------------------------
async fn handle_socket_message(socket: SocketRef, Data(data): Data<serde_json::Value>) {
    log_debug!(LOGGER, "SOCKET EVENT", "Received message");
    if let Err(e) = socket.emit("message-back", &data) {
        log_error!(LOGGER, "SOCKET EVENT", "Failed to send message back: {}", e);
    }
}

async fn handle_socket_ack(Data(data): Data<serde_json::Value>, ack: AckSender) {
    log_debug!(LOGGER, "SOCKET EVENT", "Received message with ack");
    if let Err(e) = ack.send(&data) {
        log_error!(LOGGER, "SOCKET EVENT", "Failed to send ack: {}", e);
    }
}

#[allow(dead_code, unused_must_use, unused_variables)]
fn on_connect(socket: SocketRef, Data(data): Data<serde_json::Value>) {
    //socket.on("connect", |socket: SocketRef, _| {
    log_info!(LOGGER, "SOCKET NET", "New connection from {}", socket.id);
    //});

    if let Err(e) = socket.emit("auth", &data) {
        log_error!(LOGGER, "SOCKET NET", "Failed to send auth: {}", e);
        return;
    }

    // TODO: Implement proper thread management via round robin
    let threadid = 0;

    let server_instance = SERVER.get_instance();
    let server_instance_read = server_instance.read();
    let threads = server_instance_read.threads.read();

    socket.on("message", handle_socket_message);
    socket.on("message-with-ack", handle_socket_ack);

    let player = horizon_data_types::Player::new(socket.clone(), Uuid::new_v4());

    let target_thread = Arc::clone(&threads[threadid]);
    target_thread.add_player(player.clone());

    let player_arc: Arc<horizon_data_types::Player> = Arc::new(player);

    // let casted_struct = plugin_api::get_plugin!(unreal_adapter_horizon, target_thread.plugins);

    // casted_struct.player_joined(socket, player_arc);
}

//-----------------------------------------------------------------------------
// Server startup
//-----------------------------------------------------------------------------

#[allow(dead_code, unused_variables)]
pub async fn start() -> anyhow::Result<()> {
    let start_time = std::time::Instant::now();

    let (layer, io) = SocketIo::new_layer();
    // Initialize server state so we can spawn threads

    let thread_count = config::SERVER_CONFIG
        .get()
        .map(|config| config.num_thread_pools)
        .unwrap_or_default();

    let thread_count = 32;

    println!("Preparing to start {} threads", thread_count);
    // Start 10 threads initially for handling player connections

    //let handles = Vec::new();

    let handles = Arc::new(Mutex::new(Vec::new()));
    let server_instance = &SERVER.get_instance();
    let spawn_futures: Vec<_> = (0..thread_count)
        .map(|_| {
            println!("Spawning thread");

            let handles = handles.clone();
            async move {
                if let Ok(thread_id) = server_instance.read().spawn_thread() {
                    println!("Attempting to obtain handles lock");
                    handles.lock().await.push(thread_id);
                    println!("Handle lock obtained");

                    println!("Thread spawned: {}", thread_id);
                } else {
                    println!("Failed to spawn thread");
                }
            }
        })
        .collect();

    // Configure socket namespaces
    io.ns("/", on_connect);
    io.ns("/custom", on_connect);
    println!("Accepting socket connections");
    // Build the application with routes
    let app = Router::new()
        .route("/", get(|| async { "Horizon Server Running" }))
        .layer(layer);
    // Start the server
    let address = "0.0.0.0:3000";
    log_info!(LOGGER, "SOCKET NET", "Starting server on {}", address);

    futures::future::join_all(spawn_futures).await;

    log_info!(LOGGER, "SERVER", "Spawned {} threads", thread_count);
    let elapsed = start_time.elapsed();
    log_info!(LOGGER, "SERVER", "Server initialization took {:?}", elapsed);

    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .context(format!("Failed to bind to {}", address))?;
    axum::serve(listener, app)
        .await
        .context("Failed to start server")?;
    Ok(())
}
