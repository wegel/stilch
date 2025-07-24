mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_new_window_in_tabbed_container() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-new-window");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--config", "tests/test_configs/no_gaps.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // Start first window and get its ID
    let mut window1 = env.start_window("TestWindow1", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window 1")?;
    let windows = client.get_windows()?;
    let window1_id = windows[0].get("id").and_then(|v| v.as_u64()).unwrap();
    println!("Window 1 ID: {window1_id}");

    // Start second window and get its ID
    let mut window2 = env.start_window("TestWindow2", Some("red"))?;
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
    let mut window3 = env.start_window("TestWindow3", Some("green"))?;
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

    // Focus window 2 (the middle one we created)
    client.focus_window(window2_id)?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Switch to tabbed mode
    client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Verify window 2 is visible after switching to tabbed
    let snapshot = client.get_ascii_snapshot(true, true)?;
    println!("=== After switching to tabbed mode ===");
    println!("{snapshot}");
    assert!(
        snapshot.contains(&format!(" {window2_id} ")),
        "Window 2 should be visible after switching to tabbed"
    );

    // Now create a fourth window (should be added to the tabbed container)
    let mut window4 = env.start_window("TestWindow4", Some("yellow"))?;
    client.wait_for_window_count(4, "after starting window 4")?;

    // Get the new window's ID
    let windows_after = client.get_windows()?;
    let window4_id = windows_after
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

    // Check what's visible now - the new window should be visible and focused
    let snapshot_after_new = client.get_ascii_snapshot(true, true)?;
    println!("=== After creating new window ===");
    println!("{snapshot_after_new}");

    // The new window should be visible (it should become the active tab)
    assert!(
        snapshot_after_new.contains(&format!(" {window4_id} ")),
        "New window (window 4) should be visible after creation"
    );

    // Now test navigation: Super+Left should go to window 3
    println!("\n=== TEST: Super+Left from new window ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "left"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_left = client.get_ascii_snapshot(true, true)?;
    println!("{after_left}");
    assert!(
        after_left.contains(&format!(" {window3_id} ")),
        "Window 3 should be visible after Super+Left from window 4"
    );

    // Super+Right should go back to window 4
    println!("\n=== TEST: Super+Right back to new window ===");
    client.send_command(&serde_json::json!({
        "type": "MoveFocus",
        "direction": "right"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_right = client.get_ascii_snapshot(true, true)?;
    println!("{after_right}");
    assert!(
        after_right.contains(&format!(" {window4_id} ")),
        "Should return to window 4 after Super+Right"
    );

    // Clean up
    window1.kill()?;
    window2.kill()?;
    window3.kill()?;
    window4.kill()?;

    Ok(())
}
