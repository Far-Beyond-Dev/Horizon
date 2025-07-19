/// Protocol-specific traits and implementations for zero-cost TCP/UDP and JSON/binary abstractions
use crate::events::{Event, EventError};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::sync::Arc;

/// Protocol marker traits for zero-cost abstractions
pub mod protocol {
    pub trait Protocol: Send + Sync + 'static {}
    
    #[derive(Debug, Clone, Copy)]
    pub struct Tcp;
    impl Protocol for Tcp {}
    
    #[derive(Debug, Clone, Copy)]
    pub struct Udp;
    impl Protocol for Udp {}
}

/// Serialization format marker traits for zero-cost abstractions
pub mod format {
    pub trait SerializationFormat: Send + Sync + 'static {}
    
    #[derive(Debug, Clone, Copy)]
    pub struct Json;
    impl SerializationFormat for Json {}
    
    #[derive(Debug, Clone, Copy)]
    pub struct Binary;
    impl SerializationFormat for Binary {}
}

/// Protocol-specific event trait that ensures zero-cost abstractions
pub trait ProtocolEvent<P: protocol::Protocol, F: format::SerializationFormat>: Event {
    fn serialize_for_protocol(&self) -> Result<Vec<u8>, EventError>;
    fn deserialize_for_protocol(data: &[u8]) -> Result<Self, EventError>
    where
        Self: Sized;
}

/// JSON over TCP implementation - zero cost when using TCP + JSON
impl<T> ProtocolEvent<protocol::Tcp, format::Json> for T
where
    T: Event + Serialize + for<'de> Deserialize<'de>,
{
    fn serialize_for_protocol(&self) -> Result<Vec<u8>, EventError> {
        serde_json::to_vec(self).map_err(EventError::Serialization)
    }

    fn deserialize_for_protocol(data: &[u8]) -> Result<Self, EventError>
    where
        Self: Sized,
    {
        serde_json::from_slice(data).map_err(EventError::Deserialization)
    }
}

/// Binary over UDP implementation - zero cost when using UDP + Binary
impl<T> ProtocolEvent<protocol::Udp, format::Binary> for T
where
    T: Event + BinarySerializable,
{
    fn serialize_for_protocol(&self) -> Result<Vec<u8>, EventError> {
        self.serialize_binary()
    }

    fn deserialize_for_protocol(data: &[u8]) -> Result<Self, EventError>
    where
        Self: Sized,
    {
        Self::deserialize_binary(data)
    }
}

/// Trait for binary serialization without JSON overhead
pub trait BinarySerializable: Send + Sync + 'static {
    fn serialize_binary(&self) -> Result<Vec<u8>, EventError>;
    fn deserialize_binary(data: &[u8]) -> Result<Self, EventError>
    where
        Self: Sized;
}

/// Protocol-aware event handler that ensures compile-time protocol separation
pub struct ProtocolHandler<P: protocol::Protocol, F: format::SerializationFormat, T: Event> {
    handler: Arc<dyn Fn(T) -> Result<(), EventError> + Send + Sync>,
    _protocol: PhantomData<P>,
    _format: PhantomData<F>,
}

impl<P: protocol::Protocol, F: format::SerializationFormat, T: Event> Clone for ProtocolHandler<P, F, T> {
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            _protocol: PhantomData,
            _format: PhantomData,
        }
    }
}

impl<P: protocol::Protocol, F: format::SerializationFormat, T: Event> ProtocolHandler<P, F, T> {
    pub fn new<H>(handler: H) -> Self
    where
        H: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
    {
        Self {
            handler: Arc::new(handler),
            _protocol: PhantomData,
            _format: PhantomData,
        }
    }

    pub async fn handle(&self, event: T) -> Result<(), EventError> {
        (self.handler)(event)
    }
}

/// TCP-specific event system extensions
pub mod tcp {
    use super::*;
    use crate::system::EventSystem;
    
    pub trait TcpEventSystemExt {
        async fn on_tcp_json<T, F>(
            &self,
            event_key: &str,
            handler: F,
        ) -> Result<(), EventError>
        where
            T: Event + ProtocolEvent<protocol::Tcp, format::Json>,
            F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static;

        async fn emit_tcp_json<T>(
            &self,
            event_key: &str,
            event: &T,
        ) -> Result<(), EventError>
        where
            T: Event + ProtocolEvent<protocol::Tcp, format::Json>;
    }
    
    impl TcpEventSystemExt for EventSystem {
        async fn on_tcp_json<T, F>(
            &self,
            event_key: &str,
            handler: F,
        ) -> Result<(), EventError>
        where
            T: Event + ProtocolEvent<protocol::Tcp, format::Json>,
            F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
        {
            let protocol_handler = ProtocolHandler::<protocol::Tcp, format::Json, T>::new(handler);
            
            let typed_handler = crate::events::TypedEventHandler::new(
                format!("tcp_json::{}", event_key),
                move |event: T| futures::executor::block_on(protocol_handler.handle(event))
            );
            
            self.register_handler(event_key, Box::new(typed_handler)).await
        }

        async fn emit_tcp_json<T>(
            &self,
            event_key: &str,
            event: &T,
        ) -> Result<(), EventError>
        where
            T: Event + ProtocolEvent<protocol::Tcp, format::Json>,
        {
            let data = event.serialize_for_protocol()?;
            self.emit_raw(event_key, &data).await
        }
    }
}

/// UDP-specific event system extensions
pub mod udp {
    use super::*;
    use crate::system::EventSystem;
    use crate::types::PlayerId;
    
    pub trait UdpEventSystemExt {
        async fn on_udp_binary<T, F>(
            &self,
            event_key: &str,
            handler: F,
        ) -> Result<(), EventError>
        where
            T: Event + ProtocolEvent<protocol::Udp, format::Binary>,
            F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static;

        async fn emit_udp_binary<T>(
            &self,
            player_id: PlayerId,
            event_key: &str,
            event: &T,
        ) -> Result<(), EventError>
        where
            T: Event + ProtocolEvent<protocol::Udp, format::Binary>;

        async fn broadcast_udp_binary<T>(
            &self,
            event_key: &str,
            event: &T,
        ) -> Result<(), EventError>
        where
            T: Event + ProtocolEvent<protocol::Udp, format::Binary>;
    }
    
    impl UdpEventSystemExt for EventSystem {
        async fn on_udp_binary<T, F>(
            &self,
            event_key: &str,
            handler: F,
        ) -> Result<(), EventError>
        where
            T: Event + ProtocolEvent<protocol::Udp, format::Binary>,
            F: Fn(T) -> Result<(), EventError> + Send + Sync + 'static,
        {
            let protocol_handler = ProtocolHandler::<protocol::Udp, format::Binary, T>::new(handler);
            
            let typed_handler = crate::events::TypedEventHandler::new(
                format!("udp_binary::{}", event_key),
                move |event: T| futures::executor::block_on(protocol_handler.handle(event))
            );
            
            self.register_handler(event_key, Box::new(typed_handler)).await
        }

        async fn emit_udp_binary<T>(
            &self,
            player_id: PlayerId,
            event_key: &str,
            event: &T,
        ) -> Result<(), EventError>
        where
            T: Event + ProtocolEvent<protocol::Udp, format::Binary>,
        {
            if let Some(ref udp_system) = self.get_udp_system() {
                let data = event.serialize_for_protocol()?;
                udp_system.send_raw_to_player(player_id, event_key, &data).await
            } else {
                Err(EventError::HandlerExecution("UDP system not available".to_string()))
            }
        }

        async fn broadcast_udp_binary<T>(
            &self,
            event_key: &str,
            event: &T,
        ) -> Result<(), EventError>
        where
            T: Event + ProtocolEvent<protocol::Udp, format::Binary>,
        {
            if let Some(ref udp_system) = self.get_udp_system() {
                let data = event.serialize_for_protocol()?;
                udp_system.broadcast_raw(event_key, &data).await
            } else {
                Err(EventError::HandlerExecution("UDP system not available".to_string()))
            }
        }
    }
}