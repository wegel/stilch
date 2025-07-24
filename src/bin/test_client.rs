//! Test client for the stilch compositor
//!
//! This client launches REAL applications that create REAL Wayland windows
//! and uses IPC to observe the ASCII state
//!
//! Usage: cargo run --bin test_client

use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;
use stilch::test_ipc::{TestCommand, TestIpcClient};

fn launch_terminal(title: &str) -> std::io::Result<Child> {
    // Try different terminal emulators in order of preference
    // Most will create Wayland windows when WAYLAND_DISPLAY is set

    // Try foot first (native Wayland terminal)
    if let Ok(child) = Command::new("foot")
        .arg("-T")
        .arg(title)
        .env("WAYLAND_DISPLAY", "wayland-1") // stilch's display
        .spawn()
    {
        return Ok(child);
    }

    // Try alacritty
    if let Ok(child) = Command::new("alacritty")
        .arg("--title")
        .arg(title)
        .env("WAYLAND_DISPLAY", "wayland-1")
        .spawn()
    {
        return Ok(child);
    }

    // Try weston-terminal
    if let Ok(child) = Command::new("weston-terminal")
        .arg("--title")
        .arg(title)
        .env("WAYLAND_DISPLAY", "wayland-1")
        .spawn()
    {
        return Ok(child);
    }

    // Try gnome-terminal
    Command::new("gnome-terminal")
        .arg("--title")
        .arg(title)
        .env("WAYLAND_DISPLAY", "wayland-1")
        .spawn()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Tunawm Test Client - Real Window Test");
    println!("======================================\n");

    // First check if compositor is running
    // Try the real IPC socket first, then fall back to test socket
    let socket_path = if PathBuf::from("/tmp/stilch-ipc.sock").exists() {
        println!("Using real IPC socket at /tmp/stilch-ipc.sock");
        PathBuf::from("/tmp/stilch-ipc.sock")
    } else {
        PathBuf::from("/tmp/stilch-test.sock")
    };
    println!("Checking if compositor is running at {socket_path:?}...");

    let mut ipc_client = match TestIpcClient::connect(&socket_path) {
        Ok(client) => {
            println!("Connected to compositor IPC!\n");
            client
        }
        Err(e) => {
            eprintln!("Error: Compositor not running or IPC not available: {e}");
            eprintln!("Please start the compositor with: cargo run --bin stilch -- --test");
            return Err(e.into());
        }
    };

    // Test 1: Get initial state (should be empty)
    println!("Test 1: Initial State (Empty)");
    println!("------------------------------");
    let state = ipc_client.get_state()?;
    println!("{state}");

    // Test 2: Launch REAL terminal applications
    println!("\nTest 2: Launching REAL Terminal Windows");
    println!("----------------------------------------");
    println!("Launching 3 terminal emulators that will create real Wayland windows...\n");

    // Launch first terminal
    println!("Launching terminal 1...");
    let mut term1 = launch_terminal("Test Terminal 1")?;
    thread::sleep(Duration::from_millis(500)); // Give it time to create window

    // Launch second terminal
    println!("Launching terminal 2...");
    let mut term2 = launch_terminal("Test Terminal 2")?;
    thread::sleep(Duration::from_millis(500));

    // Launch third terminal
    println!("Launching terminal 3...");
    let mut term3 = launch_terminal("Test Terminal 3")?;
    thread::sleep(Duration::from_millis(500));

    // Wait a bit for windows to be created and positioned
    println!("\nWaiting for windows to be created and positioned...");
    thread::sleep(Duration::from_secs(1));

    // Get ASCII state showing REAL windows
    println!("\nASCII State with 3 REAL windows:");
    println!("---------------------------------");
    let state = ipc_client.get_state()?;
    println!("{state}");

    // Get window information
    println!("\nWindow Information:");
    println!("-------------------");
    match ipc_client.send_command(TestCommand::GetWindows)? {
        stilch::test_ipc::TestResponse::Windows { windows } => {
            for w in windows {
                println!(
                    "Window {}: {}x{} at ({},{}), workspace: {}, focused: {}",
                    w.id, w.width, w.height, w.x, w.y, w.workspace, w.focused
                );
            }
        }
        _ => println!("Unexpected response"),
    }

    // Test 3: Move focus using IPC commands
    println!("\nTest 3: Focus Navigation");
    println!("-------------------------");
    println!("Moving focus right...");

    match ipc_client.send_command(TestCommand::MoveFocus {
        direction: stilch::test_ipc::Direction::Right,
    })? {
        stilch::test_ipc::TestResponse::Success { message } => println!("{message}"),
        _ => println!("Focus command not implemented yet"),
    }

    thread::sleep(Duration::from_millis(500));
    let state = ipc_client.get_state()?;
    println!("{state}");

    // Keep terminals alive for a bit to observe
    println!("\nKeeping windows open for 5 seconds to observe...");
    thread::sleep(Duration::from_secs(5));

    // Clean up - kill the terminals
    println!("\nCleaning up - closing terminals...");
    let _ = term1.kill();
    let _ = term2.kill();
    let _ = term3.kill();

    // Final state
    thread::sleep(Duration::from_millis(500));
    println!("\nFinal state after closing windows:");
    println!("-----------------------------------");
    let state = ipc_client.get_state()?;
    println!("{state}");

    println!("\nTest completed!");
    Ok(())
}
