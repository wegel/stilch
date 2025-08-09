//! Virtual Output Management
//!
//! This module implements stilch's unique virtual output system, which allows
//! physical monitors to be split into multiple logical displays or multiple
//! monitors to be merged into one unified workspace.
//!
//! # Concepts
//!
//! - **Physical Output**: An actual monitor connected to the system
//! - **Virtual Output**: A logical display area that may correspond to:
//!   - A complete physical output (1:1 mapping)
//!   - A portion of a physical output (split mode)
//!   - Multiple physical outputs combined (merge mode)
//!
//! # Examples
//!
//! ## Split a 4K monitor into quadrants
//! ```ignore
//! let manager = VirtualOutputManager::new();
//! manager.split_output(output, 2, 2); // 2x2 grid
//! ```
//!
//! ## Merge two monitors
//! ```ignore
//! let manager = VirtualOutputManager::new();
//! manager.merge_outputs(vec![output1, output2]);
//! ```

use crate::workspace::WorkspaceId;
use smithay::{
    output::Output,
    utils::{Logical, Point, Rectangle, Size},
};
use std::collections::HashMap;
use std::num::NonZeroU32;
use tracing::info;

/// Represents the state of a virtual output
#[derive(Debug, Clone)]
pub enum VirtualOutputState {
    /// No workspace is currently shown
    Empty,
    /// A workspace is being displayed
    ShowingWorkspace(WorkspaceId),
}

/// Type-safe identifier for virtual outputs
///
/// Uses NonZeroU32 to ensure IDs are never zero and can be
/// efficiently stored in Option without overhead
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VirtualOutputId(NonZeroU32);

impl VirtualOutputId {
    /// Create a new VirtualOutputId from a raw value
    /// Returns None if the value is zero
    pub fn from_raw(id: u32) -> Option<Self> {
        NonZeroU32::new(id).map(VirtualOutputId)
    }

    /// Create from usize for compatibility
    /// Returns None if the value is zero or too large
    pub fn from_usize(id: usize) -> Option<Self> {
        u32::try_from(id)
            .ok()
            .and_then(NonZeroU32::new)
            .map(VirtualOutputId)
    }

    /// Create for testing/legacy code
    /// # Panics
    /// Panics if id is zero
    pub fn new(id: usize) -> Self {
        Self::from_usize(id).expect("VirtualOutputId cannot be zero")
    }

    /// Get the raw value
    pub fn get(&self) -> u32 {
        self.0.get()
    }

    /// Get as usize for indexing
    pub fn as_usize(&self) -> usize {
        self.0.get() as usize
    }
}

impl std::fmt::Display for VirtualOutputId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug)]
pub struct VirtualOutput {
    id: VirtualOutputId,
    name: String,
    /// Physical outputs backing this virtual output
    physical_outputs: Vec<Output>,
    /// Logical region this virtual output represents
    logical_region: Rectangle<i32, Logical>,
    /// Current state of this virtual output
    state: VirtualOutputState,
}

#[derive(Debug)]
pub struct VirtualOutputManager {
    virtual_outputs: HashMap<VirtualOutputId, VirtualOutput>,
    next_id: u32,
    /// Mapping from physical output to virtual outputs it contains
    physical_to_virtual: HashMap<Output, Vec<VirtualOutputId>>,
}

impl VirtualOutputManager {
    pub fn new() -> Self {
        Self {
            virtual_outputs: HashMap::new(),
            next_id: 1, // Start at 1 for NonZeroU32
            physical_to_virtual: HashMap::new(),
        }
    }

    /// Create a virtual output that represents the entire physical output
    pub fn create_from_physical(
        &mut self,
        physical: Output,
        logical_rect: Rectangle<i32, Logical>,
    ) -> VirtualOutputId {
        self.create_virtual_output(
            format!("virtual-{}", self.next_id),
            vec![physical],
            logical_rect,
        )
    }

    /// Create a virtual output with a custom name and region
    pub fn create_virtual_output(
        &mut self,
        name: String,
        physical_outputs: Vec<Output>,
        logical_region: Rectangle<i32, Logical>,
    ) -> VirtualOutputId {
        let id = VirtualOutputId::from_raw(self.next_id).expect("VirtualOutputId overflow");
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("VirtualOutputId counter overflow");

        let virtual_output = VirtualOutput {
            id,
            name,
            physical_outputs: physical_outputs.clone(),
            logical_region,
            state: VirtualOutputState::Empty,
        };

        self.virtual_outputs.insert(id, virtual_output);

        // Update physical-to-virtual mappings
        for physical in physical_outputs {
            self.physical_to_virtual
                .entry(physical)
                .or_default()
                .push(id);
        }

        id
    }

    /// Create virtual outputs from multiple physical outputs based on config
    pub fn create_from_config(
        &mut self,
        name: String,
        physical_outputs: Vec<Output>,
        region: Option<Rectangle<i32, Logical>>,
    ) -> Option<VirtualOutputId> {
        if physical_outputs.is_empty() {
            return None;
        }

        // Calculate the combined region if not specified
        let logical_region = if let Some(region) = region {
            region
        } else {
            // For now, return a default region
            // In practice, the caller should always provide a region
            // or this method shouldn't be called without proper context
            Rectangle::new(
                Point::from((0, 0)),
                Size::from((1920, 1080)), // Default size
            )
        };

        Some(self.create_virtual_output(name, physical_outputs, logical_region))
    }

    /// Split a physical output into multiple virtual outputs
    pub fn split_physical(
        &mut self,
        physical: Output,
        logical_rect: Rectangle<i32, Logical>,
        split_type: SplitType,
        count: usize,
    ) -> Vec<VirtualOutputId> {
        // Remove existing virtual outputs for this physical output
        if let Some(existing) = self.physical_to_virtual.remove(&physical) {
            for id in existing {
                self.virtual_outputs.remove(&id);
            }
        }

        let mut virtual_ids = Vec::new();

        for i in 0..count {
            let id = VirtualOutputId::from_raw(self.next_id).expect("VirtualOutputId overflow");
            self.next_id += 1;

            let region = match split_type {
                SplitType::Horizontal => {
                    // Split evenly
                    let width = logical_rect.size.w / count as i32;
                    let x = logical_rect.loc.x + width * i as i32;
                    Rectangle::new(
                        Point::from((x, logical_rect.loc.y)),
                        (width, logical_rect.size.h).into(),
                    )
                }
                SplitType::Vertical => {
                    let height = logical_rect.size.h / count as i32;
                    let y = logical_rect.loc.y + height * i as i32;
                    Rectangle::new(
                        Point::from((logical_rect.loc.x, y)),
                        (logical_rect.size.w, height).into(),
                    )
                }
                SplitType::Grid(cols, rows) => {
                    let col = i % cols;
                    let row = i / cols;
                    let width = logical_rect.size.w / cols as i32;
                    let height = logical_rect.size.h / rows as i32;
                    let x = logical_rect.loc.x + width * col as i32;
                    let y = logical_rect.loc.y + height * row as i32;
                    Rectangle::new(Point::from((x, y)), (width, height).into())
                }
            };

            let virtual_output = VirtualOutput {
                id,
                name: format!("virtual-{}", self.next_id - 1),
                physical_outputs: vec![physical.clone()],
                logical_region: region,
                state: VirtualOutputState::Empty,
            };

            self.virtual_outputs.insert(id, virtual_output);
            virtual_ids.push(id);
        }

        self.physical_to_virtual
            .insert(physical, virtual_ids.clone());

        // Don't assign workspace here - let StilchState::initialize_virtual_output handle it
        // to ensure proper synchronization with WorkspaceManager

        virtual_ids
    }

    /// Merge multiple physical outputs into a single virtual output
    pub fn merge_physical(
        &mut self,
        outputs: Vec<(Output, Rectangle<i32, Logical>)>,
    ) -> VirtualOutputId {
        // Remove existing virtual outputs for these physical outputs
        for (output, _) in &outputs {
            if let Some(existing) = self.physical_to_virtual.remove(output) {
                for id in existing {
                    self.virtual_outputs.remove(&id);
                }
            }
        }

        let id = VirtualOutputId::from_raw(self.next_id).expect("VirtualOutputId overflow");
        self.next_id += 1;

        // Calculate combined region by finding bounding box
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for (_, rect) in &outputs {
            min_x = min_x.min(rect.loc.x);
            min_y = min_y.min(rect.loc.y);
            max_x = max_x.max(rect.loc.x + rect.size.w);
            max_y = max_y.max(rect.loc.y + rect.size.h);
        }

        let logical_region = Rectangle::new(
            Point::from((min_x, min_y)),
            ((max_x - min_x), (max_y - min_y)).into(),
        );

        let physical_outputs: Vec<Output> = outputs.iter().map(|(o, _)| o.clone()).collect();

        let virtual_output = VirtualOutput {
            id,
            name: format!("virtual-merged-{}", self.next_id - 1),
            physical_outputs: physical_outputs.clone(),
            logical_region,
            state: VirtualOutputState::Empty,
        };

        self.virtual_outputs.insert(id, virtual_output);

        for output in physical_outputs {
            self.physical_to_virtual.entry(output).or_default().push(id);
        }

        // Don't assign workspace here - let StilchState::initialize_virtual_output handle it
        // to ensure proper synchronization with WorkspaceManager

        id
    }

    pub fn get(&self, id: VirtualOutputId) -> Option<&VirtualOutput> {
        self.virtual_outputs.get(&id)
    }

    pub fn get_mut(&mut self, id: VirtualOutputId) -> Option<&mut VirtualOutput> {
        self.virtual_outputs.get_mut(&id)
    }

    /// List all virtual output IDs
    pub fn list_virtual_outputs(&self) -> Vec<VirtualOutputId> {
        self.virtual_outputs.keys().copied().collect()
    }

    /// Get all virtual outputs
    pub fn outputs(&self) -> impl Iterator<Item = &VirtualOutput> {
        self.virtual_outputs.values()
    }

    pub fn all_virtual_outputs(&self) -> impl Iterator<Item = &VirtualOutput> {
        self.virtual_outputs.values()
    }

    /// Remove all virtual outputs associated with a physical output
    pub fn remove_physical_output(&mut self, physical: &Output) -> Vec<VirtualOutputId> {
        let mut removed = Vec::new();

        if let Some(virtual_ids) = self.physical_to_virtual.remove(physical) {
            for id in &virtual_ids {
                if let Some(vo) = self.virtual_outputs.remove(id) {
                    info!(
                        "Removing virtual output '{}' due to physical output disconnection",
                        vo.name()
                    );
                    removed.push(*id);

                    // Also remove from any other physical outputs that share this virtual output
                    for other_physical in &vo.physical_outputs {
                        if other_physical != physical {
                            if let Some(other_ids) =
                                self.physical_to_virtual.get_mut(other_physical)
                            {
                                other_ids.retain(|vid| vid != id);
                            }
                        }
                    }
                }
            }
        }

        removed
    }

    /// Get virtual outputs that contain a logical point
    pub fn virtual_outputs_at_point(&self, point: Point<i32, Logical>) -> Vec<VirtualOutputId> {
        self.virtual_outputs
            .iter()
            .filter(|(_, vo)| vo.logical_region.contains(point))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get virtual outputs for a physical output
    pub fn virtual_outputs_for_physical(&self, physical: &Output) -> Vec<VirtualOutputId> {
        self.physical_to_virtual
            .get(physical)
            .cloned()
            .unwrap_or_default()
    }

    /// Find virtual output containing a point
    pub fn virtual_output_at(&self, point: Point<i32, Logical>) -> Option<VirtualOutputId> {
        self.virtual_outputs_at_point(point).into_iter().next()
    }

    /// Update tiling areas for all virtual outputs based on their logical regions
    pub fn update_all_tiling_areas(&mut self) {
        for virtual_output in self.virtual_outputs.values_mut() {
            virtual_output.update_tiling_area();
        }
    }

    /// Set the active workspace on a virtual output
    pub fn set_active_workspace(
        &mut self,
        virtual_output_id: VirtualOutputId,
        workspace_idx: usize,
    ) {
        if let Some(virtual_output) = self.virtual_outputs.get_mut(&virtual_output_id) {
            let workspace_id = WorkspaceId::new(workspace_idx as u8);
            virtual_output.state = VirtualOutputState::ShowingWorkspace(workspace_id);
        }
    }

    /// Clear the active workspace on a virtual output
    pub fn clear_active_workspace(&mut self, virtual_output_id: VirtualOutputId) {
        if let Some(virtual_output) = self.virtual_outputs.get_mut(&virtual_output_id) {
            virtual_output.state = VirtualOutputState::Empty;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SplitType {
    Horizontal,
    Vertical,
    Grid(usize, usize),
}

impl VirtualOutput {
    pub fn id(&self) -> VirtualOutputId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn physical_outputs(&self) -> &[Output] {
        &self.physical_outputs
    }

    pub fn logical_region(&self) -> Rectangle<i32, Logical> {
        self.logical_region
    }

    pub fn active_workspace(&self) -> Option<usize> {
        match &self.state {
            VirtualOutputState::Empty => None,
            VirtualOutputState::ShowingWorkspace(id) => Some(id.get() as usize),
        }
    }

    pub fn active_workspace_id(&self) -> Option<WorkspaceId> {
        match &self.state {
            VirtualOutputState::Empty => None,
            VirtualOutputState::ShowingWorkspace(id) => Some(*id),
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self.state, VirtualOutputState::Empty)
    }

    pub fn update_tiling_area(&mut self) {
        // This is now handled in VirtualOutputManager when switching workspaces
    }
}
