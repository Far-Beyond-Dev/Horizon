/// Event system module - broken down into manageable components
mod client;
mod core;
mod emitters;
mod handlers;
mod management;
mod stats;
mod tests;
mod udp;
mod udp_reliability;
#[cfg(test)]
mod udp_tests;

// Re-export all public items from submodules
pub use client::{ClientConnectionRef, ClientResponseSender, ClientConnectionInfo};
pub use core::EventSystem;
pub use stats::{EventSystemStats, DetailedEventSystemStats, HandlerCategoryStats};
pub use udp::{
    UdpEventSystem, UdpEventSystemExt, UdpEventHandler, UdpEventPacket, UdpConnection,
    UdpConnectionState, UdpStats, BinaryEventSerializer, BinaryEventDeserializer,
    JsonBinarySerializer, JsonBinaryDeserializer, UdpCompressionType, UdpEventHeader,
};
pub use udp_reliability::{
    UdpReliabilityManager, ReliabilityMode, AckPacket, ReliabilityStats,
    get_reliability_mode_for_event,
};

// Re-export utility functions
use crate::gorc::instance::GorcInstanceManager;
use std::sync::Arc;

/// Helper function to create an event system with GORC integration
pub fn create_event_system_with_gorc(
    gorc_instances: Arc<GorcInstanceManager>
) -> Arc<EventSystem> {
    Arc::new(EventSystem::with_gorc(gorc_instances))
}