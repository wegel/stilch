//! Layout tree management for tiling windows

use crate::window::{ContainerId, WindowId};
use smithay::utils::{Logical, Point, Rectangle, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// Normal tiling mode
    Tiling,
    /// Tabbed layout
    Tabbed,
    /// Stacked layout
    Stacked,
}

#[derive(Debug, Clone)]
pub enum LayoutNode {
    /// A window leaf node
    Window {
        id: WindowId,
        geometry: Rectangle<i32, Logical>,
    },
    /// A container with multiple children (like i3/sway)
    Container {
        id: ContainerId,
        layout: ContainerLayout,
        /// Non-empty list of children with guaranteed valid active index
        /// Using SafeChildren to ensure active_child is always valid
        children: SafeChildren,
        geometry: Rectangle<i32, Logical>,
    },
}

/// Safe container for children that guarantees active_child is always valid
#[derive(Debug, Clone)]
pub struct SafeChildren {
    /// Children before the active one
    before: Vec<LayoutNode>,
    /// The active child (always exists)
    active: Box<LayoutNode>,
    /// Children after the active one
    after: Vec<LayoutNode>,
}

impl SafeChildren {
    /// Create with a single child (which becomes active)
    pub fn single(child: LayoutNode) -> Self {
        SafeChildren {
            before: Vec::new(),
            active: Box::new(child),
            after: Vec::new(),
        }
    }

    /// Create from a vec with specified active index
    pub fn from_vec(mut children: Vec<LayoutNode>, active_index: usize) -> Option<Self> {
        if children.is_empty() {
            return None;
        }

        if active_index >= children.len() {
            return None;
        }

        let active = Box::new(children.remove(active_index));
        let (before, after) = children.split_at(active_index);

        Some(SafeChildren {
            before: before.to_vec(),
            active,
            after: after.to_vec(),
        })
    }

    /// Get the total number of children
    pub fn len(&self) -> usize {
        self.before.len() + 1 + self.after.len()
    }

    /// Get the active child
    pub fn active(&self) -> &LayoutNode {
        &self.active
    }

    /// Get the active child mutably
    pub fn active_mut(&mut self) -> &mut LayoutNode {
        &mut self.active
    }

    /// Get the active index
    pub fn active_index(&self) -> usize {
        self.before.len()
    }

    /// Get all children as a vec
    pub fn to_vec(&self) -> Vec<LayoutNode> {
        let mut result = self.before.clone();
        result.push(*self.active.clone());
        result.extend(self.after.clone());
        result
    }

    /// Iterate over all children
    pub fn iter(&self) -> impl Iterator<Item = &LayoutNode> {
        self.before
            .iter()
            .chain(std::iter::once(self.active.as_ref()))
            .chain(self.after.iter())
    }

    /// Iterate over all children mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut LayoutNode> {
        self.before
            .iter_mut()
            .chain(std::iter::once(self.active.as_mut()))
            .chain(self.after.iter_mut())
    }

    /// Get child at index
    pub fn get(&self, index: usize) -> Option<&LayoutNode> {
        if index < self.before.len() {
            self.before.get(index)
        } else if index == self.before.len() {
            Some(&self.active)
        } else {
            self.after.get(index - self.before.len() - 1)
        }
    }

    /// Set active by index (returns false if index invalid)
    pub fn set_active(&mut self, index: usize) -> bool {
        if index >= self.len() {
            return false;
        }

        if index == self.active_index() {
            return true; // Already active
        }

        // Rebuild the structure with new active index
        let mut all = self.to_vec();

        // Debug: log what we're switching to
        if let Some(LayoutNode::Window { id, .. }) = all.get(index) {
            tracing::info!(
                "SafeChildren::set_active({}) - switching to window {}",
                index,
                id
            );
        }

        self.active = Box::new(all.remove(index));
        let (before, after) = all.split_at(index);
        self.before = before.to_vec();
        self.after = after.to_vec();

        // Debug: log the new structure
        tracing::info!(
            "SafeChildren after set_active: before={}, active={}, after={}",
            self.before.len(),
            if let LayoutNode::Window { id, .. } = &*self.active {
                format!("Window {id}")
            } else {
                "Container".to_string()
            },
            self.after.len()
        );

        true
    }

    /// Add a child (becomes new active)
    pub fn push(&mut self, child: LayoutNode) {
        // Move current active and all after elements to before, new child becomes active
        let old_active = std::mem::replace(&mut self.active, Box::new(child));
        self.before.push(*old_active);
        self.before.extend(self.after.drain(..));
    }

    /// Remove child by predicate, returns removed child
    pub fn remove<F>(&mut self, predicate: F) -> Option<LayoutNode>
    where
        F: Fn(&LayoutNode) -> bool,
    {
        // Check active first
        if predicate(&self.active) {
            // Need to replace active with another child
            if !self.after.is_empty() {
                let removed = std::mem::replace(&mut self.active, Box::new(self.after.remove(0)));
                return Some(*removed);
            } else if !self.before.is_empty() {
                // Safe because we just checked it's not empty
                let last = self
                    .before
                    .pop()
                    // SAFETY: We just checked that before is not empty
                    .expect("before was not empty but pop failed");
                let removed = std::mem::replace(&mut self.active, Box::new(last));
                return Some(*removed);
            } else {
                // This would leave us empty - caller must handle
                return None;
            }
        }

        // Check before
        if let Some(pos) = self.before.iter().position(&predicate) {
            return Some(self.before.remove(pos));
        }

        // Check after
        if let Some(pos) = self.after.iter().position(&predicate) {
            return Some(self.after.remove(pos));
        }

        None
    }

    /// Swap two children by their indices
    pub fn swap(&mut self, index1: usize, index2: usize) -> bool {
        if index1 >= self.len() || index2 >= self.len() || index1 == index2 {
            return false;
        }

        // Convert to vec, swap, and rebuild
        let mut all = self.to_vec();
        all.swap(index1, index2);

        // Keep active pointing to the same element
        let active_idx = self.active_index();
        let new_active_idx = if active_idx == index1 {
            index2
        } else if active_idx == index2 {
            index1
        } else {
            active_idx
        };

        // Rebuild SafeChildren with new order
        if let Some(new_children) = SafeChildren::from_vec(all, new_active_idx) {
            *self = new_children;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerLayout {
    /// Children are arranged horizontally
    Horizontal,
    /// Children are arranged vertically  
    Vertical,
    /// Children are shown as tabs
    Tabbed,
    /// Children are stacked
    Stacked,
}

/// The layout tree for a workspace
#[derive(Debug)]
pub struct LayoutTree {
    root: Option<LayoutNode>,
    area: Rectangle<i32, Logical>,
    gap: i32,
}

impl LayoutTree {
    /// Create a new empty layout tree
    pub fn new(area: Rectangle<i32, Logical>, gap: i32) -> Self {
        Self {
            root: None,
            area,
            gap,
        }
    }

    /// Update the area for this layout
    pub fn set_area(&mut self, area: Rectangle<i32, Logical>) {
        self.area = area;
    }

    /// Add a window to the layout with specific split direction
    pub fn add_window(&mut self, window_id: WindowId, split_direction: SplitDirection) {
        if self.root.is_none() {
            // First window becomes the root
            tracing::debug!(
                "Adding first window {} to layout, geometry: {:?}",
                window_id,
                self.area
            );
            self.root = Some(LayoutNode::Window {
                id: window_id,
                geometry: self.area,
            });
        } else {
            // Use the provided split direction
            tracing::debug!(
                "Adding window {} to existing layout with split {:?}",
                window_id,
                split_direction
            );
            if let Some(mut root) = self.root.take() {
                self.add_to_node(&mut root, window_id, split_direction);
                self.root = Some(root);
            } else {
                tracing::error!("Attempted to add window to non-existent root");
                // Create a new root with just this window
                self.root = Some(LayoutNode::Window {
                    id: window_id,
                    geometry: self.area,
                });
            }
        }

        // Always recalculate geometries after adding
        self.calculate_geometries();
    }

    /// Remove a window from the layout
    pub fn remove_window(&mut self, window_id: WindowId) {
        let root = self.root.take();
        self.root = self.remove_window_recursive(root, window_id);
    }

    /// Find the next window to focus after removing a window
    pub fn find_next_focus(&self) -> Option<WindowId> {
        // For tabbed containers, focus the active child, not just the first window
        self.find_active_window(&self.root)
    }

    /// Calculate geometries for all nodes
    pub fn calculate_geometries(&mut self) {
        let area = self.area;
        let gap = self.gap;
        if let Some(root) = &mut self.root {
            Self::calculate_node_geometry_static(root, area, gap);
        }
    }

    /// Get the geometry for a specific window
    pub fn get_window_geometry(&self, window_id: WindowId) -> Option<Rectangle<i32, Logical>> {
        self.find_window_geometry(&self.root, window_id)
    }

    /// Get all window geometries (including hidden tabs)
    pub fn get_all_geometries(&self) -> Vec<(WindowId, Rectangle<i32, Logical>)> {
        let mut geometries = Vec::new();
        self.collect_all_geometries(&self.root, &mut geometries);
        geometries
    }

    /// Get only visible window geometries
    pub fn get_visible_geometries(&self) -> Vec<(WindowId, Rectangle<i32, Logical>)> {
        let mut geometries = Vec::new();
        self.collect_geometries(&self.root, &mut geometries);
        geometries
    }

    /// Set the layout mode for the container containing the given window
    pub fn set_container_layout(&mut self, window_id: WindowId, new_layout: ContainerLayout) {
        tracing::info!(
            "set_container_layout called for window {} with layout {:?}",
            window_id,
            new_layout
        );

        // Special case: if root is a single window and we're switching to tabbed/stacked,
        // we need to wrap all workspace windows in a container
        if matches!(
            new_layout,
            ContainerLayout::Tabbed | ContainerLayout::Stacked
        ) {
            if let Some(LayoutNode::Window { .. }) = &self.root {
                tracing::info!(
                    "Root is a single window, need to create container for tabbed/stacked mode"
                );
                // This shouldn't happen if we have 3 windows, but log it
            }
        }

        if let Some(root) = &mut self.root {
            Self::set_container_layout_recursive(root, window_id, new_layout);
        }
    }

    /// Toggle between horizontal and vertical split for a container
    /// If the container is tabbed/stacked, convert it to split using the preferred direction
    pub fn toggle_container_split(&mut self, window_id: WindowId, preferred_split: SplitDirection) {
        if let Some(root) = &mut self.root {
            Self::toggle_container_split_recursive(root, window_id, preferred_split);
        }
    }

    /// Switch to the next tab in a tabbed container
    /// Returns true if we should escape the container (at last tab going forward)
    pub fn next_tab(&mut self, window_id: WindowId) -> bool {
        if let Some(root) = &mut self.root {
            Self::switch_tab_recursive(root, window_id, true)
        } else {
            false
        }
    }

    /// Switch to the previous tab in a tabbed container
    /// Returns true if we should escape the container (at first tab going backward)
    pub fn prev_tab(&mut self, window_id: WindowId) -> bool {
        if let Some(root) = &mut self.root {
            Self::switch_tab_recursive(root, window_id, false)
        } else {
            false
        }
    }

    /// Check if a window is in a tabbed container
    pub fn is_window_in_tabbed_container(&self, window_id: WindowId) -> bool {
        Self::check_window_in_tabbed_container(&self.root, window_id)
    }

    /// Find all tabbed containers and their windows
    pub fn find_tabbed_containers(&self) -> Vec<(Rectangle<i32, Logical>, Vec<(WindowId, bool)>)> {
        let mut containers = Vec::new();
        Self::find_tabbed_containers_recursive(&self.root, &mut containers);
        containers
    }
    
    /// Find all stacked containers and their windows
    pub fn find_stacked_containers(&self) -> Vec<(Rectangle<i32, Logical>, Vec<(WindowId, bool)>)> {
        let mut containers = Vec::new();
        Self::find_stacked_containers_recursive(&self.root, &mut containers);
        containers
    }

    /// Move a window in the given direction
    pub fn move_window(
        &mut self,
        window_id: WindowId,
        direction: crate::config::Direction,
    ) -> bool {
        // Find all windows and their positions
        let mut windows = Vec::new();
        self.collect_windows(&self.root, &mut windows);

        tracing::debug!("move_window: Found {} windows in layout", windows.len());
        for (id, geo) in &windows {
            tracing::debug!("  Window {}: loc={:?}, size={:?}", id, geo.loc, geo.size);
        }

        // Find the source window
        let source_idx = match windows.iter().position(|(id, _)| *id == window_id) {
            Some(idx) => idx,
            None => {
                tracing::warn!("move_window: Window {} not found in layout", window_id);
                return false;
            }
        };
        let source_pos = windows[source_idx].1.loc;
        tracing::debug!(
            "move_window: Source window {} at position {:?}",
            window_id,
            source_pos
        );

        // Find the best target window in the given direction
        let mut best_target = None;
        let mut best_distance = i32::MAX;

        for (idx, (_, geometry)) in windows.iter().enumerate() {
            if idx == source_idx {
                continue;
            }

            let target_pos = geometry.loc;
            let is_in_direction = match direction {
                crate::config::Direction::Left => {
                    target_pos.x < source_pos.x
                        && (target_pos.y..target_pos.y + geometry.size.h).contains(&source_pos.y)
                }
                crate::config::Direction::Right => {
                    target_pos.x > source_pos.x
                        && (target_pos.y..target_pos.y + geometry.size.h).contains(&source_pos.y)
                }
                crate::config::Direction::Up => {
                    target_pos.y < source_pos.y
                        && (target_pos.x..target_pos.x + geometry.size.w).contains(&source_pos.x)
                }
                crate::config::Direction::Down => {
                    target_pos.y > source_pos.y
                        && (target_pos.x..target_pos.x + geometry.size.w).contains(&source_pos.x)
                }
            };

            tracing::debug!(
                "move_window: Checking window at {:?} - is_in_direction: {}",
                target_pos,
                is_in_direction
            );

            if is_in_direction {
                let distance = match direction {
                    crate::config::Direction::Left | crate::config::Direction::Right => {
                        (target_pos.x - source_pos.x).abs()
                    }
                    crate::config::Direction::Up | crate::config::Direction::Down => {
                        (target_pos.y - source_pos.y).abs()
                    }
                };

                if distance < best_distance {
                    best_distance = distance;
                    best_target = Some(idx);
                }
            }
        }

        // If we found a target, swap the windows
        if let Some(target_idx) = best_target {
            let source_id = windows[source_idx].0;
            let target_id = windows[target_idx].0;

            tracing::debug!(
                "move_window: Swapping window {} with window {}",
                source_id,
                target_id
            );

            // Swap windows in the tree
            let mut root = self.root.clone();
            tracing::debug!("move_window: Before swap_windows_in_tree");
            if self.swap_windows_in_tree(&mut root, source_id, target_id) {
                tracing::debug!("move_window: Swap successful, updating root and recalculating");
                self.root = root;
                self.calculate_geometries();
            } else {
                tracing::warn!("move_window: swap_windows_in_tree returned false");
            }

            // Debug: print new geometries
            let mut new_windows = Vec::new();
            self.collect_windows(&self.root, &mut new_windows);
            tracing::debug!("move_window: After swap, new positions:");
            for (id, geo) in &new_windows {
                tracing::debug!("  Window {}: loc={:?}, size={:?}", id, geo.loc, geo.size);
            }

            true
        } else {
            tracing::warn!(
                "move_window: No suitable target window found in direction {:?}",
                direction
            );
            false
        }
    }

    // Helper methods

    fn add_to_node(
        &mut self,
        node: &mut LayoutNode,
        window_id: WindowId,
        direction: SplitDirection,
    ) {
        match node {
            LayoutNode::Window { id, geometry } => {
                // Convert window to container with two children
                let old_id = *id;
                let old_geometry = *geometry;

                let layout = match direction {
                    SplitDirection::Horizontal => ContainerLayout::Horizontal,
                    SplitDirection::Vertical => ContainerLayout::Vertical,
                };

                let mut new_children = SafeChildren::single(LayoutNode::Window {
                    id: old_id,
                    geometry: old_geometry,
                });
                new_children.push(LayoutNode::Window {
                    id: window_id,
                    geometry: old_geometry,
                });

                *node = LayoutNode::Container {
                    id: ContainerId::next(),
                    layout,
                    children: new_children,
                    geometry: old_geometry,
                };
            }
            LayoutNode::Container {
                layout, children, ..
            } => {
                // i3/sway behavior: if layout matches direction, add as sibling
                let container_direction = match layout {
                    ContainerLayout::Horizontal => SplitDirection::Horizontal,
                    ContainerLayout::Vertical => SplitDirection::Vertical,
                    ContainerLayout::Tabbed | ContainerLayout::Stacked => {
                        // For tabbed/stacked, just add the window
                        children.push(LayoutNode::Window {
                            id: window_id,
                            geometry: Rectangle::default(),
                        });
                        // New window becomes active automatically with push()
                        tracing::info!(
                            "Added window {} to tabbed container, set as active tab",
                            window_id
                        );
                        return;
                    }
                };

                if container_direction == direction {
                    // Same direction - add as sibling
                    children.push(LayoutNode::Window {
                        id: window_id,
                        geometry: Rectangle::default(),
                    });
                } else {
                    // Different direction - replace this container with a new split
                    let old_container = node.clone();
                    let new_layout = match direction {
                        SplitDirection::Horizontal => ContainerLayout::Horizontal,
                        SplitDirection::Vertical => ContainerLayout::Vertical,
                    };

                    let mut new_children = SafeChildren::single(old_container);
                    new_children.push(LayoutNode::Window {
                        id: window_id,
                        geometry: Rectangle::default(),
                    });

                    *node = LayoutNode::Container {
                        id: ContainerId::next(),
                        layout: new_layout,
                        children: new_children,
                        geometry: Rectangle::default(),
                    };
                }
            }
        }
    }

    fn remove_window_recursive(
        &mut self,
        node: Option<LayoutNode>,
        window_id: WindowId,
    ) -> Option<LayoutNode> {
        match node {
            Some(LayoutNode::Window { id, .. }) => {
                if id == window_id {
                    None
                } else {
                    Some(LayoutNode::Window {
                        id,
                        geometry: self.area,
                    })
                }
            }
            Some(LayoutNode::Container {
                id,
                layout,
                mut children,
                geometry,
            }) => {
                // Check if we're about to remove the last child
                let is_last_child =
                    children.len() == 1 && Self::node_contains_window(children.active(), window_id);

                if is_last_child {
                    // Removing the last child means removing the container
                    None
                } else {
                    // Try to remove the window from children
                    // Note: SafeChildren::remove returns None if removing would leave it empty
                    let removed =
                        children.remove(|child| Self::node_contains_window(child, window_id));

                    if removed.is_some() {
                        // Successfully removed a child, container continues to exist
                        Some(LayoutNode::Container {
                            id,
                            layout,
                            children,
                            geometry,
                        })
                    } else if children
                        .iter()
                        .any(|child| Self::node_contains_window(child, window_id))
                    {
                        // The window is in the children but SafeChildren::remove returned None
                        // This means removing it would leave the container empty
                        None
                    } else {
                        // Try to remove recursively from each child
                        let mut any_removed = false;
                        for child in children.iter_mut() {
                            if let Some(new_child) =
                                self.remove_window_recursive(Some(child.clone()), window_id)
                            {
                                *child = new_child;
                                any_removed = true;
                            } else if Self::node_contains_window(child, window_id) {
                                // This child was completely removed (returned None)
                                // We need to handle this case
                                any_removed = true;
                            }
                        }

                        if any_removed {
                            // Rebuild children, filtering out any that became None
                            let mut remaining = Vec::new();
                            for child in children.iter() {
                                remaining.push(child.clone());
                            }

                            // Try to recreate SafeChildren with active at index 0
                            if let Some(new_children) = SafeChildren::from_vec(remaining, 0) {
                                Some(LayoutNode::Container {
                                    id,
                                    layout,
                                    children: new_children,
                                    geometry,
                                })
                            } else {
                                None
                            }
                        } else {
                            Some(LayoutNode::Container {
                                id,
                                layout,
                                children,
                                geometry,
                            })
                        }
                    }
                }
            }
            None => None,
        }
    }

    fn find_first_window(&self, node: &Option<LayoutNode>) -> Option<WindowId> {
        match node {
            Some(LayoutNode::Window { id, .. }) => Some(*id),
            Some(LayoutNode::Container { children, .. }) => {
                for child in children.iter() {
                    if let Some(id) = self.find_first_window(&Some(child.clone())) {
                        return Some(id);
                    }
                }
                None
            }
            None => None,
        }
    }

    /// Update active_child to match the given window if it's in a tabbed container
    pub fn update_active_child_for_window(&mut self, window_id: WindowId) {
        if let Some(root) = &mut self.root {
            Self::update_active_child_recursive(root, window_id);
        }
    }

    fn update_active_child_recursive(node: &mut LayoutNode, target_window: WindowId) -> bool {
        match node {
            LayoutNode::Window { id, .. } => *id == target_window,
            LayoutNode::Container {
                layout, children, ..
            } => {
                // Check if any child contains the target window
                let mut found_index = None;
                for (i, child) in children.iter_mut().enumerate() {
                    if Self::update_active_child_recursive(child, target_window) {
                        found_index = Some(i);
                        break;
                    }
                }

                if let Some(i) = found_index {
                    // This child contains the target window
                    // If we're a tabbed/stacked container, update active_child
                    match layout {
                        ContainerLayout::Tabbed | ContainerLayout::Stacked => {
                            if children.active_index() != i {
                                tracing::info!(
                                    "Updating active_child from {} to {} for focused window {}",
                                    children.active_index(),
                                    i,
                                    target_window
                                );
                                children.set_active(i);
                            }
                        }
                        _ => {}
                    }
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Find the active window in the tree, respecting active_child for tabbed containers
    fn find_active_window(&self, node: &Option<LayoutNode>) -> Option<WindowId> {
        match node {
            Some(LayoutNode::Window { id, .. }) => Some(*id),
            Some(LayoutNode::Container {
                layout, children, ..
            }) => {
                // For tabbed/stacked containers, focus the active child
                match layout {
                    ContainerLayout::Tabbed | ContainerLayout::Stacked => {
                        // SafeChildren guarantees active is always valid
                        self.find_active_window(&Some(children.active().clone()))
                    }
                    _ => {
                        // For other layouts, just find the first window
                        self.find_first_window(node)
                    }
                }
            }
            None => None,
        }
    }

    fn calculate_node_geometry_static(
        node: &mut LayoutNode,
        available: Rectangle<i32, Logical>,
        gap: i32,
    ) {
        match node {
            LayoutNode::Window { geometry, .. } => {
                *geometry = available;
            }
            LayoutNode::Container {
                layout,
                children,
                geometry,
                ..
            } => {
                *geometry = available;

                match layout {
                    ContainerLayout::Horizontal => {
                        let count = children.len() as i32;
                        let total_gap_space = gap * (count - 1);
                        let width = (available.size.w - total_gap_space) / count;

                        for (i, child) in children.iter_mut().enumerate() {
                            let x = available.loc.x + (width + gap) * i as i32;
                            let child_rect = Rectangle::new(
                                (x, available.loc.y).into(),
                                (width, available.size.h).into(),
                            );
                            Self::calculate_node_geometry_static(child, child_rect, gap);
                        }
                    }
                    ContainerLayout::Vertical => {
                        let count = children.len() as i32;
                        let total_gap_space = gap * (count - 1);
                        let height = (available.size.h - total_gap_space) / count;

                        for (i, child) in children.iter_mut().enumerate() {
                            let y = available.loc.y + (height + gap) * i as i32;
                            let child_rect = Rectangle::new(
                                (available.loc.x, y).into(),
                                (available.size.w, height).into(),
                            );
                            Self::calculate_node_geometry_static(child, child_rect, gap);
                        }
                    }
                    ContainerLayout::Tabbed => {
                        // For tabbed, reserve space for tab bar at top
                        let tab_bar_height = crate::tab_bar::TAB_BAR_HEIGHT;
                        let client_area = Rectangle::new(
                            (available.loc.x, available.loc.y + tab_bar_height).into(),
                            (available.size.w, available.size.h - tab_bar_height).into(),
                        );
                        // All children get the client area (below tab bar)
                        for child in children.iter_mut() {
                            Self::calculate_node_geometry_static(child, client_area, gap);
                        }
                    }
                    ContainerLayout::Stacked => {
                        // For stacked, reserve space for title bars - one for each window
                        let num_children = children.len();
                        let title_bar_height = crate::tab_bar::TAB_BAR_HEIGHT;
                        let total_title_height = title_bar_height * num_children as i32;
                        
                        // Calculate the client area (below all stacked title bars)
                        let client_area = Rectangle::new(
                            Point::from((available.loc.x, available.loc.y + total_title_height)),
                            Size::from((available.size.w, available.size.h - total_title_height)),
                        );
                        
                        // All children get the client area (below title bars)
                        for child in children.iter_mut() {
                            Self::calculate_node_geometry_static(child, client_area, gap);
                        }
                    }
                }
            }
        }
    }

    fn find_window_geometry(
        &self,
        node: &Option<LayoutNode>,
        window_id: WindowId,
    ) -> Option<Rectangle<i32, Logical>> {
        match node {
            Some(LayoutNode::Window { id, geometry }) => {
                if *id == window_id {
                    Some(*geometry)
                } else {
                    None
                }
            }
            Some(LayoutNode::Container { children, .. }) => {
                for child in children.iter() {
                    if let Some(geometry) =
                        self.find_window_geometry(&Some(child.clone()), window_id)
                    {
                        return Some(geometry);
                    }
                }
                None
            }
            None => None,
        }
    }

    fn collect_all_geometries(
        &self,
        node: &Option<LayoutNode>,
        geometries: &mut Vec<(WindowId, Rectangle<i32, Logical>)>,
    ) {
        // Collect ALL window geometries, including hidden tabs
        match node {
            Some(LayoutNode::Window { id, geometry }) => {
                geometries.push((*id, *geometry));
            }
            Some(LayoutNode::Container { children, .. }) => {
                // For ALL layouts, collect all children
                for child in children.iter() {
                    self.collect_all_geometries(&Some(child.clone()), geometries);
                }
            }
            None => {}
        }
    }

    fn collect_geometries(
        &self,
        node: &Option<LayoutNode>,
        geometries: &mut Vec<(WindowId, Rectangle<i32, Logical>)>,
    ) {
        match node {
            Some(LayoutNode::Window { id, geometry }) => {
                geometries.push((*id, *geometry));
            }
            Some(LayoutNode::Container {
                children, layout, ..
            }) => {
                // For tabbed/stacked layouts, only collect visible window
                match layout {
                    ContainerLayout::Tabbed | ContainerLayout::Stacked => {
                        // Only show the active child for tabbed/stacked containers
                        self.collect_geometries(&Some(children.active().clone()), geometries);
                    }
                    _ => {
                        for child in children.iter() {
                            self.collect_geometries(&Some(child.clone()), geometries);
                        }
                    }
                }
            }
            None => {}
        }
    }

    fn collect_windows(
        &self,
        node: &Option<LayoutNode>,
        windows: &mut Vec<(WindowId, Rectangle<i32, Logical>)>,
    ) {
        self.collect_geometries(node, windows);
    }

    fn swap_windows_in_tree(
        &mut self,
        node: &mut Option<LayoutNode>,
        id1: WindowId,
        id2: WindowId,
    ) -> bool {
        match node {
            Some(LayoutNode::Window { .. }) => {
                // Can't swap within a single window node
                false
            }
            Some(LayoutNode::Container { children, .. }) => {
                // Find positions of both windows
                let mut pos1 = None;
                let mut pos2 = None;

                for (i, child) in children.iter().enumerate() {
                    if self.contains_window(&Some(child.clone()), id1) {
                        pos1 = Some(i);
                    }
                    if self.contains_window(&Some(child.clone()), id2) {
                        pos2 = Some(i);
                    }
                }

                if let (Some(p1), Some(p2)) = (pos1, pos2) {
                    if p1 != p2 {
                        // Windows are in different children - swap them
                        children.swap(p1, p2)
                    } else {
                        // Both windows in same child - recurse
                        if let Some(child) = children.get(p1) {
                            let mut child_node = Some(child.clone());
                            if self.swap_windows_in_tree(&mut child_node, id1, id2) {
                                if let Some(updated_child) = child_node {
                                    // Need to update the child at position p1
                                    // This is complex with SafeChildren, need to rebuild
                                    let mut all = children.to_vec();
                                    all[p1] = updated_child;
                                    if let Some(new_children) =
                                        SafeChildren::from_vec(all, children.active_index())
                                    {
                                        *children = new_children;
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    }
                } else {
                    // Try recursing into children
                    for child in children.iter_mut() {
                        let mut child_node = Some(child.clone());
                        if self.swap_windows_in_tree(&mut child_node, id1, id2) {
                            if let Some(updated_child) = child_node {
                                *child = updated_child;
                                return true;
                            }
                        }
                    }
                    false
                }
            }
            None => false,
        }
    }

    fn contains_window(&self, node: &Option<LayoutNode>, window_id: WindowId) -> bool {
        match node {
            Some(LayoutNode::Window { id, .. }) => *id == window_id,
            Some(LayoutNode::Container { children, .. }) => children
                .iter()
                .any(|child| self.contains_window(&Some(child.clone()), window_id)),
            None => false,
        }
    }

    fn reset_node_geometry(node: &mut LayoutNode, new_geometry: Rectangle<i32, Logical>) {
        match node {
            LayoutNode::Window { geometry, .. } => {
                *geometry = new_geometry;
            }
            LayoutNode::Container {
                geometry, children, ..
            } => {
                *geometry = new_geometry;
                // Recursively reset children
                for child in children.iter_mut() {
                    Self::reset_node_geometry(child, new_geometry);
                }
            }
        }
    }

    fn set_container_layout_recursive(
        node: &mut LayoutNode,
        window_id: WindowId,
        new_layout: ContainerLayout,
    ) {
        match node {
            LayoutNode::Window { id, .. } => {
                // Can't change layout of a window node directly
                // TODO: Could wrap this window in a container with the new layout
                if *id == window_id {
                    tracing::warn!("Cannot set layout on a window node directly");
                }
            }
            LayoutNode::Container {
                layout,
                children,
                geometry,
                ..
            } => {
                // Check if this container contains the window
                let contains_window = children
                    .iter()
                    .any(|child| Self::node_contains_window(child, window_id));

                if contains_window {
                    // If this container directly contains the window, change its layout
                    let old_layout = *layout;
                    *layout = new_layout;
                    tracing::info!(
                        "Changed container layout from {:?} to {:?}",
                        old_layout,
                        new_layout
                    );

                    // When switching to tabbed/stacked mode, reset all child geometries
                    // to the full container size so they properly overlap
                    if matches!(
                        new_layout,
                        ContainerLayout::Tabbed | ContainerLayout::Stacked
                    ) {
                        // Find which child contains the focused window and make it active
                        tracing::info!(
                            "Looking for focused window {} in {} children",
                            window_id,
                            children.len()
                        );

                        // First, find the index of the child that contains the focused window
                        let mut focused_child_index = 0;
                        for (i, child) in children.iter().enumerate() {
                            // Print what's in each child
                            match child {
                                LayoutNode::Window { .. } => {}
                                LayoutNode::Container { .. } => {}
                            }

                            let contains = Self::node_contains_window(child, window_id);
                            tracing::info!(
                                "  Child {}: contains window {}? {}",
                                i,
                                window_id,
                                contains
                            );
                            if contains {
                                focused_child_index = i;
                            }
                        }

                        // Set the active child
                        children.set_active(focused_child_index);
                        tracing::info!("Set active_child to {focused_child_index}");

                        // Now reset all geometries
                        for child in children.iter_mut() {
                            Self::reset_node_geometry(child, *geometry);
                        }
                        tracing::info!("Reset child geometries for tabbed/stacked layout");
                    }
                } else {
                    // Otherwise recurse into children
                    for child in children.iter_mut() {
                        Self::set_container_layout_recursive(child, window_id, new_layout);
                    }
                }
            }
        }
    }

    fn toggle_container_split_recursive(
        node: &mut LayoutNode,
        window_id: WindowId,
        preferred_split: SplitDirection,
    ) {
        match node {
            LayoutNode::Window { .. } => {}
            LayoutNode::Container {
                layout, children, ..
            } => {
                let contains_window = children
                    .iter()
                    .any(|child| Self::node_contains_window(child, window_id));

                if contains_window {
                    *layout = match *layout {
                        ContainerLayout::Horizontal => ContainerLayout::Vertical,
                        ContainerLayout::Vertical => ContainerLayout::Horizontal,
                        ContainerLayout::Tabbed | ContainerLayout::Stacked => {
                            // Convert tabbed/stacked to split using the preferred direction
                            match preferred_split {
                                SplitDirection::Horizontal => ContainerLayout::Horizontal,
                                SplitDirection::Vertical => ContainerLayout::Vertical,
                            }
                        }
                    };
                } else {
                    for child in children.iter_mut() {
                        Self::toggle_container_split_recursive(child, window_id, preferred_split);
                    }
                }
            }
        }
    }

    fn find_tabbed_containers_recursive(
        node: &Option<LayoutNode>,
        containers: &mut Vec<(Rectangle<i32, Logical>, Vec<(WindowId, bool)>)>,
    ) {
        match node {
            Some(LayoutNode::Container {
                layout,
                children,
                geometry,
                ..
            }) => {
                if matches!(layout, ContainerLayout::Tabbed) {
                    // Collect all windows in this tabbed container
                    let mut windows = Vec::new();
                    let active_index = children.active_index();
                    for (i, child) in children.iter().enumerate() {
                        let mut child_windows = Vec::new();
                        Self::collect_window_ids(child, &mut child_windows);
                        // Mark active state for each window in this child
                        for (window_id, _) in child_windows {
                            windows.push((window_id, i == active_index));
                        }
                    }
                    if !windows.is_empty() {
                        containers.push((*geometry, windows));
                    }
                } else {
                    // Recurse into children for non-tabbed containers
                    for child in children.iter() {
                        Self::find_tabbed_containers_recursive(&Some(child.clone()), containers);
                    }
                }
            }
            _ => {}
        }
    }
    
    fn find_stacked_containers_recursive(
        node: &Option<LayoutNode>,
        containers: &mut Vec<(Rectangle<i32, Logical>, Vec<(WindowId, bool)>)>,
    ) {
        match node {
            Some(LayoutNode::Container {
                layout,
                children,
                geometry,
                ..
            }) => {
                if matches!(layout, ContainerLayout::Stacked) {
                    // Collect all windows in this stacked container
                    let mut windows = Vec::new();
                    let active_index = children.active_index();
                    for (i, child) in children.iter().enumerate() {
                        let mut child_windows = Vec::new();
                        Self::collect_window_ids(child, &mut child_windows);
                        // Mark active state for each window in this child
                        for (window_id, _) in child_windows {
                            windows.push((window_id, i == active_index));
                        }
                    }
                    if !windows.is_empty() {
                        containers.push((*geometry, windows));
                    }
                } else {
                    // Recurse into children for non-stacked containers
                    for child in children.iter() {
                        Self::find_stacked_containers_recursive(&Some(child.clone()), containers);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_window_ids(node: &LayoutNode, windows: &mut Vec<(WindowId, bool)>) {
        match node {
            LayoutNode::Window { id, .. } => {
                windows.push((*id, false)); // Active state will be set by caller
            }
            LayoutNode::Container { children, .. } => {
                for child in children.iter() {
                    Self::collect_window_ids(child, windows);
                }
            }
        }
    }

    fn check_window_in_tabbed_container(node: &Option<LayoutNode>, window_id: WindowId) -> bool {
        match node {
            Some(LayoutNode::Window { .. }) => false,
            Some(LayoutNode::Container {
                layout, children, ..
            }) => {
                // Check if this container contains the window and is tabbed
                let contains_window = children
                    .iter()
                    .any(|child| Self::node_contains_window(child, window_id));

                if contains_window
                    && matches!(layout, ContainerLayout::Tabbed | ContainerLayout::Stacked)
                {
                    return true;
                }

                // Otherwise recurse into children
                for child in children.iter() {
                    if Self::check_window_in_tabbed_container(&Some(child.clone()), window_id) {
                        return true;
                    }
                }
                false
            }
            None => false,
        }
    }

    fn switch_tab_recursive(node: &mut LayoutNode, window_id: WindowId, next: bool) -> bool {
        match node {
            LayoutNode::Window { .. } => false,
            LayoutNode::Container {
                layout, children, ..
            } => {
                // Check if this container contains the window
                let contains_window = children
                    .iter()
                    .any(|child| Self::node_contains_window(child, window_id));

                if contains_window
                    && matches!(layout, ContainerLayout::Tabbed | ContainerLayout::Stacked)
                {
                    let active_index = children.active_index();
                    tracing::info!(
                        "Tab switch: current active_child={}, next={}, num_children={}",
                        active_index,
                        next,
                        children.len()
                    );

                    // Debug: print what windows are in each child and the current structure
                    tracing::info!(
                        "Container structure: before={}, active={}, after={}",
                        children.before.len(),
                        if let LayoutNode::Window { id, .. } = children.active() {
                            format!("Window {id}")
                        } else {
                            "Container".to_string()
                        },
                        children.after.len()
                    );
                    for (i, child) in children.iter().enumerate() {
                        let is_active = i == active_index;
                        if let LayoutNode::Window { id, .. } = child {
                            tracing::info!(
                                "  Child {}: Window {}{}",
                                i,
                                id,
                                if is_active { " [ACTIVE]" } else { "" }
                            );
                        }
                    }

                    // Check if we should escape the container
                    if next && active_index == children.len() - 1 {
                        // At last tab, going forward - escape
                        tracing::info!("At last tab, escaping container");
                        return true;
                    } else if !next && active_index == 0 {
                        // At first tab, going backward - escape
                        tracing::info!("At first tab, escaping container");
                        return true;
                    }

                    // Otherwise switch to next/previous tab
                    let old_active = active_index;
                    let new_index = if next {
                        (active_index + 1) % children.len()
                    } else {
                        if active_index == 0 {
                            children.len() - 1
                        } else {
                            active_index - 1
                        }
                    };

                    // Debug: show what window we're switching to
                    if let Some(new_child) = children.get(new_index) {
                        if let LayoutNode::Window { id, .. } = new_child {
                            tracing::info!("Switching to tab {} (window {})", new_index, id);
                        }
                    }

                    children.set_active(new_index);
                    tracing::info!("Switched from tab {} to tab {}", old_active, new_index);
                    false
                } else {
                    // Recurse into children
                    for child in children.iter_mut() {
                        if Self::switch_tab_recursive(child, window_id, next) {
                            return true;
                        }
                    }
                    false
                }
            }
        }
    }

    fn node_contains_window(node: &LayoutNode, window_id: WindowId) -> bool {
        match node {
            LayoutNode::Window { id, .. } => *id == window_id,
            LayoutNode::Container { children, .. } => children
                .iter()
                .any(|child| Self::node_contains_window(child, window_id)),
        }
    }
}
