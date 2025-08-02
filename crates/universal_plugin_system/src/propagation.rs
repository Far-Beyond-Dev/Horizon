//! Event propagation logic for customizable event routing

use crate::event::EventData;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// Context information for event propagation decisions
#[derive(Debug, Clone)]
pub struct PropagationContext<K: crate::event::EventKeyType> {
    /// The event key being propagated
    pub event_key: K,
    /// Event metadata
    pub metadata: HashMap<String, String>,
}

impl<K: crate::event::EventKeyType> PropagationContext<K> {
    /// Create a new propagation context
    pub fn new(event_key: K) -> Self {
        Self {
            event_key,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the context
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
}

/// Trait for custom event propagation logic
#[async_trait]
pub trait EventPropagator<K: crate::event::EventKeyType>: Send + Sync + 'static {
    /// Determine if an event should be propagated to handlers
    /// 
    /// This method is called for each registered handler to determine
    /// if the event should be delivered to that specific handler.
    /// 
    /// # Arguments
    /// 
    /// * `event_key` - The event key being propagated
    /// * `context` - Additional context for the propagation decision
    /// 
    /// # Returns
    /// 
    /// `true` if the event should be delivered to the handler, `false` otherwise
    async fn should_propagate(&self, event_key: &K, context: &PropagationContext<K>) -> bool;

    /// Optionally transform the event before delivery
    /// 
    /// This method allows the propagator to modify event data based on
    /// the propagation context (e.g., spatial filtering, compression, etc.)
    /// 
    /// # Arguments
    /// 
    /// * `event` - The original event data
    /// * `context` - The propagation context
    /// 
    /// # Returns
    /// 
    /// The transformed event data, or `None` to use the original event
    async fn transform_event(
        &self,
        event: Arc<EventData>,
        _context: &PropagationContext<K>,
    ) -> Option<Arc<EventData>> {
        // Default implementation: no transformation
        Some(event)
    }

    /// Called when propagation begins for an event
    /// 
    /// This hook allows the propagator to perform setup or logging
    /// before event propagation starts.
    async fn on_propagation_start(&self, _event_key: &K, _context: &PropagationContext<K>) {
        // Default implementation: do nothing
    }

    /// Called when propagation ends for an event
    /// 
    /// This hook allows the propagator to perform cleanup or logging
    /// after event propagation completes.
    async fn on_propagation_end(&self, _event_key: &K, _context: &PropagationContext<K>) {
        // Default implementation: do nothing
    }
}

/// AllEq propagator that only propagates when handler and emitter event keys match exactly
/// 
/// This is the most common propagator - handlers only receive events that match
/// their exact registration key. This enforces that on_client and emit_client
/// with the same parameters will interact, etc.
#[derive(Debug, Clone, Default)]
pub struct AllEqPropagator;

impl AllEqPropagator {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl<K: crate::event::EventKeyType> EventPropagator<K> for AllEqPropagator {
    async fn should_propagate(&self, event_key: &K, context: &PropagationContext<K>) -> bool {
        // Only propagate if the keys match exactly
        *event_key == context.event_key
    }
}

/// Default propagator that delivers all events to all handlers
/// 
/// This is mainly useful for debugging or when you want broadcast behavior
#[derive(Debug, Clone, Default)]
pub struct DefaultPropagator;

impl DefaultPropagator {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl<K: crate::event::EventKeyType> EventPropagator<K> for DefaultPropagator {
    async fn should_propagate(&self, _event_key: &K, _context: &PropagationContext<K>) -> bool {
        // Default behavior: propagate all events to all handlers
        true
    }
}

/// Namespace-based propagator that filters by event namespace
/// 
/// Works with StructuredEventKey to provide efficient namespace filtering
#[derive(Debug)]
pub struct NamespacePropagator {
    /// Allowed namespaces
    allowed_namespaces: Vec<crate::event::EventNamespace>,
    /// Blocked namespaces
    blocked_namespaces: Vec<crate::event::EventNamespace>,
}

impl NamespacePropagator {
    /// Create a new namespace propagator
    pub fn new() -> Self {
        Self {
            allowed_namespaces: Vec::new(),
            blocked_namespaces: Vec::new(),
        }
    }

    /// Allow specific namespaces (whitelist mode)
    pub fn allow_namespaces(mut self, namespaces: Vec<crate::event::EventNamespace>) -> Self {
        self.allowed_namespaces = namespaces;
        self
    }

    /// Block specific namespaces (blacklist mode)
    pub fn block_namespaces(mut self, namespaces: Vec<crate::event::EventNamespace>) -> Self {
        self.blocked_namespaces = namespaces;
        self
    }

    /// Extract namespace from structured event key
    fn extract_namespace(&self, event_key: &crate::event::StructuredEventKey) -> crate::event::EventNamespace {
        match event_key {
            crate::event::StructuredEventKey::Core { .. } => crate::event::EventNamespace::Core,
            crate::event::StructuredEventKey::Client { .. } => crate::event::EventNamespace::Client,
            crate::event::StructuredEventKey::Plugin { .. } => crate::event::EventNamespace::Plugin,
            crate::event::StructuredEventKey::Gorc { .. } => crate::event::EventNamespace::Gorc,
            crate::event::StructuredEventKey::GorcInstance { .. } => crate::event::EventNamespace::GorcInstance,
            crate::event::StructuredEventKey::Custom { .. } => crate::event::EventNamespace::Custom(0), // Default custom
        }
    }
}

#[async_trait]
impl EventPropagator<crate::event::StructuredEventKey> for NamespacePropagator {
    async fn should_propagate(&self, event_key: &crate::event::StructuredEventKey, _context: &PropagationContext<crate::event::StructuredEventKey>) -> bool {
        let namespace = self.extract_namespace(event_key);

        // Check blocklist first
        if self.blocked_namespaces.contains(&namespace) {
            return false;
        }

        // If allowlist is specified, check it
        if !self.allowed_namespaces.is_empty() {
            return self.allowed_namespaces.contains(&namespace);
        }

        // Default: allow if not blocked
        true
    }
}

/// Spatial propagator for GORC-like spatial event filtering
/// 
/// This propagator demonstrates how to implement spatial event filtering
/// similar to the GORC system in Horizon.
#[derive(Debug)]
pub struct SpatialPropagator<K: crate::event::EventKeyType> {
    /// Maximum distance for event propagation
    max_distance: f32,
    /// Player positions (in a real implementation, this would come from game state)
    player_positions: std::sync::Arc<tokio::sync::RwLock<HashMap<String, (f32, f32, f32)>>>,
    /// Phantom data for the key type
    _phantom: std::marker::PhantomData<K>,
}

impl<K: crate::event::EventKeyType> SpatialPropagator<K> {
    /// Create a new spatial propagator
    pub fn new(max_distance: f32) -> Self {
        Self {
            max_distance,
            player_positions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Update a player's position
    pub async fn update_player_position(&self, player_id: &str, x: f32, y: f32, z: f32) {
        let mut positions = self.player_positions.write().await;
        positions.insert(player_id.to_string(), (x, y, z));
    }

    /// Calculate distance between two 3D points
    fn distance(pos1: (f32, f32, f32), pos2: (f32, f32, f32)) -> f32 {
        let dx = pos1.0 - pos2.0;
        let dy = pos1.1 - pos2.1;
        let dz = pos1.2 - pos2.2;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

#[async_trait]
impl<K: crate::event::EventKeyType> EventPropagator<K> for SpatialPropagator<K> {
    async fn should_propagate(&self, _event_key: &K, context: &PropagationContext<K>) -> bool {
        // Extract spatial information from the event or context
        let source_pos = match (
            context.get_metadata("source_x").and_then(|x| x.parse::<f32>().ok()),
            context.get_metadata("source_y").and_then(|y| y.parse::<f32>().ok()),
            context.get_metadata("source_z").and_then(|z| z.parse::<f32>().ok()),
        ) {
            (Some(x), Some(y), Some(z)) => (x, y, z),
            _ => return true, // If no spatial info, allow by default
        };

        let target_player = match context.get_metadata("target_player") {
            Some(player) => player,
            None => return true, // If no target player, allow by default
        };

        // Get target player position
        let positions = self.player_positions.read().await;
        let target_pos = match positions.get(target_player) {
            Some(pos) => *pos,
            None => return true, // If player not found, allow by default
        };

        // Check distance
        let distance = Self::distance(source_pos, target_pos);
        distance <= self.max_distance
    }

    async fn transform_event(
        &self,
        event: Arc<EventData>,
        context: &PropagationContext<K>,
    ) -> Option<Arc<EventData>> {
        // Example: Add distance information to the event
        if let Some(source_x) = context.get_metadata("source_x") {
            if let Some(target_player) = context.get_metadata("target_player") {
                let positions = self.player_positions.read().await;
                if let (Ok(sx), Some((tx, ty, tz))) = (source_x.parse::<f32>(), positions.get(target_player)) {
                    if let (Some(sy), Some(sz)) = (
                        context.get_metadata("source_y").and_then(|y| y.parse::<f32>().ok()),
                        context.get_metadata("source_z").and_then(|z| z.parse::<f32>().ok()),
                    ) {
                        let distance = Self::distance((sx, sy, sz), (*tx, *ty, *tz));
                        
                        // Create new event data with distance metadata
                        let mut new_event = (*event).clone();
                        new_event.metadata.insert("distance".to_string(), distance.to_string());
                        
                        return Some(Arc::new(new_event));
                    }
                }
            }
        }

        // No transformation needed
        Some(event)
    }
}

/// Channel-based propagator for GORC-like channel filtering
/// 
/// This propagator filters events based on replication channels,
/// similar to the GORC system.
#[derive(Debug)]
pub struct ChannelPropagator<K: crate::event::EventKeyType> {
    /// Channel configurations
    channel_configs: HashMap<u8, ChannelConfig>,
    /// Phantom data for the key type
    _phantom: std::marker::PhantomData<K>,
}

#[derive(Debug, Clone)]
pub struct ChannelConfig {
    /// Maximum update frequency for this channel
    pub max_frequency: f32,
    /// Maximum distance for this channel
    pub max_distance: f32,
    /// Priority level
    pub priority: u8,
}

impl<K: crate::event::EventKeyType> ChannelPropagator<K> {
    /// Create a new channel propagator
    pub fn new() -> Self {
        Self {
            channel_configs: HashMap::new(),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Add a channel configuration
    pub fn add_channel(mut self, channel: u8, config: ChannelConfig) -> Self {
        self.channel_configs.insert(channel, config);
        self
    }

    /// Extract channel from event key (works only with StructuredEventKey)
    fn extract_channel(&self, _event_key: &K) -> Option<u8> {
        // This is a generic example - in practice you'd implement this for your specific key type
        // For now, just return None for any key type that doesn't contain channel info
        None
    }
}

#[async_trait]
impl<K: crate::event::EventKeyType> EventPropagator<K> for ChannelPropagator<K> {
    async fn should_propagate(&self, event_key: &K, context: &PropagationContext<K>) -> bool {
        // Extract channel from event key
        let channel = match self.extract_channel(event_key) {
            Some(ch) => ch,
            None => return true, // If not a channel event, allow by default
        };

        // Get channel configuration
        let config = match self.channel_configs.get(&channel) {
            Some(cfg) => cfg,
            None => return true, // If no config, allow by default
        };

        // Check frequency limits (would need timestamp tracking in a real implementation)
        // For now, just use distance-based filtering
        if let Some(distance_str) = context.get_metadata("distance") {
            if let Ok(distance) = distance_str.parse::<f32>() {
                return distance <= config.max_distance;
            }
        }

        true
    }
}

/// Composite propagator that combines multiple propagators
pub struct CompositePropagator<K: crate::event::EventKeyType> {
    propagators: Vec<Box<dyn EventPropagator<K>>>,
    /// If true, ALL propagators must allow the event (AND logic)
    /// If false, ANY propagator can allow the event (OR logic)
    require_all: bool,
}

impl<K: crate::event::EventKeyType> std::fmt::Debug for CompositePropagator<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositePropagator")
            .field("propagators_count", &self.propagators.len())
            .field("require_all", &self.require_all)
            .finish()
    }
}

impl<K: crate::event::EventKeyType> CompositePropagator<K> {
    /// Create a new composite propagator with AND logic
    pub fn new_and() -> Self {
        Self {
            propagators: Vec::new(),
            require_all: true,
        }
    }

    /// Create a new composite propagator with OR logic
    pub fn new_or() -> Self {
        Self {
            propagators: Vec::new(),
            require_all: false,
        }
    }

    /// Add a propagator to the composite
    pub fn add_propagator(mut self, propagator: Box<dyn EventPropagator<K>>) -> Self {
        self.propagators.push(propagator);
        self
    }
}

#[async_trait]
impl<K: crate::event::EventKeyType> EventPropagator<K> for CompositePropagator<K> {
    async fn should_propagate(&self, event_key: &K, context: &PropagationContext<K>) -> bool {
        if self.propagators.is_empty() {
            return true;
        }

        let mut results = Vec::new();
        for propagator in &self.propagators {
            results.push(propagator.should_propagate(event_key, context).await);
        }

        if self.require_all {
            // AND logic: all must be true
            results.iter().all(|&result| result)
        } else {
            // OR logic: any must be true
            results.iter().any(|&result| result)
        }
    }

    async fn transform_event(
        &self,
        mut event: Arc<EventData>,
        context: &PropagationContext<K>,
    ) -> Option<Arc<EventData>> {
        // Apply transformations from all propagators in sequence
        for propagator in &self.propagators {
            if let Some(transformed) = propagator.transform_event(event.clone(), context).await {
                event = transformed;
            }
        }
        Some(event)
    }

    async fn on_propagation_start(&self, event_key: &K, context: &PropagationContext<K>) {
        for propagator in &self.propagators {
            propagator.on_propagation_start(event_key, context).await;
        }
    }

    async fn on_propagation_end(&self, event_key: &K, context: &PropagationContext<K>) {
        for propagator in &self.propagators {
            propagator.on_propagation_end(event_key, context).await;
        }
    }
}