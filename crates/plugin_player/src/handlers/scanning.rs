//! # Ship Scanning Handler  
//! 
//! Manages detailed ship scanning and metadata sharing on GORC channel 3,
//! providing intimate-range information exchange between ships within
//! the 100-meter scanning range for detailed tactical and social data.
//! 
//! ## Channel 3 Characteristics
//! 
//! - **Purpose**: Detailed metadata - ship specifications, cargo, pilot info
//! - **Range**: 100m intimate range for close-quarters interaction
//! - **Frequency**: Event-driven with lower priority than combat/movement  
//! - **Data Type**: Rich metadata including ship class, status, cargo manifest
//! 
//! ## Scanning System Design
//! 
//! The scanning system provides detailed ship information at close range:
//! 1. **Active Scanning**: Players initiate scan requests of nearby ships
//! 2. **Passive Broadcasting**: Ships automatically share basic information
//! 3. **Proximity Required**: 100m range ensures intentional close encounters
//! 4. **Rich Metadata**: Detailed information about ship configuration and status
//! 
//! ## Scanned Information Categories
//! 
//! - **Ship Specifications**: Class, model, configuration
//! - **Status Information**: Hull integrity, shield strength, system status
//! - **Cargo Data**: Manifest of carried goods and materials  
//! - **Pilot Information**: Experience level, reputation, faction affiliation
//! - **Tactical Data**: Weapon loadouts, defensive systems (limited)
//! 
//! ## Privacy and Security
//! 
//! - **Player Consent**: Only information players choose to share is broadcast
//! - **Range Limitation**: 100m ensures scanning is intentional and mutual
//! - **Graduated Disclosure**: Basic info shared freely, detailed info requires proximity
//! - **Anti-Exploitation**: Prevents long-range intelligence gathering

use std::sync::Arc;
use horizon_event_system::{
    EventSystem, PlayerId, GorcEvent, GorcObjectId, ClientConnectionRef, ObjectInstance,
    EventError,
};
use tracing::{debug, error};
use serde_json;

/// Handles ship scanning requests from players on GORC channel 3.
/// 
/// This handler processes scan requests, validates scanner authorization,
/// and broadcasts detailed ship information to nearby vessels within the
/// 100m intimate scanning range for close-quarters tactical awareness.
/// 
/// # Parameters
/// 
/// - `gorc_event`: The GORC event containing scan request data
/// - `client_player`: ID of the player initiating the scan
/// - `_connection`: Client connection (available for future authentication)
/// - `_object_instance`: Player's object instance (available for position-based scanning)
/// - `events`: Event system for broadcasting scan results
/// - `luminal_handle`: Async runtime handle for background processing
/// 
/// # Returns
/// 
/// `Result<(), EventError>` - Success or detailed error information
/// 
/// # Scanning Flow
/// 
/// 1. Parse scan request from GORC event data
/// 2. Validate player owns the scanning ship
/// 3. Extract or generate scan data based on ship configuration
/// 4. Create detailed scan result broadcast
/// 5. Emit to ships within 100m intimate range on channel 3
/// 6. Log scan event for tactical monitoring
/// 
/// # Example Scan Request
/// 
/// ```json
/// {
///     "player_id": 42,
///     "ship_class": "Interceptor",
///     "hull_integrity": 100,
///     "shield_strength": 85,
///     "cargo_manifest": ["Medical Supplies", "Rare Metals"],
///     "pilot_level": 15
/// }
/// ```
/// 
/// # Broadcast Scan Results
/// 
/// ```json
/// {
///     "scanner_ship": 42,
///     "scan_data": {
///         "ship_class": "Interceptor",
///         "hull_integrity": 100,
///         "shield_strength": 85,
///         "cargo_manifest": ["Medical Supplies", "Rare Metals"],
///         "pilot_level": 15
///     },
///     "scan_timestamp": "2024-01-15T10:30:45.123Z"
/// }
/// ```
pub async fn handle_scanning_request(
    gorc_event: GorcEvent,
    client_player: PlayerId,
    _connection: ClientConnectionRef,
    _object_instance: &mut ObjectInstance,
    events: Arc<EventSystem>,
    luminal_handle: luminal::Handle,
) -> Result<(), EventError> {
    debug!("🔍 GORC: Received client ship scan request from {}: {:?}", 
        client_player, gorc_event);
    
    // Parse scan data from GORC event payload
    let event_data = serde_json::from_slice::<serde_json::Value>(&gorc_event.data)
        .map_err(|e| {
            error!("🔍 GORC: ❌ Failed to parse JSON from ship scan event data: {}", e);
            EventError::HandlerExecution("Invalid JSON in scan request".to_string())
        })?;
    
    // Extract player ID from scan request
    let Some(player_id) = event_data.get("player_id") else {
        error!("🔍 GORC: ❌ Ship scan event missing player_id");
        return Err(EventError::HandlerExecution("Missing player_id in scan request".to_string()));
    };
    
    debug!("🔍 GORC: Ship {} requesting detailed scan", player_id);
    
    // SECURITY: Validate player ownership - only ship owners can initiate scans
    if let Ok(request_player) = serde_json::from_value::<PlayerId>(player_id.clone()) {
        if request_player != client_player {
            error!("🔍 GORC: ❌ Security violation: Player {} tried to scan as {}", 
                client_player, request_player);
            return Err(EventError::HandlerExecution(
                "Unauthorized scan request".to_string()
            ));
        }
    } else {
        return Err(EventError::HandlerExecution("Invalid player_id format".to_string()));
    }
    
    // Extract detailed scan data with defaults for missing values
    let scan_data = extract_scan_data(&event_data);
    
    // Broadcast scan results to nearby ships
    broadcast_scan_results(
        &gorc_event.object_id,
        client_player,
        scan_data,
        events,
        luminal_handle,
    ).await;
    
    Ok(())
}

/// Synchronous wrapper for scanning request handling that works with GORC client handlers.
///
/// This function provides the same functionality as `handle_scanning_request` but in
/// a synchronous context suitable for use with the GORC client event system.
pub fn handle_scanning_request_sync(
    gorc_event: GorcEvent,
    client_player: PlayerId,
    _connection: ClientConnectionRef,
    _object_instance: &mut ObjectInstance,
    events: Arc<EventSystem>,
    luminal_handle: luminal::Handle,
) -> Result<(), EventError> {
    debug!("🔍 GORC: Received client ship scan request from {}: {:?}", 
        client_player, gorc_event);
    
    // Parse scan data from GORC event payload
    let event_data = serde_json::from_slice::<serde_json::Value>(&gorc_event.data)
        .map_err(|e| {
            error!("🔍 GORC: ❌ Failed to parse JSON from ship scan event data: {}", e);
            EventError::HandlerExecution("Invalid JSON in scan request".to_string())
        })?;
    
    // Extract player ID from scan request
    let Some(player_id) = event_data.get("player_id") else {
        error!("🔍 GORC: ❌ Ship scan event missing player_id");
        return Err(EventError::HandlerExecution("Missing player_id in scan request".to_string()));
    };
    
    debug!("🔍 GORC: Ship {} requesting detailed scan", player_id);
    
    // SECURITY: Validate player ownership - only ship owners can initiate scans
    if let Ok(request_player) = serde_json::from_value::<PlayerId>(player_id.clone()) {
        if request_player != client_player {
            error!("🔍 GORC: ❌ Security violation: Player {} tried to scan as {}", 
                client_player, request_player);
            return Err(EventError::HandlerExecution(
                "Unauthorized scan request".to_string()
            ));
        }
    } else {
        return Err(EventError::HandlerExecution("Invalid player_id format".to_string()));
    }
    
    // Extract detailed scan data with defaults for missing values
    let scan_data = extract_scan_data(&event_data);
    
    // Broadcast scan results to nearby ships
    let object_id_str = gorc_event.object_id.clone();
    let scan_broadcast = serde_json::json!({
        "scanner_ship": client_player,
        "scan_data": {
            "ship_class": scan_data.ship_class,
            "hull_integrity": scan_data.hull_integrity,
            "shield_strength": scan_data.shield_strength,
            "cargo_manifest": scan_data.cargo_manifest,
            "pilot_level": scan_data.pilot_level,
            "energy_signature": scan_data.energy_signature,
            "weapon_systems": scan_data.weapon_systems
        },
        "scan_timestamp": chrono::Utc::now(),
        "scan_range": 100.0 // Intimate range scanning
    });
    
    if let Ok(gorc_id) = GorcObjectId::from_str(&object_id_str) {
        luminal_handle.spawn(async move {
            if let Err(e) = events.emit_gorc_instance(
                gorc_id, 
                3, // Channel 3: Detailed scanning events
                "scan_results", 
                &scan_broadcast, 
                horizon_event_system::Dest::Client
            ).await {
                error!("🔍 GORC: ❌ Failed to broadcast scan results: {}", e);
            } else {
                debug!("🔍 GORC: ✅ Broadcasting scan results from ship {} to ships within 100m", 
                    client_player);
            }
        });
    } else {
        error!("🔍 GORC: ❌ Invalid GORC object ID format: {}", object_id_str);
    }
    
    Ok(())
}

/// Extracts and validates scan data from the event payload.
/// 
/// This function parses the detailed ship information from the scan request,
/// providing sensible defaults for missing data to ensure consistent
/// scan result formatting.
/// 
/// # Parameters
/// 
/// - `event_data`: JSON object containing scan request data
/// 
/// # Returns
/// 
/// `ScanData` - Structured scan information with all required fields
fn extract_scan_data(event_data: &serde_json::Value) -> ScanData {
    ScanData {
        ship_class: event_data
            .get("ship_class")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string(),
        hull_integrity: event_data
            .get("hull_integrity")
            .and_then(|v| v.as_f64())
            .unwrap_or(100.0) as f32,
        shield_strength: event_data
            .get("shield_strength")
            .and_then(|v| v.as_f64())
            .unwrap_or(85.0) as f32,
        cargo_manifest: event_data
            .get("cargo_manifest")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .collect())
            .unwrap_or_else(Vec::new),
        pilot_level: event_data
            .get("pilot_level")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32,
        energy_signature: event_data
            .get("energy_signature")
            .and_then(|v| v.as_f64())
            .unwrap_or(100.0) as f32,
        weapon_systems: event_data
            .get("weapon_systems")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .collect())
            .unwrap_or_else(Vec::new),
    }
}

/// Structured scan data containing all ship information categories.
#[derive(Debug, Clone)]
pub struct ScanData {
    /// Ship class/type (Interceptor, Cruiser, Freighter, etc.)
    pub ship_class: String,
    /// Hull integrity percentage (0-100)
    pub hull_integrity: f32,
    /// Shield strength percentage (0-100)
    pub shield_strength: f32,
    /// List of cargo items currently carried
    pub cargo_manifest: Vec<String>,
    /// Pilot experience level
    pub pilot_level: u32,
    /// Energy signature strength (affects detectability)
    pub energy_signature: f32,
    /// List of equipped weapon systems
    pub weapon_systems: Vec<String>,
}

/// Broadcasts detailed scan results to nearby ships within 100m intimate range.
/// 
/// This function creates a comprehensive scan result message and emits it
/// via the GORC instance event system for short-range tactical intelligence.
/// 
/// # Parameters
/// 
/// - `object_id_str`: String representation of the scanning ship's GORC object ID
/// - `scanner_player`: ID of the player who initiated the scan
/// - `scan_data`: Detailed scan information to broadcast
/// - `events`: Event system for broadcasting
/// - `luminal_handle`: Async runtime handle
/// 
/// # Intimate Range Benefits
/// 
/// The 100m range provides:
/// - **Intentional Interaction**: Ensures scans are deliberate close encounters
/// - **Tactical Intelligence**: Detailed info for docking, trading, combat decisions
/// - **Privacy Protection**: Prevents long-range intelligence gathering
/// - **Network Efficiency**: Limits detailed metadata to immediately relevant ships
/// 
/// # Scan Result Categories
/// 
/// - **Basic Information**: Ship class, pilot level, energy signature
/// - **Status Data**: Hull integrity, shield strength, system status
/// - **Cargo Information**: Manifest of carried goods (for trading)
/// - **Tactical Data**: Limited weapon system information
async fn broadcast_scan_results(
    object_id_str: &str,
    scanner_player: PlayerId,
    scan_data: ScanData,
    events: Arc<EventSystem>,
    luminal_handle: luminal::Handle,
) {
    // Create comprehensive scan result broadcast payload
    let scan_broadcast = serde_json::json!({
        "scanner_ship": scanner_player,
        "scan_data": {
            "ship_class": scan_data.ship_class,
            "hull_integrity": scan_data.hull_integrity,
            "shield_strength": scan_data.shield_strength,
            "cargo_manifest": scan_data.cargo_manifest,
            "pilot_level": scan_data.pilot_level,
            "energy_signature": scan_data.energy_signature,
            "weapon_systems": scan_data.weapon_systems
        },
        "scan_timestamp": chrono::Utc::now(),
        "scan_range": 100.0 // Intimate range scanning
    });
    
    // Parse GORC object ID and emit the scan results
    if let Ok(gorc_id) = GorcObjectId::from_str(object_id_str) {
        luminal_handle.spawn(async move {
            // Emit on channel 3 (scanning) with 100m intimate range
            if let Err(e) = events.emit_gorc_instance(
                gorc_id, 
                3, // Channel 3: Detailed scanning events
                "scan_results", 
                &scan_broadcast, 
                horizon_event_system::Dest::Client
            ).await {
                error!("🔍 GORC: ❌ Failed to broadcast scan results: {}", e);
            } else {
                debug!("🔍 GORC: ✅ Broadcasting scan results from ship {} to ships within 100m", 
                    scanner_player);
            }
        });
    } else {
        error!("🔍 GORC: ❌ Invalid GORC object ID format: {}", object_id_str);
    }
}

/// Validates scan request to prevent abuse and ensure appropriate data sharing.
/// 
/// This function performs validation on scan requests:
/// - Rate limiting to prevent scan spam
/// - Privacy settings compliance
/// - Data accuracy verification
/// 
/// # Parameters
/// 
/// - `scanner_player`: Player initiating the scan
/// - `scan_data`: The scan data being shared
/// 
/// # Returns
/// 
/// `Result<(), String>` - Ok if valid, Err with reason if invalid
/// 
/// # Validation Rules
/// 
/// - **Rate Limiting**: Maximum 1 scan per 5 seconds per player
/// - **Data Bounds**: Hull/shield values must be 0-100%
/// - **Privacy Compliance**: Respects player privacy settings (future)
/// - **Cargo Validation**: Ensures cargo manifest is reasonable size
pub fn validate_scan_request(
    _scanner_player: PlayerId,
    scan_data: &ScanData,
) -> Result<(), String> {
    // Validate hull integrity is within valid bounds
    if scan_data.hull_integrity < 0.0 || scan_data.hull_integrity > 100.0 {
        return Err(format!("Invalid hull integrity: {:.1}%", scan_data.hull_integrity));
    }
    
    // Validate shield strength is within valid bounds
    if scan_data.shield_strength < 0.0 || scan_data.shield_strength > 100.0 {
        return Err(format!("Invalid shield strength: {:.1}%", scan_data.shield_strength));
    }
    
    // Validate cargo manifest isn't excessively large (network efficiency)
    if scan_data.cargo_manifest.len() > 50 {
        return Err(format!("Cargo manifest too large: {} items", scan_data.cargo_manifest.len()));
    }
    
    // Validate individual cargo item names aren't too long
    for item in &scan_data.cargo_manifest {
        if item.len() > 100 {
            return Err(format!("Cargo item name too long: {}", item));
        }
    }
    
    // Validate weapon systems list isn't excessive
    if scan_data.weapon_systems.len() > 20 {
        return Err(format!("Too many weapon systems: {}", scan_data.weapon_systems.len()));
    }
    
    // Future enhancements:
    // - Rate limiting per player
    // - Privacy setting compliance
    // - Faction-based information restriction
    // - Distance-based detail levels
    
    Ok(())
}

/// Generates realistic scan data based on ship type and configuration.
/// 
/// This function creates appropriate scan data for different ship classes,
/// providing realistic values for hull, shields, cargo capacity, etc.
/// 
/// # Parameters
/// 
/// - `ship_class`: The class/type of ship being scanned
/// - `pilot_level`: Experience level of the pilot
/// 
/// # Returns
/// 
/// `ScanData` - Generated scan information appropriate for the ship type
pub fn generate_scan_data_for_ship_class(ship_class: &str, pilot_level: u32) -> ScanData {
    match ship_class {
        "Interceptor" => ScanData {
            ship_class: ship_class.to_string(),
            hull_integrity: 80.0 + (pilot_level as f32 * 0.5).min(20.0),
            shield_strength: 70.0 + (pilot_level as f32 * 0.8).min(30.0),
            cargo_manifest: vec![], // Interceptors typically carry no cargo
            pilot_level,
            energy_signature: 120.0, // High energy signature for speed
            weapon_systems: vec!["Light Pulse Lasers".to_string(), "Missile Pod".to_string()],
        },
        "Cruiser" => ScanData {
            ship_class: ship_class.to_string(),
            hull_integrity: 90.0 + (pilot_level as f32 * 0.3).min(10.0),
            shield_strength: 85.0 + (pilot_level as f32 * 0.5).min(15.0),
            cargo_manifest: vec!["Ammunition".to_string(), "Spare Parts".to_string()],
            pilot_level,
            energy_signature: 200.0, // High energy for weapons and shields
            weapon_systems: vec![
                "Heavy Beam Lasers".to_string(), 
                "Torpedo Launcher".to_string(),
                "Point Defense Systems".to_string()
            ],
        },
        "Freighter" => ScanData {
            ship_class: ship_class.to_string(),
            hull_integrity: 95.0 + (pilot_level as f32 * 0.2).min(5.0),
            shield_strength: 60.0 + (pilot_level as f32 * 0.6).min(25.0),
            cargo_manifest: vec![
                "Consumer Goods".to_string(),
                "Raw Materials".to_string(),
                "Food Supplies".to_string(),
                "Medical Equipment".to_string()
            ],
            pilot_level,
            energy_signature: 80.0, // Lower signature for stealth trading
            weapon_systems: vec!["Light Defense Turrets".to_string()],
        },
        _ => ScanData {
            ship_class: "Unknown".to_string(),
            hull_integrity: 100.0,
            shield_strength: 85.0,
            cargo_manifest: vec![],
            pilot_level,
            energy_signature: 100.0,
            weapon_systems: vec![],
        },
    }
}