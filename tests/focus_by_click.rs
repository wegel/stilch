//! Test focus changes when clicking on windows

mod common;
use common::{TestClient, TestEnv};

#[test]
fn test_focus_window_by_clicking() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("focus-by-click");
    env.cleanup()?;

    // Start compositor with known dimensions and gaps config
    env.start_compositor(&[
        "--test",
        "--ascii-size",
        "80x24",
        "--config",
        "tests/test_configs/with_gaps.conf",
    ])?;

    let client = TestClient::new(&env.test_socket);

    // Create 3 windows and capture their IDs
    let mut _window1 = env.start_window("Window1", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window 1")?;
    let windows = client.get_windows()?;
    let window1_id = windows[0]["id"].as_u64().ok_or("Window 1 has no id")?;

    let mut _window2 = env.start_window("Window2", Some("green"))?;
    client.wait_for_window_count(2, "after starting window 2")?;
    let windows = client.get_windows()?;
    let window2_id = windows
        .iter()
        .find_map(|w| {
            let id = w["id"].as_u64()?;
            if id != window1_id {
                Some(id)
            } else {
                None
            }
        })
        .ok_or("Window 2 has no id")?;

    let mut _window3 = env.start_window("Window3", Some("red"))?;
    client.wait_for_window_count(3, "after starting window 3")?;
    let windows = client.get_windows()?;
    let window3_id = windows
        .iter()
        .find_map(|w| {
            let id = w["id"].as_u64()?;
            if id != window1_id && id != window2_id {
                Some(id)
            } else {
                None
            }
        })
        .ok_or("Window 3 has no id")?;

    // Initial state - window3 should be focused (last created)
    let initial = client.get_ascii_snapshot(true, true)?;
    println!("Initial state (Window 3 should be focused):\n{}", initial);

    // Get window positions by ID
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 3, "Should have 3 windows");

    // Find window 1 by ID
    let window1 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(window1_id))
        .ok_or("Window 1 not found")?;
    let window1_x = window1["x"].as_i64().ok_or("Window 1 has no x")?;
    let window1_y = window1["y"].as_i64().ok_or("Window 1 has no y")?;
    let window1_width = window1["width"].as_i64().ok_or("Window 1 has no width")?;
    let window1_height = window1["height"].as_i64().ok_or("Window 1 has no height")?;

    // Find window 2 by ID
    let window2 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(window2_id))
        .ok_or("Window 2 not found")?;
    let window2_x = window2["x"].as_i64().ok_or("Window 2 has no x")?;
    let window2_y = window2["y"].as_i64().ok_or("Window 2 has no y")?;
    let window2_width = window2["width"].as_i64().ok_or("Window 2 has no width")?;
    let window2_height = window2["height"].as_i64().ok_or("Window 2 has no height")?;

    // Click on window 1 to focus it (click in the center)
    let click_x = (window1_x + window1_width / 2) as i32;
    let click_y = (window1_y + window1_height / 2) as i32;
    println!("Clicking on window 1 at ({}, {})", click_x, click_y);
    println!(
        "Window 1 bounds: x={}, y={}, w={}, h={}",
        window1_x, window1_y, window1_width, window1_height
    );
    client.click_at(click_x, click_y)?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_click1 = client.get_ascii_snapshot(true, true)?;
    println!("\nAfter clicking on Window 1:\n{}", after_click1);

    // Verify window 1 is focused
    let windows = client.get_windows()?;
    let window1_focused = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(window1_id))
        .and_then(|w| w["focused"].as_bool())
        .ok_or("Window 1 focused state not found")?;
    assert!(
        window1_focused,
        "Window 1 should be focused after clicking on it"
    );

    // Click on window 2 to focus it
    client.click_at(
        (window2_x + window2_width / 2) as i32,
        (window2_y + window2_height / 2) as i32,
    )?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_click2 = client.get_ascii_snapshot(true, true)?;
    println!("\nAfter clicking on Window 2:\n{}", after_click2);

    // Verify window 2 is focused
    let windows = client.get_windows()?;
    let window2_focused = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(window2_id))
        .and_then(|w| w["focused"].as_bool())
        .ok_or("Window 2 focused state not found")?;
    assert!(
        window2_focused,
        "Window 2 should be focused after clicking on it"
    );

    // Verify only one window is focused
    let focused_count = windows
        .iter()
        .filter(|w| w["focused"].as_bool() == Some(true))
        .count();
    assert_eq!(
        focused_count, 1,
        "Exactly one window should be focused, but {} are",
        focused_count
    );

    // Click on empty space to clear focus
    // With gaps enabled, there should be empty space between windows
    // Let's check window bounds to find a gap
    println!("\nWindow positions with gaps:");
    println!(
        "Window 1: x={}, y={}, w={}, h={}",
        window1_x, window1_y, window1_width, window1_height
    );
    println!(
        "Window 2: x={}, y={}, w={}, h={}",
        window2_x, window2_y, window2_width, window2_height
    );

    // Try clicking in a gap between windows - use a coordinate that should be in the gap
    let gap_x = (window1_x + window1_width + 5) as i32; // 5 pixels to the right of window 1
    let gap_y = 100; // Near the top
    println!(
        "\nClicking on empty space at ({}, {}) to clear focus",
        gap_x, gap_y
    );
    client.click_at(gap_x, gap_y)?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let after_empty_click = client.get_ascii_snapshot(true, true)?;
    println!("\nAfter clicking on empty space:\n{}", after_empty_click);

    // Verify no window is focused
    let windows = client.get_windows()?;
    let focused_count = windows
        .iter()
        .filter(|w| w["focused"].as_bool() == Some(true))
        .count();
    assert_eq!(
        focused_count, 0,
        "No window should be focused after clicking empty space"
    );

    Ok(())
}
