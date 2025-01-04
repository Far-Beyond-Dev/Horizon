pub use horizon_plugin_api::{LoadedPlugin, Plugin, Pluginstate};
use serde::{de::DeserializeOwned, Serialize};
use socketioxide::extract::{Data, SocketRef};
use rust_socketio::client::Client;
use std::collections::HashMap;
use lazy_static::lazy_static;
use parking_lot::RwLock;
use std::sync::Arc;
use std::fs;

lazy_static! {
    pub static ref CLIENTS: Arc<RwLock<HashMap<String, Client>>> = {
        let config_data = fs::read_to_string("link_config.json").expect("Unable to read file");
        let servers = serde_json::to_string(&config_data).unwrap();

        let server_addresses: Vec<String> = serde_json::from_str(&servers).unwrap();

        let mut temp_connections = HashMap::new();
        server_addresses.iter().for_each(|address| {
            let socket = rust_socketio::ClientBuilder::new(address.clone());
            let socket_ref: rust_socketio::client::Client =
                socket.connect().expect("Failed to connect to server");
            temp_connections.insert(address.clone(), socket_ref);
        });
        return Arc::new(RwLock::new(temp_connections));
    };
}

pub trait PluginConstruct {
    fn get_structs(&self) -> Vec<&str>;
    // If you want default implementations, mark them with 'default'
    fn new(plugins: HashMap<String, (Pluginstate, Plugin)>) -> Plugin;
}

impl PluginConstruct for Plugin {
    fn new(_plugins: HashMap<String, (Pluginstate, Plugin)>) -> Plugin {
        Plugin {}
    }

    fn get_structs(&self) -> Vec<&str> {
        vec!["MyPlayer"]
    }
}

pub trait PluginAPI {}
impl PluginAPI for Plugin {}

pub struct Listner {
    pub socketref: SocketRef,
    pub servers: HashMap<std::string::String, Client>,
}

impl Listner {
    pub fn new(socket_ref: SocketRef) -> Self {
        Self {
            socketref: socket_ref,
            servers: CLIENTS.read().clone(),
        }
    }

    pub fn on<T, F>(&self, event: &str, callback: F)
    where
        T: DeserializeOwned + Serialize + Send + Sync + 'static + Clone,
        F: Fn(Data<T>, SocketRef) + Send + Sync + 'static + Clone,
    {
        let event = event.to_string();
        let event_clone = event.clone();
        let servers_clone = self.servers.clone();
        self.socketref
            .on(event_clone, move |data: Data<T>, socket: SocketRef| {
                // Forward the event to all other server in the network
                servers_clone.iter().for_each(|server| {
                    let socket = servers_clone.get(server.0).expect("Failed to get client");

                    let serialized_data =
                        serde_json::to_string(&data.0).expect("Failed to serialize data");
                    if let Err(e) = socket.emit(event.clone(), serialized_data) {
                        eprintln!("Failed to emit event: {}", e);
                    }
                });
                callback(data, socket);
            });
    }
}
