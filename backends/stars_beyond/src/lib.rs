use horizon_data_types::Player;
use socketioxide::extract::SocketRef;
pub use horizon_plugin_api::{Plugin, Pluginstate, LoadedPlugin};
use parking_lot::RwLock;
use std::sync::Arc;
use std::collections::HashMap;
// use socketioxide::packet::Str;

pub trait PluginAPI {    
    fn player_joined(&self, socket: SocketRef, player: Arc<RwLock<horizon_data_types::Player>>);   
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

impl PluginAPI for Plugin {
    fn player_joined(&self, socket: SocketRef, player: Arc<RwLock<horizon_data_types::Player>>) {
        println!("player_lib");
        setup_listeners(socket, player);
    }
}


fn setup_listeners(_socket: SocketRef, _player: Arc<RwLock<Player>>) {
}