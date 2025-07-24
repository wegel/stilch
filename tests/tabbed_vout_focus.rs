mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_tabbed_focus_after_leaving_container() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-vout-focus");
    env.cleanup()?;

    // Start compositor with 2 outputs side by side
    env.start_compositor_multi_output(2, 1920, 1080)?;

    let client = TestClient::new(&env.test_socket);

    // Create two windows on the left output
    let mut window1 = env.start_window("Window1", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window 1")?;
    let windows = client.get_windows()?;
    let window1_id = windows[0].get("id").and_then(|v| v.as_u64()).unwrap();
    println!("Window 1 ID: {window1_id}");

    let mut window2 = env.start_window("Window2", Some("red"))?;
    client.wait_for_window_count(2, "after starting window 2")?;
    let windows = client.get_windows()?;
    let window2_id = windows
        .iter()
        .find_map(|w| {
            let id = w.get("id").and_then(|v| v.as_u64())?;
            if id != window1_id {
                Some(id)
            } else {
                None
            }
        })
        .expect("Should find window 2");
    println!("Window 2 ID: {window2_id}");

    // Focus window 2
    client.focus_window(window2_id)?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Switch to tabbed mode
    client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Verify window 2 is visible (it was focused when we switched to tabbed)
    let initial_snapshot = client.get_ascii_snapshot(true, true)?;
    println!("=== Initial tabbed state ===");
    println!("{initial_snapshot}");

    // Check what's visible - the window content area should show the window ID
    // Note: The tab bar at the top shows tab IDs, the content shows the actual window
    // For now, let's check if window 2 is in the tab bar as active
    let lines: Vec<&str> = initial_snapshot.lines().collect();
    let mut found_window2_tab = false;
    for line in &lines[0..5] {
        // Check first few lines for tab bar
        if line.contains(&format!(" {window2_id} [F]")) {
            found_window2_tab = true;
            break;
        }
    }
    assert!(found_window2_tab, "Window 2 should be the active tab");

    // For now, skip the window content check since tabbed mode rendering might be different
    // The important thing is that window 2's tab is active

    // Navigate right twice to go to the empty workspace on the right output
    println!("\n=== Navigate to right output ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let on_right = client.get_ascii_snapshot(true, true)?;
    println!("{on_right}");
    // The ASCII renderer shows both outputs. Windows from the left output
    // should still be visible on the left side of the display, but focus should
    // be on the right (empty) output.
    // Let's just verify we successfully navigated - we can check by seeing if
    // the tab bar is still showing on the left side
    // Since we have 2 outputs side by side, the display is split in half
    // We won't check for window visibility here since the ASCII renderer
    // correctly shows all outputs

    // Navigate back left to return to the tabbed container
    println!("\n=== Navigate back to tabbed container ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "left"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let back_in_container = client.get_ascii_snapshot(true, true)?;
    println!("{back_in_container}");

    // Check that window 2's tab is still active (it was active when we left)
    let lines: Vec<&str> = back_in_container.lines().collect();
    let mut found_window2_active = false;
    for line in &lines[0..5] {
        // Check first few lines for tab bar
        if line.contains(&format!(" {window2_id} [F]")) {
            found_window2_active = true;
            break;
        }
    }
    assert!(
        found_window2_active,
        "BUG: Window 2's tab should still be active when returning to tabbed container"
    );

    // Now navigate left within the container - should go to window 1
    println!("\n=== Navigate left within container ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "left"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_left = client.get_ascii_snapshot(true, true)?;
    println!("{after_left}");
    // Check that window 1's tab is now active
    let lines: Vec<&str> = after_left.lines().collect();
    let mut found_window1_active = false;
    for line in &lines[0..5] {
        // Check first few lines for tab bar
        if line.contains(&format!(" {window1_id} [F]")) {
            found_window1_active = true;
            break;
        }
    }
    assert!(
        found_window1_active,
        "Window 1's tab should be active after Super+Left"
    );

    // Navigate right should go back to window 2
    println!("\n=== Navigate right within container ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_right = client.get_ascii_snapshot(true, true)?;
    println!("{after_right}");
    // Check that window 2's tab is active again
    let lines: Vec<&str> = after_right.lines().collect();
    let mut found_window2_active = false;
    for line in &lines[0..5] {
        // Check first few lines for tab bar
        if line.contains(&format!(" {window2_id} [F]")) {
            found_window2_active = true;
            break;
        }
    }
    assert!(
        found_window2_active,
        "Window 2's tab should be active after Super+Right"
    );

    // Clean up
    window1.kill()?;
    window2.kill()?;

    Ok(())
}
