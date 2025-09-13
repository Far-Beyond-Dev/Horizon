use horizon_event_system::ABI_VERSION;
use tracing::info;

fn main() {
    info!("Current ABI version: {}", ABI_VERSION);
    info!("For version 0.10.0, this should be: 0.10.0:rust_version");
    
    // Verify it's not the old hardcoded value
    assert_ne!(ABI_VERSION, "1", "ABI version should not be the old hardcoded value");
    
    // Verify it starts with the correct crate version
    let expected_prefix = format!("{}:", env!("CARGO_PKG_VERSION"));
    assert!(ABI_VERSION.starts_with(&expected_prefix), "ABI version should start with the current crate version prefix");
    
    // Verify it contains the colon separator
    assert!(ABI_VERSION.contains(':'), "ABI version should contain ':' separator");
    
    info!("✅ ABI version is correctly set to: {}", ABI_VERSION);
}
