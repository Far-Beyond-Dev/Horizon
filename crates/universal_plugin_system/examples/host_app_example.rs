//! Example showing how a host application (like Horizon) can use the universal plugin system
//! to implement their own event-driven plugin architecture.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use universal_plugin_system::*;

// ============================================================================
// Host App: Define Events, Context, and Propagators 
// ============================================================================

// Define host app events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyEvent {
    pub message: String,
    pub timestamp: u64,
}

impl Event for MyEvent {
    fn event_type() -> &'static str {
        "my_event"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatEvent {
    pub user_id: u32,
    pub message: String,
    pub channel: String,
}

impl Event for ChatEvent {
    fn event_type() -> &'static str {
        "chat_event"
    }
}

// Define host app context
pub struct MyContext {
    pub app_name: String,
    pub version: String,
}

// Host app can implement whatever context traits they need
impl MyContext {
    pub fn new(app_name: String, version: String) -> Self {
        Self { app_name, version }
    }

    pub fn log(&self, level: &str, message: &str) {
        println!("[{}] {}: {}", level, self.app_name, message);
    }
}

// Define custom propagator (optional - could use AllEqPropagator)
pub struct MyPropagator;

impl MyPropagator {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl EventPropagator<StructuredEventKey> for MyPropagator {
    async fn should_propagate(&self, _event_key: &StructuredEventKey, _context: &PropagationContext<StructuredEventKey>) -> bool {
        // Custom propagation logic - for this example, propagate everything
        true
    }
}

// ============================================================================
// Host App: Plugin Implementation
// ============================================================================

pub struct MyPlugin {
    name: String,
}

impl MyPlugin {
    pub fn new() -> Self {
        Self {
            name: "my_plugin".to_string(),
        }
    }
}

#[async_trait::async_trait]
impl SimplePlugin<StructuredEventKey, MyPropagator> for MyPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn register_handlers(
        &mut self,
        event_bus: Arc<EventBus<StructuredEventKey, MyPropagator>>,
        context: Arc<PluginContext<StructuredEventKey, MyPropagator>>,
    ) -> std::result::Result<(), PluginSystemError> {
        println!("📡 PHASE 1: Registering handlers for plugin '{}' (no events emitted yet)", self.name);
        
        // Register handler for core events
        event_bus.on("core", "user_login", |event: MyEvent| {
            println!("Plugin: User logged in - {}", event.message);
            Ok(())
        }).await?;

        // Register handler for client events
        event_bus.on_category("client", "chat", "message", |event: ChatEvent| {
            println!("Plugin: Chat message from user {} in {}: {}", 
                     event.user_id, event.channel, event.message);
            Ok(())
        }).await?;

        println!("✅ Plugin '{}' handlers registered successfully! (Still in Phase 1)", self.name);
        
        // IMPORTANT: Do NOT emit events here! Other plugins may not have registered handlers yet.
        // All event emission should happen in on_init() (Phase 2)
        
        Ok(())
    }

    async fn on_init(&mut self, context: Arc<PluginContext<StructuredEventKey, MyPropagator>>) -> std::result::Result<(), PluginSystemError> {
        println!("🔧 PHASE 2: Initializing plugin '{}' (all handlers now registered)", self.name);
        
        // Now it's safe to emit events - all plugins have registered their handlers
        context.event_bus().emit("plugin", "startup", &MyEvent {
            message: format!("Plugin '{}' is starting up", self.name),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }).await.unwrap_or_else(|e| println!("Warning: Failed to emit startup event: {}", e));
        
        println!("✅ Plugin '{}' fully initialized!", self.name);
        Ok(())
    }

    async fn on_shutdown(&mut self, _context: Arc<PluginContext<StructuredEventKey, MyPropagator>>) -> std::result::Result<(), PluginSystemError> {
        println!("🛑 Plugin shutting down");
        Ok(())
    }
}

// ============================================================================
// Host App: Setting Up the System
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    println!("🎯 Universal Plugin System - Host App Example");
    println!("==============================================");

    // Create host app context
    let context = Arc::new(MyContext::new("MyHostApp".to_string(), "1.0.0".to_string()));
    
    // Create event system with custom propagator
    let event_system = UniversalEventSystem::new(context.clone(), MyPropagator::new());

    println!("📝 Registering event handlers...");

    // Host app can register its own handlers
    event_system.on("core", "startup", |event: MyEvent| {
        println!("Host App: System startup - {}", event.message);
        Ok(())
    }).await?;

    // Demonstrate domain-based event handling
    event_system.on("client", "connection", |event: MyEvent| {
        println!("Host App: Client connection - {}", event.message);
        Ok(())
    }).await?;

    println!("📤 Emitting events...");

    // Emit events using the convenient API
    event_system.emit("core", "startup", &MyEvent {
        message: "System is starting up".to_string(),
        timestamp: 1234567890,
    }).await?;

    event_system.emit("core", "user_login", &MyEvent {
        message: "Alice logged in".to_string(),
        timestamp: 1234567891,
    }).await?;

    event_system.emit("client", "connection", &MyEvent {
        message: "New client connected".to_string(),
        timestamp: 1234567892,
    }).await?;

    println!("🔌 Loading plugin using two-phase initialization...");
    println!("   Phase 1: Plugin will register event handlers first");
    println!("   Phase 2: Plugin will initialize after ALL handlers are registered");

    // Create plugin manager and load plugin
    let config = PluginConfig::default();
    let plugin_context = PluginContext::new(event_system.event_bus.clone());
    let manager = PluginManager::new(
        event_system.event_bus.clone(),
        Arc::new(plugin_context),
        config,
    );

    // Load plugin using factory - this will trigger two-phase initialization
    let factory = plugin::SimplePluginFactory::<MyPlugin, StructuredEventKey, MyPropagator>::new(
        "my_plugin".to_string(),
        "1.0.0".to_string(),
        || MyPlugin::new(),
    );

    let plugin_name = manager.load_plugin_from_factory(Box::new(factory)).await?;
    println!("✅ Loaded plugin: {} (two-phase initialization complete)", plugin_name);

    println!("📤 Emitting more events (should reach plugin)...");

    // These events should now be handled by both the host app and the plugin
    event_system.emit("core", "user_login", &MyEvent {
        message: "Bob logged in".to_string(),
        timestamp: 1234567893,
    }).await?;

    // Emit category-based event
    let chat_key = StructuredEventKey::domain_category_event("client", "chat", "message");
    event_system.event_bus.emit_key(chat_key, &ChatEvent {
        user_id: 123,
        message: "Hello everyone!".to_string(),
        channel: "general".to_string(),
    }).await?;

    // Allow time for async event processing
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    println!("📊 Final Statistics:");
    let stats = event_system.event_bus.stats().await;
    println!("   Events emitted: {}", stats.events_emitted);
    println!("   Events handled: {}", stats.events_handled);
    println!("   Handler failures: {}", stats.handler_failures);
    println!("   Total handlers: {}", stats.total_handlers);

    println!("🔄 Shutting down...");
    manager.shutdown().await?;

    println!("\n🎉 SUCCESS! Host App Example Complete");
    println!("=====================================");
    println!("✅ Host app defined its own events, context, and propagator");
    println!("✅ Event domains are fully generic (core, client, etc.)");
    println!("✅ Plugins can register handlers for any domain/event");
    println!("✅ No hardcoded app-specific logic in universal crate");
    println!("✅ Type-safe event handling with minimal boilerplate");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generic_event_system() -> Result<()> {
        let context = Arc::new(MyContext::new("TestApp".to_string(), "1.0.0".to_string()));
        let event_system = UniversalEventSystem::new(context, MyPropagator::new());

        // Test that we can register and emit events with arbitrary domains
        event_system.on("custom_domain", "custom_event", |event: MyEvent| {
            assert_eq!(event.message, "test message");
            Ok(())
        }).await?;

        event_system.emit("custom_domain", "custom_event", &MyEvent {
            message: "test message".to_string(),
            timestamp: 123,
        }).await?;

        Ok(())
    }

    #[test]
    fn test_structured_key_flexibility() {
        // Test that structured keys can represent arbitrary hierarchies
        let core_key = StructuredEventKey::domain_event("core", "startup");
        let client_key = StructuredEventKey::domain_category_event("client", "chat", "message");
        let complex_key = StructuredEventKey::new(vec!["gorc", "Player", "0", "position_update"]);

        assert_eq!(core_key.to_string(), "core:startup");
        assert_eq!(client_key.to_string(), "client:chat:message");
        assert_eq!(complex_key.to_string(), "gorc:Player:0:position_update");

        // Test domain extraction
        assert_eq!(core_key.domain(), Some("core"));
        assert_eq!(client_key.domain(), Some("client"));
        assert_eq!(complex_key.domain(), Some("gorc"));

        // Test pattern matching
        assert!(core_key.matches_pattern(&["core", "startup"]));
        assert!(!core_key.matches_pattern(&["client", "startup"]));
        assert!(client_key.matches_pattern(&["client", "*", "message"]));
    }
}