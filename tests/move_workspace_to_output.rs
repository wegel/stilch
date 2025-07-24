mod common;

use common::{TestClient, TestEnv};
use std::thread;
use std::time::Duration;

#[test]
fn test_move_workspace_to_right_output() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("move-workspace-right");
    env.cleanup()?;

    // Start compositor with 2 outputs side by side (1920x1080 each)
    env.start_compositor_multi_output(2, 1920, 1080)?;

    let client = TestClient::new(&env.test_socket);

    // Verify we have 2 outputs
    let outputs = client.get_outputs()?;
    assert_eq!(outputs.len(), 2, "Should have 2 outputs");

    println!("\n=== Output Configuration ===");
    for output in &outputs {
        println!(
            "Output {}: x={}, y={}, width={}, height={}",
            output["id"], output["x"], output["y"], output["width"], output["height"]
        );
    }

    // Create windows on workspace 1 (on output 0)
    println!("\n=== Creating windows on workspace 1 ===");
    let mut window1 = env.start_window("Window1", Some("red"))?;
    client.wait_for_window_count(1, "after window 1")?;

    let mut window2 = env.start_window("Window2", Some("green"))?;
    client.wait_for_window_count(2, "after window 2")?;

    // Verify windows are on output 0
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 2);

    for window in &windows {
        let x = window["x"].as_i64().unwrap();
        assert!(
            x < 1920,
            "Window {} should be on output 0 (x < 1920)",
            window["id"]
        );
    }

    // Check workspace assignment
    let workspaces = client.get_workspaces()?;
    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    assert_eq!(
        ws1["output"].as_str().unwrap(),
        "output-1",
        "Workspace 1 should start on first output"
    );
    assert_eq!(
        ws1["window_count"].as_u64().unwrap(),
        2,
        "Workspace 1 should have 2 windows"
    );

    // Move workspace to right output
    println!("\n=== Moving workspace 1 to right output ===");
    client.move_workspace_to_output("right")?;

    thread::sleep(Duration::from_millis(500)); // Let the move complete

    // Verify workspace moved to output 1
    let workspaces = client.get_workspaces()?;
    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    assert_eq!(
        ws1["output"].as_str().unwrap(),
        "output-2",
        "Workspace 1 should now be on output 1"
    );

    // Verify windows moved with the workspace
    let windows = client.get_windows()?;
    println!("\n=== Window positions after move ===");
    for window in &windows {
        let x = window["x"].as_i64().unwrap();
        println!(
            "Window {}: x={}, workspace={}",
            window["id"], x, window["workspace"]
        );
        assert!(
            x >= 1920,
            "Window {} should be on output 1 (x >= 1920)",
            window["id"]
        );
        assert_eq!(
            window["workspace"].as_u64().unwrap(),
            1,
            "Window should still be on workspace 1"
        );
    }

    println!("\n✓ Workspace successfully moved to right output!");

    // Clean up
    window1.kill()?;
    window2.kill()?;

    Ok(())
}

#[test]
fn test_move_workspace_all_directions() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("move-workspace-all-dirs");
    env.cleanup()?;

    // Start compositor with 4 outputs in a 2x2 grid (1920x1080 each)
    env.start_compositor(&[
        "--test",
        "--logical-size",
        "1920x1080", // Set default output size
        "--ascii-output",
        "1920x1080+1920+0", // Top-right (output 1)
        "--ascii-output",
        "1920x1080+0+1080", // Bottom-left (output 2)
        "--ascii-output",
        "1920x1080+1920+1080", // Bottom-right (output 3)
    ])?;

    let client = TestClient::new(&env.test_socket);

    // Verify we have 4 outputs
    let outputs = client.get_outputs()?;
    assert_eq!(outputs.len(), 4, "Should have 4 outputs");

    // Create a window on workspace 1 (starts on output 0)
    let mut window = env.start_window("TestWindow", Some("blue"))?;
    client.wait_for_window_count(1, "after creating window")?;

    // Helper to verify workspace is on expected output
    let verify_workspace_on_output = |client: &TestClient,
                                      ws_id: u64,
                                      expected_output: u64|
     -> Result<(), Box<dyn std::error::Error>> {
        let workspaces = client.get_workspaces()?;
        let ws = workspaces
            .iter()
            .find(|ws| ws["id"].as_u64() == Some(ws_id))
            .unwrap();
        let expected_output_str = format!("output-{expected_output}");
        assert_eq!(
            ws["output"].as_str().unwrap(),
            expected_output_str,
            "Workspace {ws_id} should be on output {expected_output}"
        );
        Ok(())
    };

    println!("\n=== Testing all direction moves ===");

    // Start at output 1 (top-left)
    verify_workspace_on_output(&client, 1, 1)?;
    println!("Initial: Workspace 1 on output 1 (top-left)");

    // Move right to output 2 (top-right)
    client.move_workspace_to_output("right")?;
    thread::sleep(Duration::from_millis(300));
    verify_workspace_on_output(&client, 1, 2)?;
    println!("After right: Workspace 1 on output 2 (top-right)");

    // Move down to output 4 (bottom-right)
    client.move_workspace_to_output("down")?;
    thread::sleep(Duration::from_millis(300));
    verify_workspace_on_output(&client, 1, 4)?;
    println!("After down: Workspace 1 on output 4 (bottom-right)");

    // Move left to output 3 (bottom-left)
    client.move_workspace_to_output("left")?;
    thread::sleep(Duration::from_millis(300));
    verify_workspace_on_output(&client, 1, 3)?;
    println!("After left: Workspace 1 on output 3 (bottom-left)");

    // Move up to output 1 (top-left)
    client.move_workspace_to_output("up")?;
    thread::sleep(Duration::from_millis(300));
    verify_workspace_on_output(&client, 1, 1)?;
    println!("After up: Workspace 1 back on output 1 (top-left)");

    println!("\n✓ Workspace movement in all directions works correctly!");

    // Clean up
    window.kill()?;

    Ok(())
}

#[test]
fn test_multiple_windows_move_with_workspace() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("move-workspace-multi-windows");
    env.cleanup()?;

    // Start compositor with 2 outputs side by side
    env.start_compositor_multi_output(2, 1920, 1080)?;

    let client = TestClient::new(&env.test_socket);

    // Create 3 windows on workspace 1
    println!("\n=== Creating 3 windows on workspace 1 ===");
    let mut windows = Vec::new();
    for i in 1..=3 {
        let window = env.start_window(&format!("Window{i}"), None)?;
        client.wait_for_window_count(i, &format!("after window {i}"))?;
        windows.push(window);
    }

    // Get initial window information
    let window_infos = client.get_windows()?;
    assert_eq!(window_infos.len(), 3);

    // Store window IDs and verify initial positions
    let window_ids: Vec<u64> = window_infos
        .iter()
        .map(|w| w["id"].as_u64().unwrap())
        .collect();

    println!("\nInitial window positions:");
    for w in &window_infos {
        let x = w["x"].as_i64().unwrap();
        println!("Window {}: x={}, workspace={}", w["id"], x, w["workspace"]);
        assert!(x < 1920, "Window should initially be on output 0");
    }

    // Move workspace to right output
    println!("\n=== Moving workspace with all windows to right output ===");
    client.move_workspace_to_output("right")?;
    thread::sleep(Duration::from_millis(500));

    // Verify all windows moved together
    let window_infos = client.get_windows()?;
    assert_eq!(window_infos.len(), 3, "Should still have 3 windows");

    println!("\nWindow positions after move:");
    for w in &window_infos {
        let id = w["id"].as_u64().unwrap();
        let x = w["x"].as_i64().unwrap();
        let workspace = w["workspace"].as_u64().unwrap();

        println!("Window {id}: x={x}, workspace={workspace}");

        assert!(window_ids.contains(&id), "Window {id} should still exist");
        assert!(x >= 1920, "Window {id} should be on output 1 (x >= 1920)");
        assert_eq!(workspace, 1, "Window {id} should still be on workspace 1");
    }

    // Verify workspace is on output 1
    let workspaces = client.get_workspaces()?;
    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    assert_eq!(
        ws1["output"].as_str().unwrap(),
        "output-2",
        "Workspace 1 should be on output 2 (after move)"
    );
    assert_eq!(
        ws1["window_count"].as_u64().unwrap(),
        3,
        "Workspace 1 should still have 3 windows"
    );

    println!("\n✓ All windows moved together with the workspace!");

    // Clean up
    for mut window in windows {
        window.kill()?;
    }

    Ok(())
}

#[test]
fn test_move_workspace_no_output_in_direction() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("move-workspace-no-output");
    env.cleanup()?;

    // Start compositor with 2 outputs side by side (only left/right possible)
    env.start_compositor_multi_output(2, 1920, 1080)?;

    let client = TestClient::new(&env.test_socket);

    // Create a window
    let mut window = env.start_window("TestWindow", None)?;
    client.wait_for_window_count(1, "after creating window")?;

    // Verify workspace is on output 0
    let workspaces = client.get_workspaces()?;
    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    assert_eq!(
        ws1["output"].as_str().unwrap(),
        "output-1",
        "Workspace 1 should be back on first output"
    );

    println!("\n=== Testing moves to non-existent outputs ===");

    // Try to move up (no output above)
    println!("Trying to move up (should fail gracefully)...");
    match client.move_workspace_to_output("up") {
        Ok(_) => {
            // Verify workspace didn't move
            let workspaces = client.get_workspaces()?;
            let ws1 = workspaces
                .iter()
                .find(|ws| ws["id"].as_u64() == Some(1))
                .unwrap();
            assert_eq!(
                ws1["output"].as_str().unwrap(),
                "output-1",
                "Workspace should still be on first output"
            );
            println!("✓ Move up handled gracefully (no movement)");
        }
        Err(e) => {
            println!("✓ Move up failed as expected: {e}");
        }
    }

    // Try to move down (no output below)
    println!("\nTrying to move down (should fail gracefully)...");
    match client.move_workspace_to_output("down") {
        Ok(_) => {
            let workspaces = client.get_workspaces()?;
            let ws1 = workspaces
                .iter()
                .find(|ws| ws["id"].as_u64() == Some(1))
                .unwrap();
            assert_eq!(
                ws1["output"].as_str().unwrap(),
                "output-1",
                "Workspace should still be on first output"
            );
            println!("✓ Move down handled gracefully (no movement)");
        }
        Err(e) => {
            println!("✓ Move down failed as expected: {e}");
        }
    }

    // Move right (should succeed)
    println!("\nMoving right to output 1...");
    client.move_workspace_to_output("right")?;
    thread::sleep(Duration::from_millis(300));

    let workspaces = client.get_workspaces()?;
    let ws1 = workspaces
        .iter()
        .find(|ws| ws["id"].as_u64() == Some(1))
        .unwrap();
    assert_eq!(
        ws1["output"].as_str().unwrap(),
        "output-2",
        "Workspace should be on output 1"
    );
    println!("✓ Move right succeeded");

    // Try to move right again (no output to the right of output 1)
    println!("\nTrying to move right again (should fail gracefully)...");
    match client.move_workspace_to_output("right") {
        Ok(_) => {
            let workspaces = client.get_workspaces()?;
            let ws1 = workspaces
                .iter()
                .find(|ws| ws["id"].as_u64() == Some(1))
                .unwrap();
            assert_eq!(
                ws1["output"].as_str().unwrap(),
                "output-2",
                "Workspace should still be on output 1"
            );
            println!("✓ Move right handled gracefully (no movement)");
        }
        Err(e) => {
            println!("✓ Move right failed as expected: {e}");
        }
    }

    println!("\n✓ Edge cases handled correctly!");

    // Clean up
    window.kill()?;

    Ok(())
}
