/// Core EventSystem implementation
use crate::events::EventHandler;
use crate::gorc::instance::GorcInstanceManager;
use super::client::ClientResponseSender;
use super::stats::EventSystemStats;
use std::sync::Arc;
use dashmap::DashMap;
use smallvec::SmallVec;
use compact_str::CompactString;
use super::cache::SerializationBufferPool;

/// The core event system that manages event routing and handler execution.
/// 
/// This is the central hub for all event processing in the system. It provides
/// type-safe event registration and emission with support for different event
/// categories (core, client, plugin, and GORC instance events).
/// 
/// Uses DashMap for lock-free concurrent access to handlers, significantly improving
/// performance under high concurrency by eliminating reader-writer lock contention.
/// Uses SmallVec to eliminate heap allocations for the common case of 1-4 handlers per event.
pub struct EventSystem {
    /// Lock-free map of event keys to their registered handlers (optimized with SmallVec + CompactString)  
    pub(super) handlers: DashMap<CompactString, SmallVec<[Arc<dyn EventHandler>; 4]>>,
    /// System statistics for monitoring (kept as RwLock for atomic updates)
    pub(super) stats: tokio::sync::RwLock<EventSystemStats>,
    /// High-performance serialization buffer pool to reduce allocations
    pub(super) serialization_pool: SerializationBufferPool,
    /// GORC instance manager for object-specific events
    pub(super) gorc_instances: Option<Arc<GorcInstanceManager>>,
    /// Client response sender for connection-aware handlers
    pub(super) client_response_sender: Option<Arc<dyn ClientResponseSender + Send + Sync>>,
}

impl std::fmt::Debug for EventSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventSystem")
            .field("handlers", &"[handlers]")
            .field("stats", &"[stats]")
            .field("gorc_instances", &self.gorc_instances.is_some())
            .field("client_response_sender", &self.client_response_sender.is_some())
            .finish()
    }
}

impl EventSystem {
    /// Creates a new event system with no registered handlers.
    pub fn new() -> Self {
        Self {
            handlers: DashMap::new(),
            stats: tokio::sync::RwLock::new(EventSystemStats::default()),
            serialization_pool: SerializationBufferPool::default(),
            gorc_instances: None,
            client_response_sender: None,
        }
    }

    /// Creates a new event system with GORC instance manager integration
    pub fn with_gorc(gorc_instances: Arc<GorcInstanceManager>) -> Self {
        Self {
            handlers: DashMap::new(),
            stats: tokio::sync::RwLock::new(EventSystemStats::default()),
            serialization_pool: SerializationBufferPool::default(),
            gorc_instances: Some(gorc_instances),
            client_response_sender: None,
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
    #[inline]
    pub fn get_client_response_sender(&self) -> Option<Arc<dyn ClientResponseSender + Send + Sync>> {
        self.client_response_sender.clone()
    }

    /// Gets the current event system statistics
    #[inline]
    pub async fn get_stats(&self) -> EventSystemStats {
        self.stats.read().await.clone()
    }
}

impl Default for EventSystem {
    fn default() -> Self {
        Self::new()
    }
}