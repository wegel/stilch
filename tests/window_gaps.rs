mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_windows_no_gaps() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("window-gaps");
    env.cleanup()?;

    // Start compositor with no-gaps config
    env.start_compositor(&[
        "--test",
        "--ascii-size",
        "80x24",
        "--config",
        "tests/test_configs/no_gaps.conf",
    ])?;

    let client = TestClient::new(&env.test_socket);

    // Create two windows
    println!("\n=== Creating 2 windows ===");
    let mut window1 = env.start_window("Window1", Some("red"))?;
    client.wait_for_window_count(1, "after first window")?;

    let mut window2 = env.start_window("Window2", Some("blue"))?;
    client.wait_for_window_count(2, "after second window")?;

    // Get window geometries
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 2);

    let w1 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(1))
        .unwrap();
    let w2 = windows
        .iter()
        .find(|w| w["id"].as_u64() == Some(2))
        .unwrap();

    let w1_x = w1["x"].as_i64().unwrap();
    let w1_width = w1["width"].as_i64().unwrap();
    let w2_x = w2["x"].as_i64().unwrap();

    println!("\n=== Window Positions ===");
    println!("Window 1: x={w1_x}, width={w1_width}");
    println!("Window 2: x={}, width={}", w2_x, w2["width"]);

    // Windows should be adjacent with no gap
    assert_eq!(w1_x, 0, "First window should start at x=0");
    assert_eq!(
        w2_x,
        w1_x + w1_width,
        "Second window should start where first ends"
    );

    // Total width should equal workspace width
    let total_width = w1_width + w2["width"].as_i64().unwrap();
    assert_eq!(
        total_width, 3840,
        "Windows should fill entire width with no gaps"
    );

    // Get ASCII visualization
    let ascii = client.get_ascii_snapshot(true, true)?;
    println!("\n=== ASCII Visualization (No Gaps) ===");
    println!("{ascii}");

    // Verify windows are adjacent in ASCII
    // In the ASCII output, windows share a border so look for patterns like:
    // - Top corners meeting: "┐═" or "╗┌"
    // - Bottom corners meeting: "┘═" or "╝┌"
    // - Or the side borders directly adjacent
    let has_adjacent_windows = ascii.lines().any(|line| {
        // Check if line has both window 1 border (│) and window 2 border (║) next to each other
        // or corners meeting
        line.contains("┐═")
            || line.contains("╗┌")
            || line.contains("┘═")
            || line.contains("╝┌")
            || (line.contains('│') && line.contains('║'))
    });

    assert!(
        has_adjacent_windows,
        "Windows should be adjacent with shared borders in ASCII"
    );

    println!("\n✓ Windows correctly positioned with no gaps!");

    // Clean up
    window1.kill()?;
    window2.kill()?;

    Ok(())
}
