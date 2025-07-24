mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_window_close_reflow() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("window-close-reflow");
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
    client.wait_for_window_count(1, "after first window")?;

    let mut window2 = env.start_window("Window2", Some("green"))?;
    client.wait_for_window_count(2, "after second window")?;

    let mut window3 = env.start_window("Window3", Some("blue"))?;
    client.wait_for_window_count(3, "after third window")?;

    // Get initial layout
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 3);

    println!("\n=== Initial layout with 3 windows ===");
    for (i, w) in windows.iter().enumerate() {
        println!(
            "Window {}: pos=({}, {}), size={}x{}",
            i + 1,
            w["x"],
            w["y"],
            w["width"],
            w["height"]
        );
    }

    // Windows should be tiled vertically (3 columns)
    let w1 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(1))
        .unwrap();
    let w2 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(2))
        .unwrap();
    let w3 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(3))
        .unwrap();

    assert_eq!(w1["x"].as_i64().unwrap(), 0);
    assert_eq!(w2["x"].as_i64().unwrap(), 1280);
    assert_eq!(w3["x"].as_i64().unwrap(), 2560);

    // Get ASCII representation
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== ASCII with 3 windows ===");
    println!("{ascii}");

    // Close middle window (window 2)
    println!("\n=== Closing window 2 (middle) ===");
    window2.kill()?;

    // Wait for window to be removed
    client.wait_for_window_count(2, "after closing window 2")?;

    // Get new layout
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 2);

    println!("\n=== Layout after closing window 2 ===");
    for w in windows.iter() {
        println!(
            "Window {}: pos=({}, {}), size={}x{}",
            w["id"], w["x"], w["y"], w["width"], w["height"]
        );
    }

    // Windows should have reflowed to fill the space
    let w1 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(1))
        .unwrap();
    let w3 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(3))
        .unwrap();

    // Window 1 should still be on the left
    assert_eq!(w1["x"].as_i64().unwrap(), 0);
    // Window 3 should have moved left and both should be wider
    assert!(w3["x"].as_i64().unwrap() < 2560);
    assert!(w1["width"].as_i64().unwrap() > 1280);
    assert!(w3["width"].as_i64().unwrap() > 1280);

    // Total width should still be ~3840
    let total_width = w1["width"].as_i64().unwrap() + w3["width"].as_i64().unwrap();
    assert!(total_width >= 3800, "Windows should fill most of the width");

    // Get ASCII after reflow
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== ASCII after closing window 2 ===");
    println!("{ascii}");
    assert!(!ascii.contains("2"), "Window 2 should not exist");
    assert!(ascii.contains("1"), "Window 1 should still exist");
    assert!(ascii.contains("3"), "Window 3 should still exist");

    println!("\n✓ Windows reflowed correctly after close!");

    // Clean up
    window1.kill()?;
    window3.kill()?;

    Ok(())
}

#[test]
fn test_close_last_window_empty_workspace() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("close-last-window");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--ascii-size", "80x24"])?;

    let client = TestClient::new(&env.test_socket);

    // Create a single window
    println!("\n=== Creating single window ===");
    let mut window1 = env.start_window("LonelyWindow", None)?;
    client.wait_for_window_count(1, "after creating window")?;

    // Verify window is focused
    let focused = client.get_focused_window()?;
    assert_eq!(focused, Some(1), "Single window should be focused");

    // Get workspace info before closing
    let response = client.send_command(&serde_json::json!({"type": "GetWorkspaces"}))?;
    let workspaces = response["workspaces"].as_array().unwrap();
    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    assert!(ws1["visible"].as_bool().unwrap());
    assert_eq!(ws1["window_count"].as_u64().unwrap(), 1);

    // Close the only window
    println!("\n=== Closing the only window ===");
    window1.kill()?;

    // Wait for window to be removed
    client.wait_for_window_count(0, "after closing last window")?;

    // Check workspace is still visible but empty
    let response = client.send_command(&serde_json::json!({"type": "GetWorkspaces"}))?;
    let workspaces = response["workspaces"].as_array().unwrap();
    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    assert!(
        ws1["visible"].as_bool().unwrap(),
        "Workspace should still be visible"
    );
    assert_eq!(
        ws1["window_count"].as_u64().unwrap(),
        0,
        "Workspace should be empty"
    );

    // No window should be focused
    let focused = client.get_focused_window()?;
    assert_eq!(focused, None, "No window should be focused");

    // Get ASCII to verify empty workspace
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== ASCII of empty workspace ===");
    println!("{ascii}");

    // Should be mostly empty (just spaces and newlines)
    let non_space_chars: Vec<char> = ascii.chars().filter(|&c| c != ' ' && c != '\n').collect();
    assert!(non_space_chars.is_empty(), "Workspace should be empty");

    println!("\n✓ Empty workspace handled correctly!");

    Ok(())
}
