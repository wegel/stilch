use common::{TestClient, TestEnv};
use serde_json::Value;

mod common;

#[test]
fn test_cursor_cannot_move_past_screen_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("cursor-boundaries");
    env.cleanup()?;

    // Start compositor with known dimensions
    env.start_compositor(&[
        "--test",
        "--ascii-size",
        "80x24",
        "--config",
        "tests/test_configs/no_gaps.conf",
    ])?;

    let client = TestClient::new(&env.test_socket);

    // Create a window to have something on screen
    let mut _window = env.start_window("TestWindow", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window")?;

    // Get the output dimensions
    let outputs = client.get_outputs()?;
    assert_eq!(outputs.len(), 1, "Should have exactly one output");

    let output = &outputs[0];
    let width = output["width"].as_i64().ok_or("Output has no width")?;
    let height = output["height"].as_i64().ok_or("Output has no height")?;

    println!("Output dimensions: {}x{}", width, height);

    // Test 1: Try to move cursor beyond right boundary
    println!("\nTest 1: Moving cursor beyond right boundary");
    let beyond_right_x = (width + 100) as i32;
    let mid_y = (height / 2) as i32;

    client.send_command(&serde_json::json!({
        "type": "MoveMouse",
        "x": beyond_right_x,
        "y": mid_y
    }))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    // Get cursor position
    let cursor_pos = get_cursor_position(&client)?;
    println!(
        "After moving to ({}, {}), cursor is at: {:?}",
        beyond_right_x, mid_y, cursor_pos
    );

    // Cursor X should be clamped to screen width - 1
    assert!(
        cursor_pos.0 < width as f64,
        "Cursor X position {} should be less than screen width {}",
        cursor_pos.0,
        width
    );

    // Test 2: Try to move cursor beyond left boundary
    println!("\nTest 2: Moving cursor beyond left boundary");
    client.send_command(&serde_json::json!({
        "type": "MoveMouse",
        "x": -100,
        "y": mid_y
    }))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    let cursor_pos = get_cursor_position(&client)?;
    println!(
        "After moving to (-100, {}), cursor is at: {:?}",
        mid_y, cursor_pos
    );

    assert!(
        cursor_pos.0 >= 0.0,
        "Cursor X position {} should be >= 0",
        cursor_pos.0
    );

    // Test 3: Try to move cursor beyond bottom boundary
    println!("\nTest 3: Moving cursor beyond bottom boundary");
    let mid_x = (width / 2) as i32;
    let beyond_bottom_y = (height + 100) as i32;

    client.send_command(&serde_json::json!({
        "type": "MoveMouse",
        "x": mid_x,
        "y": beyond_bottom_y
    }))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    let cursor_pos = get_cursor_position(&client)?;
    println!(
        "After moving to ({}, {}), cursor is at: {:?}",
        mid_x, beyond_bottom_y, cursor_pos
    );

    assert!(
        cursor_pos.1 < height as f64,
        "Cursor Y position {} should be less than screen height {}",
        cursor_pos.1,
        height
    );

    // Test 4: Try to move cursor beyond top boundary
    println!("\nTest 4: Moving cursor beyond top boundary");
    client.send_command(&serde_json::json!({
        "type": "MoveMouse",
        "x": mid_x,
        "y": -100
    }))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    let cursor_pos = get_cursor_position(&client)?;
    println!(
        "After moving to ({}, -100), cursor is at: {:?}",
        mid_x, cursor_pos
    );

    assert!(
        cursor_pos.1 >= 0.0,
        "Cursor Y position {} should be >= 0",
        cursor_pos.1
    );

    Ok(())
}

// Helper function to get cursor position from compositor
fn get_cursor_position(client: &TestClient) -> Result<(f64, f64), Box<dyn std::error::Error>> {
    let response = client.send_command(&serde_json::json!({
        "type": "GetCursorPosition"
    }))?;

    if response["type"] == "Success" {
        // Parse the JSON message that contains the cursor position
        if let Some(message) = response["message"].as_str() {
            let data: Value = serde_json::from_str(message)?;
            if let Some(cursor_data) = data.get("data") {
                let x = cursor_data["x"]
                    .as_f64()
                    .ok_or("Cursor position has no x")?;
                let y = cursor_data["y"]
                    .as_f64()
                    .ok_or("Cursor position has no y")?;
                return Ok((x, y));
            }
        }
    }

    Err("Failed to get cursor position".into())
}
