//! Simple test to verify the plugin system compiles and works

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use universal_plugin_system::*;
use universal_plugin_system::plugin::SimplePluginFactory;
use tracing::info;

// Simple event for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestEvent {
    pub message: String,
    pub value: i32,
}

impl Event for TestEvent {
    fn event_type() -> &'static str {
        "test_event"
    }
}

// Simple test plugin
pub struct TestPlugin {
    name: String,
    events_received: u32,
}

impl TestPlugin {
    pub fn new() -> Self {
        Self {
            name: "test_plugin".to_string(),
            events_received: 0,
        }
    }
}

#[async_trait::async_trait]
impl SimplePlugin<StructuredEventKey, AllEqPropagator> for TestPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn register_handlers(
        &mut self,
        _event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
        _context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>,
    ) -> std::result::Result<(), PluginSystemError> {
        // We need to get a mutable reference to register handlers
        // In a real implementation, you'd design this better
        info!("📝 Registering test event handler");
        
        // For this test, we'll just verify the types compile correctly
        Ok(())
    }

    async fn on_init(&mut self, _context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>) -> std::result::Result<(), PluginSystemError> {
        info!("🔧 Test plugin initialized");
        Ok(())
    }

    async fn on_shutdown(&mut self, _context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>) -> std::result::Result<(), PluginSystemError> {
        info!("🛑 Test plugin shutting down. Received {} events", self.events_received);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    info!("🧪 Universal Plugin System - Simple Test");

    // Create the AllEq propagator (most common use case)
    let propagator = AllEqPropagator::new();
    
    // Create event bus with structured keys and AllEq propagation
    #[allow(unused_mut)]
    let mut event_bus = EventBus::with_propagator(propagator);
    let event_bus = Arc::new(event_bus);
    
    // Create context
    let context = Arc::new(PluginContext::new(event_bus.clone()));
    
    // Create plugin manager
    let config = PluginConfig::default();
    let manager = PluginManager::new(event_bus.clone(), context.clone(), config);
    
    // Create and load plugin using factory pattern
    let factory = SimplePluginFactory::<TestPlugin, StructuredEventKey, AllEqPropagator>::new(
        "test_plugin".to_string(),
        "1.0.0".to_string(),
        || TestPlugin::new(),
    );
    
    let plugin_name = manager.load_plugin_from_factory(Box::new(factory)).await?;
    info!("✅ Loaded plugin: {}", plugin_name);
    
    // Test basic event key creation
    let test_key = StructuredEventKey::Core { 
        event_name: "test".into() 
    };
    
    info!("🔑 Created event key: {}", test_key.to_string());
    
    // Test event creation
    let test_event = TestEvent {
        message: "Hello, Plugin System!".to_string(),
        value: 42,
    };
    
    // Create event data
    let event_data = EventData::new(&test_event)?;
    info!("📦 Created event data for: {}", event_data.type_name);
    
    // Test stats
    let stats = event_bus.stats().await;
    info!("📊 Initial stats: {} handlers registered", stats.total_handlers);
    
    // Shutdown
    manager.shutdown().await?;
    
    info!("🏁 Simple test completed successfully!");
    info!("✨ The universal plugin system compiles and basic functionality works!");
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_creation() -> Result<(), Box<dyn std::error::Error>> {
        let plugin = TestPlugin::new();
        assert_eq!(plugin.name(), "test_plugin");
        assert_eq!(plugin.version(), "1.0.0");
        Ok(())
    }

    #[tokio::test]
    async fn test_event_creation() -> Result<(), Box<dyn std::error::Error>> {
        let event = TestEvent {
            message: "test".to_string(),
            value: 123,
        };
        assert_eq!(TestEvent::event_type(), "test_event");
        assert_eq!(event.message, "test");
        assert_eq!(event.value, 123);
        Ok(())
    }

    #[tokio::test]
    async fn test_event_key_creation() -> Result<(), Box<dyn std::error::Error>> {
        let key1 = StructuredEventKey::Core { 
            event_name: "test".into() 
        };
        let key2 = StructuredEventKey::Client { 
            namespace: "chat".into(),
            event_name: "message".into() 
        };
        
        assert_eq!(key1.to_string(), "core:test");
        assert_eq!(key2.to_string(), "client:chat:message");
        
        // Test equality
        let key3 = StructuredEventKey::Core { 
            event_name: "test".into() 
        };
        assert_eq!(key1, key3);
        assert_ne!(key1, key2);
        
        Ok(())
    }

    #[tokio::test]
    async fn test_propagator() -> Result<(), Box<dyn std::error::Error>> {
        let propagator = AllEqPropagator::new();
        
        let key1 = StructuredEventKey::Core { 
            event_name: "test".into() 
        };
        let key2 = StructuredEventKey::Core { 
            event_name: "test".into() 
        };
        let key3 = StructuredEventKey::Core { 
            event_name: "different".into() 
        };
        
        let context1 = PropagationContext::new(key1.clone());
        let context2 = PropagationContext::new(key3.clone());
        
        // Should propagate when keys match
        assert!(propagator.should_propagate(&key2, &context1).await);
        
        // Should not propagate when keys don't match
        assert!(!propagator.should_propagate(&key2, &context2).await);
        
        Ok(())
    }
}