# Advanced Usage Guide

This guide covers advanced patterns, optimizations, and techniques for getting the most out of the Universal Plugin System.

## Table of Contents

- [High-Performance Event Processing](#high-performance-event-processing)
- [Complex Plugin Architectures](#complex-plugin-architectures)
- [Custom Event Bus Implementations](#custom-event-bus-implementations)
- [Memory Management and Optimization](#memory-management-and-optimization)
- [Debugging and Monitoring](#debugging-and-monitoring)
- [Integration Patterns](#integration-patterns)
- [Production Deployment](#production-deployment)

## High-Performance Event Processing

### Event Batching
Process multiple events efficiently:

```rust
use smallvec::SmallVec;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBatch<T: Event> {
    pub events: Vec<T>,
    pub batch_id: String,
    pub timestamp: u64,
}

impl<T: Event> Event for EventBatch<T> {
    fn event_type() -> &'static str {
        "event_batch"
    }
}

// High-performance batch processor
pub struct BatchProcessor<T: Event> {
    pending_events: Arc<Mutex<Vec<T>>>,
    batch_size: usize,
    flush_interval: Duration,
    event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
}

impl<T: Event + Clone + Serialize> BatchProcessor<T> {
    pub fn new(
        batch_size: usize,
        flush_interval: Duration,
        event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
    ) -> Self {
        let processor = Self {
            pending_events: Arc::new(Mutex::new(Vec::with_capacity(batch_size))),
            batch_size,
            flush_interval,
            event_bus,
        };

        // Start flush timer
        processor.start_flush_timer();
        processor
    }

    pub async fn add_event(&self, event: T) {
        let should_flush = {
            let mut pending = self.pending_events.lock().unwrap();
            pending.push(event);
            pending.len() >= self.batch_size
        };

        if should_flush {
            self.flush_batch().await;
        }
    }

    async fn flush_batch(&self) {
        let events = {
            let mut pending = self.pending_events.lock().unwrap();
            if pending.is_empty() {
                return;
            }
            std::mem::take(&mut *pending)
        };

        let batch = EventBatch {
            events,
            batch_id: uuid::Uuid::new_v4().to_string(),
            timestamp: utils::current_timestamp(),
        };

        let batch_key = StructuredEventKey::Core {
            event_name: "event_batch".into(),
        };

        if let Err(e) = self.event_bus.emit_key(batch_key, &batch).await {
            eprintln!("Failed to emit batch: {}", e);
        }
    }

    fn start_flush_timer(&self) {
        let pending = self.pending_events.clone();
        let interval = self.flush_interval;
        let event_bus = self.event_bus.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            loop {
                interval.tick().await;
                
                let events = {
                    let mut pending = pending.lock().unwrap();
                    if pending.is_empty() {
                        continue;
                    }
                    std::mem::take(&mut *pending)
                };

                if !events.is_empty() {
                    let batch = EventBatch {
                        events,
                        batch_id: uuid::Uuid::new_v4().to_string(),
                        timestamp: utils::current_timestamp(),
                    };

                    let batch_key = StructuredEventKey::Core {
                        event_name: "event_batch".into(),
                    };

                    if let Err(e) = event_bus.emit_key(batch_key, &batch).await {
                        eprintln!("Failed to emit timed batch: {}", e);
                    }
                }
            }
        });
    }
}

// Usage
let batch_processor = BatchProcessor::new(
    100,  // Batch size
    Duration::from_millis(100),  // Flush every 100ms
    event_bus.clone(),
);

// Add events to batch
for event in high_frequency_events {
    batch_processor.add_event(event).await;
}
```

### Lock-Free Event Queues
Use lock-free data structures for maximum performance:

```rust
use crossbeam::queue::SegQueue;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub struct LockFreeEventQueue<T> {
    queue: SegQueue<T>,
    size: AtomicUsize,
    max_size: usize,
    shutdown: AtomicBool,
}

impl<T> LockFreeEventQueue<T> {
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: SegQueue::new(),
            size: AtomicUsize::new(0),
            max_size,
            shutdown: AtomicBool::new(false),
        }
    }

    pub fn try_push(&self, item: T) -> Result<(), T> {
        if self.shutdown.load(Ordering::Acquire) {
            return Err(item);
        }

        let current_size = self.size.load(Ordering::Acquire);
        if current_size >= self.max_size {
            return Err(item); // Queue full
        }

        self.queue.push(item);
        self.size.fetch_add(1, Ordering::Release);
        Ok(())
    }

    pub fn try_pop(&self) -> Option<T> {
        match self.queue.pop() {
            Some(item) => {
                self.size.fetch_sub(1, Ordering::Release);
                Some(item)
            }
            None => None,
        }
    }

    pub fn len(&self) -> usize {
        self.size.load(Ordering::Acquire)
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

// High-performance event worker
pub struct EventWorker {
    queue: Arc<LockFreeEventQueue<(StructuredEventKey, Arc<EventData>)>>,
    event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
    worker_id: usize,
}

impl EventWorker {
    pub fn new(
        queue: Arc<LockFreeEventQueue<(StructuredEventKey, Arc<EventData>)>>,
        event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
        worker_id: usize,
    ) -> Self {
        Self {
            queue,
            event_bus,
            worker_id,
        }
    }

    pub async fn run(&self) {
        println!("🏃 Event worker {} started", self.worker_id);

        while !self.queue.shutdown.load(Ordering::Acquire) {
            // Process events in batches for efficiency
            let mut batch = SmallVec::<[(StructuredEventKey, Arc<EventData>); 32]>::new();
            
            // Collect a batch of events
            for _ in 0..32 {
                match self.queue.try_pop() {
                    Some(event) => batch.push(event),
                    None => break,
                }
            }

            if batch.is_empty() {
                // No events, sleep briefly
                tokio::time::sleep(Duration::from_micros(100)).await;
                continue;
            }

            // Process batch
            let mut futures = FuturesUnordered::new();
            for (key, event_data) in batch {
                let event_bus = self.event_bus.clone();
                futures.push(async move {
                    // This would normally go through the event bus's emit logic
                    // but we're directly processing for maximum performance
                    process_event_direct(event_bus, key, event_data).await
                });
            }

            // Wait for all events in batch to complete
            while let Some(result) = futures.next().await {
                if let Err(e) = result {
                    eprintln!("Worker {} event failed: {}", self.worker_id, e);
                }
            }
        }

        println!("🛑 Event worker {} stopped", self.worker_id);
    }
}
```

### Memory Pool for Events
Reduce allocation overhead:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct EventDataPool {
    pool: Mutex<Vec<Vec<u8>>>,
    max_size: usize,
    buffer_size: usize,
}

impl EventDataPool {
    pub fn new(max_size: usize, buffer_size: usize) -> Self {
        Self {
            pool: Mutex::new(Vec::with_capacity(max_size)),
            max_size,
            buffer_size,
        }
    }

    pub async fn get_buffer(&self) -> Vec<u8> {
        let mut pool = self.pool.lock().await;
        pool.pop().unwrap_or_else(|| Vec::with_capacity(self.buffer_size))
    }

    pub async fn return_buffer(&self, mut buffer: Vec<u8>) {
        buffer.clear();
        if buffer.capacity() <= self.buffer_size * 2 {
            let mut pool = self.pool.lock().await;
            if pool.len() < self.max_size {
                pool.push(buffer);
            }
        }
        // If buffer is too large or pool is full, just drop it
    }
}

// High-performance event data creation
impl EventData {
    pub async fn new_pooled<T: Event + Serialize>(
        event: &T,
        pool: &EventDataPool,
    ) -> Result<Self, EventError> {
        let mut buffer = pool.get_buffer().await;
        
        // Serialize directly into pooled buffer
        serde_json::to_writer(&mut buffer, event)
            .map_err(|e| EventError::SerializationFailed(e.to_string()))?;
        
        Ok(Self {
            data: Arc::new(buffer),
            type_name: T::event_type().to_string(),
            metadata: HashMap::new(),
        })
    }
}
```

## Complex Plugin Architectures

### Plugin Dependencies
Manage plugin load order and dependencies:

```rust
#[derive(Debug, Clone)]
pub struct PluginDependency {
    pub name: String,
    pub version_requirement: String,
    pub optional: bool,
}

#[async_trait::async_trait]
pub trait DependentPlugin<K: EventKeyType, P: EventPropagator<K>>: SimplePlugin<K, P> {
    /// Get plugin dependencies
    fn dependencies(&self) -> Vec<PluginDependency>;
    
    /// Called after all dependencies are loaded
    async fn on_dependencies_ready(
        &mut self,
        dependencies: HashMap<String, &dyn SimplePlugin<K, P>>,
        context: Arc<PluginContext<K, P>>,
    ) -> Result<(), PluginSystemError>;
}

pub struct DependencyResolver<K: EventKeyType, P: EventPropagator<K>> {
    plugins: HashMap<String, Box<dyn DependentPlugin<K, P>>>,
    load_order: Vec<String>,
}

impl<K: EventKeyType, P: EventPropagator<K>> DependencyResolver<K, P> {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            load_order: Vec::new(),
        }
    }

    pub fn add_plugin(&mut self, plugin: Box<dyn DependentPlugin<K, P>>) {
        let name = plugin.name().to_string();
        self.plugins.insert(name, plugin);
    }

    pub fn resolve_dependencies(&mut self) -> Result<(), PluginSystemError> {
        let mut loaded = HashSet::new();
        let mut in_progress = HashSet::new();
        
        for plugin_name in self.plugins.keys() {
            self.resolve_plugin_dependencies(plugin_name, &mut loaded, &mut in_progress)?;
        }
        
        Ok(())
    }

    fn resolve_plugin_dependencies(
        &mut self,
        plugin_name: &str,
        loaded: &mut HashSet<String>,
        in_progress: &mut HashSet<String>,
    ) -> Result<(), PluginSystemError> {
        if loaded.contains(plugin_name) {
            return Ok(());
        }

        if in_progress.contains(plugin_name) {
            return Err(PluginSystemError::CircularDependency(plugin_name.to_string()));
        }

        in_progress.insert(plugin_name.to_string());

        let dependencies = self.plugins.get(plugin_name)
            .ok_or_else(|| PluginSystemError::PluginNotFound(plugin_name.to_string()))?
            .dependencies();

        for dep in dependencies {
            if !dep.optional && !self.plugins.contains_key(&dep.name) {
                return Err(PluginSystemError::MissingDependency {
                    plugin: plugin_name.to_string(),
                    dependency: dep.name,
                });
            }

            if self.plugins.contains_key(&dep.name) {
                self.resolve_plugin_dependencies(&dep.name, loaded, in_progress)?;
            }
        }

        self.load_order.push(plugin_name.to_string());
        loaded.insert(plugin_name.to_string());
        in_progress.remove(plugin_name);

        Ok(())
    }

    pub fn get_load_order(&self) -> &[String] {
        &self.load_order
    }
}
```

### Plugin Communication Protocols
Structured communication between plugins:

```rust
use serde::{Deserialize, Serialize};

// Plugin communication protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginMessage {
    Request {
        request_id: String,
        sender: String,
        target: String,
        method: String,
        params: serde_json::Value,
    },
    Response {
        request_id: String,
        sender: String,
        target: String,
        result: Result<serde_json::Value, String>,
    },
    Notification {
        sender: String,
        event_type: String,
        data: serde_json::Value,
    },
}

impl Event for PluginMessage {
    fn event_type() -> &'static str {
        "plugin_message"
    }
}

// Plugin communication manager
pub struct PluginCommManager {
    pending_requests: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<serde_json::Value>>>>,
    event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
}

impl PluginCommManager {
    pub fn new(event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>) -> Self {
        let manager = Self {
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            event_bus,
        };

        // Register message handler
        manager.setup_message_handler();
        manager
    }

    fn setup_message_handler(&self) {
        let pending = self.pending_requests.clone();
        let event_bus = self.event_bus.clone();

        let message_key = StructuredEventKey::Plugin {
            plugin_name: "comm_manager".into(),
            event_name: "message".into(),
        };

        tokio::spawn(async move {
            event_bus.on_key(message_key, move |message: PluginMessage| {
                let pending = pending.clone();
                async move {
                    match message {
                        PluginMessage::Response { request_id, result, .. } => {
                            let sender = {
                                let mut pending = pending.lock().unwrap();
                                pending.remove(&request_id)
                            };

                            if let Some(sender) = sender {
                                match result {
                                    Ok(value) => {
                                        let _ = sender.send(value);
                                    }
                                    Err(error) => {
                                        eprintln!("Plugin request failed: {}", error);
                                    }
                                }
                            }
                        }
                        _ => {} // Handle other message types
                    }
                    Ok(())
                }
            }).await
        });
    }

    pub async fn call_plugin(
        &self,
        target: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = tokio::sync::oneshot::channel();

        // Store pending request
        {
            let mut pending = self.pending_requests.lock().unwrap();
            pending.insert(request_id.clone(), tx);
        }

        // Send request
        let request = PluginMessage::Request {
            request_id: request_id.clone(),
            sender: "comm_manager".to_string(),
            target: target.to_string(),
            method: method.to_string(),
            params,
        };

        let request_key = StructuredEventKey::Plugin {
            plugin_name: target.into(),
            event_name: "request".into(),
        };

        self.event_bus.emit_key(request_key, &request).await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        // Wait for response with timeout
        match tokio::time::timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err("Request cancelled".to_string()),
            Err(_) => {
                // Cleanup on timeout
                let mut pending = self.pending_requests.lock().unwrap();
                pending.remove(&request_id);
                Err("Request timeout".to_string())
            }
        }
    }
}
```

### Plugin Hot-Reloading
Dynamically reload plugins without system restart:

```rust
use notify::{Watcher, RecursiveMode, watcher};
use std::sync::mpsc;
use std::time::Duration;

pub struct HotReloadManager<K: EventKeyType, P: EventPropagator<K>> {
    plugin_manager: Arc<PluginManager<K, P>>,
    watch_paths: Vec<PathBuf>,
    _watcher: notify::RecommendedWatcher,
}

impl<K: EventKeyType, P: EventPropagator<K>> HotReloadManager<K, P> {
    pub fn new(
        plugin_manager: Arc<PluginManager<K, P>>,
        watch_paths: Vec<PathBuf>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = watcher(tx, Duration::from_secs(1))?;

        for path in &watch_paths {
            watcher.watch(path, RecursiveMode::Recursive)?;
        }

        let manager_clone = plugin_manager.clone();
        tokio::spawn(async move {
            Self::watch_for_changes(rx, manager_clone).await;
        });

        Ok(Self {
            plugin_manager,
            watch_paths,
            _watcher: watcher,
        })
    }

    async fn watch_for_changes(
        rx: mpsc::Receiver<notify::DebouncedEvent>,
        plugin_manager: Arc<PluginManager<K, P>>,
    ) {
        for event in rx {
            match event {
                notify::DebouncedEvent::Write(path) | 
                notify::DebouncedEvent::Create(path) => {
                    if let Some(plugin_name) = Self::extract_plugin_name(&path) {
                        println!("🔄 Plugin {} changed, reloading...", plugin_name);
                        
                        // Unload existing plugin
                        if plugin_manager.is_plugin_loaded(&plugin_name) {
                            if let Err(e) = plugin_manager.unload_plugin(&plugin_name).await {
                                eprintln!("Failed to unload plugin {}: {}", plugin_name, e);
                                continue;
                            }
                        }

                        // Reload plugin
                        match plugin_manager.load_plugin_from_path(&path).await {
                            Ok(_) => println!("✅ Plugin {} reloaded successfully", plugin_name),
                            Err(e) => eprintln!("❌ Failed to reload plugin {}: {}", plugin_name, e),
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn extract_plugin_name(path: &Path) -> Option<String> {
        path.file_stem()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
    }
}
```

## Custom Event Bus Implementations

### Distributed Event Bus
Route events across multiple processes or machines:

```rust
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use futures_util::{SinkExt, StreamExt};

pub struct DistributedEventBus<K: EventKeyType, P: EventPropagator<K>> {
    local_bus: Arc<EventBus<K, P>>,
    network_nodes: Arc<RwLock<HashMap<String, NetworkNode>>>,
    node_id: String,
    listen_addr: SocketAddr,
}

#[derive(Debug, Clone)]
pub struct NetworkNode {
    pub id: String,
    pub addr: SocketAddr,
    pub connection: Option<Arc<Mutex<Framed<TcpStream, LengthDelimitedCodec>>>>,
    pub last_seen: Instant,
}

impl<K: EventKeyType + Serialize + for<'de> Deserialize<'de>, P: EventPropagator<K>> DistributedEventBus<K, P> {
    pub fn new(
        local_bus: Arc<EventBus<K, P>>,
        node_id: String,
        listen_addr: SocketAddr,
    ) -> Self {
        Self {
            local_bus,
            network_nodes: Arc::new(RwLock::new(HashMap::new())),
            node_id,
            listen_addr,
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Start network listener
        let listener = TcpListener::bind(self.listen_addr).await?;
        println!("🌐 Distributed event bus listening on {}", self.listen_addr);

        let nodes = self.network_nodes.clone();
        let local_bus = self.local_bus.clone();

        tokio::spawn(async move {
            while let Ok((stream, addr)) = listener.accept().await {
                println!("📡 New connection from {}", addr);
                let nodes = nodes.clone();
                let local_bus = local_bus.clone();

                tokio::spawn(async move {
                    Self::handle_connection(stream, nodes, local_bus).await;
                });
            }
        });

        Ok(())
    }

    async fn handle_connection(
        stream: TcpStream,
        nodes: Arc<RwLock<HashMap<String, NetworkNode>>>,
        local_bus: Arc<EventBus<K, P>>,
    ) {
        let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

        while let Some(result) = framed.next().await {
            match result {
                Ok(bytes) => {
                    // Deserialize network event
                    match serde_json::from_slice::<NetworkEvent<K>>(&bytes) {
                        Ok(net_event) => {
                            // Emit to local bus
                            if let Err(e) = local_bus.emit_key(net_event.key, &net_event.data).await {
                                eprintln!("Failed to emit network event locally: {}", e);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to deserialize network event: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Network connection error: {}", e);
                    break;
                }
            }
        }
    }

    pub async fn connect_to_node(&self, node_id: String, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        let stream = TcpStream::connect(addr).await?;
        let framed = Framed::new(stream, LengthDelimitedCodec::new());

        let node = NetworkNode {
            id: node_id.clone(),
            addr,
            connection: Some(Arc::new(Mutex::new(framed))),
            last_seen: Instant::now(),
        };

        let mut nodes = self.network_nodes.write().await;
        nodes.insert(node_id, node);

        Ok(())
    }

    pub async fn emit_network<T>(&self, key: K, event: &T, target_nodes: Option<Vec<String>>) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        let network_event = NetworkEvent {
            key: key.clone(),
            data: serde_json::to_value(event)?,
            sender: self.node_id.clone(),
            timestamp: utils::current_timestamp(),
        };

        let serialized = serde_json::to_vec(&network_event)?;

        let nodes = self.network_nodes.read().await;
        let targets: Vec<_> = match target_nodes {
            Some(specific) => nodes.iter()
                .filter(|(id, _)| specific.contains(id))
                .collect(),
            None => nodes.iter().collect(),
        };

        for (_, node) in targets {
            if let Some(connection) = &node.connection {
                let mut conn = connection.lock().await;
                if let Err(e) = conn.send(serialized.clone().into()).await {
                    eprintln!("Failed to send to node {}: {}", node.id, e);
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkEvent<K> {
    key: K,
    data: serde_json::Value,
    sender: String,
    timestamp: u64,
}
```

### Persistent Event Store
Store events for replay and auditing:

```rust
use sqlx::{SqlitePool, Row};

pub struct PersistentEventBus<K: EventKeyType, P: EventPropagator<K>> {
    inner: Arc<EventBus<K, P>>,
    db_pool: SqlitePool,
    store_events: bool,
}

impl<K: EventKeyType + Serialize + for<'de> Deserialize<'de>, P: EventPropagator<K>> PersistentEventBus<K, P> {
    pub async fn new(
        inner: Arc<EventBus<K, P>>,
        database_url: &str,
    ) -> Result<Self, sqlx::Error> {
        let db_pool = SqlitePool::connect(database_url).await?;
        
        // Create events table
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_key TEXT NOT NULL,
                event_type TEXT NOT NULL,
                event_data BLOB NOT NULL,
                metadata TEXT,
                timestamp INTEGER NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
        "#)
        .execute(&db_pool)
        .await?;

        Ok(Self {
            inner,
            db_pool,
            store_events: true,
        })
    }

    pub async fn emit_key_persistent<T>(
        &self,
        key: K,
        event: &T,
    ) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        // Store event first
        if self.store_events {
            self.store_event(&key, event).await?;
        }

        // Then emit normally
        self.inner.emit_key(key, event).await
    }

    async fn store_event<T>(&self, key: &K, event: &T) -> Result<(), EventError>
    where
        T: Event + Serialize,
    {
        let event_data = serde_json::to_vec(event)
            .map_err(|e| EventError::SerializationFailed(e.to_string()))?;
        
        let key_str = serde_json::to_string(key)
            .map_err(|e| EventError::SerializationFailed(e.to_string()))?;

        sqlx::query(r#"
            INSERT INTO events (event_key, event_type, event_data, timestamp)
            VALUES (?, ?, ?, ?)
        "#)
        .bind(key_str)
        .bind(T::event_type())
        .bind(event_data)
        .bind(utils::current_timestamp() as i64)
        .execute(&self.db_pool)
        .await
        .map_err(|e| EventError::StorageFailed(e.to_string()))?;

        Ok(())
    }

    pub async fn replay_events(
        &self,
        from_timestamp: Option<u64>,
        to_timestamp: Option<u64>,
        event_types: Option<Vec<String>>,
    ) -> Result<u64, EventError> {
        let mut query = String::from("SELECT event_key, event_type, event_data FROM events WHERE 1=1");
        let mut params = Vec::new();

        if let Some(from) = from_timestamp {
            query.push_str(" AND timestamp >= ?");
            params.push(from as i64);
        }

        if let Some(to) = to_timestamp {
            query.push_str(" AND timestamp <= ?");
            params.push(to as i64);
        }

        if let Some(types) = event_types {
            let placeholders = types.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            query.push_str(&format!(" AND event_type IN ({})", placeholders));
            for event_type in types {
                params.push(event_type);
            }
        }

        query.push_str(" ORDER BY timestamp ASC");

        let mut query_builder = sqlx::query(&query);
        for param in params {
            query_builder = query_builder.bind(param);
        }

        let rows = query_builder.fetch_all(&self.db_pool).await
            .map_err(|e| EventError::StorageFailed(e.to_string()))?;

        let mut replayed = 0;
        for row in rows {
            let key_str: String = row.get("event_key");
            let event_data: Vec<u8> = row.get("event_data");

            // Deserialize key
            let key: K = serde_json::from_str(&key_str)
                .map_err(|e| EventError::DeserializationFailed(e.to_string()))?;

            // Create event data
            let event_data = Arc::new(EventData {
                data: Arc::new(event_data),
                type_name: row.get("event_type"),
                metadata: HashMap::new(),
            });

            // Emit replayed event
            if let Err(e) = self.emit_replayed_event(key, event_data).await {
                eprintln!("Failed to replay event: {}", e);
            } else {
                replayed += 1;
            }
        }

        Ok(replayed)
    }

    async fn emit_replayed_event(&self, key: K, event_data: Arc<EventData>) -> Result<(), EventError> {
        // Find handlers for this key
        if let Some(handlers) = self.inner.handlers.get(&key) {
            let handlers = handlers.value().clone();
            
            for handler in handlers.iter() {
                if let Err(e) = handler.handle(&event_data).await {
                    eprintln!("Replayed event handler failed: {}", e);
                }
            }
        }

        Ok(())
    }
}
```

## Production Deployment

### Health Monitoring
Monitor system health and performance:

```rust
use prometheus::{Counter, Histogram, Gauge, Registry};

pub struct SystemMonitor {
    events_emitted: Counter,
    events_handled: Counter,
    handler_failures: Counter,
    event_processing_time: Histogram,
    active_plugins: Gauge,
    memory_usage: Gauge,
    registry: Registry,
}

impl SystemMonitor {
    pub fn new() -> Self {
        let registry = Registry::new();
        
        let events_emitted = Counter::new("events_emitted_total", "Total events emitted")
            .expect("Counter creation failed");
        let events_handled = Counter::new("events_handled_total", "Total events handled")
            .expect("Counter creation failed");
        let handler_failures = Counter::new("handler_failures_total", "Total handler failures")
            .expect("Counter creation failed");
        let event_processing_time = Histogram::new("event_processing_seconds", "Event processing time")
            .expect("Histogram creation failed");
        let active_plugins = Gauge::new("active_plugins", "Number of active plugins")
            .expect("Gauge creation failed");
        let memory_usage = Gauge::new("memory_usage_bytes", "Memory usage in bytes")
            .expect("Gauge creation failed");

        registry.register(Box::new(events_emitted.clone())).unwrap();
        registry.register(Box::new(events_handled.clone())).unwrap();
        registry.register(Box::new(handler_failures.clone())).unwrap();
        registry.register(Box::new(event_processing_time.clone())).unwrap();
        registry.register(Box::new(active_plugins.clone())).unwrap();
        registry.register(Box::new(memory_usage.clone())).unwrap();

        Self {
            events_emitted,
            events_handled,
            handler_failures,
            event_processing_time,
            active_plugins,
            memory_usage,
            registry,
        }
    }

    pub fn record_event_emitted(&self) {
        self.events_emitted.inc();
    }

    pub fn record_event_handled(&self, processing_time: f64) {
        self.events_handled.inc();
        self.event_processing_time.observe(processing_time);
    }

    pub fn record_handler_failure(&self) {
        self.handler_failures.inc();
    }

    pub fn update_plugin_count(&self, count: f64) {
        self.active_plugins.set(count);
    }

    pub fn update_memory_usage(&self, bytes: f64) {
        self.memory_usage.set(bytes);
    }

    pub fn gather_metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }
}

// Monitoring task
pub async fn start_monitoring_task(
    monitor: Arc<SystemMonitor>,
    event_bus: Arc<EventBus<StructuredEventKey, AllEqPropagator>>,
    plugin_manager: Arc<PluginManager<StructuredEventKey, AllEqPropagator>>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        // Update metrics
        let stats = event_bus.stats().await;
        monitor.update_plugin_count(plugin_manager.plugin_count() as f64);

        // Memory usage (simplified)
        let memory_usage = get_memory_usage();
        monitor.update_memory_usage(memory_usage as f64);

        // Log health status
        if stats.handler_failures > 0 {
            eprintln!("⚠️ Health check: {} handler failures detected", stats.handler_failures);
        }

        println!("💓 Health: {} plugins, {} events/s", 
            plugin_manager.plugin_count(),
            stats.events_handled / 10  // Events per second (rough)
        );
    }
}

fn get_memory_usage() -> usize {
    // Platform-specific memory usage detection
    // This is a simplified example
    0
}
```

### Configuration Management
Centralized configuration with hot-reloading:

```rust
use serde::{Deserialize, Serialize};
use notify::{Watcher, RecursiveMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub event_bus: EventBusConfig,
    pub plugins: Vec<PluginConfig>,
    pub monitoring: MonitoringConfig,
    pub network: NetworkConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBusConfig {
    pub max_handlers_per_key: usize,
    pub event_buffer_size: usize,
    pub worker_threads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub enabled: bool,
    pub metrics_port: u16,
    pub health_check_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub listen_port: u16,
    pub max_connections: usize,
    pub connection_timeout: u64,
}

pub struct ConfigManager {
    config: Arc<RwLock<SystemConfig>>,
    config_path: PathBuf,
    _watcher: notify::RecommendedWatcher,
}

impl ConfigManager {
    pub fn new(config_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        // Load initial config
        let config_str = std::fs::read_to_string(&config_path)?;
        let config: SystemConfig = toml::from_str(&config_str)?;
        let config = Arc::new(RwLock::new(config));

        // Set up file watcher
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::watcher(tx, Duration::from_secs(1))?;
        watcher.watch(&config_path, RecursiveMode::NonRecursive)?;

        // Handle config changes
        let config_clone = config.clone();
        let path_clone = config_path.clone();
        tokio::spawn(async move {
            for event in rx {
                if let notify::DebouncedEvent::Write(_) = event {
                    match Self::reload_config(&path_clone, &config_clone).await {
                        Ok(()) => println!("🔄 Configuration reloaded"),
                        Err(e) => eprintln!("❌ Failed to reload config: {}", e),
                    }
                }
            }
        });

        Ok(Self {
            config,
            config_path,
            _watcher: watcher,
        })
    }

    async fn reload_config(
        path: &Path,
        config: &Arc<RwLock<SystemConfig>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let config_str = std::fs::read_to_string(path)?;
        let new_config: SystemConfig = toml::from_str(&config_str)?;
        
        let mut current_config = config.write().await;
        *current_config = new_config;
        
        Ok(())
    }

    pub async fn get_config(&self) -> SystemConfig {
        self.config.read().await.clone()
    }

    pub async fn update_config<F>(&self, updater: F) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnOnce(&mut SystemConfig),
    {
        let mut config = self.config.write().await;
        updater(&mut *config);
        
        // Save to file
        let config_str = toml::to_string(&*config)?;
        std::fs::write(&self.config_path, config_str)?;
        
        Ok(())
    }
}
```

This advanced guide covers the sophisticated patterns and techniques needed to build production-ready systems with the Universal Plugin System. These patterns enable high-performance, scalable, and maintainable plugin architectures.