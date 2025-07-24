mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_focus_window_by_id() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("focus-by-id");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&[
        "--test",
        "--ascii-size",
        "80x24",
        "--config",
        "tests/test_configs/no_gaps.conf",
    ])?;

    let client = TestClient::new(&env.test_socket);

    // Create three windows
    println!("\n=== Creating 3 windows ===");
    let mut window1 = env.start_window("Window1", Some("red"))?;
    client.wait_for_window_count(1, "first")?;

    let mut window2 = env.start_window("Window2", Some("green"))?;
    client.wait_for_window_count(2, "second")?;

    let mut window3 = env.start_window("Window3", Some("blue"))?;
    client.wait_for_window_count(3, "third")?;

    // Get initial focus (should be window 3, the most recent)
    let initial_focus = client.get_focused_window()?;
    println!("\nInitial focused window: {initial_focus:?}");
    assert_eq!(
        initial_focus,
        Some(3),
        "Window 3 should be focused initially"
    );

    // Get ASCII to visualize initial state
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== Initial state (Window 3 focused) ===");
    println!("{ascii}");
    assert!(ascii.contains("3 [F]"), "Window 3 should show as focused");

    // Focus window 1
    println!("\n=== Focusing window 1 ===");
    client.focus_window(1)?;

    // Wait for focus change
    client.wait_for_focus(1, "after focusing window 1")?;

    // Verify window 1 is now focused
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== After focusing window 1 ===");
    println!("{ascii}");
    assert!(ascii.contains("1 [F]"), "Window 1 should show as focused");
    assert!(!ascii.contains("3 [F]"), "Window 3 should not be focused");

    // Focus window 2
    println!("\n=== Focusing window 2 ===");
    client.focus_window(2)?;

    // Wait for focus change
    client.wait_for_focus(2, "after focusing window 2")?;

    // Verify window 2 is now focused
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== After focusing window 2 ===");
    println!("{ascii}");
    assert!(ascii.contains("2 [F]"), "Window 2 should show as focused");
    assert!(!ascii.contains("1 [F]"), "Window 1 should not be focused");

    // Try to focus non-existent window
    println!("\n=== Testing focus on non-existent window ===");
    let result = client.focus_window(999);
    assert!(result.is_err(), "Focusing non-existent window should fail");
    println!("✓ Correctly failed to focus non-existent window");

    // Verify focus didn't change
    let current_focus = client.get_focused_window()?;
    assert_eq!(current_focus, Some(2), "Focus should still be on window 2");

    println!("\n✓ All focus tests passed!");

    // Clean up
    window1.kill()?;
    window2.kill()?;
    window3.kill()?;

    Ok(())
}

#[test]
fn test_focus_after_window_close() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("focus-after-close");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&[
        "--test",
        "--ascii-size",
        "160x45",
        "--config",
        "tests/test_configs/no_gaps.conf",
    ])?;

    let client = TestClient::new(&env.test_socket);

    // Create three windows
    println!("\n=== Creating 3 windows ===");
    let mut window1 = env.start_window("Window1", Some("red"))?;
    client.wait_for_window_count(1, "first")?;

    let mut window2 = env.start_window("Window2", Some("green"))?;
    client.wait_for_window_count(2, "second")?;

    let mut window3 = env.start_window("Window3", Some("blue"))?;
    client.wait_for_window_count(3, "third")?;

    // Focus window 2 (middle window)
    println!("\n=== Focusing window 2 ===");
    client.focus_window(2)?;
    client.wait_for_focus(2, "after focusing window 2")?;

    // Get ASCII to visualize state before close
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== Before closing window 2 (focused) ===");
    println!("{ascii}");
    assert!(ascii.contains("2 [F]"), "Window 2 should be focused");

    // Close the focused window (window 2)
    println!("\n=== Closing focused window (2) ===");
    window2.kill()?;

    // Wait for window to disappear
    client.wait_for_window_count(2, "after closing window 2")?;

    // Check which window got focus
    let new_focus = client.get_focused_window()?;
    println!("New focused window: {new_focus:?}");

    // Should have transferred focus to another window
    assert!(
        new_focus.is_some(),
        "Should have a focused window after close"
    );
    assert!(
        new_focus == Some(1) || new_focus == Some(3),
        "Focus should transfer to window 1 or 3, got {new_focus:?}"
    );

    // Get ASCII to visualize final state
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== After closing window 2 ===");
    println!("{ascii}");
    assert!(!ascii.contains("2"), "Window 2 should not exist");
    assert!(ascii.contains("[F]"), "Some window should be focused");

    // Get window list to verify
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 2, "Should have 2 windows remaining");

    // Verify remaining windows are positioned correctly
    let win1 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(1))
        .unwrap();
    let win3 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(3))
        .unwrap();

    println!("\nRemaining windows:");
    println!(
        "Window 1: pos=({}, {}), size={}x{}",
        win1["x"], win1["y"], win1["width"], win1["height"]
    );
    println!(
        "Window 3: pos=({}, {}), size={}x{}",
        win3["x"], win3["y"], win3["width"], win3["height"]
    );

    // Windows should have reflowed to fill the space
    let total_width = win1["width"].as_i64().unwrap() + win3["width"].as_i64().unwrap();
    assert!(total_width >= 3800, "Windows should fill most of the width");

    println!("\n✓ Focus correctly transferred after window close!");

    // Clean up
    window1.kill()?;
    window3.kill()?;

    Ok(())
}
