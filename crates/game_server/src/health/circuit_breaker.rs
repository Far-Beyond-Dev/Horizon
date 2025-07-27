//! Circuit breaker implementation for resilience patterns.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    /// Circuit is closed, allowing all requests
    Closed,
    /// Circuit is open, rejecting all requests
    Open,
    /// Circuit is half-open, allowing limited requests to test recovery
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening the circuit
    pub failure_threshold: u32,
    /// Duration to keep circuit open before trying half-open
    pub timeout_duration: Duration,
    /// Number of successful requests needed in half-open to close circuit
    pub success_threshold: u32,
    /// Window size for tracking failures
    pub window_size: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout_duration: Duration::from_secs(60),
            success_threshold: 3,
            window_size: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Circuit breaker for handling cascading failures
#[derive(Debug)]
pub struct CircuitBreaker {
    name: String,
    config: CircuitBreakerConfig,
    state: Arc<RwLock<CircuitBreakerState>>,
    failure_count: Arc<RwLock<u32>>,
    success_count: Arc<RwLock<u32>>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    last_success_time: Arc<RwLock<Option<Instant>>>,
}

impl CircuitBreaker {
    /// Creates a new circuit breaker with the given name and configuration
    pub fn new(name: String, config: CircuitBreakerConfig) -> Self {
        Self {
            name,
            config,
            state: Arc::new(RwLock::new(CircuitBreakerState::Closed)),
            failure_count: Arc::new(RwLock::new(0)),
            success_count: Arc::new(RwLock::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            last_success_time: Arc::new(RwLock::new(None)),
        }
    }

    /// Gets the name of this circuit breaker
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Checks if the circuit breaker allows the request
    pub async fn can_execute(&self) -> bool {
        let state = self.state.read().await;
        match *state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                // Check if we should transition to half-open
                if let Some(last_failure) = *self.last_failure_time.read().await {
                    if last_failure.elapsed() >= self.config.timeout_duration {
                        drop(state);
                        self.transition_to_half_open().await;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    /// Records a successful operation
    pub async fn record_success(&self) {
        *self.last_success_time.write().await = Some(Instant::now());
        
        let state = self.state.read().await;
        match *state {
            CircuitBreakerState::HalfOpen => {
                let mut success_count = self.success_count.write().await;
                *success_count += 1;
                
                if *success_count >= self.config.success_threshold {
                    drop(state);
                    drop(success_count);
                    self.transition_to_closed().await;
                }
            }
            CircuitBreakerState::Closed => {
                // Reset failure count on success
                *self.failure_count.write().await = 0;
            }
            _ => {}
        }
    }

    /// Records a failed operation
    pub async fn record_failure(&self) {
        *self.last_failure_time.write().await = Some(Instant::now());
        
        let state = self.state.read().await;
        match *state {
            CircuitBreakerState::Closed => {
                let mut failure_count = self.failure_count.write().await;
                *failure_count += 1;
                
                if *failure_count >= self.config.failure_threshold {
                    drop(state);
                    drop(failure_count);
                    self.transition_to_open().await;
                }
            }
            CircuitBreakerState::HalfOpen => {
                // Any failure in half-open state goes back to open
                drop(state);
                self.transition_to_open().await;
            }
            _ => {}
        }
    }

    /// Checks if the circuit breaker is currently open
    pub async fn is_open(&self) -> bool {
        matches!(*self.state.read().await, CircuitBreakerState::Open)
    }

    /// Gets the current state of the circuit breaker
    pub async fn get_state(&self) -> CircuitBreakerState {
        self.state.read().await.clone()
    }

    /// Gets circuit breaker statistics
    pub async fn get_stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            name: self.name.clone(),
            state: self.get_state().await,
            failure_count: *self.failure_count.read().await,
            success_count: *self.success_count.read().await,
            last_failure_time: *self.last_failure_time.read().await,
            last_success_time: *self.last_success_time.read().await,
        }
    }

    /// Manually resets the circuit breaker to closed state
    pub async fn reset(&self) {
        self.transition_to_closed().await;
    }

    /// Transitions the circuit breaker to closed state
    async fn transition_to_closed(&self) {
        *self.state.write().await = CircuitBreakerState::Closed;
        *self.failure_count.write().await = 0;
        *self.success_count.write().await = 0;
        tracing::info!("Circuit breaker '{}' transitioned to CLOSED", self.name);
    }

    /// Transitions the circuit breaker to open state
    async fn transition_to_open(&self) {
        *self.state.write().await = CircuitBreakerState::Open;
        *self.success_count.write().await = 0;
        tracing::warn!("Circuit breaker '{}' transitioned to OPEN", self.name);
    }

    /// Transitions the circuit breaker to half-open state
    async fn transition_to_half_open(&self) {
        *self.state.write().await = CircuitBreakerState::HalfOpen;
        *self.success_count.write().await = 0;
        tracing::info!("Circuit breaker '{}' transitioned to HALF-OPEN", self.name);
    }
}

/// Circuit breaker statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStats {
    pub name: String,
    pub state: CircuitBreakerState,
    pub failure_count: u32,
    pub success_count: u32,
    #[serde(skip)]
    pub last_failure_time: Option<Instant>,
    #[serde(skip)]
    pub last_success_time: Option<Instant>,
}

/// Helper macro for executing operations with circuit breaker protection
#[macro_export]
macro_rules! with_circuit_breaker {
    ($circuit_breaker:expr, $operation:expr) => {{
        if $circuit_breaker.can_execute().await {
            match $operation.await {
                Ok(result) => {
                    $circuit_breaker.record_success().await;
                    Ok(result)
                }
                Err(error) => {
                    $circuit_breaker.record_failure().await;
                    Err(error)
                }
            }
        } else {
            Err(crate::health::circuit_breaker::CircuitBreakerError::Open(
                $circuit_breaker.name().to_string()
            ))
        }
    }};
}

/// Circuit breaker specific errors
#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError {
    #[error("Circuit breaker '{0}' is open")]
    Open(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_circuit_breaker_closed_to_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            timeout_duration: Duration::from_millis(100),
            success_threshold: 2,
            window_size: Duration::from_secs(60),
        };
        
        let cb = CircuitBreaker::new("test".to_string(), config);
        
        // Initially closed
        assert!(cb.can_execute().await);
        assert_eq!(cb.get_state().await, CircuitBreakerState::Closed);
        
        // Record failures
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.get_state().await, CircuitBreakerState::Closed);
        
        // Third failure should open circuit
        cb.record_failure().await;
        assert_eq!(cb.get_state().await, CircuitBreakerState::Open);
        assert!(!cb.can_execute().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout_duration: Duration::from_millis(50),
            success_threshold: 2,
            window_size: Duration::from_secs(60),
        };
        
        let cb = CircuitBreaker::new("test".to_string(), config);
        
        // Trip the circuit breaker
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.get_state().await, CircuitBreakerState::Open);
        
        // Wait for timeout
        sleep(Duration::from_millis(60)).await;
        
        // Should transition to half-open
        assert!(cb.can_execute().await);
        assert_eq!(cb.get_state().await, CircuitBreakerState::HalfOpen);
        
        // Record successes to close circuit
        cb.record_success().await;
        cb.record_success().await;
        assert_eq!(cb.get_state().await, CircuitBreakerState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_failure() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout_duration: Duration::from_millis(50),
            success_threshold: 2,
            window_size: Duration::from_secs(60),
        };
        
        let cb = CircuitBreaker::new("test".to_string(), config);
        
        // Trip the circuit breaker
        cb.record_failure().await;
        assert_eq!(cb.get_state().await, CircuitBreakerState::Open);
        
        // Wait for timeout
        sleep(Duration::from_millis(60)).await;
        
        // Should transition to half-open
        assert!(cb.can_execute().await);
        assert_eq!(cb.get_state().await, CircuitBreakerState::HalfOpen);
        
        // Failure in half-open should go back to open
        cb.record_failure().await;
        assert_eq!(cb.get_state().await, CircuitBreakerState::Open);
    }
}