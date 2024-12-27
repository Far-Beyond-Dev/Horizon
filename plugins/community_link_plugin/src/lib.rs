use horizon_data_types::Player;
use parking_lot::RwLock;
use std::sync::Arc;
use std::collections::HashMap;
use socketioxide::extract::{ SocketRef, Data };
use serde::de::DeserializeOwned;
pub use horizon_plugin_api::{Plugin, Pluginstate, LoadedPlugin};

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

pub struct listner {
    socketref: SocketRef,
}

impl listner {
    pub fn on<T, F>(&self, event: &str, callback: F)
    where
        T: DeserializeOwned + Send + Sync + 'static,
        F: Fn(Data<T>, SocketRef) + Send + Sync + 'static + Clone,
    {
        let event = event.to_string();
        let event_clone = event.clone();
        self.socketref.on(event_clone, move |data: Data<T>, socket: SocketRef| {
            //TODO: Pass the data to other servers as well, and strip any un-needed fields here.

            callback(data, socket);
        });
    }
}   