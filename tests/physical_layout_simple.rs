mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_simple_physical_layout() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("simple-physical-layout");
    env.cleanup()?;

    // Create a simple config with physical layout for two displays
    // Physical outputs in test mode are named "ascii" (main) and "TEST-2", "TEST-3", etc.
    let config_content = r#"
# Main display
output ascii scale 1.0 position 0,0 physical_size 300x200mm physical_position 0,0mm

# Secondary display below with a gap (200mm + 100mm gap = 300mm)
output TEST-2 scale 1.0 position 0,1080 physical_size 300x200mm physical_position 0,300mm
"#;

    let config_path = format!("/tmp/stilch-test-{}-config.conf", env.test_name);
    std::fs::write(&config_path, config_content)?;

    // Start compositor with 2 outputs
    env.start_compositor(&[
        "--test",
        "--logical-size", "1920x1080",  // First display
        "--ascii-output", "1920x1080+0+1080",  // Second display below
        "--config",
        &config_path,
    ])?;

    let client = TestClient::new(&env.test_socket);
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Get outputs to verify configuration
    let outputs = client.get_outputs()?;
    println!("Found {} outputs", outputs.len());
    
    for output in &outputs {
        println!(
            "Output: name={}, x={}, y={}, width={}, height={}",
            output["name"].as_str().unwrap_or("unknown"),
            output["x"], output["y"], 
            output["width"], output["height"]
        );
    }

    println!("\n=== Testing cursor movement from top to bottom display ===");
    
    // Start at bottom of first display
    let start_x = 960;
    let start_y = 1000;
    
    client.send_command(&serde_json::json!({
        "type": "MoveMouse",
        "x": start_x,
        "y": start_y
    }))?;
    
    std::thread::sleep(std::time::Duration::from_millis(100));
    
    println!("Getting initial cursor position...");
    let initial_pos = match get_cursor_position(&client) {
        Ok(pos) => pos,
        Err(e) => {
            println!("Error getting cursor position: {}", e);
            // Try to just continue with expected position
            (start_x as f64, start_y as f64)
        }
    };
    println!("Initial cursor position: {:?}", initial_pos);
    
    // Try to move down past the edge of the first display
    println!("Moving cursor down to y=1200 (past first display edge)...");
    client.send_command(&serde_json::json!({
        "type": "MoveMouse",
        "x": start_x,
        "y": 1200
    }))?;
    
    std::thread::sleep(std::time::Duration::from_millis(100));
    
    println!("Getting final cursor position...");
    let final_pos = match get_cursor_position(&client) {
        Ok(pos) => pos,
        Err(e) => {
            println!("Error getting final cursor position: {}", e);
            return Err(format!("Couldn't get final cursor position: {}", e).into());
        }
    };
    println!("Final cursor position: {:?}", final_pos);
    
    // Check if cursor jumped to second display
    if final_pos.1 >= 1080.0 {
        println!("✅ SUCCESS: Cursor jumped to second display (y >= 1080)");
    } else {
        println!("❌ FAILED: Cursor stuck at y={}, should be >= 1080", final_pos.1);
        return Err(format!("Cursor didn't jump to second display, stuck at y={}", final_pos.1).into());
    }
    
    Ok(())
}

fn get_cursor_position(client: &TestClient) -> Result<(f64, f64), Box<dyn std::error::Error>> {
    let response = client.send_command(&serde_json::json!({"type": "GetCursorPosition"}))?;
    
    // The position is returned in the message field as JSON
    if let Some(message) = response["message"].as_str() {
        // Parse the nested JSON
        let data: serde_json::Value = serde_json::from_str(message)?;
        
        let x = data["data"]["x"]
            .as_f64()
            .ok_or("No cursor X position")?;
        let y = data["data"]["y"]
            .as_f64()
            .ok_or("No cursor Y position")?;
        
        Ok((x, y))
    } else {
        Err("No message in response".into())
    }
}