/// Replication channel configuration and management
use super::layer::ReplicationLayer;
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, Instant};

/// Replication channel configuration and state
#[derive(Debug, Clone)]
pub struct ReplicationChannel {
    /// Channel number (0-3)
    pub id: u8,
    /// Name of the channel
    pub name: String,
    /// Description of the channel's purpose
    pub description: String,
    /// Target frequency range for this channel
    pub frequency_range: (f64, f64),
    /// Layers configured for this channel
    pub layers: Vec<ReplicationLayer>,
    /// Last update timestamp
    pub last_update: Option<Instant>,
    /// Statistics for this channel
    pub stats: ChannelStats,
    /// Whether this channel is currently active
    pub active: bool,
}

impl ReplicationChannel {
    /// Creates a new replication channel
    pub fn new(id: u8, name: String, description: String, frequency_range: (f64, f64)) -> Self {
        Self {
            id,
            name,
            description,
            frequency_range,
            layers: Vec::new(),
            last_update: None,
            stats: ChannelStats::default(),
            active: true,
        }
    }

    /// Adds a replication layer to this channel
    pub fn add_layer(&mut self, layer: ReplicationLayer) {
        if layer.channel == self.id {
            self.layers.push(layer);
        }
    }

    /// Checks if the channel is ready for update based on its frequency
    pub fn is_ready_for_update(&self) -> bool {
        if !self.active {
            return false;
        }

        match self.last_update {
            None => true,
            Some(last) => {
                let min_interval = Duration::from_millis((1000.0 / self.frequency_range.1) as u64);
                last.elapsed() >= min_interval
            }
        }
    }

    /// Marks the channel as updated
    pub fn mark_updated(&mut self) {
        self.last_update = Some(Instant::now());
        self.stats.updates_sent += 1;
    }

    /// Sets the channel active/inactive
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Gets the effective frequency based on current conditions
    pub fn get_effective_frequency(&self, load_factor: f64) -> f64 {
        let base_freq = self.frequency_range.1;
        let min_freq = self.frequency_range.0;
        
        // Reduce frequency under load
        let adjusted_freq = base_freq * (1.0 - load_factor * 0.5);
        adjusted_freq.max(min_freq)
    }

    /// Gets the maximum radius for any layer in this channel
    pub fn max_radius(&self) -> f64 {
        self.layers
            .iter()
            .map(|layer| layer.radius)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }

    /// Gets layers that replicate a specific property
    pub fn get_layers_for_property(&self, property: &str) -> Vec<&ReplicationLayer> {
        self.layers
            .iter()
            .filter(|l| l.replicates_property(property))
            .collect()
    }

    /// Optimizes the channel by removing redundant layers
    pub fn optimize(&mut self) {
        // Sort layers by priority
        self.layers.sort_by_key(|l| l.priority);
        
        // Remove layers with duplicate properties and overlapping radii
        let mut optimized_layers = Vec::new();
        for layer in &self.layers {
            let mut is_redundant = false;
            for existing in &optimized_layers {
                if layers_are_redundant(layer, existing) {
                    is_redundant = true;
                    break;
                }
            }
            if !is_redundant {
                optimized_layers.push(layer.clone());
            }
        }
        self.layers = optimized_layers;
    }
}

/// Statistics for a replication channel
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ChannelStats {
    /// Number of updates sent through this channel
    pub updates_sent: u64,
    /// Total bytes transmitted
    pub bytes_transmitted: u64,
    /// Number of subscribers
    pub subscriber_count: usize,
    /// Average update frequency achieved
    pub avg_frequency: f64,
    /// Peak subscriber count
    pub peak_subscriber_count: usize,
    /// Average latency for this channel (milliseconds)
    pub avg_latency_ms: f64,
}

impl ChannelStats {
    /// Updates the average frequency calculation
    pub fn update_frequency(&mut self, actual_frequency: f64) {
        if self.updates_sent == 0 {
            self.avg_frequency = actual_frequency;
        } else {
            // Exponential moving average
            self.avg_frequency = self.avg_frequency * 0.9 + actual_frequency * 0.1;
        }
    }

    /// Records a subscriber count update
    pub fn update_subscriber_count(&mut self, count: usize) {
        self.subscriber_count = count;
        if count > self.peak_subscriber_count {
            self.peak_subscriber_count = count;
        }
    }

    /// Records bytes transmitted
    pub fn add_bytes_transmitted(&mut self, bytes: u64) {
        self.bytes_transmitted += bytes;
    }

    /// Records latency measurement
    pub fn update_latency(&mut self, latency_ms: f64) {
        if self.updates_sent <= 1 {
            self.avg_latency_ms = latency_ms;
        } else {
            // Exponential moving average
            self.avg_latency_ms = self.avg_latency_ms * 0.9 + latency_ms * 0.1;
        }
    }

    /// Gets the efficiency ratio (frequency achieved vs theoretical max)
    pub fn efficiency_ratio(&self, target_frequency: f64) -> f64 {
        if target_frequency > 0.0 {
            self.avg_frequency / target_frequency
        } else {
            1.0
        }
    }
}

/// Helper function to determine if two layers are redundant
fn layers_are_redundant(layer1: &ReplicationLayer, layer2: &ReplicationLayer) -> bool {
    // Layers are redundant if they have overlapping properties and similar radii
    if layer1.channel != layer2.channel {
        return false;
    }
    
    let radius_diff = (layer1.radius - layer2.radius).abs();
    let radius_overlap = radius_diff < (layer1.radius * 0.1); // 10% overlap threshold
    
    let property_overlap = layer1.properties.iter()
        .any(|p| layer2.properties.contains(p));
    
    radius_overlap && property_overlap
}