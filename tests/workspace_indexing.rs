mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_default_workspace_should_be_one_not_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("default-workspace-index");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--ascii-size", "80x24"])?;

    let client = TestClient::new(&env.test_socket);

    // Get initial workspace state before any windows
    let response = client.send_command(&serde_json::json!({"type": "GetWorkspaces"}))?;
    let workspaces = response["workspaces"].as_array().unwrap();

    println!("\n=== Initial Workspace State ===");
    for ws in workspaces {
        println!(
            "Workspace {}: visible={}, focused={}",
            ws["id"], ws["visible"], ws["focused"]
        );
    }

    // Workspace 1 should be the default active workspace
    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    assert!(
        ws1["visible"].as_bool().unwrap(),
        "Workspace 1 should be visible by default"
    );
    assert!(
        ws1["focused"].as_bool().unwrap(),
        "Workspace 1 should be focused by default"
    );

    // Create a window
    println!("\n=== Creating first window ===");
    let mut window1 = env.start_window("TestWindow", None)?;
    client.wait_for_window_count(1, "after creating window")?;

    // Verify window is on workspace 1
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 1);
    let window = &windows[0];

    println!("Window created on workspace: {}", window["workspace"]);
    assert_eq!(
        window["workspace"].as_u64().unwrap(),
        1,
        "First window should be on workspace 1, not 0"
    );

    // Verify workspace info shows window count
    let response = client.send_command(&serde_json::json!({"type": "GetWorkspaces"}))?;
    let workspaces = response["workspaces"].as_array().unwrap();
    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    assert_eq!(
        ws1["window_count"].as_u64().unwrap(),
        1,
        "Workspace 1 should have 1 window"
    );

    // This is what waybar expects - workspace names/ids starting from 1
    assert_eq!(
        ws1["name"].as_str().unwrap(),
        "1",
        "Workspace name should be '1' for waybar compatibility"
    );

    println!("\n✓ Default workspace correctly starts at 1!");

    // Clean up
    window1.kill()?;

    Ok(())
}

#[test]
fn test_workspace_numbering_one_indexed() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("workspace-numbering");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--ascii-size", "80x24"])?;

    let client = TestClient::new(&env.test_socket);

    // Create windows on different workspaces
    println!("\n=== Creating windows on workspaces 1, 2, and 3 ===");

    // Window on workspace 1 (default)
    let mut window1 = env.start_window("Window1", None)?;
    client.wait_for_window_count(1, "after window 1")?;

    // Move to workspace 2 and create window
    let response = client.send_command(&serde_json::json!({
        "type": "SwitchWorkspace",
        "index": 1  // Internal 0-based index for workspace 2
    }))?;
    assert_eq!(response["type"].as_str(), Some("Success"));

    let mut window2 = env.start_window("Window2", None)?;
    client.wait_for_window_count(2, "after window 2")?;

    // Move to workspace 3 and create window
    let response = client.send_command(&serde_json::json!({
        "type": "SwitchWorkspace",
        "index": 2  // Internal 0-based index for workspace 3
    }))?;
    assert_eq!(response["type"].as_str(), Some("Success"));

    let mut window3 = env.start_window("Window3", None)?;
    client.wait_for_window_count(3, "after window 3")?;

    // Verify windows are on correct workspaces
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 3);

    println!("\n=== Window Workspace Assignments ===");
    for w in &windows {
        println!("Window {}: workspace {}", w["id"], w["workspace"]);
    }

    // Check each window is on the expected workspace (1-indexed)
    let win1 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(1))
        .unwrap();
    let win2 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(2))
        .unwrap();
    let win3 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(3))
        .unwrap();

    assert_eq!(
        win1["workspace"].as_u64().unwrap(),
        1,
        "Window 1 should be on workspace 1"
    );
    assert_eq!(
        win2["workspace"].as_u64().unwrap(),
        2,
        "Window 2 should be on workspace 2"
    );
    assert_eq!(
        win3["workspace"].as_u64().unwrap(),
        3,
        "Window 3 should be on workspace 3"
    );

    // Verify workspace info
    let response = client.send_command(&serde_json::json!({"type": "GetWorkspaces"}))?;
    let workspaces = response["workspaces"].as_array().unwrap();

    println!("\n=== Workspace Information ===");
    for ws in workspaces {
        if ws["window_count"].as_u64().unwrap() > 0 {
            println!(
                "Workspace {}: name='{}', windows={}",
                ws["id"], ws["name"], ws["window_count"]
            );
        }
    }

    // Verify workspace names are 1-indexed strings
    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    let ws2 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(2))
        .unwrap();
    let ws3 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(3))
        .unwrap();

    assert_eq!(ws1["name"].as_str().unwrap(), "1");
    assert_eq!(ws2["name"].as_str().unwrap(), "2");
    assert_eq!(ws3["name"].as_str().unwrap(), "3");

    println!("\n✓ Workspace numbering is correctly 1-indexed!");

    // Clean up
    window1.kill()?;
    window2.kill()?;
    window3.kill()?;

    Ok(())
}
