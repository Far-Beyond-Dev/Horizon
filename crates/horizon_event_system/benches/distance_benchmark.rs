// Benchmark comparing distance() vs distance_squared() performance
// 
// Run with: cargo bench --bench distance_benchmark
//
// Expected results: distance_squared() should be ~50-70% faster

use criterion::{black_box, criterion_group, criterion_main, Criterion};

#[derive(Clone, Copy, Debug)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn distance(&self, other: Vec3) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    pub fn distance_squared(&self, other: Vec3) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }
}

fn sphere_test_with_distance(center: Vec3, points: &[Vec3], radius: f64) -> usize {
    points
        .iter()
        .filter(|&&point| point.distance(center) <= radius)
        .count()
}

fn sphere_test_with_distance_squared(center: Vec3, points: &[Vec3], radius: f64) -> usize {
    let radius_sq = radius * radius;
    points
        .iter()
        .filter(|&&point| point.distance_squared(center) <= radius_sq)
        .count()
}

fn benchmark_distance_methods(c: &mut Criterion) {
    // Generate 10,000 random points (typical for a busy game server zone)
    let center = Vec3::new(0.0, 0.0, 0.0);
    let radius = 100.0;
    
    let points: Vec<Vec3> = (0..10000)
        .map(|i| {
            let angle = (i as f64) * 0.01;
            let dist = (i as f64) * 0.02;
            Vec3::new(
                dist * angle.cos(),
                dist * angle.sin(),
                (i as f64) * 0.001,
            )
        })
        .collect();

    let mut group = c.benchmark_group("sphere_containment");
    
    group.bench_function("with_distance (sqrt)", |b| {
        b.iter(|| {
            sphere_test_with_distance(
                black_box(center),
                black_box(&points),
                black_box(radius),
            )
        })
    });

    group.bench_function("with_distance_squared (optimized)", |b| {
        b.iter(|| {
            sphere_test_with_distance_squared(
                black_box(center),
                black_box(&points),
                black_box(radius),
            )
        })
    });

    group.finish();

    // Individual distance calculations
    let mut group = c.benchmark_group("single_distance");
    let point1 = Vec3::new(50.0, 30.0, 20.0);
    let point2 = Vec3::new(10.0, 15.0, 5.0);

    group.bench_function("distance", |b| {
        b.iter(|| black_box(point1).distance(black_box(point2)))
    });

    group.bench_function("distance_squared", |b| {
        b.iter(|| black_box(point1).distance_squared(black_box(point2)))
    });

    group.finish();
}

criterion_group!(benches, benchmark_distance_methods);
criterion_main!(benches);
