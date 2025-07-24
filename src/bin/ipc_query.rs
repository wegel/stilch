//! Query the real stilch IPC for workspace and window information

use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Querying stilch IPC for workspace information...\n");

    // Connect to the IPC socket
    let mut stream = UnixStream::connect("/tmp/stilch-ipc.sock")?;
    println!("Connected to IPC socket");

    // Subscribe to workspace updates
    let subscribe_msg = json!({
        "type": "subscribe",
        "events": ["workspace"]
    });

    let msg_str = serde_json::to_string(&subscribe_msg)?;
    writeln!(stream, "{msg_str}")?;
    println!("Sent subscription request");

    // Read responses
    let reader = BufReader::new(stream);
    println!("\nWaiting for workspace updates...\n");

    for line in reader.lines() {
        let line = line?;

        // Parse the JSON message
        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) {
            if msg["type"] == "workspace_update" {
                println!("Workspace Update:");
                println!("  Virtual Output: {}", msg["virtual_output"]);

                if let Some(workspaces) = msg["workspaces"].as_array() {
                    for ws in workspaces {
                        println!("  Workspace {}:", ws["id"]);
                        println!("    Active: {}", ws["active"]);
                        println!("    Windows: {}", ws["windows"]);
                        println!("    Urgent: {}", ws["urgent"]);
                    }
                }
                println!();
            } else {
                println!("Received: {}", serde_json::to_string_pretty(&msg)?);
            }
        } else {
            println!("Raw message: {line}");
        }
    }

    Ok(())
}
