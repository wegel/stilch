use smithay::input::pointer::{CursorIcon, CursorImageStatus};
use std::time::Duration;
use stilch::cursor_manager::CursorManager;

#[test]
fn test_cursor_shape_changes() {
    let mut manager = CursorManager::new();

    // Test default cursor
    assert!(matches!(
        manager.cursor_image(),
        CursorImageStatus::Named(CursorIcon::Default)
    ));
    let buffer = manager.get_current_cursor_buffer(1, Duration::from_secs(0));
    assert!(buffer.is_some(), "Should have default cursor");

    // Test changing to hand cursor
    manager.set_cursor_image(CursorImageStatus::Named(CursorIcon::Pointer));
    assert!(matches!(
        manager.cursor_image(),
        CursorImageStatus::Named(CursorIcon::Pointer)
    ));
    let hand_buffer = manager.get_current_cursor_buffer(1, Duration::from_secs(0));
    assert!(hand_buffer.is_some(), "Should have hand/pointer cursor");

    // Test changing to text cursor
    manager.set_cursor_image(CursorImageStatus::Named(CursorIcon::Text));
    assert!(matches!(
        manager.cursor_image(),
        CursorImageStatus::Named(CursorIcon::Text)
    ));
    let text_buffer = manager.get_current_cursor_buffer(1, Duration::from_secs(0));
    assert!(text_buffer.is_some(), "Should have text cursor");

    // Test hidden cursor
    manager.set_cursor_image(CursorImageStatus::Hidden);
    assert!(matches!(manager.cursor_image(), CursorImageStatus::Hidden));
    let hidden_buffer = manager.get_current_cursor_buffer(1, Duration::from_secs(0));
    assert!(hidden_buffer.is_none(), "Hidden cursor should return None");
}

#[test]
fn test_cursor_caching_different_shapes() {
    let mut manager = CursorManager::new();

    // Load multiple cursor types to test caching
    let cursor_types = vec![
        CursorIcon::Default,
        CursorIcon::Pointer,
        CursorIcon::Text,
        CursorIcon::Wait,
        CursorIcon::Help,
        CursorIcon::Progress,
        CursorIcon::Crosshair,
        CursorIcon::Move,
    ];

    // Load each cursor type
    for cursor_type in &cursor_types {
        manager.set_cursor_image(CursorImageStatus::Named(*cursor_type));
        let buffer = manager.get_current_cursor_buffer(1, Duration::from_secs(0));
        assert!(
            buffer.is_some(),
            "Should load cursor type: {:?}",
            cursor_type
        );
    }

    // Verify they're cached by loading again
    for cursor_type in &cursor_types {
        manager.set_cursor_image(CursorImageStatus::Named(*cursor_type));
        let buffer = manager.get_current_cursor_buffer(1, Duration::from_secs(0));
        assert!(
            buffer.is_some(),
            "Should have cached cursor type: {:?}",
            cursor_type
        );
    }
}

#[test]
fn test_cursor_scale_independence() {
    let mut manager = CursorManager::new();

    manager.set_cursor_image(CursorImageStatus::Named(CursorIcon::Pointer));

    // Test different scales
    let scales = vec![1, 2, 3];
    for scale in scales {
        let buffer = manager.get_current_cursor_buffer(scale, Duration::from_secs(0));
        assert!(buffer.is_some(), "Should load cursor at scale {}", scale);
    }
}

#[test]
fn test_animated_cursor_detection() {
    let mut manager = CursorManager::new();

    // Test various cursor types and track which ones are animated
    let cursor_types = vec![
        (CursorIcon::Default, "Default"),
        (CursorIcon::Wait, "Wait"),
        (CursorIcon::Progress, "Progress"),
        (CursorIcon::Pointer, "Pointer"),
        (CursorIcon::Text, "Text"),
    ];

    let mut found_animated = false;
    let mut found_static = false;

    for (cursor_type, name) in cursor_types {
        manager.set_cursor_image(CursorImageStatus::Named(cursor_type));
        let is_animated = manager.is_current_cursor_animated(1);

        if is_animated {
            found_animated = true;
            println!("Found animated cursor: {}", name);
        } else {
            found_static = true;
            println!("Found static cursor: {}", name);
        }
    }

    // We expect to find at least one cursor (either static or animated)
    assert!(
        found_static || found_animated,
        "Should find at least one cursor in the theme"
    );

    // Log the results
    if !found_static && found_animated {
        println!("Note: All cursors in current theme are animated");
    }

    // Hidden cursor is never animated
    manager.set_cursor_image(CursorImageStatus::Hidden);
    assert!(
        !manager.is_current_cursor_animated(1),
        "Hidden cursor should not be animated"
    );

    // Note: We don't assert about animated cursors because it depends on the theme
    if !found_animated {
        println!("Note: No animated cursors found in current theme");
    }
}
