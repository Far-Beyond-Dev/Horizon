use horizon_data_types::Player;
use socketioxide::extract::SocketRef;
pub use horizon_plugin_api::{Plugin, Pluginstate, LoadedPlugin};
use socketioxide::packet::Str;
use std::sync::RwLock;
use std::sync::Arc;
use std::collections::HashMap;
use PebbleVault;

// Define the trait properly
pub trait PluginAPI {    
    fn player_joined(&self, socket: SocketRef, players: Arc<RwLock<Vec<horizon_data_types::Player>>>);   
}

pub trait PluginConstruct {
    fn get_structs(&self) -> Vec<&str>;
    // If you want default implementations, mark them with 'default'
    fn new(plugins: HashMap<&'static str, LoadedPlugin>) -> Plugin;
    
}

// Implement constructor separately
impl PluginConstruct for Plugin {
    fn new(plugins: HashMap<&'static str, LoadedPlugin>) -> Plugin {        
        Plugin {}
    }

    fn get_structs(&self) -> Vec<&str> {
        vec!["MyPlayer"]
    }
}

// Implement the trait for Plugin
impl PluginAPI for Plugin {
    fn player_joined(&self, socket: SocketRef, players: Arc<RwLock<Vec<horizon_data_types::Player>>>) {
        setup_listeners(socket, players);
    }
}

//-----------------------------------------------------------------------------
// Plugin Implementation
//-----------------------------------------------------------------------------

trait PlayerAPI {
    fn player_move(&mut self, x: f64, y: f64, z: f64, socket: SocketRef);
    fn set_spawn(&mut self, x: f64, y: f64, z: f64, socket: SocketRef);
    fn modify_health(&mut self, health: f64, socket: SocketRef);
    fn set_health(&mut self, health: f64, socket: SocketRef);
    fn respawn(&mut self, socket: SocketRef);
    fn kill(&mut self, socket: SocketRef);
}

impl PlayerAPI for Player {
    fn player_move(&mut self, x: f64, y: f64, z: f64, socket: SocketRef) {
        self.transform.as_mut().expect("Failed to access location").location.expect("failed to access axis").x = x;
        self.transform.as_mut().expect("Failed to access location").location.expect("failed to access axis").y = y;
        self.transform.as_mut().expect("Failed to access location").location.expect("failed to access axis").z = z;
    }

    fn set_spawn(&mut self, x: f64, y: f64, z: f64, socket: SocketRef) {
    //    self.spawn.as_mut().expect("Failed to access spawn").location.expect("failed to access axis").x = x;
    //    self.spawn.as_mut().expect("Failed to access spawn").location.expect("failed to access axis").y = y;
    //    self.spawn.as_mut().expect("Failed to access spawn").location.expect("failed to access axis").z = z;
    }

    fn kill(&mut self, socket: SocketRef) {
    //    Send to death screen
    }

    fn respawn(&mut self, socket: SocketRef) {
    //    Send to spawn
    }

    fn set_health(&mut self, health: f64, socket: SocketRef) {
    //    self.health = health;
    }

    fn modify_health(&mut self, health: f64, socket: SocketRef) {
    //    self.health += health;
    }

}





fn setup_listeners(socket: SocketRef, players: Arc<RwLock<Vec<horizon_data_types::Player>>>) {
    socket.on("foo", || println!("bar"));
}