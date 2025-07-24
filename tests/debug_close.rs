mod common;

use common::{TestClient, TestEnv};
use std::thread;
use std::time::Duration;

#[test]
fn test_debug_window_close() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("debug-close");
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

    // Create a single window
    println!("Creating window...");
    let mut window = env.start_window("TestWindow", Some("red"))?;

    // Wait for window creation
    client.wait_for_window_count(1, "after creating window")?;

    // Get windows
    let windows = client.get_windows()?;
    if windows.is_empty() {
        return Err("No windows created".into());
    }

    let window_id = windows[0]["id"].as_u64().unwrap();
    println!("Window created with ID: {window_id}");

    // Try to close it via IPC
    println!(
        "\nTrying to close window {window_id} via DestroyWindow command..."
    );
    let response = client.send_command(&serde_json::json!({
        "type": "DestroyWindow",
        "id": window_id
    }))?;
    println!("Response: {response:?}");

    // Wait a bit
    println!("\nWaiting 2 seconds for window to close...");
    thread::sleep(Duration::from_millis(2000));

    // Check if window process is still running
    match window.try_wait() {
        Ok(Some(status)) => {
            println!("✓ Window process exited with status: {status:?}");
        }
        Ok(None) => {
            println!("✗ Window process is still running!");
            println!("Killing it manually...");
            window.kill()?;
        }
        Err(e) => {
            println!("Error checking window status: {e}");
        }
    }

    // Check window count
    let windows = client.get_windows()?;

    println!("\nFinal window count: {}", windows.len());
    if windows.is_empty() {
        println!("✓ Window was successfully removed from compositor");
    } else {
        println!("✗ Window still exists in compositor!");
        for window in &windows {
            println!("  - Window {}: {:?}", window["id"], window);
        }
    }

    println!("Test complete!");
    Ok(())
}
