# Fix for Async Anti-pattern in Event System

## Problem
The `on_core_async` handlers are using `Handle::block_on()` inside event handlers, causing "Cannot start a runtime from within a runtime" errors.

## Root Cause  
The event system API is misleading. `on_core_async` is not actually async - it accepts sync handlers that run in an async context.

## Solutions

### 1. Fix Integration Tests

**WRONG** (current approach):
```rust
event_system.on_core_async("server_tick", move |event: Value| {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.block_on(async move {  // ❌ This causes runtime conflict
            let mut c = count.lock().await;
            *c += 1;
        });
    }
    Ok(())
}).await?;
```

**RIGHT** (fixed approach):
```rust
event_system.on_core("server_tick", move |event: Value| {
    // Use synchronous state management
    let mut c = count.blocking_lock();  // or use parking_lot::Mutex
    *c += 1;
    println!("Tick #{}: {:?}", *c, event);
    Ok(())
}).await?;
```

Or better yet, use atomic operations:
```rust
let tick_count = Arc::new(AtomicU64::new(0));
let tick_count_clone = tick_count.clone();

event_system.on_core("server_tick", move |event: Value| {
    let count = tick_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
    println!("Tick #{}: {:?}", count, event);
    Ok(())
}).await?;
```

### 2. Fix Plugin Logger

**WRONG** (current approach):
```rust
events_clone.on_core_async("server_tick", move |_event: Value| {
    tokio_handle.block_on(async {  // ❌ Runtime conflict
        // async work
    });
    Ok(())
})
```

**RIGHT** (fixed approach):
Option A - Use sync operations:
```rust
events_clone.on_core("server_tick", move |_event: Value| {
    let tick = tick_counter.fetch_add(1, Ordering::SeqCst) + 1;
    if tick % 30 == 0 {
        // Log synchronously or queue for async processing
        context_clone.log(LogLevel::Info, &format!("Activity summary: {} ticks", tick));
    }
    Ok(())
})
```

Option B - Use message passing to async task:
```rust
let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

// Event handler sends to channel
events_clone.on_core("server_tick", move |event: Value| {
    let _ = tx.send(event);
    Ok(())
})

// Separate async task processes events
tokio::spawn(async move {
    while let Some(event) = rx.recv().await {
        // Do async work here
        process_tick_async(event).await;
    }
});
```

### 3. Fix API Documentation

Update the docs to remove references to `Handle::block_on()` and clarify that:
- `on_core_async` handlers are sync functions that run in async context
- Use sync state management or message passing for async work
- Consider providing truly async handler methods in the future

### 4. Consider Adding True Async Handlers

For the future, consider adding methods like:
```rust
pub async fn on_core_truly_async<T, F, Fut>(
    &self,
    event_name: &str,
    handler: F,
) -> Result<(), EventError>
where
    T: Event + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), EventError>> + Send + 'static,
```