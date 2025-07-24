mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_close_middle_tab_visibility() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-close-middle");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--config", "tests/test_configs/no_gaps.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // Start three windows and track their IDs
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
    println!("Window 3 ID: {window3_id}");

    // Focus window 2 (middle)
    client.focus_window(window2_id)?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Switch to tabbed mode
    client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Verify window 2 is visible
    let snapshot = client.get_ascii_snapshot(true, true)?;
    println!("=== After switching to tabbed (window 2 should be visible) ===");
    println!("{snapshot}");
    assert!(
        snapshot.contains(&format!(" {window2_id} ")),
        "Window 2 should be visible after switching to tabbed"
    );

    // Close window 2 (the middle tab)
    println!("\n=== Closing window 2 (middle tab) ===");
    window2.kill()?;
    client.wait_for_window_count(2, "after closing window 2")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check what's visible after closing
    let snapshot_after_close = client.get_ascii_snapshot(true, true)?;
    println!("=== After closing window 2 ===");
    println!("{snapshot_after_close}");

    // Check which window is actually visible
    let w1_visible = snapshot_after_close.contains(&format!(" {window1_id} "));
    let w3_visible = snapshot_after_close.contains(&format!(" {window3_id} "));

    // Get the focused window
    let windows_after = client.get_windows()?;
    let focused_window = windows_after
        .iter()
        .find(|w| w.get("focused").and_then(|v| v.as_bool()).unwrap_or(false))
        .and_then(|w| w.get("id").and_then(|v| v.as_u64()));

    println!(
        "After close: window1 visible={w1_visible}, window3 visible={w3_visible}, focused={focused_window:?}"
    );

    // THE BUG: After closing the middle tab:
    // - The "active" tab should go to the right one (window 3)
    // - But it shows the content of the first one (window 1)

    // Additional check: what does the focused window say?
    if let Some(focused_id) = focused_window {
        println!("Focused window ID: {focused_id}");
        println!(
            "Expected: window 3 ({}), but showing: {}",
            window3_id,
            if w1_visible {
                format!("window 1 ({window1_id})")
            } else if w3_visible {
                format!("window 3 ({window3_id})")
            } else {
                "nothing".to_string()
            }
        );

        // The bug might be that focus is set to window1 but active_child points to window3
        if focused_id == window1_id && !w1_visible {
            panic!("BUG: Focused window is {} but window {} is visible - focus and visibility mismatch!", 
                window1_id, if w3_visible { window3_id } else { 0 });
        }
    }

    // This test should FAIL to demonstrate the bug
    assert!(
        w3_visible,
        "Window 3 should be visible after closing middle tab"
    );
    assert!(
        !w1_visible,
        "BUG: Window 1 should NOT be shown after closing middle tab"
    );

    // Now test navigation - according to the bug:
    // Super+Left: the active tab changes to the left one, still showing window 1 content
    println!("\n=== TEST: Super+Left after closing middle ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "left"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_left = client.get_ascii_snapshot(true, true)?;
    println!("{after_left}");

    // After Super+Left, we should be on the first tab showing window 1 (correct)
    assert!(
        after_left.contains(&format!(" {window1_id} ")),
        "Window 1 should be visible after Super+Left"
    );

    // Super+Right should go to window 3 and NOW show its content correctly
    println!("\n=== TEST: Super+Right should now show window 3 correctly ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_right = client.get_ascii_snapshot(true, true)?;
    println!("{after_right}");

    assert!(
        after_right.contains(&format!(" {window3_id} ")),
        "Window 3 should NOW be visible after Super+Right"
    );

    // Clean up
    window1.kill()?;
    window3.kill()?;

    Ok(())
}
