mod common;

use common::{verify_window_geometry, TestClient, TestEnv};

#[test]
fn test_single_window_geometry() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("single-window");
    env.cleanup()?;

    // Start compositor with known dimensions and no-gaps config
    env.start_compositor(&[
        "--test",
        "--ascii-size",
        "80x24",
        "--config",
        "tests/test_configs/no_gaps.conf",
    ])?;

    let client = TestClient::new(&env.test_socket);

    // Verify no windows initially
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 0, "Expected no windows initially");

    // Start a simple window
    let mut simple_window = env.start_window("TestWindow", Some("blue"))?;

    // Wait for window to appear
    client.wait_for_window_count(1, "after starting window")?;

    // Get window info
    let windows = client.get_windows()?;
    let window = &windows[0];

    // Verify window geometry
    println!("\n=== WINDOW GEOMETRY ===");
    let id = window["id"].as_u64().ok_or("Window has no id")?;
    let x = window["x"].as_i64().ok_or("Window has no x")?;
    let y = window["y"].as_i64().ok_or("Window has no y")?;
    let width = window["width"].as_i64().ok_or("Window has no width")?;
    let height = window["height"].as_i64().ok_or("Window has no height")?;
    let workspace = window["workspace"]
        .as_u64()
        .ok_or("Window has no workspace")?;
    let focused = window["focused"]
        .as_bool()
        .ok_or("Window has no focused state")?;

    println!("Window ID: {id}");
    println!("Position: ({x}, {y})");
    println!("Size: {width}x{height}");
    println!("Workspace: {workspace}");
    println!("Focused: {focused}");

    // Verify expectations for a single window
    verify_window_geometry(window, 0, 0, 3840, 2160)?;
    assert_eq!(workspace, 1, "Window should be on workspace 1"); // User-facing workspaces are 1-indexed
    assert!(focused, "Single window should be focused");

    // Get ASCII snapshot to visualize
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== ASCII VISUALIZATION ===");
    println!("{ascii}");

    // Verify the window fills the entire ASCII area
    assert!(ascii.contains("╔"), "Should have focused window border");
    assert!(ascii.contains("1 [F]"), "Should show window 1 as focused");

    // Count the border characters to verify it spans the full width
    let lines: Vec<&str> = ascii.lines().collect();
    assert_eq!(lines.len(), 24, "Should have 24 lines");

    // First line should be all border
    let first_line = lines[0];
    assert!(
        first_line.starts_with("╔"),
        "First line should start with top-left corner"
    );
    assert!(
        first_line.ends_with("╗"),
        "First line should end with top-right corner"
    );
    assert_eq!(
        first_line.chars().count(),
        80,
        "First line should be 80 characters wide"
    );

    // Last line should be all border
    let last_line = lines[23];
    assert!(
        last_line.starts_with("╚"),
        "Last line should start with bottom-left corner"
    );
    assert!(
        last_line.ends_with("╝"),
        "Last line should end with bottom-right corner"
    );
    assert_eq!(
        last_line.chars().count(),
        80,
        "Last line should be 80 characters wide"
    );

    println!("\n✓ All geometry assertions passed!");

    // Clean up window
    simple_window.kill()?;

    Ok(())
}

#[test]
fn test_multiple_windows_non_overlapping() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("multi-window-overlap");
    env.cleanup()?;

    // Start compositor with gaps config
    env.start_compositor(&[
        "--test",
        "--ascii-size",
        "80x24",
        "--config",
        "tests/test_configs/with_gaps.conf",
    ])?;

    let client = TestClient::new(&env.test_socket);

    // Create first window
    println!("Creating first window (red)...");
    let mut window1 = env.start_window("Window1", Some("red"))?;
    client.wait_for_window_count(1, "after first window")?;

    // Create second window
    println!("Creating second window (blue)...");
    let mut window2 = env.start_window("Window2", Some("blue"))?;
    client.wait_for_window_count(2, "after second window")?;

    // Get windows data
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 2, "Should have exactly 2 windows");

    // Verify windows don't overlap
    println!("\n=== VERIFYING NON-OVERLAPPING LAYOUT ===");

    let w1 = &windows[0];
    let w2 = &windows[1];

    let w1_x = w1["x"].as_i64().unwrap();
    let w1_y = w1["y"].as_i64().unwrap();
    let w1_width = w1["width"].as_i64().unwrap();
    let w1_height = w1["height"].as_i64().unwrap();

    let w2_x = w2["x"].as_i64().unwrap();
    let w2_y = w2["y"].as_i64().unwrap();
    let w2_width = w2["width"].as_i64().unwrap();
    let w2_height = w2["height"].as_i64().unwrap();

    println!("Window 1: pos=({w1_x}, {w1_y}), size={w1_width}x{w1_height}");
    println!("Window 2: pos=({w2_x}, {w2_y}), size={w2_width}x{w2_height}");

    // Check for non-overlapping - windows should be side by side or top/bottom
    let w1_right = w1_x + w1_width;
    let w1_bottom = w1_y + w1_height;
    let w2_right = w2_x + w2_width;
    let w2_bottom = w2_y + w2_height;

    let overlaps =
        !(w1_right <= w2_x || w2_right <= w1_x || w1_bottom <= w2_y || w2_bottom <= w1_y);

    assert!(!overlaps, "Windows should not overlap!");

    // Verify they share the space (either horizontally or vertically split)
    // Allow for small gaps between windows (common in tiling WMs)
    const GAP_TOLERANCE: i64 = 20;

    if w1_y == w2_y && w1_height == w2_height {
        // Horizontal split
        println!("✓ Windows are horizontally split");
        let total_width = w1_width + w2_width;
        let gap = 3840 - total_width;
        println!("  Total width: {total_width}, Gap: {gap} pixels");
        assert!(
            (0..=GAP_TOLERANCE).contains(&gap),
            "Windows should fill width with max {GAP_TOLERANCE} pixel gap, got {gap}"
        );
    } else if w1_x == w2_x && w1_width == w2_width {
        // Vertical split
        println!("✓ Windows are vertically split");
        let total_height = w1_height + w2_height;
        let gap = 2160 - total_height;
        println!("  Total height: {total_height}, Gap: {gap} pixels");
        assert!(
            (0..=GAP_TOLERANCE).contains(&gap),
            "Windows should fill height with max {GAP_TOLERANCE} pixel gap, got {gap}"
        );
    } else {
        panic!("Windows are not properly tiled!");
    }

    // Get ASCII snapshot
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== ASCII VISUALIZATION ===");
    println!("{ascii}");

    // Both windows should be visible
    assert!(ascii.contains("1"), "Window 1 should be visible");
    assert!(ascii.contains("2"), "Window 2 should be visible");
    assert!(ascii.contains("[F]"), "One window should be focused");

    println!("\n✓ All non-overlapping assertions passed!");

    // Clean up
    window1.kill()?;
    window2.kill()?;

    Ok(())
}
