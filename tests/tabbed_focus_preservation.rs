mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_focused_window_becomes_active_tab() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-focus-preservation");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--config", "tests/test_configs/no_gaps.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // Create two windows
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

    // TEST: After creating 2 windows, window 2 has focus by default (it was created last)
    // Go back to window 1 (Super+Left), then switch to tabbed mode
    // The bug: window 1 shows as active tab [F] but window 2 content is displayed

    println!("\n=== TEST: Create 2 windows, go to first, then tab (reproducing the bug) ===");

    // Verify window 2 is focused initially
    let windows_initial = client.get_windows()?;
    let focused_initial = windows_initial
        .iter()
        .find(|w| w.get("focused").and_then(|v| v.as_bool()).unwrap_or(false));
    if let Some(focused) = focused_initial {
        let focused_id = focused.get("id").and_then(|v| v.as_u64()).unwrap();
        println!("Initial focused window: {focused_id}");
        assert_eq!(
            focused_id, window2_id,
            "Window 2 should be focused initially"
        );
    }

    // Go back to window 1 (Super+Left)
    println!("Going back to window 1 with Super+Left...");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "left"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Verify window 1 is now focused
    let windows_after_left = client.get_windows()?;
    let focused_after_left = windows_after_left
        .iter()
        .find(|w| w.get("focused").and_then(|v| v.as_bool()).unwrap_or(false));
    if let Some(focused) = focused_after_left {
        let focused_id = focused.get("id").and_then(|v| v.as_u64()).unwrap();
        println!("Focused window after Super+Left: {focused_id}");
        assert_eq!(
            focused_id, window1_id,
            "Window 1 should be focused after Super+Left"
        );
    }

    // Switch to tabbed mode from window 1
    println!("Switching to tabbed mode...");
    client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check which window is the active tab and which content is visible
    let snapshot = client.get_ascii_snapshot(true, true)?;
    println!("Snapshot after switching to tabbed:");
    println!("{snapshot}");

    // Check which tab is marked as active [F]
    let lines: Vec<&str> = snapshot.lines().collect();
    let mut found_window1_active = false;
    let mut found_window2_active = false;

    for line in &lines[0..5] {
        if line.contains(&format!(" {window1_id} [F]")) {
            found_window1_active = true;
            println!("Found window 1 marked as active tab [F]");
        }
        if line.contains(&format!(" {window2_id} [F]")) {
            found_window2_active = true;
            println!("Found window 2 marked as active tab [F]");
        }
    }

    // Check which window content is actually visible
    let window1_content_visible = snapshot.contains(&format!(" {window1_id} "));
    let window2_content_visible = snapshot.contains(&format!(" {window2_id} "));

    println!("\n=== RESULTS ===");
    println!("Window 1 marked as active tab [F]: {found_window1_active}");
    println!("Window 2 marked as active tab [F]: {found_window2_active}");
    println!("Window 1 content visible: {window1_content_visible}");
    println!("Window 2 content visible: {window2_content_visible}");

    // The expected behavior: window 1 should be both active AND visible
    // The bug: window 1 is marked active but window 2 content is shown
    assert!(
        found_window1_active,
        "Window 1 should be marked as active tab [F]"
    );
    assert!(
        !found_window2_active,
        "Window 2 should NOT be marked as active tab"
    );
    assert!(
        window1_content_visible,
        "Window 1 content should be visible (BUG: window 2 content is shown instead)"
    );
    assert!(
        !window2_content_visible,
        "Window 2 content should NOT be visible"
    );

    // Clean up
    window1.kill()?;
    window2.kill()?;

    Ok(())
}
