mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_container_fullscreen_from_split() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("container-fullscreen-split");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--config", "tests/test_configs/no_gaps.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // Create 2 windows in split mode (not tabbed)
    let _window1 = env.start_window("FullscreenTest1", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window 1")?;
    let windows = client.get_windows()?;
    let window1_id = windows[0].get("id").and_then(|v| v.as_u64()).unwrap();

    let _window2 = env.start_window("FullscreenTest2", Some("red"))?;
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

    // Windows should be split horizontally by default
    // Get ASCII snapshot before fullscreen
    let ascii_before = client.get_ascii_snapshot(true, true)?;
    println!("ASCII before fullscreen:\n{ascii_before}");

    // Get window 2's dimensions before fullscreen
    let windows_before = client.get_windows()?;
    let w2_before = windows_before
        .iter()
        .find(|w| w.get("id").and_then(|v| v.as_u64()) == Some(window2_id))
        .expect("Should find window 2");
    let width_before = w2_before.get("width").and_then(|v| v.as_i64()).unwrap();
    let height_before = w2_before.get("height").and_then(|v| v.as_i64()).unwrap();

    println!("Before fullscreen:");
    println!(
        "  Window 2: width={width_before}, height={height_before}"
    );

    // Window 2 should be approximately half the screen width (1920) in split mode
    // Allow some tolerance for gaps/borders
    assert!(
        width_before < 2000,
        "Window should be less than half width before fullscreen, got {width_before}"
    );

    // Toggle container fullscreen on window 2
    client.send_simple_command("FullscreenContainer")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Get ASCII snapshot after fullscreen
    let ascii_after = client.get_ascii_snapshot(true, true)?;
    println!("ASCII after fullscreen:\n{ascii_after}");

    // Check that window 2 now fills the entire workspace
    let windows_after = client.get_windows()?;
    let w2_after = windows_after
        .iter()
        .find(|w| w.get("id").and_then(|v| v.as_u64()) == Some(window2_id))
        .expect("Should find window 2");
    let width_after = w2_after.get("width").and_then(|v| v.as_i64()).unwrap();
    let height_after = w2_after.get("height").and_then(|v| v.as_i64()).unwrap();
    let fullscreen_after = w2_after
        .get("fullscreen")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    println!("After fullscreen:");
    println!(
        "  Window 2: width={width_after}, height={height_after}, fullscreen={fullscreen_after}"
    );

    // Check window 1 visibility
    let w1_after = windows_after
        .iter()
        .find(|w| w.get("id").and_then(|v| v.as_u64()) == Some(window1_id))
        .expect("Should find window 1");
    let w1_visible = w1_after
        .get("visible")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    println!("  Window 1 visible: {w1_visible}");

    assert!(fullscreen_after, "Window 2 should be fullscreen");
    // Container fullscreen should expand to full workspace width (3840)
    assert_eq!(width_after, 3840, "Window should be full workspace width");
    assert_eq!(height_after, 2160, "Window should be full workspace height");

    // Window 1 should still exist but might not be visible
    assert_eq!(windows_after.len(), 2, "Both windows should still exist");

    // Toggle off
    client.send_simple_command("FullscreenContainer")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let windows_off = client.get_windows()?;
    let w2_off = windows_off
        .iter()
        .find(|w| w.get("id").and_then(|v| v.as_u64()) == Some(window2_id))
        .expect("Should find window 2");
    let fullscreen_off = w2_off
        .get("fullscreen")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let width_off = w2_off.get("width").and_then(|v| v.as_i64()).unwrap();

    println!("After toggling off:");
    println!(
        "  Window 2: width={width_off}, fullscreen={fullscreen_off}"
    );

    assert!(!fullscreen_off, "Window should not be fullscreen");
    // Should return to split size
    assert!(
        width_off < 2000,
        "Window should return to split width, got {width_off}"
    );

    env.cleanup()?;
    Ok(())
}
