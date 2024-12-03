use socketioxide::extract::SocketRef;
use serde::{Serialize, Deserialize};
use socketioxide::extract::Data;
use horizon_data_types::Vec3D;
use uuid::Uuid;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;
use crate::server::vault_lib;
pub use horizon_plugin_api::Plugin;


use super::super::vault_lib::PluginAPI;

pub struct EventManager{
    vault_ref: Uuid,
}

impl EventManager {
    pub fn new() -> Self {
        let vault_lib = <Plugin as vault_lib::PluginAPI>::new();
        let region = vault_lib.create_or_load_region([0.0,0.0,0.0], 100000000.0).expect("Failed to create or load region");

            Self {
                vault_ref: region,
            }
        }

    pub fn create_event(&mut self, instigator: SocketRef, origin: Vec3D, radius: f64, data: Data<serde_json::Value>) {
        let event = Event::new(instigator, origin, radius, data);

        let event_uuid = uuid::Uuid::new_v4();

        let sockets: Vec<SocketRef> = Vec::new();

        for socket in sockets {
            let _ = socket.join(event_uuid.to_string());
        }

        

        // Query against Pebblevault to find all clients in range
        event.broadcast();
    }
}

pub struct Event {
    instigator: SocketRef,
    origin: Vec3D,
    radius: f64,
    data: Data<serde_json::Value>,
}

impl Event {
    pub fn new(instigator: SocketRef, origin: Vec3D, radius: f64, data: Data<serde_json::Value>) -> Self {
        Self {
            instigator,
            origin,
            radius,
            data,
        }
    }

    pub fn get_instigator(&self) -> &SocketRef {
        &self.instigator
    }

    pub fn broadcast(&self) {
        // Broadcast the event to all clients in range       
    }
}

pub fn test() {
    let mut evt_manager = EventManager::new();
}