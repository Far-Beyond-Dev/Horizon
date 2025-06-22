use crate::types::*;

pub fn setup_inventory_handler(event: InventorySettingRequest) {
    let success = setup_inventory(event.slot_count, event.inventory_count);

    if success {
        println!(
            "🎒 Inventory setup complete: {:?} slots, {:?} inventories",
            event.slot_count, event.inventory_count
        );
    } else {
        println!("❌ Failed to setup inventory");
    }
}

fn setup_inventory(slot_count: Option<u32>, inventory_count: Option<u8>) -> bool {
    let inventory_settings = InventorySettingRequest {
        slot_count,
        inventory_count,
    };

    match serde_json::to_string(&inventory_settings) {
        Ok(json_string) => {
            println!("Inventory settings: {}", json_string);
            true
        }
        Err(e) => {
            println!("Failed to serialize inventory settings: {}", e);
            false
        }
    }
}