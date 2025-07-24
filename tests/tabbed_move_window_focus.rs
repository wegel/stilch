//! Test that focus stays in current workspace when moving a window to another workspace
//!
//! This test verifies that when a window is moved to another workspace from a tabbed
//! container, the focus remains in the current workspace on the next available window.

mod common;
use common::{TestClient, TestEnv};

#[test]
fn test_focus_stays_when_moving_window_from_tabbed() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-move-focus");
    env.cleanup();
    env.start_compositor(&["--test"])?;
    let client = TestClient::new(&env.test_socket);

    // Create three windows
    let window1 = env.start_window("TestWindow1", Some("red"))?;
    client.wait_for_window_count(1, "after starting window 1")?;
    let windows = client.get_windows()?;
    let window1_id = windows[0]
        .get("id")
        .and_then(|v| v.as_u64())
        .expect("Window 1 should have ID");

    let window2 = env.start_window("TestWindow2", Some("blue"))?;
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

    let window3 = env.start_window("TestWindow3", Some("green"))?;
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

    // Switch to tabbed mode
    client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Focus window 2 (middle window)
    client.send_command(&serde_json::json!({
        "type": "FocusWindow",
        "id": window2_id
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Verify window 2 is focused
    let windows = client.get_windows()?;
    let focused_id = windows
        .iter()
        .find(|w| w.get("focused").and_then(|v| v.as_bool()).unwrap_or(false))
        .and_then(|w| w.get("id").and_then(|v| v.as_u64()));
    assert_eq!(
        focused_id,
        Some(window2_id),
        "Window 2 should be focused before move"
    );

    // Move focused window (window 2) to workspace 2 (Shift+Super+2)
    client.send_command(&serde_json::json!({
        "type": "MoveFocusedWindowToWorkspace",
        "workspace": 1  // Workspace index 1 = workspace 2
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Check that we're still on workspace 1
    let windows = client.get_windows()?;

    // Debug: print all windows
    println!("All windows after move:");
    for w in &windows {
        let id = w.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        let ws = w.get("workspace").and_then(|v| v.as_u64()).unwrap_or(999);
        let focused = w.get("focused").and_then(|v| v.as_bool()).unwrap_or(false);
        println!("  Window {id}: workspace={ws}, focused={focused}");
    }

    let workspace1_windows: Vec<_> = windows
        .iter()
        .filter(|w| w.get("workspace").and_then(|v| v.as_u64()) == Some(1))
        .collect();

    assert_eq!(
        workspace1_windows.len(),
        2,
        "Workspace 1 should have 2 windows after moving one"
    );

    // Check which window is now focused
    let focused_window = windows
        .iter()
        .find(|w| w.get("focused").and_then(|v| v.as_bool()).unwrap_or(false));

    if let Some(focused) = focused_window {
        let focused_id = focused
            .get("id")
            .and_then(|v| v.as_u64())
            .expect("Focused window should have ID");
        let focused_workspace = focused
            .get("workspace")
            .and_then(|v| v.as_u64())
            .expect("Focused window should have workspace");

        // The focus should stay in workspace 1 (displayed as 1), not follow the window to workspace 2
        assert_eq!(
            focused_workspace, 1,
            "Focus should remain in workspace 1, but focused window {focused_id} is in workspace {focused_workspace}"
        );

        // The focused window should be one of the remaining windows in workspace 1
        assert!(
            focused_id == window1_id || focused_id == window3_id,
            "Focus should be on window 1 or 3, but is on window {focused_id}"
        );
    } else {
        panic!("No window is focused after moving window to another workspace");
    }

    // Verify window 2 is actually in workspace 2
    let window2_data = windows
        .iter()
        .find(|w| w.get("id").and_then(|v| v.as_u64()) == Some(window2_id))
        .expect("Window 2 should still exist");
    let window2_workspace = window2_data
        .get("workspace")
        .and_then(|v| v.as_u64())
        .expect("Window 2 should have workspace");
    assert_eq!(window2_workspace, 2, "Window 2 should be in workspace 2");

    env.cleanup();
    Ok(())
}

#[test]
fn test_focus_stays_when_moving_last_window_from_tabbed() -> Result<(), Box<dyn std::error::Error>>
{
    let mut env = TestEnv::new("tabbed-move-last");
    env.cleanup();
    env.start_compositor(&["--test"])?;
    let client = TestClient::new(&env.test_socket);

    // Create just one window in tabbed mode
    let window1 = env.start_window("TestWindow1", Some("red"))?;
    client.wait_for_window_count(1, "after starting window 1")?;
    let windows = client.get_windows()?;
    let window1_id = windows[0]
        .get("id")
        .and_then(|v| v.as_u64())
        .expect("Window 1 should have ID");

    // Switch to tabbed mode (even with one window)
    client.send_command(&serde_json::json!({
        "type": "SetLayout",
        "mode": "tabbed"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Move the only window to workspace 2
    client.send_command(&serde_json::json!({
        "type": "MoveFocusedWindowToWorkspace",
        "workspace": 1  // Workspace index 1 = workspace 2
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Check that workspace 1 is now empty
    let windows = client.get_windows()?;
    let workspace1_windows: Vec<_> = windows
        .iter()
        .filter(|w| w.get("workspace").and_then(|v| v.as_u64()) == Some(1))
        .collect();

    assert_eq!(
        workspace1_windows.len(),
        0,
        "Workspace 1 should be empty after moving the only window"
    );

    // Since workspace 1 is empty, there should be no focused window in workspace 1
    // The system might focus workspace 2 or have no focus, but it shouldn't
    // have a focused window in the now-empty workspace 1
    let focused_window = windows
        .iter()
        .find(|w| w.get("focused").and_then(|v| v.as_bool()).unwrap_or(false));

    if let Some(focused) = focused_window {
        let focused_workspace = focused
            .get("workspace")
            .and_then(|v| v.as_u64())
            .expect("Focused window should have workspace");
        // If there is a focused window, it should NOT be in workspace 1 (which is empty)
        assert_ne!(
            focused_workspace, 1,
            "Focus should not be in empty workspace 1"
        );
    }

    env.cleanup();
    Ok(())
}
