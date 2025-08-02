//! Application-specific propagators for event classes
//!
//! This demonstrates how host applications should implement custom propagators
//! for their specific event classes and use cases. These propagators should NOT
//! be part of the universal system itself.

use universal_plugin_system::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// GORC-specific propagator that only handles GORC class events with spatial awareness
/// 
/// This propagator demonstrates how to create class-specific propagation logic.
/// It only processes events that follow the GORC class structure and have spatial awareness enabled.
#[derive(Debug)]
pub struct GorcSpatialPropagator {
    /// Maximum distance for spatial propagation
    max_distance: f32,
    /// Player positions (in a real implementation, this would come from game state)
    player_positions: std::sync::Arc<tokio::sync::RwLock<HashMap<String, (f32, f32, f32)>>>,
}

impl GorcSpatialPropagator {
    /// Create a new GORC spatial propagator
    pub fn new(max_distance: f32) -> Self {
        Self {
            max_distance,
            player_positions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Update a player's position
    pub async fn update_player_position(&self, player_id: &str, x: f32, y: f32, z: f32) {
        let mut positions = self.player_positions.write().await;
        positions.insert(player_id.to_string(), (x, y, z));
    }

    /// Check if this event key is a GORC class event with spatial awareness
    fn is_gorc_spatial_event(&self, event_key: &StructuredEventKey) -> bool {
        // GORC class structure: [domain, object_type, channel, event_name, spatial_aware]
        if event_key.segments.len() == 5 {
            if let Some(spatial_aware_str) = event_key.segments.get(4) {
                return spatial_aware_str.as_str() == "true";
            }
        }
        false
    }

    /// Extract spatial metadata from a GORC class event key
    fn extract_gorc_metadata(&self, event_key: &StructuredEventKey) -> Option<(String, String, u8)> {
        if event_key.segments.len() == 5 {
            let domain = event_key.segments[0].to_string();
            let object_type = event_key.segments[1].to_string();
            let channel = event_key.segments[2].parse::<u8>().ok()?;
            return Some((domain, object_type, channel));
        }
        None
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
impl EventPropagator<StructuredEventKey> for GorcSpatialPropagator {
    async fn should_propagate(&self, event_key: &StructuredEventKey, context: &PropagationContext<StructuredEventKey>) -> bool {
        // Only handle GORC spatial events
        if !self.is_gorc_spatial_event(event_key) {
            return true; // Allow non-GORC events to pass through
        }

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
        event: Arc<event::EventData>,
        context: &PropagationContext<StructuredEventKey>,
    ) -> Option<Arc<event::EventData>> {
        // Only transform GORC spatial events
        if !self.is_gorc_spatial_event(&context.event_key) {
            return Some(event); // Pass through non-GORC events unchanged
        }

        // Add GORC-specific metadata
        if let Some((domain, object_type, channel)) = self.extract_gorc_metadata(&context.event_key) {
            let mut new_event = (*event).clone();
            new_event.metadata.insert("gorc_domain".to_string(), domain);
            new_event.metadata.insert("gorc_object_type".to_string(), object_type);
            new_event.metadata.insert("gorc_channel".to_string(), channel.to_string());
            
            // Add distance if available
            if let Some(source_x) = context.get_metadata("source_x") {
                if let Some(target_player) = context.get_metadata("target_player") {
                    let positions = self.player_positions.read().await;
                    if let (Ok(sx), Some((tx, ty, tz))) = (source_x.parse::<f32>(), positions.get(target_player)) {
                        if let (Some(sy), Some(sz)) = (
                            context.get_metadata("source_y").and_then(|y| y.parse::<f32>().ok()),
                            context.get_metadata("source_z").and_then(|z| z.parse::<f32>().ok()),
                        ) {
                            let distance = Self::distance((sx, sy, sz), (*tx, *ty, *tz));
                            new_event.metadata.insert("distance".to_string(), distance.to_string());
                        }
                    }
                }
            }
            
            return Some(Arc::new(new_event));
        }

        Some(event)
    }
}

/// Priority-based propagator for extended class events
/// 
/// This propagator demonstrates filtering based on priority levels from the extended event class.
/// It only propagates events that meet minimum priority thresholds.
#[derive(Debug)]
pub struct PriorityPropagator {
    /// Minimum priority level required for propagation (0-255)
    min_priority: u8,
}

impl PriorityPropagator {
    /// Create a new priority propagator
    pub fn new(min_priority: u8) -> Self {
        Self { min_priority }
    }

    /// Check if this event key is an extended class event with priority
    fn is_extended_class_event(&self, event_key: &StructuredEventKey) -> bool {
        // Extended class structure: [domain, category, event_name, priority, region, persistent]
        event_key.segments.len() == 6
    }

    /// Extract priority from extended class event key
    fn extract_priority(&self, event_key: &StructuredEventKey) -> Option<u8> {
        if event_key.segments.len() == 6 {
            event_key.segments.get(3)?.parse::<u8>().ok()
        } else {
            None
        }
    }
}

#[async_trait]
impl EventPropagator<StructuredEventKey> for PriorityPropagator {
    async fn should_propagate(&self, event_key: &StructuredEventKey, _context: &PropagationContext<StructuredEventKey>) -> bool {
        // Only filter extended class events
        if !self.is_extended_class_event(event_key) {
            return true; // Allow non-extended events to pass through
        }

        // Check priority level
        match self.extract_priority(event_key) {
            Some(priority) => priority >= self.min_priority,
            None => true, // If can't extract priority, allow by default
        }
    }

    async fn transform_event(
        &self,
        event: Arc<event::EventData>,
        context: &PropagationContext<StructuredEventKey>,
    ) -> Option<Arc<event::EventData>> {
        // Add priority metadata for extended class events
        if self.is_extended_class_event(&context.event_key) {
            if let Some(priority) = self.extract_priority(&context.event_key) {
                let mut new_event = (*event).clone();
                new_event.metadata.insert("extracted_priority".to_string(), priority.to_string());
                
                // Add priority classification
                let priority_class = match priority {
                    0..=63 => "low",
                    64..=191 => "medium", 
                    192..=254 => "high",
                    255 => "critical",
                };
                new_event.metadata.insert("priority_class".to_string(), priority_class.to_string());
                
                return Some(Arc::new(new_event));
            }
        }

        Some(event)
    }
}

/// Region-based propagator for extended class events
/// 
/// This propagator filters events based on region information from extended class events.
/// It only propagates events from allowed regions.
#[derive(Debug)]
pub struct RegionPropagator {
    /// Allowed regions for event propagation
    allowed_regions: Vec<String>,
}

impl RegionPropagator {
    /// Create a new region propagator
    pub fn new(allowed_regions: Vec<&str>) -> Self {
        Self {
            allowed_regions: allowed_regions.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Extract region from extended class event key
    fn extract_region<'a>(&self, event_key: &'a StructuredEventKey) -> Option<&'a str> {
        // Extended class structure: [domain, category, event_name, priority, region, persistent]
        if event_key.segments.len() == 6 {
            event_key.segments.get(4).map(|s| s.as_str())
        } else {
            None
        }
    }

    /// Check if this event key is an extended class event
    fn is_extended_class_event(&self, event_key: &StructuredEventKey) -> bool {
        event_key.segments.len() == 6
    }
}

#[async_trait]
impl EventPropagator<StructuredEventKey> for RegionPropagator {
    async fn should_propagate(&self, event_key: &StructuredEventKey, _context: &PropagationContext<StructuredEventKey>) -> bool {
        // Only filter extended class events
        if !self.is_extended_class_event(event_key) {
            return true; // Allow non-extended events to pass through
        }

        // Check if region is allowed
        match self.extract_region(event_key) {
            Some(region) => self.allowed_regions.contains(&region.to_string()),
            None => true, // If can't extract region, allow by default
        }
    }

    async fn transform_event(
        &self,
        event: Arc<event::EventData>,
        context: &PropagationContext<StructuredEventKey>,
    ) -> Option<Arc<event::EventData>> {
        // Add region metadata for extended class events
        if self.is_extended_class_event(&context.event_key) {
            if let Some(region) = self.extract_region(&context.event_key) {
                let mut new_event = (*event).clone();
                new_event.metadata.insert("extracted_region".to_string(), region.to_string());
                return Some(Arc::new(new_event));
            }
        }

        Some(event)
    }
}

/// Spatial propagator for application-specific spatial event filtering
/// 
/// This propagator demonstrates how to implement spatial event filtering
/// for applications that need location-based event distribution.
#[derive(Debug)]
pub struct SpatialPropagator<K: EventKeyType> {
    /// Maximum distance for event propagation
    max_distance: f32,
    /// Player positions (in a real implementation, this would come from game state)
    player_positions: std::sync::Arc<tokio::sync::RwLock<HashMap<String, (f32, f32, f32)>>>,
    /// Phantom data for the key type
    _phantom: std::marker::PhantomData<K>,
}

impl<K: EventKeyType> SpatialPropagator<K> {
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
impl<K: EventKeyType> EventPropagator<K> for SpatialPropagator<K> {
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
        event: Arc<event::EventData>,
        context: &PropagationContext<K>,
    ) -> Option<Arc<event::EventData>> {
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

/// Channel-based propagator for application-specific channel filtering
/// 
/// This propagator filters events based on replication channels,
/// for applications that need channel-based event distribution.
#[derive(Debug)]
pub struct ChannelPropagator<K: EventKeyType> {
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

impl<K: EventKeyType> ChannelPropagator<K> {
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
impl<K: EventKeyType> EventPropagator<K> for ChannelPropagator<K> {
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

/// Class-aware composite propagator that applies different propagation logic based on event class
/// 
/// This propagator routes events to different propagators based on their class structure,
/// allowing for class-specific optimization and logic.
pub struct ClassAwarePropagator {
    /// Propagator for GORC class events (5 segments with spatial awareness)
    gorc_propagator: Option<Box<dyn EventPropagator<StructuredEventKey>>>,
    /// Propagator for extended class events (6 segments)
    extended_propagator: Option<Box<dyn EventPropagator<StructuredEventKey>>>,
    /// Propagator for custom class events (4 segments)
    custom_propagator: Option<Box<dyn EventPropagator<StructuredEventKey>>>,
    /// Default propagator for all other events
    default_propagator: Box<dyn EventPropagator<StructuredEventKey>>,
}

impl std::fmt::Debug for ClassAwarePropagator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClassAwarePropagator")
            .field("has_gorc_propagator", &self.gorc_propagator.is_some())
            .field("has_extended_propagator", &self.extended_propagator.is_some())
            .field("has_custom_propagator", &self.custom_propagator.is_some())
            .field("has_default_propagator", &true)
            .finish()
    }
}

impl ClassAwarePropagator {
    /// Create a new class-aware propagator with default AllEq behavior
    pub fn new() -> Self {
        Self {
            gorc_propagator: None,
            extended_propagator: None,
            custom_propagator: None,
            default_propagator: Box::new(AllEqPropagator::new()),
        }
    }

    /// Set the propagator for GORC class events
    pub fn with_gorc_propagator(mut self, propagator: Box<dyn EventPropagator<StructuredEventKey>>) -> Self {
        self.gorc_propagator = Some(propagator);
        self
    }

    /// Set the propagator for extended class events
    pub fn with_extended_propagator(mut self, propagator: Box<dyn EventPropagator<StructuredEventKey>>) -> Self {
        self.extended_propagator = Some(propagator);
        self
    }

    /// Set the propagator for custom class events
    pub fn with_custom_propagator(mut self, propagator: Box<dyn EventPropagator<StructuredEventKey>>) -> Self {
        self.custom_propagator = Some(propagator);
        self
    }

    /// Set the default propagator for all other events
    pub fn with_default_propagator(mut self, propagator: Box<dyn EventPropagator<StructuredEventKey>>) -> Self {
        self.default_propagator = propagator;
        self
    }

    /// Determine the event class based on key structure
    fn classify_event(&self, event_key: &StructuredEventKey) -> EventClass {
        match event_key.segments.len() {
            5 => {
                // Check if it's a GORC class event: [domain, object_type, channel, event_name, spatial_aware]
                if let Some(spatial_aware_str) = event_key.segments.get(4) {
                    if spatial_aware_str.as_str() == "true" || spatial_aware_str.as_str() == "false" {
                        return EventClass::Gorc;
                    }
                }
                EventClass::Unknown
            }
            6 => {
                // Check if it's an extended class event: [domain, category, event_name, priority, region, persistent]
                if let Some(priority_str) = event_key.segments.get(3) {
                    if priority_str.parse::<u8>().is_ok() {
                        return EventClass::Extended;
                    }
                }
                EventClass::Unknown
            }
            4 => {
                // Check if it's a custom class event: [domain, event_name, metadata, flag]
                if let Some(flag_str) = event_key.segments.get(3) {
                    if flag_str.as_str() == "true" || flag_str.as_str() == "false" {
                        return EventClass::Custom;
                    }
                }
                EventClass::Unknown
            }
            2 => EventClass::Basic,
            3 => EventClass::Categorized,
            _ => EventClass::Unknown,
        }
    }

    /// Get the appropriate propagator for the event class
    fn get_propagator(&self, class: EventClass) -> &dyn EventPropagator<StructuredEventKey> {
        match class {
            EventClass::Gorc => self.gorc_propagator.as_ref().map(|p| p.as_ref()).unwrap_or(self.default_propagator.as_ref()),
            EventClass::Extended => self.extended_propagator.as_ref().map(|p| p.as_ref()).unwrap_or(self.default_propagator.as_ref()),
            EventClass::Custom => self.custom_propagator.as_ref().map(|p| p.as_ref()).unwrap_or(self.default_propagator.as_ref()),
            _ => self.default_propagator.as_ref(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum EventClass {
    Basic,      // 2 segments: [domain, event_name]
    Categorized, // 3 segments: [domain, category, event_name]
    Custom,     // 4 segments: [domain, event_name, metadata, flag]
    Gorc,       // 5 segments: [domain, object_type, channel, event_name, spatial_aware]
    Extended,   // 6 segments: [domain, category, event_name, priority, region, persistent]
    Unknown,    // Any other structure
}

#[async_trait]
impl EventPropagator<StructuredEventKey> for ClassAwarePropagator {
    async fn should_propagate(&self, event_key: &StructuredEventKey, context: &PropagationContext<StructuredEventKey>) -> bool {
        let event_class = self.classify_event(event_key);
        let propagator = self.get_propagator(event_class);
        propagator.should_propagate(event_key, context).await
    }

    async fn transform_event(
        &self,
        event: Arc<event::EventData>,
        context: &PropagationContext<StructuredEventKey>,
    ) -> Option<Arc<event::EventData>> {
        let event_class = self.classify_event(&context.event_key);
        let propagator = self.get_propagator(event_class);
        propagator.transform_event(event, context).await
    }

    async fn on_propagation_start(&self, event_key: &StructuredEventKey, context: &PropagationContext<StructuredEventKey>) {
        let event_class = self.classify_event(event_key);
        let propagator = self.get_propagator(event_class);
        propagator.on_propagation_start(event_key, context).await;
    }

    async fn on_propagation_end(&self, event_key: &StructuredEventKey, context: &PropagationContext<StructuredEventKey>) {
        let event_class = self.classify_event(event_key);
        let propagator = self.get_propagator(event_class);
        propagator.on_propagation_end(event_key, context).await;
    }
}