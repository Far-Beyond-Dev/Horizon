# Plugin Development Guide

Learn how to create plugins for the Universal Plugin System, from simple event handlers to complex modular systems.

## Plugin Basics

### What is a Plugin?

A plugin is a self-contained module that:
- Responds to events from the system
- Can emit its own events  
- Has a defined lifecycle (init, run, shutdown)
- Runs in isolation for safety
- Can access shared services through context

### Plugin Types

The system supports two plugin interfaces:

1. **SimplePlugin** - High-level, easy to use (recommended)
2. **Plugin** - Low-level, FFI-compatible (for dynamic loading)

## Creating Your First Plugin

### 1. Define Your Plugin Structure

```rust
use universal_plugin_system::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct ChatPlugin {
    name: String,
    message_count: std::sync::atomic::AtomicU32,
    config: ChatConfig,
}

#[derive(Debug, Clone)]
pub struct ChatConfig {
    pub max_message_length: usize,
    pub allowed_channels: Vec<String>,
    pub spam_threshold: u32,
}

impl ChatPlugin {
    pub fn new(config: ChatConfig) -> Self {
        Self {
            name: "chat_plugin".to_string(),
            message_count: std::sync::atomic::AtomicU32::new(0),
            config,
        }
    }
}
```

### 2. Implement the SimplePlugin Trait

```rust
#[async_trait::async_trait]
impl SimplePlugin<StructuredEventKey, AllEqPropagator> for ChatPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn register_handlers(
        &mut self,
        event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
        context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>,
    ) -> Result<(), PluginSystemError> {
        // Register handler for chat messages
        let chat_key = StructuredEventKey::Client {
            namespace: "chat".into(),
            event_name: "message".into(),
        };

        // Clone data needed by the handler
        let max_length = self.config.max_message_length;
        let allowed_channels = self.config.allowed_channels.clone();
        let counter = self.message_count.clone();
        let event_bus_clone = event_bus.clone();

        event_bus.on_key(chat_key, move |event: ChatMessageEvent| {
            let max_length = max_length;
            let allowed_channels = allowed_channels.clone();
            let counter = counter.clone();
            let event_bus = event_bus_clone.clone();

            async move {
                // Validate message
                if event.message.len() > max_length {
                    eprintln!("❌ Message too long: {} characters", event.message.len());
                    return Ok(());
                }

                if !allowed_channels.contains(&event.channel) {
                    eprintln!("❌ Invalid channel: {}", event.channel);
                    return Ok(());
                }

                // Process message
                println!("💬 [{}] Player {}: {}", 
                    event.channel, 
                    event.player_id, 
                    event.message
                );

                // Update counter
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                // Emit processed message event
                let processed = ChatMessageProcessedEvent {
                    original_message: event.clone(),
                    processed_at: utils::current_timestamp(),
                    plugin_name: "chat_plugin".to_string(),
                };

                let processed_key = StructuredEventKey::Plugin {
                    plugin_name: "chat_plugin".into(),
                    event_name: "message_processed".into(),
                };

                event_bus.emit_key(processed_key, &processed).await?;

                Ok(())
            }
        }).await?;

        println!("📝 Chat plugin registered for chat messages");
        Ok(())
    }

    async fn on_init(
        &mut self, 
        context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>
    ) -> Result<(), PluginSystemError> {
        println!("🔧 Chat plugin initializing...");
        
        // Access context services
        if let Some(logging) = context.get_provider::<LoggingProvider>() {
            logging.log("Chat plugin started".to_string()).await;
        }

        // Perform initialization
        // - Load configuration
        // - Connect to databases
        // - Set up internal state

        println!("✅ Chat plugin initialized successfully");
        Ok(())
    }

    async fn on_shutdown(
        &mut self, 
        _context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>
    ) -> Result<(), PluginSystemError> {
        let count = self.message_count.load(std::sync::atomic::Ordering::Relaxed);
        println!("🛑 Chat plugin shutting down. Processed {} messages", count);
        
        // Cleanup
        // - Save state
        // - Close connections
        // - Release resources

        Ok(())
    }
}
```

### 3. Define Your Events

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageEvent {
    pub player_id: u64,
    pub channel: String,
    pub message: String,
    pub timestamp: u64,
}

impl Event for ChatMessageEvent {
    fn event_type() -> &'static str {
        "chat_message"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageProcessedEvent {
    pub original_message: ChatMessageEvent,
    pub processed_at: u64,
    pub plugin_name: String,
}

impl Event for ChatMessageProcessedEvent {
    fn event_type() -> &'static str {
        "chat_message_processed"
    }
}
```

### 4. Load Your Plugin

```rust
use universal_plugin_system::plugin::SimplePluginFactory;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create event bus
    let event_bus = Arc::new(EventBus::with_propagator(AllEqPropagator::new()));
    
    // Create context
    let context = Arc::new(PluginContext::new(event_bus.clone()));
    
    // Create plugin manager
    let config = PluginConfig::default();
    let manager = PluginManager::new(event_bus.clone(), context.clone(), config);

    // Create plugin factory
    let chat_config = ChatConfig {
        max_message_length: 500,
        allowed_channels: vec!["general".to_string(), "trade".to_string()],
        spam_threshold: 5,
    };

    let factory = SimplePluginFactory::<ChatPlugin, StructuredEventKey, AllEqPropagator>::new(
        "chat_plugin".to_string(),
        "1.0.0".to_string(),
        move || ChatPlugin::new(chat_config.clone()),
    );

    // Load plugin
    let plugin_name = manager.load_plugin_from_factory(Box::new(factory)).await?;
    println!("✅ Loaded plugin: {}", plugin_name);

    // Test the plugin
    let chat_event = ChatMessageEvent {
        player_id: 12345,
        channel: "general".to_string(),
        message: "Hello, world!".to_string(),
        timestamp: utils::current_timestamp(),
    };

    let chat_key = StructuredEventKey::Client {
        namespace: "chat".into(),
        event_name: "message".into(),
    };

    event_bus.emit_key(chat_key, &chat_event).await?;

    // Shutdown when done
    manager.shutdown().await?;
    Ok(())
}
```

## Advanced Plugin Patterns

### State Management

```rust
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

pub struct EconomyPlugin {
    name: String,
    player_balances: Arc<Mutex<HashMap<u64, f64>>>,
    transaction_log: Arc<Mutex<Vec<Transaction>>>,
}

impl EconomyPlugin {
    pub fn new() -> Self {
        Self {
            name: "economy_plugin".to_string(),
            player_balances: Arc::new(Mutex::new(HashMap::new())),
            transaction_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_balance(&self, player_id: u64) -> f64 {
        self.player_balances
            .lock()
            .unwrap()
            .get(&player_id)
            .copied()
            .unwrap_or(1000.0) // Starting balance
    }

    fn update_balance(&self, player_id: u64, amount: f64) -> Result<(), String> {
        let mut balances = self.player_balances.lock().unwrap();
        let current = balances.get(&player_id).copied().unwrap_or(1000.0);
        
        if current + amount < 0.0 {
            return Err("Insufficient funds".to_string());
        }

        balances.insert(player_id, current + amount);
        Ok(())
    }
}
```

### Inter-Plugin Communication

```rust
// Plugin A emits events that Plugin B listens for
#[async_trait::async_trait]
impl SimplePlugin<StructuredEventKey, AllEqPropagator> for EconomyPlugin {
    async fn register_handlers(
        &mut self,
        event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
        _context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>,
    ) -> Result<(), PluginSystemError> {
        // Listen for quest completion events from quest plugin
        let quest_key = StructuredEventKey::Plugin {
            plugin_name: "quest_system".into(),
            event_name: "quest_completed".into(),
        };

        let balances = self.player_balances.clone();
        let event_bus_clone = event_bus.clone();

        event_bus.on_key(quest_key, move |event: QuestCompletedEvent| {
            let balances = balances.clone();
            let event_bus = event_bus_clone.clone();

            async move {
                // Award money for quest completion
                let reward = calculate_quest_reward(&event);
                
                {
                    let mut balances = balances.lock().unwrap();
                    let current = balances.get(&event.player_id).copied().unwrap_or(1000.0);
                    balances.insert(event.player_id, current + reward);
                }

                // Emit economy event
                let transaction = TransactionEvent {
                    player_id: event.player_id,
                    amount: reward,
                    transaction_type: "quest_reward".to_string(),
                    description: format!("Reward for quest: {}", event.quest_name),
                };

                let transaction_key = StructuredEventKey::Plugin {
                    plugin_name: "economy".into(),
                    event_name: "transaction".into(),
                };

                event_bus.emit_key(transaction_key, &transaction).await?;

                Ok(())
            }
        }).await?;

        Ok(())
    }
}
```

### Configuration and Context

```rust
use universal_plugin_system::context::ContextProvider;

// Custom context provider for database access
pub struct DatabaseProvider {
    connection_pool: Arc<DatabasePool>,
}

#[async_trait::async_trait]
impl ContextProvider<StructuredEventKey, AllEqPropagator> for DatabaseProvider {
    async fn provide(&self, _context: &mut PluginContext<StructuredEventKey, AllEqPropagator>) {
        // Plugins can access this through context
    }

    fn name(&self) -> &str {
        "database"
    }
}

// Plugin using context
#[async_trait::async_trait]
impl SimplePlugin<StructuredEventKey, AllEqPropagator> for DataPlugin {
    async fn on_init(
        &mut self, 
        context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>
    ) -> Result<(), PluginSystemError> {
        // Access database through context
        if let Some(db) = context.get_provider::<DatabaseProvider>() {
            // Use database connection
            self.load_data_from_db(db).await?;
        }

        Ok(())
    }
}
```

### Error Handling and Recovery

```rust
#[async_trait::async_trait]
impl SimplePlugin<StructuredEventKey, AllEqPropagator> for RobustPlugin {
    async fn register_handlers(
        &mut self,
        event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
        _context: Arc<PluginContext<StructuredEventKey, AllEqPropagator>>,
    ) -> Result<(), PluginSystemError> {
        let retry_count = Arc::new(std::sync::atomic::AtomicU32::new(0));

        event_bus.on_key(key, move |event: RiskyEvent| {
            let retry_count = retry_count.clone();

            async move {
                let mut attempts = 0;
                const MAX_RETRIES: u32 = 3;

                while attempts < MAX_RETRIES {
                    match process_risky_operation(&event).await {
                        Ok(result) => {
                            println!("✅ Operation succeeded: {:?}", result);
                            retry_count.store(0, std::sync::atomic::Ordering::Relaxed);
                            return Ok(());
                        }
                        Err(e) => {
                            attempts += 1;
                            eprintln!("❌ Attempt {} failed: {}", attempts, e);
                            
                            if attempts < MAX_RETRIES {
                                // Exponential backoff
                                let delay = 2_u64.pow(attempts) * 100;
                                tokio::time::sleep(Duration::from_millis(delay)).await;
                            }
                        }
                    }
                }

                // All retries failed
                let total_failures = retry_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                eprintln!("💥 Operation failed after {} attempts (total failures: {})", 
                    MAX_RETRIES, total_failures);

                Err(EventError::ProcessingFailed(
                    format!("Failed after {} retries", MAX_RETRIES)
                ))
            }
        }).await?;

        Ok(())
    }
}
```

### Plugin Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use universal_plugin_system::plugin::SimplePluginFactory;

    #[tokio::test]
    async fn test_chat_plugin() -> Result<(), Box<dyn std::error::Error>> {
        // Create test environment
        let event_bus = Arc::new(EventBus::with_propagator(AllEqPropagator::new()));
        let context = Arc::new(PluginContext::new(event_bus.clone()));
        let config = PluginConfig::default();
        let manager = PluginManager::new(event_bus.clone(), context.clone(), config);

        // Load plugin
        let chat_config = ChatConfig {
            max_message_length: 100,
            allowed_channels: vec!["test".to_string()],
            spam_threshold: 3,
        };

        let factory = SimplePluginFactory::<ChatPlugin, StructuredEventKey, AllEqPropagator>::new(
            "test_chat".to_string(),
            "1.0.0".to_string(),
            move || ChatPlugin::new(chat_config.clone()),
        );

        let plugin_name = manager.load_plugin_from_factory(Box::new(factory)).await?;
        assert_eq!(plugin_name, "test_chat");

        // Test valid message
        let valid_message = ChatMessageEvent {
            player_id: 123,
            channel: "test".to_string(),
            message: "Hello!".to_string(),
            timestamp: utils::current_timestamp(),
        };

        let chat_key = StructuredEventKey::Client {
            namespace: "chat".into(),
            event_name: "message".into(),
        };

        // This should succeed
        event_bus.emit_key(chat_key.clone(), &valid_message).await?;

        // Test invalid message (too long)
        let invalid_message = ChatMessageEvent {
            player_id: 123,
            channel: "test".to_string(),
            message: "a".repeat(200), // Too long
            timestamp: utils::current_timestamp(),
        };

        // This should be rejected by the plugin
        event_bus.emit_key(chat_key, &invalid_message).await?;

        // Check statistics
        let stats = event_bus.stats().await;
        assert!(stats.events_emitted > 0);

        // Cleanup
        manager.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_plugin_communication() -> Result<(), Box<dyn std::error::Error>> {
        // Test that plugins can communicate via events
        let event_bus = Arc::new(EventBus::with_propagator(AllEqPropagator::new()));
        
        // Set up event counter
        let received_events = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = received_events.clone();

        // Register test handler
        let test_key = StructuredEventKey::Plugin {
            plugin_name: "test_plugin".into(),
            event_name: "test_event".into(),
        };

        event_bus.on_key(test_key.clone(), move |_event: TestEvent| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(())
            }
        }).await?;

        // Emit test event
        let test_event = TestEvent {
            data: "test_data".to_string(),
        };

        event_bus.emit_key(test_key, &test_event).await?;

        // Give handlers time to execute
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Verify event was received
        assert_eq!(received_events.load(std::sync::atomic::Ordering::Relaxed), 1);

        Ok(())
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestEvent {
        data: String,
    }

    impl Event for TestEvent {
        fn event_type() -> &'static str {
            "test_event"
        }
    }
}
```

## Best Practices

### 1. Plugin Design
- **Single Responsibility**: Each plugin should have a clear, focused purpose
- **Loose Coupling**: Minimize dependencies between plugins
- **Event-Driven**: Communicate via events, not direct calls
- **Stateless When Possible**: Prefer stateless designs for easier testing

### 2. Event Design
- **Use Structured Keys**: Leverage the type system for better performance
- **Meaningful Names**: Use clear, descriptive event names
- **Rich Data**: Include all necessary context in event data
- **Versioning**: Plan for event schema evolution

### 3. Error Handling
- **Graceful Degradation**: Handle errors without crashing
- **Logging**: Provide detailed error information
- **Recovery**: Implement retry logic where appropriate
- **Isolation**: Don't let one plugin's errors affect others

### 4. Performance
- **Async Everything**: Use async/await for all I/O operations
- **Batch When Possible**: Group related operations
- **Cache Smartly**: Cache expensive computations
- **Profile**: Measure before optimizing

### 5. Testing
- **Unit Tests**: Test plugin logic in isolation
- **Integration Tests**: Test plugin interactions
- **Mock Services**: Use test doubles for external dependencies
- **Load Testing**: Verify performance under load

This guide should get you started building robust, efficient plugins for the Universal Plugin System!