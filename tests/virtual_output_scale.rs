mod common;

use common::{TestClient, TestEnv};

#[test]
fn test_default_virtual_output_respects_scale() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("virtual-output-scale");
    env.cleanup()?;

    // Start compositor with 3840x2160 physical display and 2.0 scale
    env.start_compositor(&[
        "--test",
        "--logical-size",
        "3840x2160", // Physical size
        "--config",
        "tests/test_configs/scale_2.conf",
    ])?;

    let client = TestClient::new(&env.test_socket);

    // Get outputs to check virtual output dimensions
    let outputs = client.get_outputs()?;
    assert_eq!(outputs.len(), 1, "Should have one virtual output");

    let output = &outputs[0];
    println!("Virtual output with scale 2.0:");
    println!(
        "  x={}, y={}, width={}, height={}",
        output["x"], output["y"], output["width"], output["height"]
    );

    // With physical size 3840x2160 and scale 2.0,
    // logical size should be 1920x1080
    let expected_width: i32 = 1920;
    let expected_height: i32 = 1080;

    let actual_width = output["width"].as_i64().unwrap() as i32;
    let actual_height = output["height"].as_i64().unwrap() as i32;

    assert_eq!(
        actual_width, expected_width,
        "Virtual output width should be {expected_width} (3840/2.0) but got {actual_width}"
    );
    assert_eq!(
        actual_height, expected_height,
        "Virtual output height should be {expected_height} (2160/2.0) but got {actual_height}"
    );
    Ok(())
}

#[test]
fn test_default_virtual_output_with_fractional_scale() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnv::new("virtual-output-fractional-scale");
    env.cleanup()?;

    // Start compositor with 2880x1800 physical display and 1.5 scale
    env.start_compositor(&[
        "--test",
        "--logical-size",
        "2880x1800", // Physical size
        "--config",
        "tests/test_configs/scale_15.conf",
    ])?;

    let client = TestClient::new(&env.test_socket);

    // Get outputs to check virtual output dimensions
    let outputs = client.get_outputs()?;
    assert_eq!(outputs.len(), 1, "Should have one virtual output");

    let output = &outputs[0];
    println!("Virtual output with scale 1.5:");
    println!(
        "  x={}, y={}, width={}, height={}",
        output["x"], output["y"], output["width"], output["height"]
    );

    // With physical size 2880x1800 and scale 1.5,
    // logical size should be 1920x1200
    let expected_width = 1920;
    let expected_height = 1200;

    let actual_width = output["width"].as_i64().unwrap();
    let actual_height = output["height"].as_i64().unwrap();

    assert_eq!(
        actual_width, expected_width,
        "Virtual output width should be {expected_width} (2880/1.5) but got {actual_width}"
    );
    assert_eq!(
        actual_height, expected_height,
        "Virtual output height should be {expected_height} (1800/1.5) but got {actual_height}"
    );

    Ok(())
}
