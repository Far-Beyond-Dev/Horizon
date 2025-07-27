//! Security module for input validation, rate limiting, and protection mechanisms.

use crate::config::SecurityConfig;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

pub mod input_validation;
pub mod rate_limiter;

/// Central security manager for the game server
#[derive(Debug)]
pub struct SecurityManager {
    config: SecurityConfig,
    rate_limiter: rate_limiter::RateLimiter,
    connection_tracker: Arc<RwLock<HashMap<IpAddr, ConnectionInfo>>>,
}

#[derive(Debug, Clone)]
struct ConnectionInfo {
    count: u32,
    last_seen: Instant,
}

impl SecurityManager {
    /// Creates a new security manager with the given configuration
    pub fn new(config: SecurityConfig) -> Self {
        let rate_limiter = rate_limiter::RateLimiter::new(
            config.max_requests_per_minute,
            Duration::from_secs(60),
        );

        Self {
            config,
            rate_limiter,
            connection_tracker: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Validates an incoming connection attempt
    pub async fn validate_connection(&self, ip: IpAddr) -> Result<(), SecurityError> {
        // Check if IP is banned
        if self.config.banned_ips.contains(&ip) {
            return Err(SecurityError::BannedIp(ip));
        }

        // Check connection limits per IP
        if self.config.enable_ddos_protection {
            let mut tracker = self.connection_tracker.write().await;
            let info = tracker.entry(ip).or_insert(ConnectionInfo {
                count: 0,
                last_seen: Instant::now(),
            });

            if info.count >= self.config.max_connections_per_ip {
                return Err(SecurityError::TooManyConnections(ip));
            }

            info.count += 1;
            info.last_seen = Instant::now();
        }

        Ok(())
    }

    /// Validates an incoming message
    pub async fn validate_message(&self, ip: IpAddr, message: &[u8]) -> Result<(), SecurityError> {
        // Check message size
        if message.len() > self.config.max_message_size {
            return Err(SecurityError::MessageTooLarge(message.len()));
        }

        // Apply rate limiting
        if self.config.enable_rate_limiting {
            if !self.rate_limiter.check_rate_limit(ip).await {
                return Err(SecurityError::RateLimitExceeded(ip));
            }
        }

        // Validate message content
        input_validation::validate_json_message(message, &self.config)?;

        Ok(())
    }

    /// Registers a connection disconnect
    pub async fn on_disconnect(&self, ip: IpAddr) {
        if self.config.enable_ddos_protection {
            let mut tracker = self.connection_tracker.write().await;
            if let Some(info) = tracker.get_mut(&ip) {
                info.count = info.count.saturating_sub(1);
                if info.count == 0 {
                    tracker.remove(&ip);
                }
            }
        }
    }

    /// Cleans up stale connection tracking data
    pub async fn cleanup_stale_connections(&self) {
        if !self.config.enable_ddos_protection {
            return;
        }

        let mut tracker = self.connection_tracker.write().await;
        let cutoff = Instant::now() - Duration::from_secs(300); // 5 minutes
        
        tracker.retain(|_, info| info.last_seen > cutoff);
    }

    /// Gets current security statistics
    pub async fn get_stats(&self) -> SecurityStats {
        let connection_count = if self.config.enable_ddos_protection {
            self.connection_tracker.read().await.len()
        } else {
            0
        };

        SecurityStats {
            tracked_ips: connection_count,
            rate_limited_requests: self.rate_limiter.get_blocked_count().await,
            banned_ips: self.config.banned_ips.len(),
        }
    }
}

/// Security-related statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityStats {
    pub tracked_ips: usize,
    pub rate_limited_requests: u64,
    pub banned_ips: usize,
}

/// Security-related errors
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("IP address {0} is banned")]
    BannedIp(IpAddr),
    
    #[error("Too many connections from IP {0}")]
    TooManyConnections(IpAddr),
    
    #[error("Message too large: {0} bytes")]
    MessageTooLarge(usize),
    
    #[error("Rate limit exceeded for IP {0}")]
    RateLimitExceeded(IpAddr),
    
    #[error("Invalid message format: {0}")]
    InvalidMessageFormat(String),
    
    #[error("Malicious content detected")]
    MaliciousContent,
}