/// High-performance binary serialization for UDP events
use crate::events::{Event, EventError};
use crate::protocol::BinarySerializable;
use crate::types::{PlayerId, Vec3};
use std::mem;

/// Binary event wrapper for zero-cost UDP transmission
#[repr(C)]
#[derive(Debug, Clone)]
pub struct BinaryEvent<T> {
    pub event_type: u16,
    pub timestamp: u64,
    pub data: T,
}

impl<T> BinaryEvent<T>
where
    T: BinarySerializable + Send + Sync + 'static + std::fmt::Debug,
{
    pub fn new(event_type: u16, data: T) -> Self {
        Self {
            event_type,
            timestamp: crate::utils::current_timestamp(),
            data,
        }
    }
}

impl<T> Event for BinaryEvent<T>
where
    T: BinarySerializable + Send + Sync + 'static + std::fmt::Debug,
{
    fn type_name() -> &'static str
    where
        Self: Sized,
    {
        std::any::type_name::<Self>()
    }

    fn serialize(&self) -> Result<Vec<u8>, EventError> {
        self.serialize_binary()
    }

    fn deserialize(data: &[u8]) -> Result<Self, EventError>
    where
        Self: Sized,
    {
        Self::deserialize_binary(data)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl<T> BinarySerializable for BinaryEvent<T>
where
    T: BinarySerializable,
{
    fn serialize_binary(&self) -> Result<Vec<u8>, EventError> {
        let mut buffer = Vec::new();
        
        buffer.extend_from_slice(&self.event_type.to_le_bytes());
        buffer.extend_from_slice(&self.timestamp.to_le_bytes());
        
        let data_bytes = self.data.serialize_binary()?;
        buffer.extend_from_slice(&data_bytes);
        
        Ok(buffer)
    }

    fn deserialize_binary(data: &[u8]) -> Result<Self, EventError>
    where
        Self: Sized,
    {
        if data.len() < 10 {
            return Err(EventError::HandlerExecution("Binary data too short".to_string()));
        }

        let event_type = u16::from_le_bytes([data[0], data[1]]);
        let timestamp = u64::from_le_bytes([
            data[2], data[3], data[4], data[5], data[6], data[7], data[8], data[9],
        ]);

        let inner_data = T::deserialize_binary(&data[10..])?;

        Ok(Self {
            event_type,
            timestamp,
            data: inner_data,
        })
    }
}

/// Binary position update event - zero allocation when used directly
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BinaryPositionUpdate {
    pub player_id: [u8; 16], // UUID bytes
    pub position: Vec3,
    pub velocity: Vec3,
    pub sequence: u32,
}

impl BinarySerializable for BinaryPositionUpdate {
    fn serialize_binary(&self) -> Result<Vec<u8>, EventError> {
        let mut buffer = Vec::with_capacity(mem::size_of::<Self>());
        
        buffer.extend_from_slice(&self.player_id);
        buffer.extend_from_slice(&self.position.x.to_le_bytes());
        buffer.extend_from_slice(&self.position.y.to_le_bytes());
        buffer.extend_from_slice(&self.position.z.to_le_bytes());
        buffer.extend_from_slice(&self.velocity.x.to_le_bytes());
        buffer.extend_from_slice(&self.velocity.y.to_le_bytes());
        buffer.extend_from_slice(&self.velocity.z.to_le_bytes());
        buffer.extend_from_slice(&self.sequence.to_le_bytes());
        
        Ok(buffer)
    }

    fn deserialize_binary(data: &[u8]) -> Result<Self, EventError>
    where
        Self: Sized,
    {
        if data.len() < mem::size_of::<Self>() {
            return Err(EventError::HandlerExecution("Binary position data too short".to_string()));
        }

        let mut player_id = [0u8; 16];
        player_id.copy_from_slice(&data[0..16]);

        let position = Vec3 {
            x: f32::from_le_bytes([data[16], data[17], data[18], data[19]]),
            y: f32::from_le_bytes([data[20], data[21], data[22], data[23]]),
            z: f32::from_le_bytes([data[24], data[25], data[26], data[27]]),
        };

        let velocity = Vec3 {
            x: f32::from_le_bytes([data[28], data[29], data[30], data[31]]),
            y: f32::from_le_bytes([data[32], data[33], data[34], data[35]]),
            z: f32::from_le_bytes([data[36], data[37], data[38], data[39]]),
        };

        let sequence = u32::from_le_bytes([data[40], data[41], data[42], data[43]]);

        Ok(Self {
            player_id,
            position,
            velocity,
            sequence,
        })
    }
}

impl BinaryPositionUpdate {
    pub fn new(player_id: PlayerId, position: Vec3, velocity: Vec3, sequence: u32) -> Self {
        Self {
            player_id: player_id.0.as_bytes().clone(),
            position,
            velocity,
            sequence,
        }
    }

    pub fn get_player_id(&self) -> Result<PlayerId, EventError> {
        let uuid = uuid::Uuid::from_bytes(self.player_id);
        Ok(PlayerId(uuid))
    }
}

/// Binary action event - for player inputs
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BinaryActionEvent {
    pub player_id: [u8; 16],
    pub action_type: u8,
    pub action_data: u32,
    pub timestamp_offset: u16, // Offset from packet timestamp for precision
}

impl BinarySerializable for BinaryActionEvent {
    fn serialize_binary(&self) -> Result<Vec<u8>, EventError> {
        let mut buffer = Vec::with_capacity(mem::size_of::<Self>());
        
        buffer.extend_from_slice(&self.player_id);
        buffer.push(self.action_type);
        buffer.extend_from_slice(&self.action_data.to_le_bytes());
        buffer.extend_from_slice(&self.timestamp_offset.to_le_bytes());
        
        Ok(buffer)
    }

    fn deserialize_binary(data: &[u8]) -> Result<Self, EventError>
    where
        Self: Sized,
    {
        if data.len() < mem::size_of::<Self>() {
            return Err(EventError::HandlerExecution("Binary action data too short".to_string()));
        }

        let mut player_id = [0u8; 16];
        player_id.copy_from_slice(&data[0..16]);

        let action_type = data[16];
        let action_data = u32::from_le_bytes([data[17], data[18], data[19], data[20]]);
        let timestamp_offset = u16::from_le_bytes([data[21], data[22]]);

        Ok(Self {
            player_id,
            action_type,
            action_data,
            timestamp_offset,
        })
    }
}

impl BinaryActionEvent {
    pub fn new(
        player_id: PlayerId,
        action_type: u8,
        action_data: u32,
        timestamp_offset: u16,
    ) -> Self {
        Self {
            player_id: player_id.0.as_bytes().clone(),
            action_type,
            action_data,
            timestamp_offset,
        }
    }

    pub fn get_player_id(&self) -> Result<PlayerId, EventError> {
        let uuid = uuid::Uuid::from_bytes(self.player_id);
        Ok(PlayerId(uuid))
    }
}

/// Event type constants for binary events
pub mod event_types {
    pub const POSITION_UPDATE: u16 = 0x0001;
    pub const ACTION_EVENT: u16 = 0x0002;
    pub const HEALTH_UPDATE: u16 = 0x0003;
    pub const INVENTORY_UPDATE: u16 = 0x0004;
}

/// Type aliases for commonly used binary events
pub type BinaryPositionEvent = BinaryEvent<BinaryPositionUpdate>;
pub type BinaryPlayerActionEvent = BinaryEvent<BinaryActionEvent>;