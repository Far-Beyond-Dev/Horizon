pub use horizon_plugin_api::{LoadedPlugin, Plugin, Pluginstate};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use socketioxide::extract::{Data, SocketRef};
use std::collections::HashMap;
use rust_socketio::client::Client;
use socketioxide::socket::Sid;
use lazy_static::lazy_static;
use parking_lot::RwLock;
use std::sync::Arc;
use std::fs;

// Auth structures
#[derive(Serialize, Deserialize, Clone)]
pub struct AuthCredentials {
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AuthState {
    username: String,
    server_id: Sid,
    timestamp: u64,
}

lazy_static! {
    // Cross-instance client connections
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
        Arc::new(RwLock::new(temp_connections))
    };

    // Authentication storage
    static ref AUTHENTICATED_USERS: Arc<RwLock<HashMap<Sid, AuthState>>> = 
        Arc::new(RwLock::new(HashMap::new()));
    
    static ref VALID_CREDENTIALS: HashMap<&'static str, &'static str> = {
        let mut creds = HashMap::new();
        creds.insert("admin", "admin123");
        creds.insert("user1", "password123");
        creds.insert("test", "test123");
        creds
    };
}

pub trait PluginConstruct {
    fn get_structs(&self) -> Vec<&str>;
    fn new(plugins: HashMap<String, (Pluginstate, Plugin)>) -> Plugin;
}

impl PluginConstruct for Plugin {
    fn new(_plugins: HashMap<String, (Pluginstate, Plugin)>) -> Plugin {
        Plugin {}
    }

    fn get_structs(&self) -> Vec<&str> {
        vec!["AuthCredentials", "AuthState"]
    }
}

pub trait PluginAPI {
    fn player_joined(&self, socket: SocketRef) {
        println!("Setting up auth and link listeners");
        let listener = Listner::new(socket);
        setup_auth_listeners(&listener);
    }
}

impl PluginAPI for Plugin {}

pub struct Listner {
    pub socketref: SocketRef,
    pub servers: HashMap<String, Client>,
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
                // Check if the event is auth-related or user is authenticated
                let is_auth_event = event.starts_with("auth");
                let is_authenticated = AUTHENTICATED_USERS.read().contains_key(&socket.id);

                // Only proceed if it's an auth event or user is authenticated
                if is_auth_event || is_authenticated {
                    // Forward the event to all other servers in the network
                    servers_clone.iter().for_each(|server| {
                        let socket = servers_clone.get(server.0).expect("Failed to get client");
                        let serialized_data =
                            serde_json::to_string(&data.0).expect("Failed to serialize data");
                        if let Err(e) = socket.emit(event.clone(), serialized_data) {
                            eprintln!("Failed to emit event: {}", e);
                        }
                    });
                    
                    // Only call the callback if authenticated or it's an auth event
                    callback(data, socket);
                } else {
                    // Optionally notify the client that they need to authenticate
                    let _ = socket.emit("auth_required", "Authentication required for this action");
                    println!("Blocked unauthenticated event: {} from socket {}", event, socket.id);
                }
            });
    }
}

fn setup_auth_listeners(listener: &Listner) {
    // Handle initial authentication
    listener.on::<AuthCredentials, _>("auth", move |data: Data<AuthCredentials>, socket: SocketRef| {
        
        let credentials = data.0;
        let auth_success = validate_credentials(&credentials);
        
        if auth_success {
            // Create auth state
            let auth_state = AuthState {
                username: credentials.username.clone(),
                server_id: socket.id.clone(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };

            // Store authenticated user
            AUTHENTICATED_USERS.write().insert(socket.id.clone(), auth_state.clone());
            
            // Send success response
            let _ = socket.emit("auth_response", "Authentication successful");
            println!("User {} authenticated successfully", credentials.username);
        } else {
            // Send failure response and disconnect
            let _ = socket.emit("auth_response", "Authentication failed");
            println!("Authentication failed for user {}", credentials.username);
            let _ = socket.disconnect();
        }
    });

    // Handle auth state replication
    listener.on::<AuthState, _>("auth_state_sync", move |data: Data<AuthState>, socket: SocketRef| {
        let auth_state = data.0;
        let username = auth_state.username.clone();
        AUTHENTICATED_USERS.write().insert(auth_state.server_id.clone(), auth_state);
        println!("Replicated auth state for user: {}", username);
    });

    // Handle disconnection
    let socket = listener.socketref.clone();
    socket.on_disconnect(move |socket: SocketRef| {
        if let Some(auth_state) = AUTHENTICATED_USERS.write().remove(&socket.id) {
            println!("User {} disconnected from {}", auth_state.username, auth_state.server_id);
        }
    });
}

fn validate_credentials(credentials: &AuthCredentials) -> bool {
    if let Some(&stored_password) = VALID_CREDENTIALS.get(credentials.username.as_str()) {
        stored_password == credentials.password
    } else {
        false
    }
}
