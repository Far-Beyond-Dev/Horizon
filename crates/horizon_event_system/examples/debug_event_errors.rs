/// Example demonstrating improved debugging output for event serialization/deserialization failures
use horizon_event_system::{Event, create_horizon_event_system};
use serde::{Deserialize, Serialize};

// A valid event for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ValidEvent {
    message: String,
}

// An event that will cause type mismatches
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DifferentEvent {
    data: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Note: The enhanced debug output will show in the console if tracing is set up
    
    println!("🔍 Testing improved event debugging output...\n");
    
    let events = create_horizon_event_system();
    
    // Test 1: Valid serialization/deserialization
    println!("1. Testing valid event:");
    let valid_event = ValidEvent {
        message: "Hello, World!".to_string(),
    };
    
    let serialized = Event::serialize(&valid_event);
    match serialized {
        Ok(data) => {
            println!("✅ Valid event serialized successfully ({} bytes)", data.len());
            
            let deserialized = <ValidEvent as Event>::deserialize(&data);
            match deserialized {
                Ok(event) => println!("✅ Valid event deserialized successfully: {:?}", event),
                Err(e) => println!("❌ Unexpected deserialization failure: {}", e),
            }
        }
        Err(e) => println!("❌ Unexpected serialization failure: {}", e),
    }
    
    println!();
    
    // Test 2: Type mismatch during deserialization
    println!("2. Testing type mismatch (DifferentEvent data -> ValidEvent):");
    let different_event = DifferentEvent { data: 42 };
    let serialized_different = Event::serialize(&different_event)?;
    
    // Try to deserialize as ValidEvent (this should fail with better debugging)
    let wrong_deserialize = <ValidEvent as Event>::deserialize(&serialized_different);
    match wrong_deserialize {
        Ok(_) => println!("❌ Unexpected success - this should have failed!"),
        Err(e) => println!("✅ Expected deserialization failure with enhanced debugging: {}", e),
    }
    
    println!();
    
    // Test 3: Handler registration with type mismatch
    println!("3. Testing handler with type mismatch:");
    
    // Register a handler for ValidEvent
    events.on_core("test_event", |event: ValidEvent| {
        println!("Handler received: {:?}", event);
        Ok(())
    }).await?;
    
    // Emit a DifferentEvent to the same event name (this should show warning with context)
    let result = events.emit_core("test_event", &different_event).await;
    match result {
        Ok(_) => println!("✅ Event emission succeeded (handler should show warning)"),
        Err(e) => println!("❌ Event emission failed: {}", e),
    }
    
    println!();
    
    // Test 4: Large data preview
    println!("4. Testing large data deserialization failure:");
    let large_json = serde_json::json!({
        "message": "This is a very long message ".repeat(20) + " that will be truncated in the debug output when deserialization fails because it's too long to display completely in the logs."
    });
    let large_data = serde_json::to_vec(&large_json)?;
    
    // Try to deserialize as ValidEvent (wrong structure, should fail)
    let large_fail = <ValidEvent as Event>::deserialize(&large_data);
    match large_fail {
        Ok(_) => println!("❌ Unexpected success!"),
        Err(e) => println!("✅ Large data deserialization failure with truncated preview: {}", e),
    }
    
    println!("\n🎯 Testing complete! Check the logs above for enhanced debugging output.");
    
    Ok(())
}