use horizon_event_system::ABI_VERSION;

fn main() {
    println!("Current ABI version: {}", ABI_VERSION);
    println!("For version 0.9.0, this should be: 900");
    
    // Verify it's not the old hardcoded value
    assert_ne!(ABI_VERSION, 1, "ABI version should not be the old hardcoded value");
    
    // Verify it's a reasonable value for version 0.9.0
    assert_eq!(ABI_VERSION, 900, "ABI version should be 900 for version 0.9.0");
    
    println!("✅ ABI version is correctly set to: {}", ABI_VERSION);
}
