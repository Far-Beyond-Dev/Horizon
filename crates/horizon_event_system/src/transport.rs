//! Transport layer abstractions for TCP and UDP communication.
//!
//! This module provides compile-time abstractions for different transport protocols,
//! enabling zero-cost selection between TCP and UDP with full type safety.

use async_trait::async_trait;
use std::net::SocketAddr;
use std::io;

/// Marker trait for transport protocols with compile-time guarantees.
pub trait TransportProtocol: Send + Sync + 'static {
    /// The name of the transport protocol for debugging/logging.
    const NAME: &'static str;
    
    /// Whether this transport is connection-oriented (TCP = true, UDP = false).
    const CONNECTION_ORIENTED: bool;
    
    /// Whether this transport guarantees delivery (TCP = true, UDP = false).
    const RELIABLE: bool;
}

/// TCP transport protocol marker.
#[derive(Debug, Clone, Copy)]
pub struct TcpTransport;

impl TransportProtocol for TcpTransport {
    const NAME: &'static str = "TCP";
    const CONNECTION_ORIENTED: bool = true;
    const RELIABLE: bool = true;
}

/// UDP transport protocol marker.
#[derive(Debug, Clone, Copy)]
pub struct UdpTransport;

impl TransportProtocol for UdpTransport {
    const NAME: &'static str = "UDP";
    const CONNECTION_ORIENTED: bool = false;
    const RELIABLE: bool = false;
}

/// Generic connection handle that abstracts over different transport protocols.
#[async_trait]
pub trait Connection: Send + Sync {
    /// Send data to the connected peer.
    async fn send(&mut self, data: &[u8]) -> io::Result<()>;
    
    /// Receive data from the connected peer.
    /// Returns None if the connection is closed.
    async fn receive(&mut self) -> io::Result<Option<Vec<u8>>>;
    
    /// Get the remote address of the peer.
    fn remote_addr(&self) -> SocketAddr;
    
    /// Close the connection gracefully.
    async fn close(&mut self) -> io::Result<()>;
}

/// Generic server listener that can bind to different transport protocols.
#[async_trait]
pub trait ServerListener<T: TransportProtocol>: Send + Sync {
    type Connection: Connection;
    
    /// Accept a new connection.
    /// For UDP, this simulates connections by tracking peer addresses.
    async fn accept(&mut self) -> io::Result<Self::Connection>;
    
    /// Get the local address the server is bound to.
    fn local_addr(&self) -> io::Result<SocketAddr>;
}

/// Factory for creating transport-specific listeners.
pub struct TransportFactory;

impl TransportFactory {
    /// Create a TCP server listener.
    pub async fn tcp_listener(addr: SocketAddr) -> io::Result<TcpServerListener> {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind(addr).await?;
        Ok(TcpServerListener { listener })
    }
    
    /// Create a UDP server listener.
    pub async fn udp_listener(addr: SocketAddr) -> io::Result<UdpServerListener> {
        use tokio::net::UdpSocket;
        let socket = UdpSocket::bind(addr).await?;
        Ok(UdpServerListener { 
            socket: std::sync::Arc::new(socket),
            connection_map: std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        })
    }
}

/// TCP-specific server listener implementation.
pub struct TcpServerListener {
    listener: tokio::net::TcpListener,
}

#[async_trait]
impl ServerListener<TcpTransport> for TcpServerListener {
    type Connection = TcpConnection;
    
    async fn accept(&mut self) -> io::Result<Self::Connection> {
        let (stream, addr) = self.listener.accept().await?;
        Ok(TcpConnection { stream, addr })
    }
    
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
}

/// TCP connection implementation.
pub struct TcpConnection {
    stream: tokio::net::TcpStream,
    addr: SocketAddr,
}

impl TcpConnection {
    /// Create a new TCP connection from a stream.
    pub fn new(stream: tokio::net::TcpStream, addr: SocketAddr) -> Self {
        Self { stream, addr }
    }
}

#[async_trait]
impl Connection for TcpConnection {
    async fn send(&mut self, data: &[u8]) -> io::Result<()> {
        use tokio::io::AsyncWriteExt;
        // For simplicity, we'll send length-prefixed messages
        let len = data.len() as u32;
        self.stream.write_all(&len.to_be_bytes()).await?;
        self.stream.write_all(data).await?;
        Ok(())
    }
    
    async fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
        use tokio::io::AsyncReadExt;
        // Read length prefix
        let mut len_bytes = [0u8; 4];
        match self.stream.read_exact(&mut len_bytes).await {
            Ok(_) => {},
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }
        
        let len = u32::from_be_bytes(len_bytes) as usize;
        if len > 10_000_000 { // 10MB limit
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Message too large"));
        }
        
        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data).await?;
        Ok(Some(data))
    }
    
    fn remote_addr(&self) -> SocketAddr {
        self.addr
    }
    
    async fn close(&mut self) -> io::Result<()> {
        use tokio::io::AsyncWriteExt;
        self.stream.shutdown().await
    }
}

/// UDP-specific server listener implementation.
/// This creates "virtual connections" for UDP by tracking peer addresses.
pub struct UdpServerListener {
    socket: std::sync::Arc<tokio::net::UdpSocket>,
    connection_map: std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<SocketAddr, tokio::sync::mpsc::Sender<Vec<u8>>>>>,
}

#[async_trait]
impl ServerListener<UdpTransport> for UdpServerListener {
    type Connection = UdpConnection;
    
    async fn accept(&mut self) -> io::Result<Self::Connection> {
        let socket = self.socket.clone();
        let mut buf = vec![0u8; 65536];
        
        let (len, addr) = socket.recv_from(&mut buf).await?;
        buf.truncate(len);
        
        // Create a virtual connection for this peer
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        {
            let mut map = self.connection_map.lock().await;
            map.insert(addr, tx.clone());
        }
        
        Ok(UdpConnection {
            socket,
            peer_addr: addr,
            receiver: tokio::sync::Mutex::new(rx),
            connection_map: self.connection_map.clone(),
            first_message: Some(buf),
        })
    }
    
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

/// UDP connection implementation that simulates a connection to a specific peer.
pub struct UdpConnection {
    socket: std::sync::Arc<tokio::net::UdpSocket>,
    peer_addr: SocketAddr,
    receiver: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Vec<u8>>>,
    connection_map: std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<SocketAddr, tokio::sync::mpsc::Sender<Vec<u8>>>>>,
    first_message: Option<Vec<u8>>,
}

#[async_trait]
impl Connection for UdpConnection {
    async fn send(&mut self, data: &[u8]) -> io::Result<()> {
        self.socket.send_to(data, self.peer_addr).await?;
        Ok(())
    }
    
    async fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
        // Return the first message if we have it
        if let Some(msg) = self.first_message.take() {
            return Ok(Some(msg));
        }
        
        let mut receiver = self.receiver.lock().await;
        match receiver.recv().await {
            Some(data) => Ok(Some(data)),
            None => Ok(None), // Connection closed
        }
    }
    
    fn remote_addr(&self) -> SocketAddr {
        self.peer_addr
    }
    
    async fn close(&mut self) -> io::Result<()> {
        // Remove from connection map
        let mut map = self.connection_map.lock().await;
        map.remove(&self.peer_addr);
        Ok(())
    }
}