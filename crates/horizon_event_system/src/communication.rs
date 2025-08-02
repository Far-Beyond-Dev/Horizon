//! Generalized communication stack combining transport and serialization.
//!
//! This module provides a type-safe, zero-cost abstraction over different
//! combinations of transport protocols (TCP/UDP) and serialization formats (JSON/Binary).

use crate::transport::{Connection, TransportProtocol, TcpTransport, UdpTransport};
use crate::serialization::{SerializationFormat, JsonFormat, BinaryFormat, JsonSerializer, BinarySerializer, SerializationError};
use serde::{Serialize, Deserialize};
use std::marker::PhantomData;
use std::net::SocketAddr;

/// Generic communication endpoint that combines transport and serialization.
pub struct CommunicationEndpoint<T: TransportProtocol, S: SerializationFormat> {
    connection: Box<dyn Connection>,
    _transport: PhantomData<T>,
    _serialization: PhantomData<S>,
}

impl<T: TransportProtocol, S: SerializationFormat> CommunicationEndpoint<T, S> {
    /// Create a new communication endpoint with the given connection.
    pub fn new(connection: Box<dyn Connection>) -> Self {
        Self {
            connection,
            _transport: PhantomData,
            _serialization: PhantomData,
        }
    }
    
    /// Get the remote address of the peer.
    pub fn remote_addr(&self) -> SocketAddr {
        self.connection.remote_addr()
    }
    
    /// Close the connection gracefully.
    pub async fn close(&mut self) -> Result<(), CommunicationError> {
        self.connection.close().await
            .map_err(CommunicationError::Transport)
    }
    
    /// Get information about the transport protocol.
    pub fn transport_info() -> TransportInfo {
        TransportInfo {
            name: T::NAME,
            connection_oriented: T::CONNECTION_ORIENTED,
            reliable: T::RELIABLE,
        }
    }
    
    /// Get information about the serialization format.
    pub fn format_info() -> FormatInfo {
        FormatInfo {
            name: S::NAME,
            human_readable: S::HUMAN_READABLE,
            mime_type: S::MIME_TYPE,
        }
    }
}

// Specialized implementations for each format combination
impl<T: TransportProtocol> CommunicationEndpoint<T, JsonFormat> {
    /// Send a JSON message through the endpoint.
    pub async fn send_message<M: Serialize>(&mut self, message: &M) -> Result<(), CommunicationError> {
        let serializer = JsonSerializer::new();
        let data = serializer.serialize(message)
            .map_err(CommunicationError::Serialization)?;
        
        self.connection.send(&data).await
            .map_err(CommunicationError::Transport)?;
        
        Ok(())
    }
    
    /// Receive and deserialize a JSON message from the endpoint.
    pub async fn receive_message<M: for<'de> Deserialize<'de>>(&mut self) -> Result<Option<M>, CommunicationError> {
        match self.connection.receive().await {
            Ok(Some(data)) => {
                let serializer = JsonSerializer::new();
                let message = serializer.deserialize(&data)
                    .map_err(CommunicationError::Serialization)?;
                Ok(Some(message))
            },
            Ok(None) => Ok(None),
            Err(e) => Err(CommunicationError::Transport(e)),
        }
    }
}

impl<T: TransportProtocol> CommunicationEndpoint<T, BinaryFormat> {
    /// Send a binary message through the endpoint.
    pub async fn send_message<M: Serialize>(&mut self, message: &M) -> Result<(), CommunicationError> {
        let serializer = BinarySerializer::new();
        let data = serializer.serialize(message)
            .map_err(CommunicationError::Serialization)?;
        
        self.connection.send(&data).await
            .map_err(CommunicationError::Transport)?;
        
        Ok(())
    }
    
    /// Receive and deserialize a binary message from the endpoint.
    pub async fn receive_message<M: for<'de> Deserialize<'de>>(&mut self) -> Result<Option<M>, CommunicationError> {
        match self.connection.receive().await {
            Ok(Some(data)) => {
                let serializer = BinarySerializer::new();
                let message = serializer.deserialize(&data)
                    .map_err(CommunicationError::Serialization)?;
                Ok(Some(message))
            },
            Ok(None) => Ok(None),
            Err(e) => Err(CommunicationError::Transport(e)),
        }
    }
}

/// Information about a transport protocol.
#[derive(Debug, Clone)]
pub struct TransportInfo {
    pub name: &'static str,
    pub connection_oriented: bool,
    pub reliable: bool,
}

/// Information about a serialization format.
#[derive(Debug, Clone)]
pub struct FormatInfo {
    pub name: &'static str,
    pub human_readable: bool,
    pub mime_type: &'static str,
}

/// Errors that can occur during communication.
#[derive(Debug, thiserror::Error)]
pub enum CommunicationError {
    #[error("Transport error: {0}")]
    Transport(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] SerializationError),
    
    #[error("Protocol mismatch: expected {expected}, got {actual}")]
    ProtocolMismatch { expected: String, actual: String },
    
    #[error("Connection closed")]
    ConnectionClosed,
}

/// Type aliases for common communication endpoint combinations.
pub type TcpJsonEndpoint = CommunicationEndpoint<TcpTransport, JsonFormat>;
pub type TcpBinaryEndpoint = CommunicationEndpoint<TcpTransport, BinaryFormat>;
pub type UdpJsonEndpoint = CommunicationEndpoint<UdpTransport, JsonFormat>;
pub type UdpBinaryEndpoint = CommunicationEndpoint<UdpTransport, BinaryFormat>;

/// Compile-time validation trait for protocol combinations.
pub trait ProtocolCompatible<T: TransportProtocol, S: SerializationFormat> {
    /// Check if this combination is valid at compile time.
    const VALID: bool = true;
    
    /// Get a description of why this combination might be suboptimal.
    fn compatibility_notes() -> Vec<&'static str> {
        Vec::new()
    }
}

/// TCP + JSON combination (default, fully compatible).
impl ProtocolCompatible<TcpTransport, JsonFormat> for () {
    const VALID: bool = true;
    
    fn compatibility_notes() -> Vec<&'static str> {
        vec!["Standard web-compatible combination", "Good for debugging"]
    }
}

/// TCP + Binary combination (high performance).
impl ProtocolCompatible<TcpTransport, BinaryFormat> for () {
    const VALID: bool = true;
    
    fn compatibility_notes() -> Vec<&'static str> {
        vec!["High performance combination", "Compact message size"]
    }
}

/// UDP + JSON combination (suboptimal but valid).
impl ProtocolCompatible<UdpTransport, JsonFormat> for () {
    const VALID: bool = true;
    
    fn compatibility_notes() -> Vec<&'static str> {
        vec![
            "JSON overhead may be significant for UDP",
            "Consider binary format for better performance"
        ]
    }
}

/// UDP + Binary combination (optimal for UDP).
impl ProtocolCompatible<UdpTransport, BinaryFormat> for () {
    const VALID: bool = true;
    
    fn compatibility_notes() -> Vec<&'static str> {
        vec!["Optimal combination for UDP", "Minimal overhead"]
    }
}

/// Builder for creating communication endpoints with compile-time validation.
pub struct EndpointBuilder<T: TransportProtocol, S: SerializationFormat> {
    _transport: PhantomData<T>,
    _serialization: PhantomData<S>,
}

impl<T: TransportProtocol, S: SerializationFormat> EndpointBuilder<T, S> {
    /// Create a new endpoint builder.
    pub fn new() -> Self
    where
        (): ProtocolCompatible<T, S>,
    {
        Self {
            _transport: PhantomData,
            _serialization: PhantomData,
        }
    }
    
    /// Build the endpoint with the given connection.
    pub fn build(
        self,
        connection: Box<dyn Connection>,
    ) -> CommunicationEndpoint<T, S> {
        CommunicationEndpoint::new(connection)
    }
    
    /// Get compatibility notes for this combination.
    pub fn compatibility_notes(&self) -> Vec<&'static str>
    where
        (): ProtocolCompatible<T, S>,
    {
        <() as ProtocolCompatible<T, S>>::compatibility_notes()
    }
}

impl<T: TransportProtocol, S: SerializationFormat> Default for EndpointBuilder<T, S>
where
    (): ProtocolCompatible<T, S>,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Factory for creating common endpoint combinations.
pub struct CommunicationFactory;

impl CommunicationFactory {
    /// Create a TCP + JSON endpoint builder.
    pub fn tcp_json() -> EndpointBuilder<TcpTransport, JsonFormat> {
        EndpointBuilder::new()
    }
    
    /// Create a TCP + Binary endpoint builder.
    pub fn tcp_binary() -> EndpointBuilder<TcpTransport, BinaryFormat> {
        EndpointBuilder::new()
    }
    
    /// Create a UDP + JSON endpoint builder.
    pub fn udp_json() -> EndpointBuilder<UdpTransport, JsonFormat> {
        EndpointBuilder::new()
    }
    
    /// Create a UDP + Binary endpoint builder.
    pub fn udp_binary() -> EndpointBuilder<UdpTransport, BinaryFormat> {
        EndpointBuilder::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compile_time_validation() {
        // These should compile fine
        let _tcp_json = CommunicationFactory::tcp_json();
        let _tcp_binary = CommunicationFactory::tcp_binary();
        let _udp_json = CommunicationFactory::udp_json();
        let _udp_binary = CommunicationFactory::udp_binary();
    }
    
    #[test]
    fn test_transport_info() {
        assert_eq!(TcpJsonEndpoint::transport_info().name, "TCP");
        assert_eq!(TcpJsonEndpoint::transport_info().connection_oriented, true);
        assert_eq!(UdpJsonEndpoint::transport_info().connection_oriented, false);
    }
    
    #[test]
    fn test_format_info() {
        assert_eq!(TcpJsonEndpoint::format_info().name, "JSON");
        assert_eq!(TcpJsonEndpoint::format_info().human_readable, true);
        assert_eq!(TcpBinaryEndpoint::format_info().human_readable, false);
    }
    
    #[test]
    fn test_compatibility_notes() {
        let tcp_json = CommunicationFactory::tcp_json();
        let notes = tcp_json.compatibility_notes();
        assert!(!notes.is_empty());
        
        let udp_binary = CommunicationFactory::udp_binary();
        let notes = udp_binary.compatibility_notes();
        assert!(!notes.is_empty());
    }
}