mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_close_window_in_tabbed_container() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-close-window");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--config", "tests/test_configs/no_gaps.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // Start first window and get its ID
    let mut window1 = env.start_window("Window1", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window 1")?;
    let windows = client.get_windows()?;
    let window1_id = windows[0].get("id").and_then(|v| v.as_u64()).unwrap();
    println!("Window 1 ID: {window1_id}");

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
    println!("Window 2 ID: {window2_id}");

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
    println!("Window 3 ID: {window3_id}");

    // Start fourth window and get its ID
    let mut window4 = env.start_window("Window4", Some("yellow"))?;
    client.wait_for_window_count(4, "after starting window 4")?;
    let windows = client.get_windows()?;
    let window4_id = windows
        .iter()
        .find_map(|w| {
            let id = w.get("id").and_then(|v| v.as_u64())?;
            if id != window1_id && id != window2_id && id != window3_id {
                Some(id)
            } else {
                None
            }
        })
        .expect("Should find window 4");
    println!("Window 4 ID: {window4_id}");

    // Focus window 2 (second tab)
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
    println!("=== After switching to tabbed (window 2 focused) ===");
    println!("{snapshot}");
    assert!(
        snapshot.contains(&format!(" {window2_id} ")),
        "Window 2 should be visible"
    );

    // Navigate to window 3 (Super+Right)
    println!("\n=== Navigate to window 3 ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let snapshot_w3 = client.get_ascii_snapshot(true, true)?;
    println!("{snapshot_w3}");
    assert!(
        snapshot_w3.contains(&format!(" {window3_id} ")),
        "Window 3 should be visible"
    );

    // Now close window 3 (Super+Q would close the focused window)
    println!("\n=== Close window 3 ===");
    window3.kill()?;
    client.wait_for_window_count(3, "after closing window 3")?;

    // Check what's visible after closing - should show the next tab (window 4) or previous (window 2)
    let snapshot_after_close = client.get_ascii_snapshot(true, true)?;
    println!("=== After closing window 3 ===");
    println!("{snapshot_after_close}");

    // Typically, after closing a tab, the next tab becomes active (window 4)
    // Or if it was the last tab, the previous one (window 2)
    let w4_visible = snapshot_after_close.contains(&format!(" {window4_id} "));
    let w2_visible = snapshot_after_close.contains(&format!(" {window2_id} "));
    println!("After close: w2_visible={w2_visible}, w4_visible={w4_visible}");

    // Let's say window 4 is now visible
    let visible_after_close = if w4_visible { window4_id } else { window2_id };
    println!("Visible window after close: {visible_after_close}");

    // Now test the navigation issue you mentioned
    // Super+Left should go to window 2
    println!("\n=== TEST: Super+Left after close ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "left"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_left = client.get_ascii_snapshot(true, true)?;
    println!("{after_left}");

    // Check which window is visible
    let w1_visible_left = after_left.contains(&format!(" {window1_id} "));
    let w2_visible_left = after_left.contains(&format!(" {window2_id} "));
    let w4_visible_left = after_left.contains(&format!(" {window4_id} "));
    println!("After left: w1={w1_visible_left}, w2={w2_visible_left}, w4={w4_visible_left}");

    // Super+Right should go back to where we were (the tab after the closed one)
    println!("\n=== TEST: Super+Right to return ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_right = client.get_ascii_snapshot(true, true)?;
    println!("{after_right}");

    // Check if we returned to the correct window
    let w1_visible_right = after_right.contains(&format!(" {window1_id} "));
    let w2_visible_right = after_right.contains(&format!(" {window2_id} "));
    let w4_visible_right = after_right.contains(&format!(" {window4_id} "));
    println!("After right: w1={w1_visible_right}, w2={w2_visible_right}, w4={w4_visible_right}");

    // We should return to the window that was visible after the close
    if visible_after_close == window4_id {
        assert!(w4_visible_right, "Should return to window 4");
    } else {
        assert!(w2_visible_right, "Should return to window 2");
    }

    // Clean up
    window1.kill()?;
    window2.kill()?;
    window4.kill()?;

    Ok(())
}
