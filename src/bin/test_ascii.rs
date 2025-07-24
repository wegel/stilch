//! Query the test IPC for ASCII state

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use stilch::test_ipc::{TestCommand, TestResponse};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Querying test mode for ASCII state...\n");

    // Connect to the test IPC socket
    let mut stream = UnixStream::connect("/tmp/stilch-test.sock")?;
    println!("Connected to test IPC socket");

    // Send GetState command
    let cmd = TestCommand::GetState;
    let msg_str = serde_json::to_string(&cmd)?;
    writeln!(stream, "{msg_str}")?;
    println!("Sent GetState command");

    // Read response
    let reader = BufReader::new(stream.try_clone()?);
    for line in reader.lines() {
        let line = line?;

        // Parse the response
        if let Ok(response) = serde_json::from_str::<TestResponse>(&line) {
            match response {
                TestResponse::State { ascii } => {
                    println!("ASCII State:");
                    println!("{ascii}");
                    return Ok(());
                }
                TestResponse::Windows { windows } => {
                    println!("Windows:");
                    for w in windows {
                        println!(
                            "  Window {}: {}x{} at ({},{})",
                            w.id, w.width, w.height, w.x, w.y
                        );
                    }
                    return Ok(());
                }
                other => {
                    println!("Response: {other:?}");
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}
