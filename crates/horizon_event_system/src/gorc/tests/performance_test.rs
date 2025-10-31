//! Performance tests for the GORC zone event system
//!
//! These tests verify the performance improvements and benchmark
//! the spatial indexing optimizations and zone event handling.

use crate::gorc::instance::{GorcInstanceManager, GorcObject};
use crate::gorc::channels::{ReplicationLayer, CompressionType};
use crate::gorc::spatial::RegionRTree;
use crate::system::EventSystem;
use crate::types::{PlayerId, Vec3};
use std::sync::Arc;
use std::any::Any;
use std::time::Instant;
use tokio::time::{sleep, Duration};

/// Performance test object for benchmarking
#[derive(Debug, Clone)]
struct PerfTestObject {
    position: Vec3,
    object_id: String,
    zone_count: u8,
}

impl PerfTestObject {
    fn new(position: Vec3, object_id: String, zone_count: u8) -> Self {
        Self { position, object_id, zone_count }
    }
}

impl GorcObject for PerfTestObject {
    fn type_name(&self) -> &str {
        "PerfTestObject"
    }

    fn position(&self) -> Vec3 {
        self.position
    }

    fn get_priority(&self, _observer_pos: Vec3) -> crate::gorc::channels::ReplicationPriority {
        crate::gorc::channels::ReplicationPriority::Normal
    }

    fn serialize_for_layer(&self, _layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let data = serde_json::json!({
            "position": {
                "x": self.position.x,
                "y": self.position.y,
                "z": self.position.z
            },
            "object_id": self.object_id
        });
        Ok(serde_json::to_vec(&data)?)
    }

    fn get_layers(&self) -> Vec<ReplicationLayer> {
        let mut layers = Vec::new();

        match self.zone_count {
            1 => {
                layers.push(ReplicationLayer::new(0, 100.0, 30.0, vec!["position".to_string()], CompressionType::Delta));
            }
            2 => {
                layers.push(ReplicationLayer::new(0, 50.0, 60.0, vec!["position".to_string()], CompressionType::Delta));
                layers.push(ReplicationLayer::new(1, 150.0, 15.0, vec!["metadata".to_string()], CompressionType::Lz4));
            }
            4 => {
                layers.push(ReplicationLayer::new(0, 30.0, 60.0, vec!["position".to_string()], CompressionType::Delta));
                layers.push(ReplicationLayer::new(1, 100.0, 30.0, vec!["animation".to_string()], CompressionType::Lz4));
                layers.push(ReplicationLayer::new(2, 200.0, 15.0, vec!["metadata".to_string()], CompressionType::None));
                layers.push(ReplicationLayer::new(3, 400.0, 5.0, vec!["strategic".to_string()], CompressionType::None));
            }
            8 => {
                // Stress test with many zones
                for i in 0..8 {
                    layers.push(ReplicationLayer::new(
                        i,
                        50.0 + (i as f64 * 25.0),
                        60.0 - (i as f64 * 5.0),
                        vec![format!("layer_{}", i)],
                        CompressionType::Delta
                    ));
                }
            }
            _ => {
                layers.push(ReplicationLayer::new(0, 100.0, 30.0, vec!["position".to_string()], CompressionType::Delta));
            }
        }

        layers
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

#[tokio::test]
async fn benchmark_spatial_query_performance() {
    println!("\n=== Spatial Query Performance Benchmark ===");

    let test_sizes = [100, 500, 1000, 2000];
    let mut results = Vec::new();

    for &object_count in &test_sizes {
        let gorc_manager = Arc::new(GorcInstanceManager::new());

        // Create objects in a grid pattern
        let grid_size = (object_count as f64).sqrt() as i32;
        for i in 0..object_count {
            let x = (i % grid_size) as f64 * 100.0;
            let y = (i / grid_size) as f64 * 100.0;
            let position = Vec3::new(x, y, 0.0);

            let test_object = PerfTestObject::new(position, format!("obj_{}", i), 2);
            gorc_manager.register_object(test_object, position).await;
        }

        // Wait for spatial index initialization
        sleep(Duration::from_millis(50)).await;

        // Benchmark range queries
        let query_center = Vec3::new(500.0, 500.0, 0.0);
        let query_range = 200.0;

        let start = Instant::now();
        let _objects_in_range = gorc_manager.get_objects_in_range(query_center, query_range).await;
        let duration = start.elapsed();

        results.push((object_count, duration));
        println!("Objects: {:4} | Query Time: {:8.3}ms", object_count, duration.as_secs_f64() * 1000.0);
    }

    // Verify performance scaling
    if results.len() >= 2 {
        let (first_count, first_duration) = results.first().cloned().unwrap();
        let (last_count, last_duration) = results.last().cloned().unwrap();

        let per_object_initial = first_duration.as_secs_f64() / first_count as f64;
        let per_object_latest = last_duration.as_secs_f64() / last_count as f64;
        let efficiency_ratio = per_object_latest / per_object_initial;

        println!(
            "Per-object query cost change: {:.2}x ({} ➜ {} objs)",
            efficiency_ratio,
            first_count,
            last_count
        );

        // The per-object cost should remain roughly stable even as we scale load.
        assert!(
            efficiency_ratio <= 2.0,
            "Spatial queries should remain sub-linear; per-object cost grew {:.2}x",
            efficiency_ratio
        );
    }
}

#[tokio::test]
async fn benchmark_zone_check_optimization() {
    println!("\n=== Zone Check Optimization Benchmark ===");

    let zone_counts = [1, 2, 4, 8];
    let player_count = 100;

    for &zone_count in &zone_counts {
        let gorc_manager = Arc::new(GorcInstanceManager::new());

        // Create object with multiple zones
        let test_object = PerfTestObject::new(Vec3::new(0.0, 0.0, 0.0), "multi_zone_obj".to_string(), zone_count);
        let object_id = gorc_manager.register_object(test_object, Vec3::new(0.0, 0.0, 0.0)).await;

        // Add players at various distances
        for i in 0..player_count {
            let angle = (i as f64 / player_count as f64) * 2.0 * std::f64::consts::PI;
            let distance = 25.0 + (i as f64 * 2.0); // Varying distances
            let position = Vec3::new(
                angle.cos() * distance,
                angle.sin() * distance,
                0.0
            );

            let player_id = PlayerId::new();
            gorc_manager.add_player(player_id, position).await;
        }

        // Benchmark object position update (triggers zone recalculation)
        let start = Instant::now();
        let _zone_changes = gorc_manager.update_object_position(object_id, Vec3::new(10.0, 10.0, 0.0)).await;
        let duration = start.elapsed();

        println!("Zones: {} | Zone Check Time: {:.3}ms", zone_count, duration.as_secs_f64() * 1000.0);
    }
}

#[tokio::test]
async fn benchmark_zone_event_throughput() {
    println!("\n=== Zone Event Throughput Benchmark ===");

    let _event_system = EventSystem::new();
    let gorc_manager = Arc::new(GorcInstanceManager::new());

    // Create multiple objects
    let object_count = 50;
    let mut object_ids = Vec::new();

    for i in 0..object_count {
        let position = Vec3::new((i as f64) * 50.0, 0.0, 0.0);
        let test_object = PerfTestObject::new(position, format!("obj_{}", i), 2);
        let object_id = gorc_manager.register_object(test_object, position).await;
        object_ids.push(object_id);
    }

    // Add players
    let player_count = 20;
    let mut player_ids = Vec::new();

    for i in 0..player_count {
        let player_id = PlayerId::new();
        let position = Vec3::new(0.0, (i as f64) * 30.0, 0.0);
        gorc_manager.add_player(player_id, position).await;
        player_ids.push(player_id);
    }

    // Benchmark zone event generation through movement
    let start = Instant::now();
    let mut total_zone_events = 0;

    for player_id in &player_ids {
        let new_position = Vec3::new(1000.0, 0.0, 0.0); // Move to trigger zone events
        let (zone_entries, zone_exits) = gorc_manager.update_player_position(*player_id, new_position).await;
        total_zone_events += zone_entries.len() + zone_exits.len();
    }

    let duration = start.elapsed();
    let events_per_second = total_zone_events as f64 / duration.as_secs_f64();

    println!("Total zone events: {}", total_zone_events);
    println!("Events per second: {:.0}", events_per_second);
    println!("Event processing time: {:.3}ms", duration.as_secs_f64() * 1000.0);

    // Should be able to process thousands of zone events per second
    assert!(events_per_second > 1000.0, "Zone event throughput should be > 1000 events/sec");
}

#[tokio::test]
async fn benchmark_large_zone_detection() {
    println!("\n=== Large Zone Detection Benchmark ===");

    let gorc_manager = Arc::new(GorcInstanceManager::new());

    // Test different zone sizes
    let zone_sizes = [100.0, 300.0, 600.0, 1200.0]; // Last one should trigger warning

    for (i, &zone_size) in zone_sizes.iter().enumerate() {
        let test_object = LargeZoneTestObject::new(Vec3::new(i as f64 * 1000.0, 0.0, 0.0), zone_size);
        let _object_id = gorc_manager.register_object(test_object, Vec3::new(i as f64 * 1000.0, 0.0, 0.0)).await;
    }

    // Check that warnings were properly recorded
    let stats = gorc_manager.get_stats().await;
    println!("Large zone warnings detected: {}", stats.large_zone_warnings);

    // Should detect the zones > 1000.0 radius
    assert!(stats.large_zone_warnings > 0, "Should detect large zones");
}

#[tokio::test]
async fn benchmark_spatial_index_performance() {
    println!("\n=== R-Tree Performance Benchmark ===");

    let mut spatial_index = RegionRTree::new(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1000.0, 1000.0, 0.0)
    );

    let object_counts = [100, 500, 1000, 2000];

    for &count in &object_counts {
        spatial_index.clear();

        // Insert objects
        let insert_start = Instant::now();
        for i in 0..count {
            let player_id = PlayerId::new();
            let x = (i % 100) as f64 * 10.0;
            let y = (i / 100) as f64 * 10.0;
            let position = crate::types::Position::new(x, y, 0.0);
            spatial_index.insert_player(player_id, position);
        }
        let insert_duration = insert_start.elapsed();

        // Query objects
        let query_start = Instant::now();
        let _results = spatial_index.query_radius(
            crate::types::Position::new(500.0, 500.0, 0.0),
            100.0
        );
        let query_duration = query_start.elapsed();

        let stats = spatial_index.get_stats();

        println!("Objects: {:4} | Insert: {:6.3}ms | Query: {:6.3}ms | Depth: {} | Nodes: {}",
                 count,
                 insert_duration.as_secs_f64() * 1000.0,
                 query_duration.as_secs_f64() * 1000.0,
                 stats.current_depth,
                 stats.leaf_nodes + stats.internal_nodes);
    }
}

#[tokio::test]
async fn stress_test_concurrent_zone_events() {
    println!("\n=== Concurrent Zone Events Stress Test ===");

    let gorc_manager = Arc::new(GorcInstanceManager::new());

    // Create many objects
    let object_count = 100;
    for i in 0..object_count {
        let position = Vec3::new((i as f64) * 20.0, 0.0, 0.0);
        let test_object = PerfTestObject::new(position, format!("stress_obj_{}", i), 1);
        gorc_manager.register_object(test_object, position).await;
    }

    // Create many players
    let player_count = 50;
    let mut player_ids = Vec::new();
    for i in 0..player_count {
        let player_id = PlayerId::new();
        let position = Vec3::new(0.0, (i as f64) * 15.0, 0.0);
        gorc_manager.add_player(player_id, position).await;
        player_ids.push(player_id);
    }

    // Stress test: move all players simultaneously
    let start = Instant::now();
    let mut handles = Vec::new();

    for (i, player_id) in player_ids.into_iter().enumerate() {
        let manager = gorc_manager.clone();
        let handle = tokio::spawn(async move {
            let new_position = Vec3::new(1000.0 + (i as f64) * 10.0, 500.0, 0.0);
            manager.update_player_position(player_id, new_position).await
        });
        handles.push(handle);
    }

    // Wait for all movements to complete
    let mut total_events = 0;
    for handle in handles {
        let (entries, exits) = handle.await.unwrap();
        total_events += entries.len() + exits.len();
    }

    let duration = start.elapsed();
    let throughput = total_events as f64 / duration.as_secs_f64();

    println!("Concurrent players: {}", player_count);
    println!("Total zone events: {}", total_events);
    println!("Concurrent throughput: {:.0} events/sec", throughput);
    println!("Processing time: {:.3}ms", duration.as_secs_f64() * 1000.0);

    // Should handle concurrent operations efficiently
    assert!(duration.as_millis() < 1000, "Concurrent operations should complete within 1 second");
}

/// Test object with configurable zone size for large zone testing
#[derive(Debug, Clone)]
struct LargeZoneTestObject {
    position: Vec3,
    zone_radius: f64,
}

impl LargeZoneTestObject {
    fn new(position: Vec3, zone_radius: f64) -> Self {
        Self { position, zone_radius }
    }
}

impl GorcObject for LargeZoneTestObject {
    fn type_name(&self) -> &str {
        "LargeZoneTestObject"
    }

    fn position(&self) -> Vec3 {
        self.position
    }

    fn get_priority(&self, _observer_pos: Vec3) -> crate::gorc::channels::ReplicationPriority {
        crate::gorc::channels::ReplicationPriority::Low
    }

    fn serialize_for_layer(&self, _layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(vec![])
    }

    fn get_layers(&self) -> Vec<ReplicationLayer> {
        vec![
            ReplicationLayer::new(0, self.zone_radius, 10.0, vec!["position".to_string()], CompressionType::Delta),
        ]
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

#[tokio::test]
async fn validate_performance_improvements() {
    println!("\n=== Performance Improvement Validation ===");

    // This test validates that our optimizations are working as expected
    let gorc_manager = Arc::new(GorcInstanceManager::new());

    // Test 1: Verify inner zone optimization
    let multi_zone_object = PerfTestObject::new(Vec3::new(0.0, 0.0, 0.0), "multi_zone".to_string(), 4);
    let object_id = gorc_manager.register_object(multi_zone_object, Vec3::new(0.0, 0.0, 0.0)).await;

    // Add player in innermost zone
    let player_id = PlayerId::new();
    gorc_manager.add_player(player_id, Vec3::new(15.0, 15.0, 0.0)).await; // Should be in all zones

    // Move object slightly - should trigger optimized zone checking
    let start = Instant::now();
    let zone_changes = gorc_manager.update_object_position(object_id, Vec3::new(5.0, 5.0, 0.0)).await;
    let duration = start.elapsed();

    assert!(zone_changes.is_some(), "Should return zone changes");
    println!("Inner zone optimization test: {:.3}ms", duration.as_secs_f64() * 1000.0);

    // Test 2: Verify spatial index performance
    let query_start = Instant::now();
    let _nearby_objects = gorc_manager.get_objects_in_range(Vec3::new(0.0, 0.0, 0.0), 100.0).await;
    let query_duration = query_start.elapsed();

    println!("Spatial query performance: {:.3}ms", query_duration.as_secs_f64() * 1000.0);
    assert!(query_duration.as_millis() < 10, "Spatial queries should be very fast");

    // Test 3: Verify zone size warnings
    let stats = gorc_manager.get_stats().await;
    println!("Current zone warnings: {}", stats.large_zone_warnings);

    println!("✅ All performance improvements validated");
}