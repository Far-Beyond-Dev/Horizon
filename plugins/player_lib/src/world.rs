use socketioxide::extract::{ SocketRef, Data };
use serde_json::Value;
use uuid::Uuid;

#[allow(dead_code)]
pub struct Object {
    uuid: Uuid,
    name: String,
    replicated: bool,
}

impl Object {
    pub fn new(name: String, replicated: bool) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            name, // Fetch name from method params
            replicated, // Fetch replicated from method params
        }
    }

    pub fn get_uuid(&self) -> Uuid {
        self.uuid
    }
    pub fn send_event(&self, instigator: SocketRef, event_name: String, event_data: Value) {
        let _ = socketioxide::socket::Socket::within(&instigator, self.uuid.to_string()).emit(event_name, &event_data);
    }
}

