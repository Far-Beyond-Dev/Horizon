use uuid::Uuid;
use horizon_data_types::Vec3D;


mod structs;

#[allow(dead_code)]
struct Actor {
    name: String,
    location: Vec3D,
    uuid: Uuid,
    has_collision: bool,
    replication: bool,
    replication_distance: f64,
}

#[allow(dead_code, unused_variables)]
impl Actor {
    fn new(name: &str, has_collision: bool) -> Self {
        Self {
            name: name.to_string(),
            location: Vec3D { x: 0.0, y: 0.0, z: 0.0 },
            uuid: Uuid::new_v4(),
            has_collision,
            replication: false,
            replication_distance: 0.0,
        }
    }

    fn check_collision(&self, other_actor: &Actor) -> bool {
        // Check collision between self and other
        // Return true if collision detected, false otherwise
        let dx = self.location.x - other_actor.location.x;
        let dy = self.location.y - other_actor.location.y;
        let dz = self.location.z - other_actor.location.z;
        let distance = (dx * dx + dy * dy + dz * dz).sqrt();
        return distance < other_actor.replication_distance;
    }
}

#[allow(dead_code, unused_variables, unused_mut)]
fn get_overlapping_colissions(main_actor: Actor, actors: Vec<Actor>) -> Vec<Uuid> {
    let mut overlapping_collisions = Vec::new();

    //for other_actor in actors {
    //    if actor.check_collision(&other_actor) {
    //        overlapping_collisions.push(other_actor.uuid);
    //    }
    //}

    overlapping_collisions
}