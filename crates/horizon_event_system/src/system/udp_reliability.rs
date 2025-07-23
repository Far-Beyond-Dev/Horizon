/// UDP reliability and error handling mechanisms
/// Provides acknowledgment, retransmission, and congestion control for UDP events
use crate::events::EventError;
use crate::types::PlayerId;
use super::udp::{UdpEventPacket, UdpCompressionType};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use tokio::time::{Duration, Instant};
use std::net::SocketAddr;
use serde::{Serialize, Deserialize};
use tracing::{debug, warn, error, info};

/// Reliability modes for UDP transmission
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ReliabilityMode {
    /// Fire-and-forget, no acknowledgment required
    Unreliable,
    /// Requires acknowledgment, will retransmit
    Reliable,
    /// Sequenced delivery, drops out-of-order packets
    Sequenced,
    /// Ordered delivery, buffers out-of-order packets
    Ordered,
}

/// Acknowledgment packet for reliable delivery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckPacket {
    /// Sequence number being acknowledged
    pub sequence: u32,
    /// Player ID sending the acknowledgment
    pub player_id: PlayerId,
    /// Timestamp when packet was received
    pub received_at: u64,
    /// Round trip time measurement
    pub rtt_measurement: Option<u32>,
}

/// Pending packet awaiting acknowledgment
#[derive(Debug, Clone)]
pub struct PendingPacket {
    /// The packet data
    pub packet: UdpEventPacket,
    /// Target address
    pub target_addr: SocketAddr,
    /// Send timestamp
    pub sent_at: Instant,
    /// Number of retransmission attempts
    pub retries: u32,
    /// Next retry time
    pub next_retry: Instant,
    /// Reliability mode for this packet
    pub reliability: ReliabilityMode,
}

/// Congestion control state for a connection
#[derive(Debug, Clone)]
pub struct CongestionControl {
    /// Congestion window size (packets)
    pub cwnd: f32,
    /// Slow start threshold
    pub ssthresh: u32,
    /// Round trip time average (ms)
    pub rtt_avg: f32,
    /// Round trip time variance
    pub rtt_var: f32,
    /// Retransmission timeout (ms)
    pub rto: u32,
    /// Congestion control state
    pub state: CongestionState,
    /// Packets in flight
    pub packets_in_flight: u32,
    /// Last congestion event time
    pub last_congestion: Instant,
}

/// Congestion control states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CongestionState {
    SlowStart,
    CongestionAvoidance,
    FastRecovery,
}

impl Default for CongestionControl {
    fn default() -> Self {
        Self {
            cwnd: 1.0,
            ssthresh: 65535,
            rtt_avg: 100.0, // 100ms default
            rtt_var: 50.0,
            rto: DEFAULT_RTO_MS, // 1 second default
            state: CongestionState::SlowStart,
            packets_in_flight: 0,
            last_congestion: Instant::now(),
        }
    }
}

/// Sequence buffer for ordered delivery
#[derive(Debug)]
pub struct SequenceBuffer {
    /// Expected next sequence number
    pub next_expected: u32,
    /// Buffer for out-of-order packets
    pub buffer: HashMap<u32, Vec<u8>>,
    /// Maximum buffer size
    pub max_buffer_size: usize,
}

impl SequenceBuffer {
    pub fn new(max_buffer_size: usize) -> Self {
        Self {
            next_expected: 0,
            buffer: HashMap::new(),
            max_buffer_size,
        }
    }

    /// Process a packet and return any packets ready for delivery
    pub fn process_packet(&mut self, sequence: u32, data: Vec<u8>) -> Vec<(u32, Vec<u8>)> {
        let mut deliverable = Vec::new();

        if sequence == self.next_expected {
            // In-order packet
            deliverable.push((sequence, data));
            self.next_expected = self.next_expected.wrapping_add(1);

            // Check if buffered packets are now deliverable
            while let Some(buffered_data) = self.buffer.remove(&self.next_expected) {
                deliverable.push((self.next_expected, buffered_data));
                self.next_expected = self.next_expected.wrapping_add(1);
            }
        } else if sequence > self.next_expected {
            // Future packet, buffer it if space available
            if self.buffer.len() < self.max_buffer_size {
                self.buffer.insert(sequence, data);
            } else {
                warn!("Sequence buffer full, dropping packet {}", sequence);
            }
        }
        // Ignore past packets (sequence < next_expected)

        deliverable
    }
}

/// UDP reliability manager
pub struct UdpReliabilityManager {
    /// Pending packets awaiting acknowledgment
    pending_packets: Arc<RwLock<HashMap<u32, PendingPacket>>>,
    /// Congestion control per connection
    congestion_control: Arc<RwLock<HashMap<SocketAddr, CongestionControl>>>,
    /// Sequence buffers for ordered delivery
    sequence_buffers: Arc<RwLock<HashMap<SocketAddr, SequenceBuffer>>>,
    /// Maximum retransmission attempts
    max_retries: u32,
    /// Base retransmission timeout
    base_rto: Duration,
    /// Maximum sequence buffer size per connection
    max_sequence_buffer_size: usize,
    /// Running state
    running: Arc<Mutex<bool>>,
}

impl UdpReliabilityManager {
    /// Create a new reliability manager
    pub fn new(
        max_retries: u32,
        base_rto: Duration,
        max_sequence_buffer_size: usize,
    ) -> Self {
        Self {
            pending_packets: Arc::new(RwLock::new(HashMap::new())),
            congestion_control: Arc::new(RwLock::new(HashMap::new())),
            sequence_buffers: Arc::new(RwLock::new(HashMap::new())),
            max_retries,
            base_rto,
            max_sequence_buffer_size,
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Start the reliability manager
    pub async fn start(&self) {
        let mut running = self.running.lock().await;
        if *running {
            return;
        }
        *running = true;
        drop(running);

        info!("Starting UDP reliability manager");

        // Start the retransmission timer
        self.start_retransmission_timer().await;
    }

    /// Stop the reliability manager
    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        *running = false;
        info!("UDP reliability manager stopped");
    }

    /// Track a sent packet for potential retransmission
    pub async fn track_packet(
        &self,
        packet: UdpEventPacket,
        target_addr: SocketAddr,
        reliability: ReliabilityMode,
    ) -> Result<(), EventError> {
        if reliability == ReliabilityMode::Unreliable {
            return Ok(()); // No tracking needed
        }

        let sequence = packet.header.sequence;
        let now = Instant::now();
        
        // Calculate initial RTO based on congestion control
        let rto = {
            let cc_guard = self.congestion_control.read().await;
            cc_guard.get(&target_addr)
                .map(|cc| Duration::from_millis(cc.rto as u64))
                .unwrap_or(self.base_rto)
        };

        let pending = PendingPacket {
            packet,
            target_addr,
            sent_at: now,
            retries: 0,
            next_retry: now + rto,
            reliability,
        };

        let mut pending_packets = self.pending_packets.write().await;
        pending_packets.insert(sequence, pending);

        // Update congestion control
        let mut cc_guard = self.congestion_control.write().await;
        if let Some(cc) = cc_guard.get_mut(&target_addr) {
            cc.packets_in_flight += 1;
        } else {
            let mut cc = CongestionControl::default();
            cc.packets_in_flight = 1;
            cc_guard.insert(target_addr, cc);
        }

        debug!("Tracking packet {} for retransmission", sequence);
        Ok(())
    }

    /// Process an acknowledgment packet
    pub async fn process_ack(&self, ack: &AckPacket, from_addr: SocketAddr) -> Result<(), EventError> {
        let sequence = ack.sequence;
        
        // Remove from pending packets
        let packet_rtt = {
            let mut pending_packets = self.pending_packets.write().await;
            if let Some(pending) = pending_packets.remove(&sequence) {
                Some(pending.sent_at.elapsed())
            } else {
                None
            }
        };

        if let Some(rtt) = packet_rtt {
            debug!("Received ACK for packet {} (RTT: {:?})", sequence, rtt);
            
            // Update congestion control with RTT measurement
            self.update_congestion_control(from_addr, rtt, false).await;
        }

        Ok(())
    }

    /// Process a received packet for ordered delivery
    pub async fn process_received_packet(
        &self,
        sequence: u32,
        data: Vec<u8>,
        from_addr: SocketAddr,
        reliability: ReliabilityMode,
    ) -> Result<Vec<(u32, Vec<u8>)>, EventError> {
        match reliability {
            ReliabilityMode::Unreliable => {
                // No ordering, deliver immediately
                Ok(vec![(sequence, data)])
            }
            ReliabilityMode::Reliable => {
                // Just deliver, no ordering required
                Ok(vec![(sequence, data)])
            }
            ReliabilityMode::Sequenced => {
                // Drop out-of-order packets
                let mut sequence_buffers = self.sequence_buffers.write().await;
                let buffer = sequence_buffers.entry(from_addr)
                    .or_insert_with(|| SequenceBuffer::new(self.max_sequence_buffer_size));
                
                if sequence >= buffer.next_expected {
                    buffer.next_expected = sequence.wrapping_add(1);
                    Ok(vec![(sequence, data)])
                } else {
                    // Drop old packet
                    Ok(vec![])
                }
            }
            ReliabilityMode::Ordered => {
                // Buffer out-of-order packets
                let mut sequence_buffers = self.sequence_buffers.write().await;
                let buffer = sequence_buffers.entry(from_addr)
                    .or_insert_with(|| SequenceBuffer::new(self.max_sequence_buffer_size));
                
                Ok(buffer.process_packet(sequence, data))
            }
        }
    }

    /// Get packets that need retransmission
    pub async fn get_packets_for_retransmission(&self) -> Vec<PendingPacket> {
        let now = Instant::now();
        let mut packets_to_retry = Vec::new();
        
        let mut pending_packets = self.pending_packets.write().await;
        let mut to_remove = Vec::new();
        
        for (sequence, packet) in pending_packets.iter_mut() {
            if now >= packet.next_retry {
                if packet.retries >= self.max_retries {
                    // Maximum retries reached, remove packet
                    to_remove.push(*sequence);
                    warn!("Packet {} exceeded maximum retries, dropping", sequence);
                } else {
                    // Schedule for retransmission
                    packet.retries += 1;
                    
                    // Exponential backoff
                    let backoff_factor = 2_u32.pow(packet.retries.min(5));
                    let rto = self.base_rto * backoff_factor;
                    packet.next_retry = now + rto;
                    
                    packets_to_retry.push(packet.clone());
                    debug!("Retransmitting packet {} (attempt {})", sequence, packet.retries);
                }
            }
        }
        
        // Remove failed packets
        for sequence in to_remove {
            if let Some(failed_packet) = pending_packets.remove(&sequence) {
                // Update congestion control for timeout
                let addr = failed_packet.target_addr;
                drop(pending_packets);
                self.update_congestion_control(addr, Duration::from_secs(10), true).await;
                pending_packets = self.pending_packets.write().await;
            }
        }
        
        packets_to_retry
    }

    /// Update congestion control based on RTT or timeout
    async fn update_congestion_control(
        &self,
        addr: SocketAddr,
        rtt: Duration,
        is_timeout: bool,
    ) {
        let mut cc_guard = self.congestion_control.write().await;
        let cc = cc_guard.entry(addr).or_insert_with(CongestionControl::default);
        
        if is_timeout {
            // Timeout occurred - enter fast recovery
            cc.ssthresh = (cc.cwnd / 2.0).max(1.0) as u32;
            cc.cwnd = 1.0;
            cc.state = CongestionState::SlowStart;
            cc.rto = (cc.rto * 2).min(60000); // Max 60 seconds
            cc.last_congestion = Instant::now();
            cc.packets_in_flight = cc.packets_in_flight.saturating_sub(1);
            warn!("Congestion detected for {}, entering slow start", addr);
        } else {
            // Successful ACK
            cc.packets_in_flight = cc.packets_in_flight.saturating_sub(1);
            
            // Update RTT estimates
            let rtt_ms = rtt.as_millis() as f32;
            if cc.rtt_avg == 0.0 {
                cc.rtt_avg = rtt_ms;
                cc.rtt_var = rtt_ms / 2.0;
            } else {
                let alpha = 0.125; // RFC 6298
                let beta = 0.25;
                cc.rtt_var = (1.0 - beta) * cc.rtt_var + beta * (cc.rtt_avg - rtt_ms).abs();
                cc.rtt_avg = (1.0 - alpha) * cc.rtt_avg + alpha * rtt_ms;
            }
            
            // Update RTO
            cc.rto = ((cc.rtt_avg + 4.0 * cc.rtt_var).max(1000.0) as u32).min(60000);
            
            // Update congestion window
            match cc.state {
                CongestionState::SlowStart => {
                    cc.cwnd += 1.0;
                    if cc.cwnd >= cc.ssthresh as f32 {
                        cc.state = CongestionState::CongestionAvoidance;
                    }
                }
                CongestionState::CongestionAvoidance => {
                    cc.cwnd += 1.0 / cc.cwnd;
                }
                CongestionState::FastRecovery => {
                    cc.state = CongestionState::CongestionAvoidance;
                }
            }
            
            debug!("Updated congestion control for {}: cwnd={:.2}, rtt={:.1}ms, rto={}ms", 
                   addr, cc.cwnd, cc.rtt_avg, cc.rto);
        }
    }

    /// Check if we can send more packets (congestion control)
    pub async fn can_send_packet(&self, addr: SocketAddr) -> bool {
        let cc_guard = self.congestion_control.read().await;
        if let Some(cc) = cc_guard.get(&addr) {
            cc.packets_in_flight < cc.cwnd as u32
        } else {
            true // No congestion control yet, allow sending
        }
    }

    /// Get reliability statistics
    pub async fn get_reliability_stats(&self) -> ReliabilityStats {
        let pending_count = self.pending_packets.read().await.len();
        let connection_count = self.congestion_control.read().await.len();
        let buffer_count = self.sequence_buffers.read().await.len();
        
        ReliabilityStats {
            pending_packets: pending_count,
            active_connections: connection_count,
            sequence_buffers: buffer_count,
            max_retries: self.max_retries,
            base_rto_ms: self.base_rto.as_millis() as u32,
        }
    }

    /// Start the retransmission timer task
    async fn start_retransmission_timer(&self) {
        let pending_packets = self.pending_packets.clone();
        let congestion_control = self.congestion_control.clone();
        let running = self.running.clone();
        let max_retries = self.max_retries;
        let base_rto = self.base_rto;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            
            loop {
                {
                    let running_guard = running.lock().await;
                    if !*running_guard {
                        break;
                    }
                }

                interval.tick().await;
                
                // This would trigger retransmissions in a real implementation
                // For now, we just clean up old entries
                Self::cleanup_old_entries(&pending_packets, &congestion_control, max_retries, base_rto).await;
            }
            
            info!("UDP retransmission timer stopped");
        });
    }

    /// Clean up old congestion control entries and failed packets
    async fn cleanup_old_entries(
        pending_packets: &Arc<RwLock<HashMap<u32, PendingPacket>>>,
        congestion_control: &Arc<RwLock<HashMap<SocketAddr, CongestionControl>>>,
        max_retries: u32,
        base_rto: Duration,
    ) {
        let now = Instant::now();
        let cleanup_threshold = Duration::from_secs(300); // 5 minutes
        
        // Clean up old congestion control entries
        {
            let mut cc_guard = congestion_control.write().await;
            cc_guard.retain(|_addr, cc| {
                now.duration_since(cc.last_congestion) < cleanup_threshold
            });
        }
        
        // Clean up extremely old pending packets
        {
            let mut pending_guard = pending_packets.write().await;
            let old_threshold = now - base_rto * (max_retries + 10);
            pending_guard.retain(|_seq, packet| {
                packet.sent_at > old_threshold
            });
        }
    }
}

/// Reliability statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityStats {
    pub pending_packets: usize,
    pub active_connections: usize,
    pub sequence_buffers: usize,
    pub max_retries: u32,
    pub base_rto_ms: u32,
}

/// Helper function to determine appropriate reliability mode for an event type
pub fn get_reliability_mode_for_event(event_type: &str) -> ReliabilityMode {
    match event_type {
        // Critical events need reliability
        "player_connect" | "player_disconnect" | "authentication" => ReliabilityMode::Reliable,
        
        // Position updates benefit from sequencing
        "position_update" | "movement" => ReliabilityMode::Sequenced,
        
        // Chat and actions need ordering
        "chat_message" | "action" | "command" => ReliabilityMode::Ordered,
        
        // Everything else is unreliable by default for performance
        _ => ReliabilityMode::Unreliable,
    }
}