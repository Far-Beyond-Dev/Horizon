//! Event propagation logic for customizable event routing
//!
//! This module provides the basic propagation traits and universal propagators
//! that work with any event key structure. Application-specific propagators
//! (like spatial filtering, priority filtering, etc.) should be implemented
//! by the host application using these traits.
//!
//! ## Universal Propagators Included
//!
//! - **AllEqPropagator**: Basic exact key matching (works with any key type)
//! - **DefaultPropagator**: Allows all events through (mainly for debugging)
//! - **DomainPropagator**: Filters by domain (first segment of StructuredEventKey)
//! - **UniversalAllEqPropagator**: Exact matching for StructuredEventKey across event classes
//! - **CompositePropagator**: Combines multiple propagators with AND/OR logic
//!
//! ## Application-Specific Propagators
//!
//! Applications should implement their own propagators for specific use cases:
//! - Spatial/GORC propagators for game engines
//! - Priority-based propagators for different event urgency levels
//! - Permission-based propagators for security filtering
//! - Channel-based propagators for network optimization
//! - Class-aware propagators that route based on event structure
//!
//! See the `examples/application_propagators.rs` file for examples of how to implement
//! application-specific propagators outside of the universal system.

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

/// Domain-based propagator that filters by the first segment (domain) of structured event keys
/// 
/// This propagator works with StructuredEventKey to provide efficient domain filtering.
/// It uses the first segment of the key as the domain identifier.
#[derive(Debug)]
pub struct DomainPropagator {
    /// Allowed domains (whitelist mode)
    allowed_domains: Vec<String>,
    /// Blocked domains (blacklist mode) 
    blocked_domains: Vec<String>,
}

impl DomainPropagator {
    /// Create a new domain propagator
    pub fn new() -> Self {
        Self {
            allowed_domains: Vec::new(),
            blocked_domains: Vec::new(),
        }
    }

    /// Allow specific domains (whitelist mode)
    pub fn allow_domains(mut self, domains: Vec<&str>) -> Self {
        self.allowed_domains = domains.into_iter().map(|s| s.to_string()).collect();
        self
    }

    /// Block specific domains (blacklist mode)
    pub fn block_domains(mut self, domains: Vec<&str>) -> Self {
        self.blocked_domains = domains.into_iter().map(|s| s.to_string()).collect();
        self
    }

    /// Extract domain from structured event key (first segment)
    fn extract_domain<'a>(&self, event_key: &'a crate::event::StructuredEventKey) -> Option<&'a str> {
        event_key.domain()
    }
}

#[async_trait]
impl EventPropagator<crate::event::StructuredEventKey> for DomainPropagator {
    async fn should_propagate(&self, event_key: &crate::event::StructuredEventKey, _context: &PropagationContext<crate::event::StructuredEventKey>) -> bool {
        let domain = match self.extract_domain(event_key) {
            Some(domain) => domain,
            None => return false, // No domain means no propagation
        };

        // Check blocklist first
        if self.blocked_domains.iter().any(|blocked| blocked == domain) {
            return false;
        }

        // If allowlist is specified, check it
        if !self.allowed_domains.is_empty() {
            return self.allowed_domains.iter().any(|allowed| allowed == domain);
        }

        // Default: allow if not blocked
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

// ============================================================================
// Universal Propagators - Use AllEqPropagator for exact key matching
// ============================================================================

/// AllEq propagator that works across multiple event classes
/// 
/// This propagator checks that all metadata fields in the event key match exactly.
/// It works with any event class structure, making it truly universal.
#[derive(Debug, Clone, Default)]
pub struct UniversalAllEqPropagator;

impl UniversalAllEqPropagator {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EventPropagator<crate::event::StructuredEventKey> for UniversalAllEqPropagator {
    async fn should_propagate(&self, event_key: &crate::event::StructuredEventKey, context: &PropagationContext<crate::event::StructuredEventKey>) -> bool {
        // Check that all segments match exactly
        if event_key.segments.len() != context.event_key.segments.len() {
            return false;
        }

        for (handler_segment, context_segment) in event_key.segments.iter().zip(context.event_key.segments.iter()) {
            if handler_segment != context_segment {
                return false;
            }
        }

        true
    }
}