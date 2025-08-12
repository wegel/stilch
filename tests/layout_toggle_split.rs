// Test for layout toggle split functionality
// Since the test IPC doesn't support SetLayout commands,
// this test verifies that the toggle_container_split method works correctly

use smithay::utils::Rectangle;
use stilch::window::WindowId;
use stilch::workspace::layout::{ContainerLayout, LayoutTree, SplitDirection};

#[test]
fn test_toggle_container_split_from_tabbed() {
    let workspace_rect = Rectangle::from_size((800, 600).into());
    let mut layout = LayoutTree::new(workspace_rect, 0);

    // Add three windows to the layout
    let window1 = WindowId::new(1);
    let window2 = WindowId::new(2);
    let window3 = WindowId::new(3);

    layout.add_window(window1, SplitDirection::Horizontal);
    layout.add_window(window2, SplitDirection::Horizontal);
    layout.add_window(window3, SplitDirection::Horizontal);

    // Calculate geometries
    layout.calculate_geometries();

    // Set the container to tabbed
    layout.set_container_layout(window1, ContainerLayout::Tabbed);
    layout.calculate_geometries();

    // Verify it's tabbed - only one window should be visible
    let visible = layout.get_visible_geometries();
    assert_eq!(
        visible.len(),
        1,
        "Only one window should be visible in tabbed layout"
    );

    // Toggle to split (should convert to horizontal split by default)
    layout.toggle_container_split(window1, SplitDirection::Horizontal);
    layout.calculate_geometries();

    // Now all windows should be visible
    let visible = layout.get_visible_geometries();
    assert_eq!(
        visible.len(),
        3,
        "All windows should be visible after toggling to split"
    );

    // Verify they're arranged horizontally
    let geometries: Vec<_> = visible.iter().map(|(_, geom)| geom).collect();
    assert_eq!(
        geometries[0].loc.y, geometries[1].loc.y,
        "Windows should be on same y coordinate"
    );
    assert_eq!(
        geometries[1].loc.y, geometries[2].loc.y,
        "Windows should be on same y coordinate"
    );
    assert_ne!(
        geometries[0].loc.x, geometries[1].loc.x,
        "Windows should have different x coordinates"
    );
}

#[test]
fn test_toggle_container_split_between_directions() {
    let workspace_rect = Rectangle::from_size((800, 600).into());
    let mut layout = LayoutTree::new(workspace_rect, 0);

    // Add two windows
    let window1 = WindowId::new(1);
    let window2 = WindowId::new(2);

    layout.add_window(window1, SplitDirection::Horizontal);
    layout.add_window(window2, SplitDirection::Horizontal);
    layout.calculate_geometries();

    // Initially they should be in split layout (horizontal by default)
    let visible = layout.get_visible_geometries();
    assert_eq!(visible.len(), 2, "Both windows should be visible");

    // Verify horizontal split
    let geometries: Vec<_> = visible.iter().map(|(_, geom)| geom).collect();
    assert_eq!(
        geometries[0].loc.y, geometries[1].loc.y,
        "Should be horizontal split initially"
    );

    // Toggle to vertical
    layout.toggle_container_split(window1, SplitDirection::Vertical);
    layout.calculate_geometries();

    let visible = layout.get_visible_geometries();
    let geometries: Vec<_> = visible.iter().map(|(_, geom)| geom).collect();
    assert_eq!(
        geometries[0].loc.x, geometries[1].loc.x,
        "Should be vertical split after toggle"
    );
    assert_ne!(
        geometries[0].loc.y, geometries[1].loc.y,
        "Should have different y coordinates"
    );

    // Toggle back to horizontal
    layout.toggle_container_split(window1, SplitDirection::Horizontal);
    layout.calculate_geometries();

    let visible = layout.get_visible_geometries();
    let geometries: Vec<_> = visible.iter().map(|(_, geom)| geom).collect();
    assert_eq!(
        geometries[0].loc.y, geometries[1].loc.y,
        "Should be back to horizontal split"
    );
    assert_ne!(
        geometries[0].loc.x, geometries[1].loc.x,
        "Should have different x coordinates"
    );
}

#[test]
fn test_toggle_container_split_from_stacked() {
    let workspace_rect = Rectangle::from_size((800, 600).into());
    let mut layout = LayoutTree::new(workspace_rect, 0);

    // Add three windows
    let window1 = WindowId::new(1);
    let window2 = WindowId::new(2);
    let window3 = WindowId::new(3);

    layout.add_window(window1, SplitDirection::Horizontal);
    layout.add_window(window2, SplitDirection::Horizontal);
    layout.add_window(window3, SplitDirection::Horizontal);
    layout.calculate_geometries();

    // Set to stacked layout
    layout.set_container_layout(window1, ContainerLayout::Stacked);
    layout.calculate_geometries();

    // Verify only one window is visible
    let visible = layout.get_visible_geometries();
    assert_eq!(
        visible.len(),
        1,
        "Only one window should be visible in stacked layout"
    );

    // Toggle to split (vertical)
    layout.toggle_container_split(window1, SplitDirection::Vertical);
    layout.calculate_geometries();

    // All windows should now be visible
    let visible = layout.get_visible_geometries();
    assert_eq!(
        visible.len(),
        3,
        "All windows should be visible after toggling to split"
    );

    // Verify they're arranged vertically
    let geometries: Vec<_> = visible.iter().map(|(_, geom)| geom).collect();
    assert_eq!(
        geometries[0].loc.x, geometries[1].loc.x,
        "Windows should be on same x coordinate"
    );
    assert_eq!(
        geometries[1].loc.x, geometries[2].loc.x,
        "Windows should be on same x coordinate"
    );
    assert_ne!(
        geometries[0].loc.y, geometries[1].loc.y,
        "Windows should have different y coordinates"
    );
}
