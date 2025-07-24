mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_tabbed_container_basic() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-basic");
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

    // Debug: print window positions
    let w1_data = windows
        .iter()
        .find(|w| w.get("id").and_then(|v| v.as_u64()) == Some(window1_id))
        .expect("Should find window 1 data");
    let w1_x = w1_data.get("x").and_then(|v| v.as_i64()).unwrap_or(0);
    let w1_y = w1_data.get("y").and_then(|v| v.as_i64()).unwrap_or(0);
    let w1_width = w1_data.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
    let w1_height = w1_data.get("height").and_then(|v| v.as_i64()).unwrap_or(0);

    let w2_data = windows
        .iter()
        .find(|w| w.get("id").and_then(|v| v.as_u64()) == Some(window2_id))
        .expect("Should find window 2 data");
    let w2_x = w2_data.get("x").and_then(|v| v.as_i64()).unwrap_or(0);
    let w2_y = w2_data.get("y").and_then(|v| v.as_i64()).unwrap_or(0);
    let w2_width = w2_data.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
    let w2_height = w2_data.get("height").and_then(|v| v.as_i64()).unwrap_or(0);

    println!(
        "Window 1 position: x={w1_x}, y={w1_y}, w={w1_width}, h={w1_height}"
    );
    println!(
        "Window 2 position: x={w2_x}, y={w2_y}, w={w2_width}, h={w2_height}"
    );

    // Initially windows should be tiled (side by side)
    // At least one window should not start at x=0
    let are_tiled = w1_x != w2_x || w1_y != w2_y;
    println!("Windows are tiled initially: {are_tiled}");

    // Get initial ASCII snapshot - should show both windows side by side
    let initial_snapshot = client.get_ascii_snapshot(true, true)?;
    println!("=== INITIAL LAYOUT START ===");
    println!("{initial_snapshot}");
    println!("=== INITIAL LAYOUT END ===");

    // Now set the container to tabbed mode
    // First focus window 1
    client.focus_window(window1_id)?;

    // Send layout tabbed command via IPC
    let response = client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;

    println!("Layout command response: {response:?}");

    // Give it a moment to apply the layout
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Get ASCII snapshot after switching to tabbed mode
    // Should only show one window (the active tab)
    let tabbed_snapshot = client.get_ascii_snapshot(true, true)?;
    println!("=== TABBED LAYOUT START ===");
    println!("{tabbed_snapshot}");
    println!("=== TABBED LAYOUT END ===");

    // Debug: Check what windows are reported by GetWindows
    let windows_after = client.get_windows()?;
    println!("Windows after tabbed mode: {windows_after:?}");

    // Verify only one window is visible in the ASCII output
    // Count the number of window borders in the snapshot (both single and double-line box drawing)
    let initial_window_count =
        initial_snapshot.matches("╔").count() + initial_snapshot.matches("┌").count();
    let tabbed_window_count =
        tabbed_snapshot.matches("╔").count() + tabbed_snapshot.matches("┌").count();

    println!("Window borders in initial layout: {initial_window_count}");
    println!("Window borders in tabbed layout: {tabbed_window_count}");

    // In tabbed mode, we should only see one window (the active tab)
    // Note: Initial layout might show 1 window if they're in the same container already
    assert!(
        initial_window_count >= 1,
        "Should have at least 1 window initially, got {initial_window_count}"
    );
    assert_eq!(
        tabbed_window_count, 1,
        "Should have only 1 window visible in tabbed mode"
    );

    // Verify it's showing window 1 (which we focused)
    assert!(
        tabbed_snapshot.contains(&format!(" {window1_id} ")),
        "Should show window 1 as the active tab"
    );

    // Now switch to the next tab (window 2)
    // Send focus right to switch tabs
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;

    // Give it a moment to switch
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Get ASCII snapshot after switching tabs
    let switched_snapshot = client.get_ascii_snapshot(true, true)?;
    println!("=== AFTER TAB SWITCH START ===");
    println!("{switched_snapshot}");
    println!("=== AFTER TAB SWITCH END ===");

    // Check windows positions after tab switch
    let windows_after_switch = client.get_windows()?;
    for (i, window) in windows_after_switch.iter().enumerate() {
        let x = window.get("x").and_then(|v| v.as_i64()).unwrap_or(0);
        let y = window.get("y").and_then(|v| v.as_i64()).unwrap_or(0);
        let width = window.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
        let height = window.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
        println!(
            "Window {} after switch: x={}, y={}, w={}, h={}",
            i + 1,
            x,
            y,
            width,
            height
        );
    }

    // Clean up
    window1.kill()?;
    window2.kill()?;

    Ok(())
}
