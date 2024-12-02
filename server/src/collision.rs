use std::collections::HashMap;
use std::time::{Instant, Duration};
use std::thread;
use nalgebra::Point3;
use rstar::{RTree, RTreeObject, AABB};

// Spatial wrapper for spheres to work with R-tree
#[derive(Debug, Clone)]
struct SpatialSphere {
    id: usize,
    center: Point3<f32>,
    radius: f32,
}

impl RTreeObject for SpatialSphere {
    type Envelope = AABB<[f32; 3]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            [
                self.center.x - self.radius,
                self.center.y - self.radius,
                self.center.z - self.radius
            ],
            [
                self.center.x + self.radius,
                self.center.y + self.radius,
                self.center.z + self.radius
            ],
        )
    }
}

// We'll use Arc to make our callbacks shareable and cloneable
use std::sync::Arc;
type CallbackFn = Arc<dyn Fn() + Send + Sync>;

// Sphere with event handling
#[derive(Clone)]
struct Sphere {
    center: Point3<f32>,
    radius: f32,
    on_enter: CallbackFn,
    on_exit: CallbackFn,
    contains_player: bool,
}

impl Sphere {
    fn check_position(&mut self, position: Point3<f32>) -> bool {
        let dx = position.x - self.center.x;
        let dy = position.y - self.center.y;
        let dz = position.z - self.center.z;
        let is_inside = dx * dx + dy * dy + dz * dz <= self.radius * self.radius;

        let state_changed = is_inside != self.contains_player;
        
        if state_changed {
            if is_inside {
                (self.on_enter)();
            } else {
                (self.on_exit)();
            }
            self.contains_player = is_inside;
        }

        state_changed
    }
}

pub struct SphereSystem {
    spheres: HashMap<usize, Sphere>,
    spatial_index: RTree<SpatialSphere>,
    next_id: usize,
}

impl SphereSystem {
    pub fn new() -> Self {
        Self {
            spheres: HashMap::new(),
            spatial_index: RTree::new(),
            next_id: 0,
        }
    }

    pub fn add_sphere(
        &mut self,
        center: Point3<f32>,
        radius: f32,
        on_enter: impl Fn() + Send + Sync + 'static,
        on_exit: impl Fn() + Send + Sync + 'static,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;

        let sphere = Sphere {
            center,
            radius,
            on_enter: Arc::new(on_enter),
            on_exit: Arc::new(on_exit),
            contains_player: false,
        };

        // Add to both storage and spatial index
        let spatial = SpatialSphere { id, center, radius };
        self.spatial_index.insert(spatial);
        self.spheres.insert(id, sphere);
        
        id
    }

    pub fn remove_sphere(&mut self, id: usize) {
        if self.spheres.remove(&id).is_some() {
            // Create a new R-tree without the removed sphere
            let spheres: Vec<_> = self.spatial_index
                .iter()
                .filter(|s| s.id != id)
                .cloned()
                .collect();
            self.spatial_index = RTree::bulk_load(spheres);
        }
    }

    pub fn update_position(&mut self, position: Point3<f32>) {
        // Create a bounding box for the query with a small margin
        // Use a larger query box based on the maximum sphere radius (8.0)
        let query_box = AABB::from_corners(
            [
                position.x - 10.0,  // Increased from 1.0 to catch nearby spheres
                position.y - 10.0,
                position.z - 10.0
            ],
            [
                position.x + 10.0,
                position.y + 10.0,
                position.z + 10.0
            ],
        );

        // Get only spheres that could possibly contain the point
        let nearby_spheres: Vec<_> = self.spatial_index
            .locate_in_envelope(&query_box)
            .map(|spatial| spatial.id)
            .collect();

        // Update only nearby spheres
        for &id in &nearby_spheres {
            if let Some(sphere) = self.spheres.get_mut(&id) {
                sphere.check_position(position);
            }
        }
    }

    // Bulk insertion method for better performance when adding many spheres
    pub fn bulk_add_spheres(
        &mut self,
        spheres: Vec<(Point3<f32>, f32, Arc<dyn Fn() + Send + Sync>, Arc<dyn Fn() + Send + Sync>)>
    ) {
        let mut spatial_spheres = Vec::with_capacity(spheres.len());
        
        for (center, radius, on_enter, on_exit) in spheres {
            let id = self.next_id;
            self.next_id += 1;

            let sphere = Sphere {
                center,
                radius,
                on_enter,
                on_exit,
                contains_player: false,
            };

            spatial_spheres.push(SpatialSphere { id, center, radius });
            self.spheres.insert(id, sphere);
        }

        // Bulk load into R-tree
        self.spatial_index = RTree::bulk_load(spatial_spheres);
    }

    // Method to help with testing/debugging
    #[cfg(test)]
    pub fn sphere_count(&self) -> usize {
        self.spheres.len()
    }
}

pub fn main() {
    let mut system = SphereSystem::new();
    
    // Add some test spheres in a grid pattern
    for x in 0..10 {
        for y in 0..10 {
            for z in 0..10 {
                let pos = Point3::new(x as f32 * 1.0, y as f32 * 1.0, z as f32 * 1.0);
                system.add_sphere(
                    pos,
                    8.0, // radius
                    {
                        let pos = pos;
                        move || println!("Entered sphere at position ({:.1}, {:.1}, {:.1})", 
                            pos.x, pos.y, pos.z)
                    },
                    {
                        let pos = pos;
                        move || println!("Left sphere at position ({:.1}, {:.1}, {:.1})", 
                            pos.x, pos.y, pos.z)
                    }
                );
            }
        }
    }

    println!("Moving player in 3D figure-8 pattern...");
    println!("Press Ctrl+C to stop");

    let start_time = Instant::now();
    let movement_speed = 0.1; // Adjust this to change movement speed
    
    loop {
        let elapsed = start_time.elapsed().as_secs_f32();
        
        // Create a figure-8 pattern in 3D space
        // Adjust the movement to intersect with our grid of spheres
        let x = 20.0 + (30.0 * (elapsed * movement_speed).sin());
        let y = 20.0 + (30.0 * (elapsed * movement_speed * 2.0).sin());
        let z = 20.0 + (30.0 * (elapsed * movement_speed).cos());
        
        let position = Point3::new(x, y, z);
        system.update_position(position);
        
        // Sleep briefly to control update rate
        thread::sleep(Duration::from_millis(16)); // ~60 FPS
        
        // Optional: print current position periodically
        if elapsed as i32 % 5 == 0 {
            println!("Player position: ({:.1}, {:.1}, {:.1})", x, y, z);
        }
    }
}