#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::setup::*;
    use crate::tests::test_client::TestClient;
    use crate::backend::ascii::AsciiFrame;
    
    #[test]
    fn test_window_close_triggers_render() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .init();

        let (mut test_state, output, _event_rx, mut ascii_backend) = create_ascii_test_state(800, 600);
        
        // Create first window
        let mut client1 = TestClient::new("test_client_1");
        let _surface1 = client1.create_window();
        
        // Create second window
        let mut client2 = TestClient::new("test_client_2");
        let _surface2 = client2.create_window();
        
        // Process client events to create windows
        test_state.dispatch_clients(&mut client1).unwrap();
        test_state.dispatch_clients(&mut client2).unwrap();
        
        // Wait for windows to be mapped
        std::thread::sleep(std::time::Duration::from_millis(50));
        
        // Initial render to establish baseline
        test_state.render(&output, &mut ascii_backend);
        let initial_frame = ascii_backend.last_frame().clone();
        
        // Count windows in initial frame
        let initial_window_count = initial_frame.windows.len();
        assert_eq!(initial_window_count, 2, "Should have 2 windows initially");
        
        // Close the first window
        drop(client1);
        
        // Wait a bit for the close to process
        std::thread::sleep(std::time::Duration::from_millis(50));
        
        // Render again
        test_state.render(&output, &mut ascii_backend);
        let final_frame = ascii_backend.last_frame().clone();
        
        // Verify window was removed from the rendered output
        let final_window_count = final_frame.windows.len();
        assert_eq!(
            final_window_count, 1, 
            "Should have 1 window after closing. Initial: {}, Final: {}", 
            initial_window_count, final_window_count
        );
        
        // Verify the frame actually changed
        assert_ne!(
            initial_frame, final_frame,
            "Frame should have changed after window close"
        );
    }
}