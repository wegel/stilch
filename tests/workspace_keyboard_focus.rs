mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_keyboard_focus_updates_on_workspace_switch() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("workspace-keyboard-focus");
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

    // Start window on workspace 1
    let mut window1 = env.start_window("Window1", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window1")?;

    // Verify window1 has keyboard focus
    let windows = client.get_windows()?;
    let w1 = &windows[0];
    let w1_id = w1["id"].as_u64().unwrap();
    assert_eq!(
        w1["focused"].as_bool(),
        Some(true),
        "Window1 should have keyboard focus initially"
    );

    // Get the current focused window ID
    let initial_focused = client
        .get_focused_window()?
        .expect("Should have a focused window");
    assert_eq!(initial_focused, w1_id, "Focused window should be window1");

    // Switch to workspace 2 (index 1)
    client.switch_workspace(1)?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Start window on workspace 2
    let mut window2 = env.start_window("Window2", Some("red"))?;
    client.wait_for_window_count(2, "after starting window2")?;

    // Verify window2 has keyboard focus
    let focused_id = client
        .get_focused_window()?
        .expect("Should have a focused window");
    assert_eq!(
        focused_id, 2,
        "Window2 should have keyboard focus on workspace 2"
    );

    // Switch back to workspace 1 (index 0)
    client.switch_workspace(0)?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Verify keyboard focus returned to window1
    let focused_after_switch = client
        .get_focused_window()?
        .expect("Should have a focused window");
    assert_eq!(
        focused_after_switch, w1_id,
        "Keyboard focus should return to window1 when switching back to workspace 1"
    );

    // Send a key event to verify it goes to the right window
    client.send_command(&serde_json::json!({
        "type": "SendKey",
        "key": "a"
    }))?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // The key event should have gone to window1, not window2
    // In a real test we'd verify this, but for now we just ensure focus is correct
    let final_focused = client
        .get_focused_window()?
        .expect("Should have a focused window");
    assert_eq!(
        final_focused, w1_id,
        "Keyboard focus should still be on window1"
    );

    // Clean up
    window1.kill()?;
    window2.kill()?;

    Ok(())
}

#[test]
fn test_keyboard_focus_preserved_when_switching_empty_workspace(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("workspace-empty-focus");
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

    // Start window on workspace 1
    let mut window1 = env.start_window("Window1", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window1")?;

    let w1_id = 1u64;

    // Switch to empty workspace 2 (index 1)
    client.switch_workspace(1)?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Should have no focused window on empty workspace
    let focused_on_empty = client.get_focused_window()?;
    assert_eq!(
        focused_on_empty, None,
        "Empty workspace should have no focused window"
    );

    // Switch back to workspace 1 (index 0)
    client.switch_workspace(0)?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Keyboard focus should return to window1
    let focused_after_return = client
        .get_focused_window()?
        .expect("Should have a focused window");
    assert_eq!(
        focused_after_return, w1_id,
        "Keyboard focus should return to window1 after visiting empty workspace"
    );

    // Clean up
    window1.kill()?;

    Ok(())
}
