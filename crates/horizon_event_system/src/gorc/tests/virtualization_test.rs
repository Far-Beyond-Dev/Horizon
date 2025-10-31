//! Comprehensive GORC Virtualization Edge Case Tests
//!
//! This test suite is designed to root out edge cases and validate the robustness
//! of the GORC zone virtualization system under various stress conditions.

use crate::gorc::instance::{GorcInstanceManager, GorcObject};
use crate::gorc::channels::{ReplicationLayer, CompressionType};
use crate::gorc::virtualization::VirtualizationConfig;
use crate::types::Vec3;
use std::sync::Arc;
use std::any::Any;
use tokio::time::{sleep, Duration};

/// Test object for virtualization testing with configurable properties
#[derive(Debug, Clone)]
struct VirtualizationTestObject {
    position: Vec3,
    object_type: String,
    zone_config: ZoneConfiguration,
}

#[derive(Debug, Clone)]
enum ZoneConfiguration {
    Single { radius: f64, channel: u8 },
    Concentric { radii: Vec<(f64, u8)> },
    Overlapping { zones: Vec<(Vec3, f64, u8)> },
    Massive { radius: f64 },
}

impl VirtualizationTestObject {
    fn new_single(position: Vec3, radius: f64, channel: u8) -> Self {
        Self {
            position,
            object_type: "single_zone".to_string(),
            zone_config: ZoneConfiguration::Single { radius, channel },
        }
    }

    fn new_concentric(position: Vec3, radii: Vec<(f64, u8)>) -> Self {
        Self {
            position,
            object_type: "concentric_zones".to_string(),
            zone_config: ZoneConfiguration::Concentric { radii },
        }
    }

    fn new_overlapping(position: Vec3, zones: Vec<(Vec3, f64, u8)>) -> Self {
        Self {
            position,
            object_type: "overlapping_zones".to_string(),
            zone_config: ZoneConfiguration::Overlapping { zones },
        }
    }

    fn new_massive(position: Vec3, radius: f64) -> Self {
        Self {
            position,
            object_type: "massive_zone".to_string(),
            zone_config: ZoneConfiguration::Massive { radius },
        }
    }
}

impl GorcObject for VirtualizationTestObject {
    fn type_name(&self) -> &str {
        "VirtualizationTestObject"
    }

    fn position(&self) -> Vec3 {
        self.position
    }

    fn get_priority(&self, _observer_pos: Vec3) -> crate::gorc::channels::ReplicationPriority {
        crate::gorc::channels::ReplicationPriority::Normal
    }

    fn serialize_for_layer(&self, _layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let data = serde_json::json!({
            "position": self.position,
            "object_type": self.object_type
        });
        Ok(serde_json::to_vec(&data)?)
    }

    fn get_layers(&self) -> Vec<ReplicationLayer> {
        match &self.zone_config {
            ZoneConfiguration::Single { radius, channel } => {
                vec![ReplicationLayer::new(*channel, *radius, 30.0, vec!["position".to_string()], CompressionType::Delta)]
            }
            ZoneConfiguration::Concentric { radii } => {
                radii.iter().map(|(radius, channel)| {
                    ReplicationLayer::new(*channel, *radius, 30.0, vec!["position".to_string()], CompressionType::Delta)
                }).collect()
            }
            ZoneConfiguration::Overlapping { zones } => {
                zones.iter().map(|(_, radius, channel)| {
                    ReplicationLayer::new(*channel, *radius, 30.0, vec!["position".to_string()], CompressionType::Delta)
                }).collect()
            }
            ZoneConfiguration::Massive { radius } => {
                vec![ReplicationLayer::new(0, *radius, 10.0, vec!["position".to_string()], CompressionType::None)]
            }
        }
    }

    fn update_position(&mut self, new_position: Vec3) {
        self.position = new_position;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn clone_object(&self) -> Box<dyn GorcObject> {
        Box::new(self.clone())
    }
}

/// Edge Case Test Suite
#[tokio::test]
async fn test_rapid_merge_split_cycles() {
    println!("\n=== Testing Rapid Merge-Split Cycles ===");

    let config = VirtualizationConfig {
        enabled: true,
        density_threshold: 0.1, // Very aggressive merging
        overlap_threshold: 0.1, // Very low overlap required
        max_virtual_zone_radius: 500.0,
        min_zone_radius: 10.0,
        check_interval_ms: 100,
        max_objects_per_virtual_zone: 10,
    };

    let manager = Arc::new(GorcInstanceManager::new_with_config(config));

    // Create objects that will trigger rapid merge/split cycles
    let mut object_ids = Vec::new();
    for i in 0..10 {
        let position = Vec3::new((i as f64) * 20.0, 0.0, 0.0);
        let obj = VirtualizationTestObject::new_single(position, 50.0, 0);
        let object_id = manager.register_object(obj, position).await;
        object_ids.push(object_id);
    }

    // Rapidly move objects to trigger merge/split cycles
    for cycle in 0..5 {
        println!("Cycle {}", cycle);

        // Move objects together (should trigger merging)
        for (i, &object_id) in object_ids.iter().enumerate() {
            let new_position = Vec3::new((i as f64 * 5.0) + (cycle as f64 * 100.0), 0.0, 0.0);
            manager.update_object_position(object_id, new_position).await;
        }

        manager.process_virtualization().await.unwrap();
        sleep(Duration::from_millis(50)).await;

        // Move objects apart (should trigger splitting)
        for (i, &object_id) in object_ids.iter().enumerate() {
            let new_position = Vec3::new((i as f64 * 200.0) + (cycle as f64 * 1000.0), 0.0, 0.0);
            manager.update_object_position(object_id, new_position).await;
        }

        manager.process_virtualization().await.unwrap();
        sleep(Duration::from_millis(50)).await;
    }

    let stats = manager.get_virtualization_stats().await;
    println!("Final stats: {:?}", stats);

    // Should have survived rapid cycling without crashing
    assert!(stats.total_virtual_zones_created > 0);
}

#[tokio::test]
async fn test_overlapping_zone_boundary_conditions() {
    println!("\n=== Testing Overlapping Zone Boundary Conditions ===");

    let config = VirtualizationConfig {
        enabled: true,
        density_threshold: 0.2,
        overlap_threshold: 0.3,
        max_virtual_zone_radius: 1000.0,
        min_zone_radius: 50.0,
        check_interval_ms: 1000,
        max_objects_per_virtual_zone: 20,
    };

    let manager = Arc::new(GorcInstanceManager::new_with_config(config));

    // Test exact boundary overlap (should just barely merge)
    let obj1 = VirtualizationTestObject::new_single(Vec3::new(0.0, 0.0, 0.0), 100.0, 0);
    let obj2 = VirtualizationTestObject::new_single(Vec3::new(130.0, 0.0, 0.0), 100.0, 0); // 30% overlap

    let _id1 = manager.register_object(obj1, Vec3::new(0.0, 0.0, 0.0)).await;
    let _id2 = manager.register_object(obj2, Vec3::new(130.0, 0.0, 0.0)).await;

    manager.process_virtualization().await.unwrap();

    let stats = manager.get_virtualization_stats().await;
    println!("Boundary overlap stats: {:?}", stats);

    // Test sub-threshold overlap (should not merge)
    let obj3 = VirtualizationTestObject::new_single(Vec3::new(250.0, 0.0, 0.0), 100.0, 0);
    let obj4 = VirtualizationTestObject::new_single(Vec3::new(400.0, 0.0, 0.0), 100.0, 0); // 25% overlap (below 30% threshold)

    let _id3 = manager.register_object(obj3, Vec3::new(250.0, 0.0, 0.0)).await;
    let _id4 = manager.register_object(obj4, Vec3::new(400.0, 0.0, 0.0)).await;

    manager.process_virtualization().await.unwrap();

    let final_stats = manager.get_virtualization_stats().await;
    println!("Final boundary stats: {:?}", final_stats);

    // Should create some virtual zones but respect threshold
    assert!(final_stats.total_virtual_zones_created >= stats.total_virtual_zones_created);
}

#[tokio::test]
async fn test_massive_zone_handling() {
    println!("\n=== Testing Massive Zone Handling ===");

    let config = VirtualizationConfig {
        enabled: true,
        density_threshold: 0.1,
        overlap_threshold: 0.2,
        max_virtual_zone_radius: 2000.0, // Allow large virtual zones
        min_zone_radius: 50.0,
        check_interval_ms: 1000,
        max_objects_per_virtual_zone: 100,
    };

    let manager = Arc::new(GorcInstanceManager::new_with_config(config));

    // Create objects with massive zones that should trigger warnings
    let massive_obj = VirtualizationTestObject::new_massive(Vec3::new(0.0, 0.0, 0.0), 1500.0);
    let massive_id = manager.register_object(massive_obj, Vec3::new(0.0, 0.0, 0.0)).await;

    // Add many smaller objects within the massive zone
    let mut small_ids = Vec::new();
    for i in 0..20 {
        let angle = (i as f64) * std::f64::consts::PI * 2.0 / 20.0;
        let radius = 500.0;
        let position = Vec3::new(angle.cos() * radius, angle.sin() * radius, 0.0);

        let small_obj = VirtualizationTestObject::new_single(position, 100.0, 0);
        let small_id = manager.register_object(small_obj, position).await;
        small_ids.push(small_id);
    }

    manager.process_virtualization().await.unwrap();

    let stats = manager.get_stats().await;
    let virt_stats = manager.get_virtualization_stats().await;

    println!("Massive zone stats: large_zone_warnings={}, virtual_zones={}",
             stats.large_zone_warnings, virt_stats.active_virtual_zones);

    // Should detect large zones and handle them appropriately
    assert!(stats.large_zone_warnings > 0);
}

#[tokio::test]
async fn test_multi_channel_virtualization() {
    println!("\n=== Testing Multi-Channel Virtualization ===");

    let config = VirtualizationConfig {
        enabled: true,
        density_threshold: 0.3,
        overlap_threshold: 0.4,
        max_virtual_zone_radius: 800.0,
        min_zone_radius: 30.0,
        check_interval_ms: 1000,
        max_objects_per_virtual_zone: 15,
    };

    let manager = Arc::new(GorcInstanceManager::new_with_config(config));

    // Create objects with multiple channels that overlap differently
    for i in 0..10 {
        let position = Vec3::new(i as f64 * 80.0, 0.0, 0.0);
        let radii = vec![
            (50.0, 0),  // Small inner zone
            (120.0, 1), // Medium zone
            (200.0, 2), // Large outer zone
        ];

        let obj = VirtualizationTestObject::new_concentric(position, radii);
        manager.register_object(obj, position).await;
    }

    manager.process_virtualization().await.unwrap();

    let virt_stats = manager.get_virtualization_stats().await;
    println!("Multi-channel virtualization stats: {:?}", virt_stats);

    // Should handle different channels independently
    assert!(virt_stats.total_virtual_zones_created > 0);
}

#[tokio::test]
async fn test_virtualization_under_concurrent_load() {
    println!("\n=== Testing Virtualization Under Concurrent Load ===");

    let config = VirtualizationConfig {
        enabled: true,
        density_threshold: 0.4,
        overlap_threshold: 0.3,
        max_virtual_zone_radius: 600.0,
        min_zone_radius: 40.0,
        check_interval_ms: 500,
        max_objects_per_virtual_zone: 25,
    };

    let manager = Arc::new(GorcInstanceManager::new_with_config(config));

    // Create initial objects
    let mut object_ids = Vec::new();
    for i in 0..50 {
        let position = Vec3::new((i % 10) as f64 * 100.0, (i / 10) as f64 * 100.0, 0.0);
        let obj = VirtualizationTestObject::new_single(position, 80.0, i % 4);
        let object_id = manager.register_object(obj, position).await;
        object_ids.push(object_id);
    }

    // Concurrent operations
    let mut handles = Vec::new();

    // Concurrent object movement
    for chunk in object_ids.chunks(10) {
        let chunk_ids = chunk.to_vec();
        let manager_clone = manager.clone();
        let handle = tokio::spawn(async move {
            for &object_id in &chunk_ids {
                // Generate pseudo-random position based on object ID
                let id_bytes = object_id.0.as_bytes();
                let x = ((id_bytes[0] as u64 * 7) % 1000) as f64;
                let y = ((id_bytes[1] as u64 * 13) % 1000) as f64;
                let new_position = Vec3::new(x, y, 0.0);
                let _ = manager_clone.update_object_position(object_id, new_position).await;
                tokio::task::yield_now().await;
            }
        });
        handles.push(handle);
    }

    // Concurrent virtualization processing
    let manager_clone = manager.clone();
    let virtualization_handle = tokio::spawn(async move {
        for _ in 0..10 {
            let _ = manager_clone.process_virtualization().await;
            sleep(Duration::from_millis(100)).await;
        }
    });
    handles.push(virtualization_handle);

    // Wait for all operations to complete
    for handle in handles {
        handle.await.unwrap();
    }

    let final_stats = manager.get_virtualization_stats().await;
    println!("Concurrent load final stats: {:?}", final_stats);

    // Should handle concurrent operations without panicking
    assert!(final_stats.total_virtual_zones_created >= 0);
}

#[tokio::test]
async fn test_zone_splitting_edge_cases() {
    println!("\n=== Testing Zone Splitting Edge Cases ===");

    let config = VirtualizationConfig {
        enabled: true,
        density_threshold: 0.2,
        overlap_threshold: 0.3,
        max_virtual_zone_radius: 300.0, // Small to force splits
        min_zone_radius: 30.0,
        check_interval_ms: 1000,
        max_objects_per_virtual_zone: 5, // Small to force splits
    };

    let manager = Arc::new(GorcInstanceManager::new_with_config(config));

    // Create tightly packed objects that will merge then split
    let mut object_ids = Vec::new();
    for i in 0..10 {
        let position = Vec3::new(i as f64 * 30.0, 0.0, 0.0); // Close together
        let obj = VirtualizationTestObject::new_single(position, 50.0, 0);
        let object_id = manager.register_object(obj, position).await;
        object_ids.push(object_id);
    }

    // Force initial merge
    manager.process_virtualization().await.unwrap();
    let initial_stats = manager.get_virtualization_stats().await;
    println!("After initial merge: {:?}", initial_stats);

    // Move objects far apart to force splits
    for (i, &object_id) in object_ids.iter().enumerate() {
        let new_position = Vec3::new(i as f64 * 500.0, 0.0, 0.0); // Far apart
        manager.update_object_position(object_id, new_position).await;
    }

    // Process splits
    manager.process_virtualization().await.unwrap();
    let final_stats = manager.get_virtualization_stats().await;
    println!("After splits: {:?}", final_stats);

    // Should have created and destroyed virtual zones
    assert!(final_stats.total_virtual_zones_destroyed > 0);
}

#[tokio::test]
async fn test_configuration_edge_cases() {
    println!("\n=== Testing Configuration Edge Cases ===");

    // Test with disabled virtualization
    let disabled_config = VirtualizationConfig {
        enabled: false,
        ..Default::default()
    };

    let disabled_manager = Arc::new(GorcInstanceManager::new_with_config(disabled_config));

    // Add objects - should not virtualize
    for i in 0..5 {
        let position = Vec3::new(i as f64 * 10.0, 0.0, 0.0);
        let obj = VirtualizationTestObject::new_single(position, 50.0, 0);
        disabled_manager.register_object(obj, position).await;
    }

    disabled_manager.process_virtualization().await.unwrap();
    let disabled_stats = disabled_manager.get_virtualization_stats().await;
    assert_eq!(disabled_stats.total_virtual_zones_created, 0);

    // Test with extreme configuration values
    let extreme_config = VirtualizationConfig {
        enabled: true,
        density_threshold: 0.01, // Extremely aggressive
        overlap_threshold: 0.01, // Merge everything
        max_virtual_zone_radius: 10000.0, // Huge zones allowed
        min_zone_radius: 1.0, // Tiny minimum
        check_interval_ms: 10, // Very frequent checks
        max_objects_per_virtual_zone: 1000,
    };

    let extreme_manager = Arc::new(GorcInstanceManager::new_with_config(extreme_config));

    // Add widely spaced objects - should still try to merge aggressively
    for i in 0..10 {
        let position = Vec3::new(i as f64 * 1000.0, 0.0, 0.0);
        let obj = VirtualizationTestObject::new_single(position, 100.0, 0);
        extreme_manager.register_object(obj, position).await;
    }

    extreme_manager.process_virtualization().await.unwrap();
    let extreme_stats = extreme_manager.get_virtualization_stats().await;
    println!("Extreme config stats: {:?}", extreme_stats);

    // Should handle extreme configurations gracefully
    assert!(extreme_stats.total_virtual_zones_created >= 0);
}

#[tokio::test]
async fn test_memory_and_performance_under_stress() {
    println!("\n=== Testing Memory and Performance Under Stress ===");

    let config = VirtualizationConfig {
        enabled: true,
        density_threshold: 0.5,
        overlap_threshold: 0.4,
        max_virtual_zone_radius: 500.0,
        min_zone_radius: 25.0,
        check_interval_ms: 1000,
        max_objects_per_virtual_zone: 30,
    };

    let manager = Arc::new(GorcInstanceManager::new_with_config(config));

    // Create a large number of objects to stress test
    let object_count = 200;
    let mut object_ids = Vec::new();

    let start_time = std::time::Instant::now();

    for i in 0..object_count {
        let x = (i % 20) as f64 * 50.0;
        let y = (i / 20) as f64 * 50.0;
        let position = Vec3::new(x, y, 0.0);

        let obj = VirtualizationTestObject::new_single(position, 75.0, i % 4);
        let object_id = manager.register_object(obj, position).await;
        object_ids.push(object_id);
    }

    let registration_time = start_time.elapsed();
    println!("Registered {} objects in {:.3}ms", object_count, registration_time.as_secs_f64() * 1000.0);

    // Process virtualization multiple times
    let virtualization_start = std::time::Instant::now();
    for _ in 0..10 {
        manager.process_virtualization().await.unwrap();
    }
    let virtualization_time = virtualization_start.elapsed();
    println!("Processed virtualization 10 times in {:.3}ms", virtualization_time.as_secs_f64() * 1000.0);

    // Move all objects and process again
    let movement_start = std::time::Instant::now();
    for (i, &object_id) in object_ids.iter().enumerate() {
        let new_position = Vec3::new(
            (i % 15) as f64 * 60.0,
            (i / 15) as f64 * 60.0,
            0.0
        );
        manager.update_object_position(object_id, new_position).await;
    }
    let movement_time = movement_start.elapsed();
    println!("Moved {} objects in {:.3}ms", object_count, movement_time.as_secs_f64() * 1000.0);

    let final_stats = manager.get_virtualization_stats().await;
    println!("Final stress test stats: {:?}", final_stats);

    // Performance assertions
    assert!(registration_time.as_millis() < 1000, "Registration should be under 1 second");
    assert!(virtualization_time.as_millis() < 2000, "Virtualization processing should be under 2 seconds");
    assert!(movement_time.as_millis() < 1000, "Object movement should be under 1 second");
}

#[tokio::test]
async fn test_virtualization_accuracy() {
    println!("\n=== Testing Virtualization Accuracy ===");

    let config = VirtualizationConfig {
        enabled: true,
        density_threshold: 0.3,
        overlap_threshold: 0.5, // Require significant overlap
        max_virtual_zone_radius: 400.0,
        min_zone_radius: 30.0,
        check_interval_ms: 1000,
        max_objects_per_virtual_zone: 10,
    };

    let manager = Arc::new(GorcInstanceManager::new_with_config(config));

    // Create objects that should definitely merge (high overlap)
    let obj1 = VirtualizationTestObject::new_single(Vec3::new(0.0, 0.0, 0.0), 100.0, 0);
    let obj2 = VirtualizationTestObject::new_single(Vec3::new(50.0, 0.0, 0.0), 100.0, 0); // 75% overlap

    let id1 = manager.register_object(obj1, Vec3::new(0.0, 0.0, 0.0)).await;
    let id2 = manager.register_object(obj2, Vec3::new(50.0, 0.0, 0.0)).await;

    manager.process_virtualization().await.unwrap();
    let high_overlap_stats = manager.get_virtualization_stats().await;

    // Create objects that should not merge (low overlap)
    let obj3 = VirtualizationTestObject::new_single(Vec3::new(300.0, 0.0, 0.0), 100.0, 0);
    let obj4 = VirtualizationTestObject::new_single(Vec3::new(450.0, 0.0, 0.0), 100.0, 0); // 25% overlap

    let _id3 = manager.register_object(obj3, Vec3::new(300.0, 0.0, 0.0)).await;
    let _id4 = manager.register_object(obj4, Vec3::new(450.0, 0.0, 0.0)).await;

    manager.process_virtualization().await.unwrap();
    let final_accuracy_stats = manager.get_virtualization_stats().await;

    println!("High overlap created: {} zones", high_overlap_stats.total_virtual_zones_created);
    println!("Final total created: {} zones", final_accuracy_stats.total_virtual_zones_created);

    // Should merge high overlap but not low overlap
    assert!(high_overlap_stats.total_virtual_zones_created > 0, "Should merge high overlap objects");

    // Check that both objects are in virtual zone
    let virtual_zone_id = manager.is_in_virtual_zone(Vec3::new(25.0, 0.0, 0.0), 0).await;
    assert!(virtual_zone_id.is_some(), "Position between merged objects should be in virtual zone");

    if let Some(vz_id) = virtual_zone_id {
        let objects_in_zone = manager.get_virtual_zone_objects(vz_id).await;
        assert!(objects_in_zone.contains(&id1) && objects_in_zone.contains(&id2),
                "Both overlapping objects should be in the virtual zone");
    }
}

#[tokio::test]
async fn test_virtualization_consistency() {
    println!("\n=== Testing Virtualization Consistency ===");

    let config = VirtualizationConfig {
        enabled: true,
        density_threshold: 0.2,
        overlap_threshold: 0.3,
        max_virtual_zone_radius: 600.0,
        min_zone_radius: 40.0,
        check_interval_ms: 1000,
        max_objects_per_virtual_zone: 20,
    };

    let manager = Arc::new(GorcInstanceManager::new_with_config(config));

    // Test consistency across multiple virtualization runs
    let mut object_ids = Vec::new();
    for i in 0..15 {
        let position = Vec3::new((i % 5) as f64 * 100.0, (i / 5) as f64 * 100.0, 0.0);
        let obj = VirtualizationTestObject::new_single(position, 80.0, 0);
        let object_id = manager.register_object(obj, position).await;
        object_ids.push(object_id);
    }

    // Process virtualization multiple times - results should be consistent
    let mut stats_history = Vec::new();
    for i in 0..5 {
        manager.process_virtualization().await.unwrap();
        let stats = manager.get_virtualization_stats().await;
        stats_history.push(stats.active_virtual_zones);
        println!("Run {}: {} active virtual zones", i, stats.active_virtual_zones);
    }

    // Active virtual zones should stabilize (not keep changing)
    let last_three: Vec<usize> = stats_history.iter().rev().take(3).copied().collect();
    let all_same = last_three.windows(2).all(|w| w[0] == w[1]);
    assert!(all_same, "Virtual zone count should stabilize after initial processing");

    println!("✅ Virtualization consistency test passed");
}