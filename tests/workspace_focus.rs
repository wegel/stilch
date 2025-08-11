mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_workspace_switching_updates_focus() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("workspace-focus");
    env.cleanup()?;

    // Start compositor with known dimensions
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

    // Verify window1 is focused
    let windows = client.get_windows()?;
    let w1 = &windows[0];
    assert_eq!(
        w1["focused"].as_bool(),
        Some(true),
        "Window1 should be focused initially"
    );
    let w1_id = w1["id"].as_u64().unwrap();

    // Switch to workspace 2 (index 1, since workspaces are 0-indexed internally)
    client.switch_workspace(1)?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Start window on workspace 2
    let mut window2 = env.start_window("Window2", Some("red"))?;
    client.wait_for_window_count(2, "after starting window2")?;

    // Verify window2 is focused and window1 is not
    let windows = client.get_windows()?;
    let (w1_info, w2_info) = if windows[0]["id"].as_u64().unwrap() == w1_id {
        (&windows[0], &windows[1])
    } else {
        (&windows[1], &windows[0])
    };

    assert_eq!(
        w2_info["focused"].as_bool(),
        Some(true),
        "Window2 should be focused on workspace 2"
    );
    assert_eq!(
        w1_info["focused"].as_bool(),
        Some(false),
        "Window1 should not be focused"
    );

    // Switch back to workspace 1 (index 0)
    client.switch_workspace(0)?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Verify window1 is focused again
    let windows = client.get_windows()?;
    let (w1_info, w2_info) = if windows[0]["id"].as_u64().unwrap() == w1_id {
        (&windows[0], &windows[1])
    } else {
        (&windows[1], &windows[0])
    };

    assert_eq!(
        w1_info["focused"].as_bool(),
        Some(true),
        "Window1 should be focused when switching back to workspace 1"
    );
    assert_eq!(
        w2_info["focused"].as_bool(),
        Some(false),
        "Window2 should not be focused when on workspace 1"
    );

    // Clean up
    window1.kill()?;
    window2.kill()?;

    Ok(())
}

#[test]
fn test_workspace_switching_with_multiple_windows() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("workspace-multi-focus");
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

    // Start two windows on workspace 1
    let mut window1 = env.start_window("Window1", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window1")?;

    let mut window2 = env.start_window("Window2", Some("green"))?;
    client.wait_for_window_count(2, "after starting window2")?;

    // Window2 should be focused (most recently created)
    let windows = client.get_windows()?;

    // In test mode, windows don't have titles, so we need to find window2 by ID
    // Window2 was the second window created, so it should have ID 2
    let window2_id = 2u64;
    let w2 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(window2_id))
        .expect("Window2 not found");
    assert_eq!(
        w2["focused"].as_bool(),
        Some(true),
        "Window2 should be focused"
    );

    // Switch to workspace 2 (index 1)
    client.switch_workspace(1)?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Start window on workspace 2
    let mut window3 = env.start_window("Window3", Some("red"))?;
    client.wait_for_window_count(3, "after starting window3")?;

    // Switch back to workspace 1 (index 0)
    client.switch_workspace(0)?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Window2 should still be focused (was focused before switching away)
    let windows = client.get_windows()?;
    let w2 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(window2_id))
        .expect("Window2 not found after switching back");
    assert_eq!(
        w2["focused"].as_bool(),
        Some(true),
        "Window2 should remain focused when returning to workspace 1"
    );

    // Clean up
    window1.kill()?;
    window2.kill()?;
    window3.kill()?;

    Ok(())
}
