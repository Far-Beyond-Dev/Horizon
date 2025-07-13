/// Priority queue management for network replication
use super::types::{ReplicationUpdate, NetworkError};
use crate::gorc::channels::ReplicationPriority;
use crate::types::PlayerId;
use std::collections::{HashMap, VecDeque};
use tokio::time::Instant;

/// Priority-based update queue that ensures high-priority updates are sent first
#[derive(Debug)]
pub struct PriorityUpdateQueue {
    /// Queues for each priority level
    queues: HashMap<ReplicationPriority, VecDeque<ReplicationUpdate>>,
    /// Maximum size per priority queue
    max_sizes: HashMap<ReplicationPriority, usize>,
    /// Total updates in all queues
    total_updates: usize,
}

impl PriorityUpdateQueue {
    /// Creates a new priority queue
    pub fn new(max_sizes: HashMap<ReplicationPriority, usize>) -> Self {
        let mut queues = HashMap::new();
        queues.insert(ReplicationPriority::Critical, VecDeque::new());
        queues.insert(ReplicationPriority::High, VecDeque::new());
        queues.insert(ReplicationPriority::Normal, VecDeque::new());
        queues.insert(ReplicationPriority::Low, VecDeque::new());

        Self {
            queues,
            max_sizes,
            total_updates: 0,
        }
    }

    /// Adds an update to the appropriate priority queue
    pub fn push(&mut self, update: ReplicationUpdate) -> bool {
        let priority = update.priority;
        
        if let Some(queue) = self.queues.get_mut(&priority) {
            let max_size = self.max_sizes.get(&priority).copied().unwrap_or(100);
            
            if queue.len() >= max_size {
                // Queue full, drop oldest update
                queue.pop_front();
                self.total_updates = self.total_updates.saturating_sub(1);
            }
            
            queue.push_back(update);
            self.total_updates += 1;
            true
        } else {
            false
        }
    }

    /// Pops the highest priority update
    pub fn pop(&mut self) -> Option<ReplicationUpdate> {
        // Check priorities in order: Critical -> High -> Normal -> Low
        for priority in [ReplicationPriority::Critical, ReplicationPriority::High, 
                        ReplicationPriority::Normal, ReplicationPriority::Low] {
            if let Some(queue) = self.queues.get_mut(&priority) {
                if let Some(update) = queue.pop_front() {
                    self.total_updates = self.total_updates.saturating_sub(1);
                    return Some(update);
                }
            }
        }
        None
    }

    /// Peeks at the highest priority update without removing it
    pub fn peek(&self) -> Option<&ReplicationUpdate> {
        for priority in [ReplicationPriority::Critical, ReplicationPriority::High,
                        ReplicationPriority::Normal, ReplicationPriority::Low] {
            if let Some(queue) = self.queues.get(&priority) {
                if let Some(update) = queue.front() {
                    return Some(update);
                }
            }
        }
        None
    }

    /// Returns the total number of updates in all queues
    pub fn len(&self) -> usize {
        self.total_updates
    }

    /// Returns true if all queues are empty
    pub fn is_empty(&self) -> bool {
        self.total_updates == 0
    }

    /// Clears all queues
    pub fn clear(&mut self) {
        for queue in self.queues.values_mut() {
            queue.clear();
        }
        self.total_updates = 0;
    }

    /// Gets the number of updates in a specific priority queue
    pub fn priority_len(&self, priority: ReplicationPriority) -> usize {
        self.queues.get(&priority).map(|q| q.len()).unwrap_or(0)
    }

    /// Drains up to `count` updates from the highest priority queues
    pub fn drain(&mut self, count: usize) -> Vec<ReplicationUpdate> {
        let mut updates = Vec::with_capacity(count);
        
        for _ in 0..count {
            if let Some(update) = self.pop() {
                updates.push(update);
            } else {
                break;
            }
        }
        
        updates
    }
}

/// Per-player network state
#[derive(Debug)]
pub struct PlayerNetworkState {
    /// Player identifier
    pub player_id: PlayerId,
    /// Update queue for this player
    pub update_queue: PriorityUpdateQueue,
    /// Bandwidth tracking
    pub bytes_sent_this_second: u32,
    /// Last bandwidth reset time
    pub last_bandwidth_reset: Instant,
    /// Current batch being assembled
    pub current_batch: Option<Vec<ReplicationUpdate>>,
    /// Batch creation time
    pub batch_start_time: Option<Instant>,
    /// Sequence number for this player's updates
    pub sequence_counter: u32,
    /// Network statistics for this player
    pub stats: PlayerStats,
}

/// Per-player network statistics
#[derive(Debug, Default)]
pub struct PlayerStats {
    pub updates_sent: u64,
    pub bytes_sent: u64,
    pub updates_dropped: u64,
    pub avg_latency_ms: f32,
    pub packet_loss_rate: f32,
}

impl PlayerNetworkState {
    /// Creates a new player network state
    pub fn new(player_id: PlayerId, max_queue_sizes: HashMap<ReplicationPriority, usize>) -> Self {
        Self {
            player_id,
            update_queue: PriorityUpdateQueue::new(max_queue_sizes),
            bytes_sent_this_second: 0,
            last_bandwidth_reset: Instant::now(),
            current_batch: None,
            batch_start_time: None,
            sequence_counter: 0,
            stats: PlayerStats::default(),
        }
    }

    /// Checks if the player has bandwidth available
    pub fn has_bandwidth(&mut self, bytes_needed: u32, max_bandwidth: u32) -> bool {
        let now = Instant::now();
        
        // Reset bandwidth counter every second
        if now.duration_since(self.last_bandwidth_reset).as_secs() >= 1 {
            self.bytes_sent_this_second = 0;
            self.last_bandwidth_reset = now;
        }
        
        self.bytes_sent_this_second + bytes_needed <= max_bandwidth
    }

    /// Records bandwidth usage
    pub fn consume_bandwidth(&mut self, bytes: u32) {
        self.bytes_sent_this_second += bytes;
        self.stats.bytes_sent += bytes as u64;
    }

    /// Gets the next sequence number
    pub fn next_sequence(&mut self) -> u32 {
        self.sequence_counter = self.sequence_counter.wrapping_add(1);
        self.sequence_counter
    }

    /// Adds an update to this player's queue
    pub fn queue_update(&mut self, update: ReplicationUpdate) -> Result<(), NetworkError> {
        if !self.update_queue.push(update) {
            self.stats.updates_dropped += 1;
            Err(NetworkError::QueueCapacityExceeded { 
                priority: ReplicationPriority::Normal 
            })
        } else {
            Ok(())
        }
    }

    /// Starts a new batch
    pub fn start_batch(&mut self) {
        self.current_batch = Some(Vec::new());
        self.batch_start_time = Some(Instant::now());
    }

    /// Adds an update to the current batch
    pub fn add_to_batch(&mut self, update: ReplicationUpdate) -> bool {
        if let Some(ref mut batch) = self.current_batch {
            batch.push(update);
            true
        } else {
            false
        }
    }

    /// Finishes the current batch and returns it
    pub fn finish_batch(&mut self) -> Option<Vec<ReplicationUpdate>> {
        self.batch_start_time = None;
        self.current_batch.take()
    }

    /// Checks if the current batch should be sent
    pub fn should_send_batch(&self, max_batch_size: usize, max_batch_age_ms: u64) -> bool {
        if let Some(ref batch) = self.current_batch {
            if batch.len() >= max_batch_size {
                return true;
            }
            
            if let Some(start_time) = self.batch_start_time {
                if start_time.elapsed().as_millis() as u64 >= max_batch_age_ms {
                    return true;
                }
            }
        }
        
        false
    }
}