use horizon_data_types::Player;
use socketioxide::extract::SocketRef;
pub use horizon_plugin_api::{Plugin, Pluginstate, LoadedPlugin};
use parking_lot::RwLock;
use std::sync::Arc;
use std::collections::HashMap;
use player_lib::world;
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
        setup_listeners(&socket, player);

        let player_char = world::Object::new("Player Character".to_string(), true);
        println!("Player joined: {:?}", player_char.get_uuid().to_string());

        let player_json_value: serde_json::Value = serde_json::json!({ "joined": true });
        player_char.send_event(socket, "playerjoined".to_string(), player_json_value);
        println!("Sent player joined event Successfully");
    }
}


fn setup_listeners(_socket: &SocketRef, _player: Arc<RwLock<Player>>) {
    
}