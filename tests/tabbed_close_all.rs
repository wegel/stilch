//! Test that closing all windows in a tabbed container removes the container
//!
//! This test verifies that when all windows in a tabbed container are closed,
//! the container itself is removed and no phantom windows remain.

mod common;
use common::{TestClient, TestEnv};

#[test]
fn test_close_all_windows_in_tabbed_container() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("tabbed-close-all");
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

    // Verify we're in tabbed mode
    let snapshot = client.get_ascii_snapshot(true, true)?;

    // Should show one of the windows (likely window 3 since it was last created)
    assert!(
        snapshot.contains(&format!(" {window1_id} "))
            || snapshot.contains(&format!(" {window2_id} "))
            || snapshot.contains(&format!(" {window3_id} ")),
        "One window should be visible in tabbed mode"
    );

    // Close first window using Super+Q (via KillFocusedWindow command)
    client.send_command(&serde_json::json!({
        "type": "KillFocusedWindow"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(200));
    client.wait_for_window_count(2, "after closing first window")?;

    // Close second window using Super+Q
    client.send_command(&serde_json::json!({
        "type": "KillFocusedWindow"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(200));
    client.wait_for_window_count(1, "after closing second window")?;

    // Close third window using Super+Q
    client.send_command(&serde_json::json!({
        "type": "KillFocusedWindow"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Verify no windows remain
    client.wait_for_window_count(0, "after closing all windows")?;

    // The workspace should be empty - no phantom windows or containers
    let final_snapshot = client.get_ascii_snapshot(true, true)?;

    // Check that the snapshot shows an empty workspace
    // It should not contain any window IDs
    assert!(
        !final_snapshot.contains(" 1 ")
            && !final_snapshot.contains(" 2 ")
            && !final_snapshot.contains(" 3 ")
            && !final_snapshot.contains(" 4 "),
        "Workspace should be empty, but found window IDs in: {final_snapshot}"
    );

    // An empty workspace should show all blank lines (spaces)
    // If we see any window borders or tab bars, that's a phantom container!
    let lines: Vec<&str> = final_snapshot.lines().collect();

    // Check that all lines are blank (only spaces)
    let all_blank = lines
        .iter()
        .all(|line| line.chars().all(|c| c == ' ' || c == '\n'));

    // Check for any window border characters
    let has_window_borders = lines.iter().any(|line| {
        line.contains("╔")
            || line.contains("═")
            || line.contains("╗")
            || line.contains("║")
            || line.contains("╚")
            || line.contains("╝")
            || line.contains("┌")
            || line.contains("─")
            || line.contains("┐")
            || line.contains("│")
            || line.contains("└")
            || line.contains("┘")
            || line.contains("[")
            || line.contains("]") // Tab indicators
    });

    assert!(
        all_blank && !has_window_borders,
        "Empty workspace should show only blank space, but found window borders or content - this indicates a phantom container!"
    );

    env.cleanup();
    Ok(())
}
