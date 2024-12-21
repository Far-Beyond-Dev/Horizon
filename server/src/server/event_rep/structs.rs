use socketioxide::extract::SocketRef;
use serde::{Serialize, Deserialize};
use socketioxide::extract::Data;
use horizon_data_types::Vec3D;
use PebbleVault::VaultManager;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

static VAULT_MANAGER: Lazy<Arc<Mutex<VaultManager<PebbleVaultCustomData>>>> = 
    Lazy::new(|| {
        let vault_manager = VaultManager::new("./pv-horizon-plugin-data").expect("Failed to create VaultManager");
        Arc::new(Mutex::new(vault_manager))
    });

/// Custom data structure for PebbleVault objects
///
/// This struct represents the custom data associated with each spatial object
/// in the PebbleVault system. It can be extended or modified to suit specific
/// game or application needs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PebbleVaultCustomData {
    /// Name of the object
    pub name: String,
    /// Custom value associated with the object
    pub value: i32,
}

#[allow(dead_code, unused_variables)]
pub struct EventManager<'a, T: Clone + Serialize + for<'de> Deserialize<'de> + PartialEq + Sized> {
    vault_ref: &'a mut PebbleVault::VaultManager<T>,
}

#[allow(dead_code, unused_variables)]
impl<'a, T: Clone + Serialize + for<'de> Deserialize<'de> + PartialEq + Sized> EventManager<'a, T> {
    pub fn new(pebble_vault_ref: &'a mut PebbleVault::VaultManager<T>) -> Self {
            Self {
                vault_ref: pebble_vault_ref
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

#[allow(dead_code, unused_variables)]
pub struct Event {
    instigator: SocketRef,
    origin: Vec3D,
    radius: f64,
    data: Data<serde_json::Value>,
}

#[allow(dead_code)]
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

#[allow(dead_code, unused_variables, unused_mut)]
pub fn test() {
    let mut vault_manager = VAULT_MANAGER.lock().unwrap();
    let mut evt_manager = EventManager::new(&mut *vault_manager);
}