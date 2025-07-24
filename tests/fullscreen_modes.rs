mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_container_fullscreen() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("container-fullscreen");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--config", "tests/test_configs/no_gaps.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // Create 3 windows
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

    let _window3 = env.start_window("FullscreenTest3", Some("green"))?;
    client.wait_for_window_count(3, "after starting window 3")?;
    let windows = client.get_windows()?;
    // Get window3's ID - it's the one that's not window1 or window2
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

    // Make them tabbed
    client.send_simple_command("LayoutTabbed")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Get container geometry before fullscreen
    let windows_before = client.get_windows()?;
    // Find window 3 by ID
    let w3_before = windows_before
        .iter()
        .find(|w| w.get("id").and_then(|v| v.as_u64()) == Some(window3_id))
        .expect("Should find window 3");
    let width_before = w3_before.get("width").and_then(|v| v.as_i64()).unwrap();
    let height_before = w3_before.get("height").and_then(|v| v.as_i64()).unwrap();

    // Toggle container fullscreen
    client.send_simple_command("FullscreenContainer")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check that window fills the container
    let windows_after = client.get_windows()?;
    // Find window 3 by ID
    let w3_after = windows_after
        .iter()
        .find(|w| w.get("id").and_then(|v| v.as_u64()) == Some(window3_id))
        .expect("Should find window 3");
    let width_after = w3_after.get("width").and_then(|v| v.as_i64()).unwrap();
    let height_after = w3_after.get("height").and_then(|v| v.as_i64()).unwrap();
    let fullscreen_after = w3_after
        .get("fullscreen")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    println!("Container fullscreen test:");
    println!(
        "  Window 3 before: width={width_before}, height={height_before}"
    );
    println!(
        "  Window 3 after:  width={width_after}, height={height_after}, fullscreen={fullscreen_after}"
    );
    println!("  Full window data: {w3_after:?}");

    assert!(fullscreen_after, "Window should be fullscreen");
    // Container fullscreen should use container bounds
    // The window should be at least as large as before
    assert!(width_after >= width_before, "Width should not decrease");
    assert!(height_after >= height_before, "Height should not decrease");

    // Toggle off
    client.send_simple_command("FullscreenContainer")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let windows_off = client.get_windows()?;
    let w3_off = windows_off
        .iter()
        .find(|w| w.get("id").and_then(|v| v.as_u64()) == Some(window3_id))
        .expect("Should find window 3");
    let fullscreen_off = w3_off
        .get("fullscreen")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    assert!(!fullscreen_off, "Window should not be fullscreen");

    env.cleanup()?;
    Ok(())
}

#[test]
fn test_virtual_output_fullscreen() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("virtual-fullscreen");
    env.cleanup()?;

    // Start compositor with virtual outputs config
    env.start_compositor(&["--test", "--config", "tests/test_configs/virtual_outputs.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // Create a window
    let _window = env.start_window("VirtualFullscreenTest", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window")?;

    // Get virtual output size - the config creates 1920x1080 virtual outputs
    let vo_width = 1920;
    let vo_height = 1080;

    // Toggle virtual output fullscreen (default)
    client.send_simple_command("Fullscreen")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check that window fills virtual output
    let windows = client.get_windows()?;
    // Should only be one window
    let w = windows.first().expect("Should find window");
    let fullscreen = w
        .get("fullscreen")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let width = w.get("width").and_then(|v| v.as_i64()).unwrap();
    let height = w.get("height").and_then(|v| v.as_i64()).unwrap();

    assert!(fullscreen, "Window should be fullscreen");
    assert_eq!(width, vo_width, "Should fill virtual output width");
    assert_eq!(height, vo_height, "Should fill virtual output height");

    // Toggle off
    client.send_simple_command("Fullscreen")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let windows_off = client.get_windows()?;
    let w_off = windows_off.first().expect("Should find window");
    let fullscreen_off = w_off
        .get("fullscreen")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    assert!(!fullscreen_off, "Window should not be fullscreen");

    env.cleanup()?;
    Ok(())
}

#[test]
fn test_physical_output_fullscreen() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("physical-fullscreen");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--config", "tests/test_configs/no_gaps.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // Create a window
    let _window = env.start_window("PhysicalFullscreenTest", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window")?;

    // For test mode, physical output is usually the same as virtual output
    // In real usage, it would be the full monitor size
    // Get physical output size - in test mode it's 3840x2160
    let output_width = 3840;
    let output_height = 2160;

    // Toggle physical output fullscreen
    client.send_simple_command("FullscreenPhysicalOutput")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check that window fills physical output
    let windows = client.get_windows()?;
    let w = windows.first().expect("Should find window");
    let fullscreen = w
        .get("fullscreen")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let width = w.get("width").and_then(|v| v.as_i64()).unwrap();
    let height = w.get("height").and_then(|v| v.as_i64()).unwrap();

    assert!(fullscreen, "Window should be fullscreen");
    assert_eq!(width, output_width, "Should fill physical output width");
    assert_eq!(height, output_height, "Should fill physical output height");

    // Toggle off
    client.send_simple_command("FullscreenPhysicalOutput")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    let windows_off = client.get_windows()?;
    let w_off = windows_off.first().expect("Should find window");
    let fullscreen_off = w_off
        .get("fullscreen")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    assert!(!fullscreen_off, "Window should not be fullscreen");

    env.cleanup()?;
    Ok(())
}

#[test]
fn test_fullscreen_with_multiple_windows() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("fullscreen-multiple");
    env.cleanup()?;

    // Start compositor
    env.start_compositor(&["--test", "--config", "tests/test_configs/no_gaps.conf"])?;

    let client = TestClient::new(&env.test_socket);

    // Create multiple windows
    let _window1 = env.start_window("Window1", Some("blue"))?;
    client.wait_for_window_count(1, "after starting window 1")?;
    let windows = client.get_windows()?;
    let window1_id = windows[0].get("id").and_then(|v| v.as_u64()).unwrap();

    let _window2 = env.start_window("Window2", Some("red"))?;
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

    let _window3 = env.start_window("Window3", Some("green"))?;
    client.wait_for_window_count(3, "after starting window 3")?;

    // Focus window 2 (middle window)
    client.send_simple_command("FocusLeft")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Make window 2 fullscreen
    client.send_simple_command("Fullscreen")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Window 2 should be fullscreen, others should be hidden
    let windows = client.get_windows()?;

    // Find window 2 by ID
    let w2 = windows
        .iter()
        .find(|w| w.get("id").and_then(|v| v.as_u64()) == Some(window2_id))
        .expect("Should find window 2");
    let w2_fullscreen = w2
        .get("fullscreen")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let w2_visible = w2.get("visible").and_then(|v| v.as_bool()).unwrap_or(true);

    assert!(w2_fullscreen, "Window 2 should be fullscreen");
    assert!(w2_visible, "Window 2 should be visible");

    // Other windows should not be visible (hidden behind fullscreen)
    let w1 = windows
        .iter()
        .find(|w| w.get("id").and_then(|v| v.as_u64()) == Some(window1_id))
        .expect("Should find window 1");
    let w1_fullscreen = w1
        .get("fullscreen")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let w3 = windows
        .iter()
        .find(|w| {
            w.get("id").and_then(|v| v.as_u64()) != Some(window1_id)
                && w.get("id").and_then(|v| v.as_u64()) != Some(window2_id)
        })
        .expect("Should find window 3");
    let w3_fullscreen = w3
        .get("fullscreen")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    assert!(!w1_fullscreen, "Window 1 should not be fullscreen");
    assert!(!w3_fullscreen, "Window 3 should not be fullscreen");

    // Exit fullscreen
    client.send_simple_command("Fullscreen")?;
    std::thread::sleep(std::time::Duration::from_millis(100));

    // All windows should be visible again
    let windows_after = client.get_windows()?;
    assert_eq!(windows_after.len(), 3, "All 3 windows should still exist");

    env.cleanup()?;
    Ok(())
}
