use stilch::workspace::layout::{ContainerLayout, LayoutTree, SplitDirection};
use stilch::window::WindowId;
use smithay::utils::Rectangle;

#[test]
fn test_move_tab_left_in_tabbed_container() {
    let workspace_rect = Rectangle::from_size((800, 600).into());
    let mut layout = LayoutTree::new(workspace_rect, 0);

    // Create 3 windows in a tabbed container
    let win1 = WindowId::new(1);
    let win2 = WindowId::new(2);
    let win3 = WindowId::new(3);

    layout.add_window(win1, SplitDirection::Horizontal);
    layout.add_window(win2, SplitDirection::Horizontal);
    layout.add_window(win3, SplitDirection::Horizontal);

    // Change to tabbed layout
    layout.set_container_layout(win1, ContainerLayout::Tabbed);

    // Initial order should be win1, win2, win3
    let windows = layout.get_windows();
    assert_eq!(windows[0], win1);
    assert_eq!(windows[1], win2);
    assert_eq!(windows[2], win3);

    // Move win3 left (should swap with win2)
    assert!(layout.move_tab_left(win3));

    // New order should be win1, win3, win2
    let windows = layout.get_windows();
    assert_eq!(windows[0], win1);
    assert_eq!(windows[1], win3);
    assert_eq!(windows[2], win2);

    // Move win3 left again (should swap with win1)
    assert!(layout.move_tab_left(win3));

    // New order should be win3, win1, win2
    let windows = layout.get_windows();
    assert_eq!(windows[0], win3);
    assert_eq!(windows[1], win1);
    assert_eq!(windows[2], win2);

    // Try to move win3 left when it's already first (should fail)
    assert!(!layout.move_tab_left(win3));
}

#[test]
fn test_move_tab_right_in_tabbed_container() {
    let workspace_rect = Rectangle::from_size((800, 600).into());
    let mut layout = LayoutTree::new(workspace_rect, 0);

    // Create 3 windows in a tabbed container
    let win1 = WindowId::new(1);
    let win2 = WindowId::new(2);
    let win3 = WindowId::new(3);

    layout.add_window(win1, SplitDirection::Horizontal);
    layout.add_window(win2, SplitDirection::Horizontal);
    layout.add_window(win3, SplitDirection::Horizontal);

    // Change to tabbed layout
    layout.set_container_layout(win1, ContainerLayout::Tabbed);

    // Move win1 right (should swap with win2)
    assert!(layout.move_tab_right(win1));

    // New order should be win2, win1, win3
    let windows = layout.get_windows();
    assert_eq!(windows[0], win2);
    assert_eq!(windows[1], win1);
    assert_eq!(windows[2], win3);

    // Move win1 right again (should swap with win3)
    assert!(layout.move_tab_right(win1));

    // New order should be win2, win3, win1
    let windows = layout.get_windows();
    assert_eq!(windows[0], win2);
    assert_eq!(windows[1], win3);
    assert_eq!(windows[2], win1);

    // Try to move win1 right when it's already last (should fail)
    assert!(!layout.move_tab_right(win1));
}

#[test]
fn test_move_tab_in_stacked_container() {
    let workspace_rect = Rectangle::from_size((800, 600).into());
    let mut layout = LayoutTree::new(workspace_rect, 0);

    // Create 3 windows in a stacked container
    let win1 = WindowId::new(1);
    let win2 = WindowId::new(2);
    let win3 = WindowId::new(3);

    layout.add_window(win1, SplitDirection::Horizontal);
    layout.add_window(win2, SplitDirection::Horizontal);
    layout.add_window(win3, SplitDirection::Horizontal);

    // Change to stacked layout
    layout.set_container_layout(win1, ContainerLayout::Stacked);

    // Move win2 left (should swap with win1)
    assert!(layout.move_tab_left(win2));

    // New order should be win2, win1, win3
    let windows = layout.get_windows();
    assert_eq!(windows[0], win2);
    assert_eq!(windows[1], win1);
    assert_eq!(windows[2], win3);

    // Move win3 right when already last (should fail)
    assert!(!layout.move_tab_right(win3));
}

#[test]
fn test_move_tab_in_split_container_fails() {
    let workspace_rect = Rectangle::from_size((800, 600).into());
    let mut layout = LayoutTree::new(workspace_rect, 0);

    // Create 2 windows (default is split)
    let win1 = WindowId::new(1);
    let win2 = WindowId::new(2);

    layout.add_window(win1, SplitDirection::Horizontal);
    layout.add_window(win2, SplitDirection::Horizontal);

    // Moving tabs should fail in split containers
    assert!(!layout.move_tab_left(win2));
    assert!(!layout.move_tab_right(win1));
}