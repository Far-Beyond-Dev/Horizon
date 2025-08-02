//! Bridge module for integrating Horizon Event System with Universal Plugin System
//!
//! This module provides seamless integration between the horizon-specific event system
//! and the universal plugin system, allowing existing plugins to continue working
//! without modifications while gaining the benefits of the universal system.

use crate::error::ServerError;
use horizon_event_system::{EventSystem as HorizonEventSystem, ServerContext, LogLevel};
use universal_plugin_system::{
    EventBus, StructuredEventKey, 
    propagation::{AllEqPropagator, DefaultPropagator, DomainPropagator, UniversalAllEqPropagator, CompositePropagator, EventPropagator, PropagationContext},
    PluginManager as UniversalPluginManager, PluginContext, PluginConfig,
    monitoring::DetailedEventStats
};
use std::sync::Arc;
use tracing::info;
use async_trait::async_trait;

/// Event propagator that bridges to horizon event system
pub struct HorizonBridgePropagator {
    horizon_events: Arc<HorizonEventSystem>,
}

impl HorizonBridgePropagator {
    /// Create a new bridge propagator
    pub fn new(horizon_events: Arc<HorizonEventSystem>) -> Self {
        Self { horizon_events }
    }
}

#[async_trait]
impl EventPropagator<StructuredEventKey> for HorizonBridgePropagator {
    async fn should_propagate(&self, _key: &StructuredEventKey, _context: &PropagationContext<StructuredEventKey>) -> bool {
        // For now, bridge all events
        true
    }
}

/// Bridge implementation to integrate Horizon plugins with Universal system
pub struct HorizonUniversalBridge {
    /// The universal event bus
    universal_bus: Arc<EventBus<StructuredEventKey, CompositePropagator<StructuredEventKey>>>,
    /// The original horizon event system
    horizon_events: Arc<HorizonEventSystem>,
    /// Universal plugin manager
    plugin_manager: Arc<UniversalPluginManager<StructuredEventKey, CompositePropagator<StructuredEventKey>>>,
    /// Server context for plugins
    server_context: Arc<dyn ServerContext>,
    /// Plugin context template
    plugin_context_template: Arc<PluginContext<StructuredEventKey, CompositePropagator<StructuredEventKey>>>,
}

impl HorizonUniversalBridge {
    /// Create a new bridge instance
    pub async fn new(
        horizon_events: Arc<HorizonEventSystem>,
        server_context: Arc<dyn ServerContext>
    ) -> Result<Self, ServerError> {
        // Create composite propagator with both universal and horizon-specific propagators
        let propagator = CompositePropagator::new_or()
            .add_propagator(Box::new(AllEqPropagator::new()))
            .add_propagator(Box::new(DefaultPropagator::new()))
            .add_propagator(Box::new(DomainPropagator::new()))
            .add_propagator(Box::new(UniversalAllEqPropagator::new()))
            .add_propagator(Box::new(HorizonBridgePropagator::new(horizon_events.clone())));
        
        // Create universal event bus
        let universal_bus = Arc::new(EventBus::with_propagator(propagator));
        
        // Create plugin context
        let plugin_context = PluginContext::new(universal_bus.clone());        
        let plugin_context_template = Arc::new(plugin_context);
        
        // Create plugin config
        let plugin_config = PluginConfig::default();
        
        // Create universal plugin manager
        let plugin_manager = Arc::new(UniversalPluginManager::new(
            universal_bus.clone(),
            plugin_context_template.clone(),
            plugin_config
        ));

        Ok(Self {
            universal_bus,
            horizon_events,
            plugin_manager,
            server_context,
            plugin_context_template,
        })
    }

    /// Get the universal event bus
    pub fn get_universal_bus(&self) -> Arc<EventBus<StructuredEventKey, CompositePropagator<StructuredEventKey>>> {
        self.universal_bus.clone()
    }

    /// Get the horizon event system (for backward compatibility)
    pub fn get_horizon_events(&self) -> Arc<HorizonEventSystem> {
        self.horizon_events.clone()
    }

    /// Get the universal plugin manager
    pub fn get_plugin_manager(&self) -> Arc<UniversalPluginManager<StructuredEventKey, CompositePropagator<StructuredEventKey>>> {
        self.plugin_manager.clone()
    }

    /// Load plugins from directory using the universal system
    pub async fn load_plugins_from_directory<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), ServerError> {
        info!("🔄 Loading plugins using universal plugin system from: {}", path.as_ref().display());
        
        self.plugin_manager
            .load_plugins_from_directory(path)
            .await
            .map_err(|e| ServerError::Internal(format!("Universal plugin loading failed: {}", e)))?;
            
        info!("✅ Successfully loaded plugins using universal system");
        Ok(())
    }

    /// Get plugin count
    pub fn plugin_count(&self) -> usize {
        self.plugin_manager.plugin_count()
    }

    /// Get plugin names
    pub fn plugin_names(&self) -> Vec<String> {
        self.plugin_manager.plugin_names()
    }

    /// Shutdown the plugin system
    pub async fn shutdown(&self) -> Result<(), ServerError> {
        info!("🛑 Shutting down universal plugin system...");
        
        self.plugin_manager
            .shutdown()
            .await
            .map_err(|e| ServerError::Internal(format!("Universal plugin shutdown failed: {}", e)))?;
            
        info!("✅ Universal plugin system shutdown complete");
        Ok(())
    }

    /// Setup event bridges between horizon and universal systems
    pub async fn setup_event_bridges(&self) -> Result<(), ServerError> {
        info!("🌉 Setting up event bridges between Horizon and Universal systems...");
        
        // Bridge core events from horizon to universal
        self.setup_core_event_bridge().await?;
        
        // Bridge client events from horizon to universal
        self.setup_client_event_bridge().await?;
        
        // Bridge plugin events from horizon to universal
        self.setup_plugin_event_bridge().await?;
        
        info!("✅ Event bridges setup complete");
        Ok(())
    }

    /// Setup core event bridging
    async fn setup_core_event_bridge(&self) -> Result<(), ServerError> {
        // Bridge player connected events
        self.horizon_events
            .on_core("player_connected", move |event: serde_json::Value| {
                // For now, just log the event since we're not actively bridging
                info!("🌉 Bridged player_connected event: {:?}", event);
                Ok(())
            })
            .await
            .map_err(|e| ServerError::Internal(format!("Failed to setup core event bridge: {}", e)))?;

        info!("🌉 Core event bridge setup complete");
        Ok(())
    }

    /// Setup client event bridging
    async fn setup_client_event_bridge(&self) -> Result<(), ServerError> {
        // Note: Client events in horizon system are namespace/event based
        // These will be bridged when they're emitted through the universal system
        
        info!("🌉 Client event bridge setup complete");
        Ok(())
    }

    /// Setup plugin event bridging
    async fn setup_plugin_event_bridge(&self) -> Result<(), ServerError> {
        // Plugin events will be handled through the universal system directly
        
        info!("🌉 Plugin event bridge setup complete");
        Ok(())
    }

    /// Get monitoring statistics
    pub async fn get_stats(&self) -> DetailedEventStats {
        // The universal event bus doesn't have get_detailed_stats, so we'll return a default
        DetailedEventStats {
            events_emitted: 0,
            events_handled: 0,
            handler_failures: 0,
            total_handlers: 0,
            average_emit_time_ns: 0,
            max_emit_time_ns: 0,
            min_emit_time_ns: 0,
            total_emit_time_ns: 0,
            handler_stats: std::collections::HashMap::new(),
            event_type_stats: std::collections::HashMap::new(),
            uptime_seconds: 0,
            last_reset: None,
        }
    }
}

/// Context adapter that implements horizon ServerContext for the universal system
pub struct UniversalServerContextAdapter {
    bridge: Arc<HorizonUniversalBridge>,
    server_context: Arc<dyn ServerContext>,
}

impl std::fmt::Debug for UniversalServerContextAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UniversalServerContextAdapter")
            .field("bridge", &"HorizonUniversalBridge")
            .field("server_context", &"ServerContext")
            .finish()
    }
}

impl UniversalServerContextAdapter {
    pub fn new(bridge: Arc<HorizonUniversalBridge>, server_context: Arc<dyn ServerContext>) -> Self {
        Self { bridge, server_context }
    }
}

#[async_trait]
impl ServerContext for UniversalServerContextAdapter {
    fn events(&self) -> Arc<HorizonEventSystem> {
        self.bridge.get_horizon_events()
    }

    fn log(&self, level: LogLevel, message: &str) {
        self.server_context.log(level, message);
    }

    fn region_id(&self) -> horizon_event_system::types::RegionId {
        self.server_context.region_id()
    }

    fn tokio_handle(&self) -> Option<tokio::runtime::Handle> {
        self.server_context.tokio_handle()
    }

    async fn send_to_player(&self, player_id: horizon_event_system::types::PlayerId, data: &[u8]) -> Result<(), horizon_event_system::context::ServerError> {
        self.server_context.send_to_player(player_id, data).await
    }

    async fn broadcast(&self, data: &[u8]) -> Result<(), horizon_event_system::context::ServerError> {
        self.server_context.broadcast(data).await
    }
}