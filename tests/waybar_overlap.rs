mod common;

use common::{TestClient, TestEnv};
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn test_waybar_overlap_bug() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("waybar-overlap");
    env.cleanup()?;

    // Start compositor in test mode
    println!("\n=== Testing Waybar Overlap Bug ===");
    println!("1. Starting compositor...");
    env.start_compositor(&["--test", "--ascii-size", "80x24"])?;

    let client = TestClient::new(&env.test_socket);

    // Create a minimal waybar config
    let waybar_config = r#"{
    "layer": "top",
    "height": 30,
    "modules-center": ["custom/test"],
    "custom/test": {
        "format": "TEST BAR - Windows should start below this 30px area",
        "interval": "once"
    }
}"#;

    let config_path = "/tmp/test-waybar-config.json";
    fs::write(config_path, waybar_config)?;

    // Start waybar
    println!("2. Starting waybar at TOP (creates 30px exclusive zone)...");
    let mut waybar = Command::new("waybar")
        .args(["-c", config_path])
        .env("WAYLAND_DISPLAY", &env.wayland_display)
        .spawn()?;

    thread::sleep(Duration::from_secs(3)); // Give waybar more time to setup

    // Start simple_window
    println!("3. Starting simple_window...");
    let mut window = env.start_window("TestWindow", Some("red"))?;

    println!("   simple_window PID: {}", window.id());
    thread::sleep(Duration::from_secs(3));

    // Query window positions via IPC
    println!("\n4. Querying window positions...");

    let windows = client.get_windows()?;

    println!("\n=== WINDOW POSITIONS ===");
    let mut overlapping_windows = Vec::new();

    for window in &windows {
        let id = window["id"].as_u64().unwrap_or(0);
        let x = window["x"].as_i64().unwrap_or(0);
        let y = window["y"].as_i64().unwrap_or(0);
        let width = window["width"].as_i64().unwrap_or(0);
        let height = window["height"].as_i64().unwrap_or(0);

        println!(
            "Window {id}: x={x}, y={y}, width={width}, height={height}"
        );

        if y < 30 {
            overlapping_windows.push((id, y));
        }
    }

    println!("\n=== BUG ANALYSIS ===");
    if overlapping_windows.is_empty() {
        println!("✓ GOOD: All windows start at y >= 30 (below waybar)");
    } else {
        println!(
            "✗ BUG CONFIRMED: {} windows overlap with waybar!",
            overlapping_windows.len()
        );
        for (id, y) in &overlapping_windows {
            println!("  - Window {id} is at y={y} (should be >= 30)");
        }
    }

    // Cleanup
    let _ = window.kill();
    let _ = waybar.kill();
    let _ = fs::remove_file(config_path);

    // Fail test if bug detected
    assert!(
        overlapping_windows.is_empty(),
        "Found {} windows overlapping with waybar",
        overlapping_windows.len()
    );

    Ok(())
}
