mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_window_workspace_movement() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("workspace-movement");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--ascii-size", "80x24"])?;

    let client = TestClient::new(&env.test_socket);

    // Create three windows on workspace 1
    println!("\n=== Creating 3 windows on workspace 1 ===");
    let mut window1 = env.start_window("Window1", Some("red"))?;
    client.wait_for_window_count(1, "first")?;

    let mut window2 = env.start_window("Window2", Some("green"))?;
    client.wait_for_window_count(2, "second")?;

    let mut window3 = env.start_window("Window3", Some("blue"))?;
    client.wait_for_window_count(3, "third")?;

    // Verify all windows are on workspace 1
    let windows = client.get_windows()?;
    for w in &windows {
        assert_eq!(
            w["workspace"].as_u64().unwrap(),
            1,
            "Window {} should be on workspace 1",
            w["id"]
        );
    }

    // Get workspace info
    let response = client.send_command(&serde_json::json!({"type": "GetWorkspaces"}))?;
    let workspaces = response["workspaces"].as_array().unwrap();
    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    assert_eq!(ws1["window_count"].as_u64().unwrap(), 3);
    assert!(ws1["visible"].as_bool().unwrap());

    // Move window 2 to workspace 2
    println!("\n=== Moving window 2 to workspace 2 ===");
    let response = client.send_command(&serde_json::json!({
        "type": "MoveWindowToWorkspace",
        "window_id": 2,
        "workspace": 1  // This is 0-based index for workspace 2
    }))?;
    assert_eq!(response["type"].as_str(), Some("Success"));

    // Verify window 2 is now on workspace 2
    let windows = client.get_windows()?;
    let win2 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(2))
        .unwrap();
    assert_eq!(
        win2["workspace"].as_u64().unwrap(),
        2,
        "Window 2 should be on workspace 2"
    );

    // Verify workspace counts
    let response = client.send_command(&serde_json::json!({"type": "GetWorkspaces"}))?;
    let workspaces = response["workspaces"].as_array().unwrap();

    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    assert_eq!(
        ws1["window_count"].as_u64().unwrap(),
        2,
        "Workspace 1 should have 2 windows"
    );

    let ws2 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(2))
        .unwrap();
    assert_eq!(
        ws2["window_count"].as_u64().unwrap(),
        1,
        "Workspace 2 should have 1 window"
    );

    // Switch to workspace 2
    println!("\n=== Switching to workspace 2 ===");
    let response = client.send_command(&serde_json::json!({
        "type": "SwitchWorkspace",
        "index": 1  // Workspace 2 is index 1 (0-based)
    }))?;
    assert_eq!(response["type"].as_str(), Some("Success"));

    // Verify workspace 2 is now visible
    let response = client.send_command(&serde_json::json!({"type": "GetWorkspaces"}))?;
    let workspaces = response["workspaces"].as_array().unwrap();

    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    assert!(
        !ws1["visible"].as_bool().unwrap(),
        "Workspace 1 should not be visible"
    );

    let ws2 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(2))
        .unwrap();
    assert!(
        ws2["visible"].as_bool().unwrap(),
        "Workspace 2 should be visible"
    );

    // Get ASCII to see only window 2
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== ASCII on workspace 2 (only window 2) ===");
    println!("{ascii}");
    assert!(ascii.contains("2"), "Window 2 should be visible");
    assert!(!ascii.contains("1"), "Window 1 should not be visible");
    assert!(!ascii.contains("3"), "Window 3 should not be visible");

    // Move window 1 to workspace 2 as well
    println!("\n=== Moving window 1 to workspace 2 ===");
    let response = client.send_command(&serde_json::json!({
        "type": "MoveWindowToWorkspace",
        "window_id": 1,
        "workspace": 1  // This is 0-based index for workspace 2
    }))?;
    assert_eq!(response["type"].as_str(), Some("Success"));

    // Verify both windows are now on workspace 2
    let windows = client.get_windows()?;
    let ws2_windows: Vec<_> = windows
        .iter()
        .filter(|w| w["workspace"].as_u64() == Some(2))
        .collect();
    assert_eq!(ws2_windows.len(), 2, "Workspace 2 should have 2 windows");

    // Get ASCII to see both windows
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== ASCII on workspace 2 (windows 1 and 2) ===");
    println!("{ascii}");
    assert!(ascii.contains("1"), "Window 1 should be visible");
    assert!(ascii.contains("2"), "Window 2 should be visible");
    assert!(!ascii.contains("3"), "Window 3 should not be visible");

    // Switch back to workspace 1
    println!("\n=== Switching back to workspace 1 ===");
    let response = client.send_command(&serde_json::json!({
        "type": "SwitchWorkspace",
        "index": 0  // Workspace 1 is index 0
    }))?;
    assert_eq!(response["type"].as_str(), Some("Success"));

    // Should only see window 3
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== ASCII on workspace 1 (only window 3) ===");
    println!("{ascii}");
    assert!(!ascii.contains("1"), "Window 1 should not be visible");
    assert!(!ascii.contains("2"), "Window 2 should not be visible");
    assert!(ascii.contains("3"), "Window 3 should be visible");

    println!("\n✓ All workspace movement tests passed!");

    // Clean up
    window1.kill()?;
    window2.kill()?;
    window3.kill()?;

    Ok(())
}
