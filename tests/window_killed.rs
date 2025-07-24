mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_window_killed_externally() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("window-killed");
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

    // Get initial state
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 3);

    // Verify initial layout
    println!("\n=== Initial layout ===");
    for w in &windows {
        println!(
            "Window {}: pos=({}, {}), size={}x{}",
            w["id"], w["x"], w["y"], w["width"], w["height"]
        );
    }

    // Get focused window
    let focused = client.get_focused_window()?;
    println!("Initially focused: Window {focused:?}");

    // Kill window 2 externally (not through compositor)
    println!("\n=== Killing window 2 externally ===");
    let window2_pid = window2.id();
    window2.kill()?;
    window2.wait()?;
    println!("Window 2 (PID {window2_pid}) killed");

    // Give compositor time to notice the window is gone
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Verify window was removed
    client.wait_for_window_count(2, "after killing window 2")?;

    // Get new state
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 2);

    // Verify remaining windows
    println!("\n=== After killing window 2 ===");
    for w in &windows {
        println!(
            "Window {}: pos=({}, {}), size={}x{}",
            w["id"], w["x"], w["y"], w["width"], w["height"]
        );
    }

    // Verify windows reflowed
    let w1 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(1))
        .unwrap();
    let w3 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(3))
        .unwrap();

    assert_eq!(w1["x"].as_i64().unwrap(), 0);
    assert!(
        w3["x"].as_i64().unwrap() < 2560,
        "Window 3 should have moved left"
    );

    // Check focus transferred
    let new_focus = client.get_focused_window()?;
    println!("Focus after kill: Window {new_focus:?}");
    assert!(new_focus.is_some(), "Should have a focused window");
    assert!(
        new_focus == Some(1) || new_focus == Some(3),
        "Focus should transfer to remaining window"
    );

    // Get ASCII to verify layout
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== ASCII after window 2 killed ===");
    println!("{ascii}");

    assert!(!ascii.contains("2"), "Window 2 should not be visible");
    assert!(ascii.contains("1"), "Window 1 should be visible");
    assert!(ascii.contains("3"), "Window 3 should be visible");
    assert!(ascii.contains("[F]"), "One window should be focused");

    println!("\n✓ Compositor correctly handled externally killed window!");

    // Clean up
    window1.kill()?;
    window3.kill()?;

    Ok(())
}

#[test]
fn test_all_windows_killed_empty_workspace() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("all-windows-killed");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--ascii-size", "80x24"])?;

    let client = TestClient::new(&env.test_socket);

    // Create two windows
    println!("\n=== Creating 2 windows ===");
    let mut window1 = env.start_window("Window1", None)?;
    client.wait_for_window_count(1, "after first window")?;

    let mut window2 = env.start_window("Window2", None)?;
    client.wait_for_window_count(2, "after second window")?;

    // Verify both windows exist
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 2);

    // Kill both windows
    println!("\n=== Killing all windows ===");
    window1.kill()?;
    window2.kill()?;

    // Wait for both to be removed
    client.wait_for_window_count(0, "after killing all windows")?;

    // Verify workspace is empty but still active
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

    // Should be empty (just spaces and newlines)
    let non_space_chars: Vec<char> = ascii.chars().filter(|&c| c != ' ' && c != '\n').collect();
    assert!(non_space_chars.is_empty(), "Workspace should be empty");

    println!("\n✓ Empty workspace handled correctly after all windows killed!");

    Ok(())
}
