mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_tabbed_active_tab_content_consistency() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-visibility-consistency");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--config", "tests/test_configs/no_gaps.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // Create three windows to test various scenarios
    let mut window1 = env.start_window("Window1", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window 1")?;
    let windows = client.get_windows()?;
    let window1_id = windows[0].get("id").and_then(|v| v.as_u64()).unwrap();

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

    let mut window3 = env.start_window("Window3", Some("green"))?;
    client.wait_for_window_count(3, "after starting window 3")?;
    let windows = client.get_windows()?;
    let window3_id = windows
        .iter()
        .find_map(|w| {
            let id = w.get("id").and_then(|v| v.as_u64())?;
            if id != window1_id && id != window2_id {
                Some(id)
            } else {
                None
            }
        })
        .expect("Should find window 3");

    println!(
        "Window IDs: w1={window1_id}, w2={window2_id}, w3={window3_id}"
    );

    // Helper function to verify tab consistency
    let verify_tab_consistency = |client: &TestClient,
                                  expected_active_id: u64,
                                  context: &str|
     -> Result<(), Box<dyn std::error::Error>> {
        let snapshot = client.get_ascii_snapshot(true, true)?;

        // Check which tab is marked as active [F]
        let lines: Vec<&str> = snapshot.lines().collect();
        let mut active_tab_id = None;

        for line in &lines[0..5] {
            if line.contains(&format!(" {window1_id} [F]")) {
                active_tab_id = Some(window1_id);
            } else if line.contains(&format!(" {window2_id} [F]")) {
                active_tab_id = Some(window2_id);
            } else if line.contains(&format!(" {window3_id} [F]")) {
                active_tab_id = Some(window3_id);
            }
        }

        // Check which window content is actually visible
        let mut visible_window_id = None;
        if snapshot.contains(&format!(" {window1_id} ")) {
            visible_window_id = Some(window1_id);
        } else if snapshot.contains(&format!(" {window2_id} ")) {
            visible_window_id = Some(window2_id);
        } else if snapshot.contains(&format!(" {window3_id} ")) {
            visible_window_id = Some(window3_id);
        }

        println!("\n=== {context} ===");
        println!("Expected active: {expected_active_id}");
        println!("Tab marked [F]: {active_tab_id:?}");
        println!("Content visible: {visible_window_id:?}");

        // Verify consistency: active tab and visible content must match
        assert_eq!(
            active_tab_id,
            Some(expected_active_id),
            "{context}: Tab marked as active should be window {expected_active_id}"
        );
        assert_eq!(
            visible_window_id,
            Some(expected_active_id),
            "{context}: Visible content should be window {expected_active_id}"
        );
        assert_eq!(
            active_tab_id, visible_window_id,
            "{context}: Active tab marker and visible content must be consistent!"
        );

        Ok(())
    };

    // TEST 1: Switch to tabbed from window 3 (rightmost)
    println!("\n=== TEST 1: Switch to tabbed from window 3 ===");

    // Window 3 should have focus (it was created last)
    client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    verify_tab_consistency(
        &client,
        window3_id,
        "After switching to tabbed from window 3",
    )?;

    // TEST 2: Navigate left to window 2
    println!("\n=== TEST 2: Navigate left to window 2 ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "left"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    verify_tab_consistency(&client, window2_id, "After Super+Left to window 2")?;

    // TEST 3: Navigate left to window 1
    println!("\n=== TEST 3: Navigate left to window 1 ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "left"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    verify_tab_consistency(&client, window1_id, "After Super+Left to window 1")?;

    // TEST 4: Switch back to tiled, focus window 2, then back to tabbed
    println!("\n=== TEST 4: Tiled -> Focus window 2 -> Tabbed ===");

    // Switch to tiled
    client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "toggle_split"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Focus window 2
    client.focus_window(window2_id)?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Switch back to tabbed
    client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    verify_tab_consistency(
        &client,
        window2_id,
        "After switching to tabbed from window 2",
    )?;

    // TEST 5: Navigate right to window 3
    println!("\n=== TEST 5: Navigate right to window 3 ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    verify_tab_consistency(&client, window3_id, "After Super+Right to window 3")?;

    // TEST 6: Close middle window and verify consistency
    println!("\n=== TEST 6: Close window 2 (middle) ===");

    // First navigate to window 2
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "left"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Close window 2
    window2.kill()?;
    client.wait_for_window_count(2, "after closing window 2")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Should now be on window 3 (the next tab)
    let snapshot = client.get_ascii_snapshot(true, true)?;
    let lines: Vec<&str> = snapshot.lines().collect();

    // Check active tab
    let mut active_tab_id = None;
    for line in &lines[0..5] {
        if line.contains(&format!(" {window1_id} [F]")) {
            active_tab_id = Some(window1_id);
        } else if line.contains(&format!(" {window3_id} [F]")) {
            active_tab_id = Some(window3_id);
        }
    }

    // Check visible content
    let mut visible_window_id = None;
    if snapshot.contains(&format!(" {window1_id} ")) {
        visible_window_id = Some(window1_id);
    } else if snapshot.contains(&format!(" {window3_id} ")) {
        visible_window_id = Some(window3_id);
    }

    println!("\n=== After closing window 2 ===");
    println!("Tab marked [F]: {active_tab_id:?}");
    println!("Content visible: {visible_window_id:?}");

    // The important check: active tab and visible content must match
    assert_eq!(
        active_tab_id, visible_window_id,
        "After closing middle tab: Active tab marker and visible content must be consistent!"
    );

    // Clean up
    window1.kill()?;
    window3.kill()?;

    println!("\n=== All consistency checks passed! ===");

    Ok(())
}

#[test]
fn test_tabbed_focus_preservation_consistency() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-focus-preservation-consistency");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--config", "tests/test_configs/no_gaps.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // This test specifically reproduces the bug where:
    // 1. Create 2 windows
    // 2. Go back to first window
    // 3. Switch to tabbed
    // 4. Check that both the active tab AND visible content are window 1

    let mut window1 = env.start_window("Window1", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window 1")?;
    let windows = client.get_windows()?;
    let window1_id = windows[0].get("id").and_then(|v| v.as_u64()).unwrap();

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

    println!("Window IDs: w1={window1_id}, w2={window2_id}");

    // Go back to window 1 (Super+Left)
    println!("\nGoing back to window 1 with Super+Left...");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "left"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Verify window 1 is focused
    let windows_after_left = client.get_windows()?;
    let focused_window = windows_after_left
        .iter()
        .find(|w| w.get("focused").and_then(|v| v.as_bool()).unwrap_or(false))
        .and_then(|w| w.get("id").and_then(|v| v.as_u64()));
    assert_eq!(
        focused_window,
        Some(window1_id),
        "Window 1 should be focused after Super+Left"
    );

    // Switch to tabbed mode
    println!("Switching to tabbed mode...");
    client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Get snapshot and check consistency
    let snapshot = client.get_ascii_snapshot(true, true)?;
    println!("\nSnapshot after switching to tabbed:");
    println!("{snapshot}");

    // Check which tab is marked as active [F]
    let lines: Vec<&str> = snapshot.lines().collect();
    let mut active_tab_id = None;

    for line in &lines[0..5] {
        if line.contains(&format!(" {window1_id} [F]")) {
            active_tab_id = Some(window1_id);
            println!("Found window 1 marked as active tab [F]");
        } else if line.contains(&format!(" {window2_id} [F]")) {
            active_tab_id = Some(window2_id);
            println!("Found window 2 marked as active tab [F]");
        }
    }

    // Check which window content is actually visible
    let window1_visible = snapshot.contains(&format!(" {window1_id} "));
    let window2_visible = snapshot.contains(&format!(" {window2_id} "));

    println!("\n=== CONSISTENCY CHECK ===");
    println!(
        "Window 1 marked as active [F]: {}",
        active_tab_id == Some(window1_id)
    );
    println!("Window 1 content visible: {window1_visible}");
    println!(
        "Window 2 marked as active [F]: {}",
        active_tab_id == Some(window2_id)
    );
    println!("Window 2 content visible: {window2_visible}");

    // The critical assertions:
    // 1. Window 1 should be marked as active
    assert_eq!(
        active_tab_id,
        Some(window1_id),
        "Window 1 should be marked as active tab [F]"
    );

    // 2. Window 1 content should be visible
    assert!(window1_visible, "Window 1 content should be visible");
    assert!(!window2_visible, "Window 2 content should NOT be visible");

    // 3. Most importantly: active tab and visible content must match
    if window1_visible && window2_visible {
        panic!("Both windows are visible! This should never happen in tabbed mode");
    }

    if active_tab_id == Some(window1_id) && !window1_visible {
        panic!("CONSISTENCY ERROR: Window 1 is marked active but not visible!");
    }

    if active_tab_id == Some(window2_id) && !window2_visible {
        panic!("CONSISTENCY ERROR: Window 2 is marked active but not visible!");
    }

    println!("\n=== Consistency check PASSED ===");

    // Clean up
    window1.kill()?;
    window2.kill()?;

    Ok(())
}
