mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_window_split_with_125_scale() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("window-split-125-scale");
    env.cleanup()?;

    // Start compositor with 1920x1080 physical display and 1.25 scale from config
    env.start_compositor(&[
        "--test",
        "--logical-size",
        "1920x1080", // Physical size
        "--config",
        "tests/test_configs/scale_125.conf",
    ])?;

    let client = TestClient::new(&env.test_socket);

    // Create first window
    let mut window1 = env.start_window("Window1", Some("blue"))?;
    client.wait_for_window_count(1, "after window 1")?;

    // First window should fill the entire workspace (minus gaps if any)
    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 1);
    let w1 = &windows[0];

    println!("Single window dimensions with 1.25 scale:");
    println!(
        "  Window 1: x={}, y={}, width={}, height={}",
        w1["x"], w1["y"], w1["width"], w1["height"]
    );

    let w1_width = w1["width"].as_i64().unwrap();
    let w1_height = w1["height"].as_i64().unwrap();

    // With 1.25 scale, logical size should be 1920/1.25 x 1080/1.25 = 1536x864
    let logical_width = 1536;
    let logical_height = 864;

    // Check that single window fills the logical space
    assert!(
        w1_width <= logical_width,
        "Window 1 width {w1_width} should not exceed logical width {logical_width}"
    );
    assert!(
        w1_height <= logical_height,
        "Window 1 height {w1_height} should not exceed logical height {logical_height}"
    );
    println!("  Logical space: {logical_width}x{logical_height}");

    // Create second window - should split horizontally by default
    let mut window2 = env.start_window("Window2", Some("red"))?;
    client.wait_for_window_count(2, "after window 2")?;

    let windows = client.get_windows()?;
    assert_eq!(windows.len(), 2);

    // Find windows by position (leftmost is window 1)
    let (w1_split, w2_split) =
        if windows[0]["x"].as_i64().unwrap() < windows[1]["x"].as_i64().unwrap() {
            (&windows[0], &windows[1])
        } else {
            (&windows[1], &windows[0])
        };

    println!("\nTwo window dimensions after split:");
    println!(
        "  Window 1: x={}, y={}, width={}, height={}",
        w1_split["x"], w1_split["y"], w1_split["width"], w1_split["height"]
    );
    println!(
        "  Window 2: x={}, y={}, width={}, height={}",
        w2_split["x"], w2_split["y"], w2_split["width"], w2_split["height"]
    );

    let w1_split_width = w1_split["width"].as_i64().unwrap();
    let w2_split_width = w2_split["width"].as_i64().unwrap();
    let w1_split_x = w1_split["x"].as_i64().unwrap();
    let w2_split_x = w2_split["x"].as_i64().unwrap();

    // Windows should be split evenly (approximately half of logical width each)
    let expected_width = logical_width / 2; // 768
    let tolerance = 50; // Allow some tolerance for gaps and rounding

    println!("\nExpected width per window: {expected_width} (±{tolerance})");
    println!("Actual widths: w1={w1_split_width}, w2={w2_split_width}");

    // Check that windows are approximately half width
    assert!(
        (w1_split_width - expected_width).abs() <= tolerance,
        "Window 1 width {w1_split_width} should be close to {expected_width}"
    );
    assert!(
        (w2_split_width - expected_width).abs() <= tolerance,
        "Window 2 width {w2_split_width} should be close to {expected_width}"
    );

    // Check that windows don't overlap
    assert!(
        w1_split_x + w1_split_width <= w2_split_x + 10, // Allow small gap
        "Windows should not overlap: w1 ends at {}, w2 starts at {}",
        w1_split_x + w1_split_width,
        w2_split_x
    );

    // Check that windows together span the logical width
    let total_width = w1_split_width + w2_split_width;
    println!("Total width covered: {total_width} (logical space: {logical_width})");
    assert!(
        total_width <= logical_width,
        "Combined width {total_width} should not exceed logical width {logical_width}"
    );

    // Windows should not exceed logical height
    let w1_split_height = w1_split["height"].as_i64().unwrap();
    let w2_split_height = w2_split["height"].as_i64().unwrap();
    assert!(
        w1_split_height <= logical_height,
        "Window 1 height {w1_split_height} should not exceed {logical_height}"
    );
    assert!(
        w2_split_height <= logical_height,
        "Window 2 height {w2_split_height} should not exceed {logical_height}"
    );

    // Check that the split is centered (not 2/3 to one side)
    let midpoint = logical_width / 2;
    let w2_start = w2_split_x;
    println!("\nMidpoint check: expected around {midpoint}, w2 starts at {w2_start}");
    assert!(
        (w2_start - midpoint).abs() <= tolerance,
        "Second window should start near midpoint {midpoint}, but starts at {w2_start}"
    );

    println!("\n✓ Windows split correctly with 1.25 scale factor!");

    // Clean up
    window1.kill()?;
    window2.kill()?;

    Ok(())
}
