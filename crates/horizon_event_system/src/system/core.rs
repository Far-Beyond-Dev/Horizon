/// Core EventSystem implementation
use crate::events::{Event, EventHandler, TypedEventHandler, EventError, GorcEvent};
use crate::gorc::instance::{GorcObjectId, GorcInstanceManager, ObjectInstance};
use crate::types::PlayerId;
use super::client::{ClientConnectionRef, ClientResponseSender};
use super::stats::EventSystemStats;
use super::udp::UdpEventSystem;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// The core event system that manages event routing and handler execution.
/// 
/// This is the central hub for all event processing in the system. It provides
/// type-safe event registration and emission with support for different event
/// categories (core, client, plugin, GORC instance events, and UDP events).
pub struct EventSystem {
    /// Map of event keys to their registered handlers
    pub(super) handlers: RwLock<HashMap<String, Vec<Arc<dyn EventHandler>>>>,
    /// System statistics for monitoring
    pub(super) stats: RwLock<EventSystemStats>,
    /// GORC instance manager for object-specific events
    pub(super) gorc_instances: Option<Arc<GorcInstanceManager>>,
    /// Client response sender for connection-aware handlers
    pub(super) client_response_sender: Option<Arc<dyn ClientResponseSender + Send + Sync>>,
    /// UDP event system for binary socket communication
    pub(super) udp_system: Option<Arc<UdpEventSystem>>,
}

impl std::fmt::Debug for EventSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventSystem")
            .field("handlers", &"[handlers]")
            .field("stats", &"[stats]")
            .field("gorc_instances", &self.gorc_instances.is_some())
            .field("client_response_sender", &self.client_response_sender.is_some())
            .field("udp_system", &self.udp_system.is_some())
            .finish()
    }
}

impl EventSystem {
    /// Creates a new event system with no registered handlers.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            stats: RwLock::new(EventSystemStats::default()),
            gorc_instances: None,
            client_response_sender: None,
            udp_system: None,
        }
    }

    /// Creates a new event system with GORC instance manager integration
    pub fn with_gorc(gorc_instances: Arc<GorcInstanceManager>) -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            stats: RwLock::new(EventSystemStats::default()),
            gorc_instances: Some(gorc_instances),
            client_response_sender: None,
            udp_system: None,
        }
    }

    /// Sets the GORC instance manager for this event system
    pub fn set_gorc_instances(&mut self, gorc_instances: Arc<GorcInstanceManager>) {
        self.gorc_instances = Some(gorc_instances);
    }

    /// Sets the client response sender for connection-aware handlers
    pub fn set_client_response_sender(&mut self, sender: Arc<dyn ClientResponseSender + Send + Sync>) {
        self.client_response_sender = Some(sender);
    }

    /// Gets the client response sender if available
    pub fn get_client_response_sender(&self) -> Option<Arc<dyn ClientResponseSender + Send + Sync>> {
        self.client_response_sender.clone()
    }

    /// Sets the UDP event system for socket communication
    pub fn set_udp_system(&mut self, udp_system: Arc<UdpEventSystem>) {
        self.udp_system = Some(udp_system);
    }

    /// Gets the UDP event system if available
    pub fn get_udp_system(&self) -> Option<Arc<UdpEventSystem>> {
        self.udp_system.clone()
    }

    /// Gets the current event system statistics
    pub async fn get_stats(&self) -> EventSystemStats {
        self.stats.read().await.clone()
    }
}

impl Default for EventSystem {
    fn default() -> Self {
        Self::new()
    }
}