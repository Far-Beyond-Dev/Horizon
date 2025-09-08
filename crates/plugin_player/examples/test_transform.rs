use ue_types::{Transform, Vector, DQuat};

fn main() {
    // Create a sample transform
    let transform = Transform {
        location: Vector::new(10.0, 20.0, 30.0),
        rotation: DQuat::from_rotation_y(1.57), // 90 degrees
        scale: Vector::new(1.0, 1.0, 1.0),
    };
    
    // Serialize to JSON
    let json = serde_json::to_string_pretty(&transform).unwrap();
    println!("Transform JSON:");
    println!("{}", json);
    
    // Test individual Vector
    let vec = Vector::new(1.0, 2.0, 3.0);
    let vec_json = serde_json::to_string_pretty(&vec).unwrap();
    println!("\nVector JSON:");
    println!("{}", vec_json);
    
    // Test individual Quaternion
    let quat = DQuat::from_rotation_z(0.5);
    let quat_json = serde_json::to_string_pretty(&quat).unwrap();
    println!("\nQuaternion JSON:");
    println!("{}", quat_json);
}
