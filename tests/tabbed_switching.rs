mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_initial_tab_switching() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-initial-switch");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--config", "tests/test_configs/no_gaps.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // Start first window and get its ID
    let mut window1 = env.start_window("Window1", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window 1")?;
    let windows = client.get_windows()?;
    let window1_id = windows[0].get("id").and_then(|v| v.as_u64()).unwrap();

    // Start second window and get its ID
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

    // Start third window and get its ID
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

    // Focus window 1 first
    client.focus_window(window1_id)?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Switch to tabbed mode
    let layout_response = client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;
    println!("Layout response: {layout_response:?}");
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Get initial state
    let initial_snapshot = client.get_ascii_snapshot(true, true)?;
    println!("=== INITIAL TABBED STATE ===");
    println!("{initial_snapshot}");

    // Check what's actually visible
    let w1_str = format!(" {window1_id} ");
    let w2_str = format!(" {window2_id} ");
    let w3_str = format!(" {window3_id} ");
    println!(
        "Looking for '{}' (window1_id={})",
        w1_str.trim(),
        window1_id
    );
    println!(
        "Looking for '{}' (window2_id={})",
        w2_str.trim(),
        window2_id
    );
    println!(
        "Looking for '{}' (window3_id={})",
        w3_str.trim(),
        window3_id
    );

    let w1_visible = initial_snapshot.contains(&w1_str);
    let w2_visible = initial_snapshot.contains(&w2_str);
    let w3_visible = initial_snapshot.contains(&w3_str);

    println!(
        "Initial visibility: w1={w1_visible}, w2={w2_visible}, w3={w3_visible}"
    );
    println!("Window {window1_id} should be visible");
    assert!(
        w1_visible,
        "Window {window1_id} should be visible initially"
    );

    // TEST 1: First Super+Right should switch to window 2
    println!("\n=== TEST 1: First Super+Right ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_first_right = client.get_ascii_snapshot(true, true)?;
    println!("{after_first_right}");

    // Check which window is visible - should be window 2
    let has_window2 = after_first_right.contains(&format!(" {window2_id} "));
    let has_window1 = after_first_right.contains(&format!(" {window1_id} "));
    let has_window3 = after_first_right.contains(&format!(" {window3_id} "));

    println!("After first Super+Right:");
    println!("  Window {window1_id} visible: {has_window1}");
    println!("  Window {window2_id} visible: {has_window2}");
    println!("  Window {window3_id} visible: {has_window3}");

    assert!(
        has_window2,
        "Window {window2_id} should be visible after first Super+Right"
    );
    assert!(
        !has_window1,
        "Window {window1_id} should NOT be visible after first Super+Right"
    );

    // TEST 2: Second Super+Right should switch to window 3
    println!("\n=== TEST 2: Second Super+Right ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_second_right = client.get_ascii_snapshot(true, true)?;
    println!("{after_second_right}");

    assert!(
        after_second_right.contains(&format!(" {window3_id} ")),
        "Window {window3_id} should be visible after second Super+Right"
    );
    assert!(
        !after_second_right.contains(&format!(" {window2_id} ")),
        "Window {window2_id} should NOT be visible"
    );

    // TEST 3: Now test Super+Left goes back
    println!("\n=== TEST 3: Super+Left ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "left"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_left = client.get_ascii_snapshot(true, true)?;
    println!("{after_left}");

    assert!(
        after_left.contains(&format!(" {window2_id} ")),
        "Window {window2_id} should be visible after Super+Left from window 3"
    );

    // Clean up
    window1.kill()?;
    window2.kill()?;
    window3.kill()?;

    Ok(())
}

#[test]
fn test_tab_escape_at_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-escape");
    env.cleanup()?;

    // Start compositor with 2 outputs side by side
    env.start_compositor_multi_output(2, 1920, 1080)?;

    let client = TestClient::new(&env.test_socket);

    // Create two windows on the left output
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

    // Focus window 1 and switch to tabbed mode
    client.focus_window(window1_id)?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Verify window 1 is visible (leftmost tab)
    let initial_state = client.get_ascii_snapshot(true, true)?;
    println!("=== Initial state (window 1 should be active) ===");
    println!("{initial_state}");

    // Check that window 1's tab is active
    let lines: Vec<&str> = initial_state.lines().collect();
    let mut found_window1_active = false;
    for line in &lines[0..5] {
        if line.contains(&format!(" {window1_id} [F]")) {
            found_window1_active = true;
            break;
        }
    }
    assert!(
        found_window1_active,
        "Window 1 should be the active tab initially"
    );

    // TEST: Navigate right twice from leftmost tab to escape to right output
    println!("\n=== TEST: Escape from tabbed container to right output ===");

    // First Super+Right should go to window 2 (next tab)
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_first_right = client.get_ascii_snapshot(true, true)?;
    println!("After first Super+Right (should be on window 2):");
    println!("{after_first_right}");

    // Second Super+Right should escape to the right output (empty)
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let on_right_output = client.get_ascii_snapshot(true, true)?;
    println!("After second Super+Right (should be on right output):");
    println!("{on_right_output}");

    // The right output should be empty, but the left output's tabbed container
    // should still be visible on the left side of the display
    // Since we're now focused on the right output, keyboard focus should be cleared

    // TEST: Navigate back left to return to the tabbed container
    println!("\n=== TEST: Return to tabbed container ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "left"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let back_in_container = client.get_ascii_snapshot(true, true)?;
    println!("After Super+Left (should be back in tabbed container):");
    println!("{back_in_container}");

    // Should be back in the tabbed container with window 2 still active
    let lines: Vec<&str> = back_in_container.lines().collect();
    let mut found_window2_active = false;
    for line in &lines[0..5] {
        if line.contains(&format!(" {window2_id} [F]")) {
            found_window2_active = true;
            break;
        }
    }
    assert!(
        found_window2_active,
        "Window 2 should still be the active tab when returning"
    );

    // Clean up
    window1.kill()?;
    window2.kill()?;

    Ok(())
}

#[test]
fn test_focus_consistency_after_tabbed() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-focus-consistency");
    env.cleanup()?;

    env.start_compositor(&["--test", "--config", "tests/test_configs/no_gaps.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // Start two windows
    let mut window1 = env.start_window("Window1", Some("blue"))?;
    let mut window2 = env.start_window("Window2", Some("red"))?;

    client.wait_for_window_count(2, "after starting windows")?;

    let windows = client.get_windows()?;
    let window1_id = windows[0].get("id").and_then(|v| v.as_u64()).unwrap();
    let _window2_id = windows[1].get("id").and_then(|v| v.as_u64()).unwrap();

    // Focus window 1 and switch to tabbed
    client.focus_window(window1_id)?;
    client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check that window 1 is still focused and visible
    let snapshot = client.get_ascii_snapshot(true, true)?;
    println!("=== After switching to tabbed ===");
    println!("{snapshot}");

    // Get focused window from IPC
    let windows_after = client.get_windows()?;
    let focused_window = windows_after
        .iter()
        .find(|w| w.get("focused").and_then(|v| v.as_bool()).unwrap_or(false));

    if let Some(focused) = focused_window {
        let focused_id = focused.get("id").and_then(|v| v.as_u64()).unwrap();
        println!("Focused window ID: {focused_id}");
        assert_eq!(
            focused_id, window1_id,
            "Window 1 should remain focused after switching to tabbed"
        );
    } else {
        panic!("No window is marked as focused!");
    }

    // Clean up
    window1.kill()?;
    window2.kill()?;

    Ok(())
}
