//! Rate limiting implementation using token bucket algorithm.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Token bucket rate limiter for controlling request rates
#[derive(Debug)]
pub struct RateLimiter {
    buckets: Arc<RwLock<HashMap<IpAddr, TokenBucket>>>,
    max_tokens: u32,
    refill_interval: Duration,
    blocked_count: Arc<std::sync::atomic::AtomicU64>,
}

#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: u32,
    last_refill: Instant,
}

impl RateLimiter {
    /// Creates a new rate limiter with the specified parameters
    pub fn new(max_tokens: u32, refill_interval: Duration) -> Self {
        Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            max_tokens,
            refill_interval,
            blocked_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Checks if a request from the given IP should be allowed
    pub async fn check_rate_limit(&self, ip: IpAddr) -> bool {
        let mut buckets = self.buckets.write().await;
        let now = Instant::now();

        let bucket = buckets.entry(ip).or_insert(TokenBucket {
            tokens: self.max_tokens,
            last_refill: now,
        });

        // Refill tokens based on elapsed time
        let elapsed = now.duration_since(bucket.last_refill);
        if elapsed >= self.refill_interval {
            let intervals_passed = elapsed.as_millis() / self.refill_interval.as_millis();
            let tokens_to_add = (intervals_passed as u32).min(self.max_tokens - bucket.tokens);
            bucket.tokens = (bucket.tokens + tokens_to_add).min(self.max_tokens);
            bucket.last_refill = now;
        }

        // Check if we have tokens available
        if bucket.tokens > 0 {
            bucket.tokens -= 1;
            true
        } else {
            self.blocked_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            false
        }
    }

    /// Gets the total number of blocked requests
    pub async fn get_blocked_count(&self) -> u64 {
        self.blocked_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Cleans up old rate limit entries
    pub async fn cleanup_old_entries(&self) {
        let mut buckets = self.buckets.write().await;
        let cutoff = Instant::now() - Duration::from_secs(3600); // 1 hour
        
        buckets.retain(|_, bucket| bucket.last_refill > cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(5, Duration::from_secs(60));
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        // Should allow first 5 requests
        for _ in 0..5 {
            assert!(limiter.check_rate_limit(ip).await);
        }

        // Should block the 6th request
        assert!(!limiter.check_rate_limit(ip).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_refill() {
        let limiter = RateLimiter::new(2, Duration::from_millis(100));
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        // Use up tokens
        assert!(limiter.check_rate_limit(ip).await);
        assert!(limiter.check_rate_limit(ip).await);
        assert!(!limiter.check_rate_limit(ip).await);

        // Wait for refill (extra time for test reliability)
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Should be able to make requests again
        assert!(limiter.check_rate_limit(ip).await);
    }
}