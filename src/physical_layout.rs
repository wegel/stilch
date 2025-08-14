/// Physical display layout management
/// 
/// This module handles the mapping between physical display positions
/// (in actual millimeters) and logical positions (used by Wayland).
/// The goal is to maintain cursor continuity across displays with different scales.

use smithay::utils::{Logical, Physical, Point, Rectangle, Size, Transform};
use std::collections::HashMap;
use tracing::{info, warn};

/// Represents a display's physical and logical geometry
#[derive(Debug, Clone)]
pub struct PhysicalDisplay {
    /// Display name
    pub name: String,
    /// Resolution in pixels
    pub pixel_size: Size<i32, Physical>,
    /// Physical size in millimeters
    pub physical_size_mm: Size<f64, Physical>,
    /// Physical position in millimeters from top-left origin
    pub physical_position_mm: Point<f64, Physical>,
    /// Scale factor
    pub scale: f64,
    /// Transform (rotation/flip)
    pub transform: Transform,
    /// Logical position (calculated)
    pub logical_position: Point<i32, Logical>,
    /// Logical size (calculated)
    pub logical_size: Size<i32, Logical>,
}

impl PhysicalDisplay {
    /// Get the physical bounds of this display
    pub fn physical_bounds(&self) -> Rectangle<f64, Physical> {
        // Physical bounds are just position and size - no transform needed
        Rectangle::new(self.physical_position_mm, self.physical_size_mm)
    }
    
    /// Get DPI for this display
    pub fn dpi(&self) -> (f64, f64) {
        let mm_to_inch = 1.0 / 25.4;
        
        // Simple DPI calculation - pixels per inch
        let dpi_x = self.pixel_size.w as f64 / (self.physical_size_mm.w * mm_to_inch);
        let dpi_y = self.pixel_size.h as f64 / (self.physical_size_mm.h * mm_to_inch);
        
        (dpi_x, dpi_y)
    }
    
    /// Convert a physical point (in mm) to logical coordinates on this display
    pub fn physical_to_logical(&self, physical_mm: Point<f64, Physical>) -> Option<Point<f64, Logical>> {
        let bounds = self.physical_bounds();
        
        // Check if point is within this display's physical bounds (with small tolerance)
        if physical_mm.x < bounds.loc.x - 0.5 || physical_mm.x > bounds.loc.x + bounds.size.w + 0.5 ||
           physical_mm.y < bounds.loc.y - 0.5 || physical_mm.y > bounds.loc.y + bounds.size.h + 0.5 {
            return None;
        }
        
        // Get position relative to display's physical origin
        let rel_x = physical_mm.x - bounds.loc.x;
        let rel_y = physical_mm.y - bounds.loc.y;
        
        // Convert to normalized coordinates (0.0 to 1.0)
        let norm_x = rel_x / self.physical_size_mm.w;
        let norm_y = rel_y / self.physical_size_mm.h;
        
        // Map to logical coordinates
        let logical_x = norm_x * self.logical_size.w as f64;
        let logical_y = norm_y * self.logical_size.h as f64;
        
        // Add display's logical offset
        Some(Point::from((
            logical_x + self.logical_position.x as f64,
            logical_y + self.logical_position.y as f64,
        )))
    }
    
    /// Convert a logical point to physical coordinates (in mm)
    pub fn logical_to_physical(&self, logical: Point<f64, Logical>) -> Point<f64, Physical> {
        // Get position relative to display's logical origin
        let rel_x = logical.x - self.logical_position.x as f64;
        let rel_y = logical.y - self.logical_position.y as f64;
        
        // Convert to normalized coordinates (0.0 to 1.0)
        let norm_x = rel_x / self.logical_size.w as f64;
        let norm_y = rel_y / self.logical_size.h as f64;
        
        // Map to physical coordinates
        let mm_x = norm_x * self.physical_size_mm.w;
        let mm_y = norm_y * self.physical_size_mm.h;
        
        // Add display's physical offset
        let bounds = self.physical_bounds();
        Point::from((
            mm_x + bounds.loc.x,
            mm_y + bounds.loc.y,
        ))
    }
}

/// Manages the physical-to-logical mapping for displays
#[derive(Debug)]
pub struct PhysicalLayoutManager {
    displays: HashMap<String, PhysicalDisplay>,
    /// Name of the display the cursor is currently on
    current_display: Option<String>,
}

impl PhysicalLayoutManager {
    pub fn new() -> Self {
        Self {
            displays: HashMap::new(),
            current_display: None,
        }
    }
    
    /// Add or update a display
    pub fn add_display(&mut self, display_info: PhysicalDisplay) {
        info!(
            "Adding display '{}': {:.1}x{:.1}mm at ({:.1}, {:.1})mm, scale: {}",
            display_info.name,
            display_info.physical_size_mm.w, display_info.physical_size_mm.h,
            display_info.physical_position_mm.x, display_info.physical_position_mm.y,
            display_info.scale
        );
        
        self.displays.insert(display_info.name.clone(), display_info);
    }
    
    /// Remove a display
    pub fn remove_display(&mut self, name: &str) {
        self.displays.remove(name);
        if self.current_display.as_deref() == Some(name) {
            self.current_display = None;
        }
    }
    
    /// Find which display contains a physical point
    pub fn display_at_physical_point(&self, point: Point<f64, Physical>) -> Option<&PhysicalDisplay> {
        self.displays.values().find(|d| {
            d.physical_bounds().contains(point.to_i32_round())
        })
    }
    
    /// Find the nearest display to a physical point (for gap jumping)
    pub fn nearest_display_to_point(&self, point: Point<f64, Physical>) -> Option<&PhysicalDisplay> {
        self.displays
            .values()
            .min_by_key(|d| {
                let bounds = d.physical_bounds();
                // Calculate center manually
                let center_x = bounds.loc.x + bounds.size.w / 2.0;
                let center_y = bounds.loc.y + bounds.size.h / 2.0;
                
                // Calculate distance to display center
                let dx = point.x - center_x;
                let dy = point.y - center_y;
                ((dx * dx + dy * dy) * 1000.0) as i32 // Scale up for integer comparison
            })
    }
    
    /// Handle relative pointer motion - only intervene at display boundaries
    pub fn handle_relative_motion(
        &mut self,
        current_logical: Point<f64, Logical>,
        delta: Point<f64, Logical>,
    ) -> Point<f64, Logical> {
        // Calculate the new position normally
        let new_logical = current_logical + delta;
        
        // Find current display
        let current_display = self.displays.values().find(|d| {
            let bounds = Rectangle::new(d.logical_position, d.logical_size);
            bounds.to_f64().contains(current_logical)
        });
        
        let Some(current_disp) = current_display else {
            // No display found, try to find one at the new position
            if let Some(new_display) = self.displays.values().find(|d| {
                let bounds = Rectangle::new(d.logical_position, d.logical_size);
                bounds.to_f64().contains(new_logical)
            }) {
                self.current_display = Some(new_display.name.clone());
            }
            return new_logical;
        };
        
        // Check if new position is still on the same display
        let display_bounds = Rectangle::new(current_disp.logical_position, current_disp.logical_size);
        if display_bounds.to_f64().contains(new_logical) {
            // Still on same display, no intervention needed
            self.current_display = Some(current_disp.name.clone());
            return new_logical;
        }
        
        // We're trying to cross a display boundary
        // Convert current position to physical coordinates
        let current_physical = current_disp.logical_to_physical(current_logical);
        
        // Calculate where we would be physically after the movement
        // We need to convert the delta to physical space
        let (dpi_x, dpi_y) = current_disp.dpi();
        let mm_per_logical_x = 25.4 / (dpi_x / current_disp.scale);
        let mm_per_logical_y = 25.4 / (dpi_y / current_disp.scale);
        
        let physical_delta = Point::<f64, Physical>::from((
            delta.x * mm_per_logical_x,
            delta.y * mm_per_logical_y,
        ));
        
        let target_physical = Point::from((
            current_physical.x + physical_delta.x,
            current_physical.y + physical_delta.y,
        ));
        
        // Check if there's a display at the target physical position
        let target_display = self.displays.values().find(|d| {
            let bounds = d.physical_bounds();
            bounds.to_f64().contains(target_physical)
        });
        
        if let Some(target) = target_display {
            // There's a display at the target position - transition to it
            self.current_display = Some(target.name.clone());
            
            // Convert the physical position to logical coordinates on the target display
            if let Some(new_pos) = target.physical_to_logical(target_physical) {
                return new_pos;
            }
        }
        
        // No display at exact target position - check for gap jumping
        // Determine which boundary was crossed by checking position relative to current display
        let current_bounds = current_disp.physical_bounds();
        let crossed_left = target_physical.x < current_bounds.loc.x;
        let crossed_right = target_physical.x > current_bounds.loc.x + current_bounds.size.w;
        let crossed_top = target_physical.y < current_bounds.loc.y;
        let crossed_bottom = target_physical.y > current_bounds.loc.y + current_bounds.size.h;
        
        // Find displays in the direction of the crossed boundary
        let potential_targets = if crossed_bottom {
                // Crossed bottom boundary - find displays below
                self.displays.values()
                    .filter(|d| {
                        let bounds = d.physical_bounds();
                        let is_below = bounds.loc.y > current_disp.physical_position_mm.y + current_disp.physical_size_mm.h - 10.0;
                        let overlaps_x = target_physical.x >= bounds.loc.x - 10.0 && 
                                       target_physical.x <= bounds.loc.x + bounds.size.w + 10.0;
                        is_below && overlaps_x
                    })
                    .min_by_key(|d| {
                        // Prefer closest display
                        let bounds = d.physical_bounds();
                        ((bounds.loc.y - target_physical.y).abs() * 100.0) as i32
                    })
        } else if crossed_top {
                // Crossed top boundary - find displays above
                self.displays.values()
                    .filter(|d| {
                        let bounds = d.physical_bounds();
                        // Display is above current display
                        bounds.loc.y + bounds.size.h < current_disp.physical_position_mm.y + 10.0 &&
                        // And horizontally overlaps with target position
                        target_physical.x >= bounds.loc.x - 10.0 && 
                        target_physical.x <= bounds.loc.x + bounds.size.w + 10.0
                    })
                    .min_by_key(|d| {
                        // Prefer closest display
                        let bounds = d.physical_bounds();
                        ((bounds.loc.y + bounds.size.h - target_physical.y).abs() * 100.0) as i32
                    })
        } else if crossed_right {
                // Crossed right boundary - find displays to the right
                self.displays.values()
                    .filter(|d| {
                        let bounds = d.physical_bounds();
                        // Display is to the right of current display
                        bounds.loc.x > current_disp.physical_position_mm.x + current_disp.physical_size_mm.w - 10.0 &&
                        // And vertically overlaps with target position
                        target_physical.y >= bounds.loc.y - 10.0 && 
                        target_physical.y <= bounds.loc.y + bounds.size.h + 10.0
                    })
                    .min_by_key(|d| {
                        // Prefer closest display
                        let bounds = d.physical_bounds();
                        ((bounds.loc.x - target_physical.x).abs() * 100.0) as i32
                    })
        } else if crossed_left {
                // Crossed left boundary - find displays to the left
                self.displays.values()
                    .filter(|d| {
                        let bounds = d.physical_bounds();
                        // Display is to the left of current display
                        bounds.loc.x + bounds.size.w < current_disp.physical_position_mm.x + 10.0 &&
                        // And vertically overlaps with target position
                        target_physical.y >= bounds.loc.y - 10.0 && 
                        target_physical.y <= bounds.loc.y + bounds.size.h + 10.0
                    })
                    .min_by_key(|d| {
                        // Prefer closest display
                        let bounds = d.physical_bounds();
                        ((bounds.loc.x + bounds.size.w - target_physical.x).abs() * 100.0) as i32
                    })
        } else {
            // No single boundary crossed or diagonal crossing - don't gap jump
            None
        };
        
        if let Some(gap_target) = potential_targets {
            // Jump the gap to the target display
            self.current_display = Some(gap_target.name.clone());
            
            // Clamp the physical position to the target display bounds
            let bounds = gap_target.physical_bounds();
            
            let jumped_physical = Point::from((
                target_physical.x.clamp(bounds.loc.x + 0.1, bounds.loc.x + bounds.size.w - 0.1),
                target_physical.y.clamp(bounds.loc.y + 0.1, bounds.loc.y + bounds.size.h - 0.1),
            ));
            
            // Convert to logical coordinates on the target display
            if let Some(new_pos) = gap_target.physical_to_logical(jumped_physical) {
                return new_pos;
            } else {
                warn!("Failed to convert jumped physical position to logical for display '{}'", gap_target.name);
            }
        }
        
        // No display found even with gap jumping - clamp to current display edge
        let clamped = Point::from((
            new_logical.x.clamp(
                display_bounds.loc.x as f64,
                (display_bounds.loc.x + display_bounds.size.w) as f64 - 0.01,
            ),
            new_logical.y.clamp(
                display_bounds.loc.y as f64,
                (display_bounds.loc.y + display_bounds.size.h) as f64 - 0.01,
            ),
        ));
        
        clamped
    }
    
    /// Handle absolute pointer motion (for touch/tablet input)
    pub fn handle_absolute_motion(
        &mut self,
        output_name: &str,
        normalized: Point<f64, Physical>,
    ) -> Option<Point<f64, Logical>> {
        let display = self.displays.get(output_name)?;
        
        // Convert normalized (0.0-1.0) to physical mm
        let bounds = display.physical_bounds();
        let physical_mm = Point::from((
            bounds.loc.x + normalized.x * bounds.size.w,
            bounds.loc.y + normalized.y * bounds.size.h,
        ));
        
        self.current_display = Some(output_name.to_string());
        
        display.physical_to_logical(physical_mm)
    }
    
    /// Initialize cursor position from logical coordinates
    pub fn set_logical_position(&mut self, logical: Point<f64, Logical>) {
        // Find display containing this logical position
        if let Some(display) = self.displays.values().find(|d| {
            let bounds = Rectangle::new(
                d.logical_position,
                d.logical_size,
            );
            bounds.to_f64().contains(logical)
        }) {
            self.current_display = Some(display.name.clone());
        }
    }
    
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_physical_cursor_continuity() {
        // set up two displays with different scales
        // Display 1: 2560x1440 at scale 1.5 (170 DPI), positioned at origin
        // Display 2: 1920x1080 at scale 1.0 (100 DPI), positioned to the right
        
        let mut layout = PhysicalLayoutManager::new();
        
        // main display: 2560x1440 pixels, ~345x194mm physical (15.6" diagonal)
        let display1 = PhysicalDisplay {
            name: "Display1".to_string(),
            pixel_size: Size::from((2560, 1440)),
            physical_size_mm: Size::from((345.0, 194.0)),
            physical_position_mm: Point::from((0.0, 0.0)),
            scale: 1.5,
            transform: Transform::Normal,
            logical_position: Point::from((0, 0)),
            logical_size: Size::from((1707, 960)), // 2560/1.5 x 1440/1.5
        };
        
        // secondary display: 1920x1080 pixels, ~476x268mm physical (21.5" diagonal)  
        // Positioned to the right of display1 in physical space (345mm from left)
        let display2 = PhysicalDisplay {
            name: "Display2".to_string(),
            pixel_size: Size::from((1920, 1080)),
            physical_size_mm: Size::from((476.0, 268.0)),
            physical_position_mm: Point::from((345.0, 0.0)),
            scale: 1.0,
            transform: Transform::Normal,
            logical_position: Point::from((1707, 0)), // Right of display1
            logical_size: Size::from((1920, 1080)),
        };
        
        layout.add_display(display1);
        layout.add_display(display2);
        
        // test 1: Move cursor from middle of Display1 to Display2
        // Start at center of Display1 in logical coordinates
        let start_pos = Point::<f64, Logical>::from((853.5, 480.0));
        layout.set_logical_position(start_pos);
        
        // Move right by 900 logical pixels - should cross into Display2
        let delta = Point::<f64, Logical>::from((900.0, 0.0));
        let new_pos = layout.handle_relative_motion(start_pos, delta);
        
        // Should be on Display2 now
        assert!(new_pos.x > 1707.0, "Cursor should be on Display2");
        assert!(new_pos.x < 1707.0 + 1920.0, "Cursor should still be within Display2");
        
        // test 2: Move from Display2 back to Display1
        let start_pos2 = Point::<f64, Logical>::from((1800.0, 500.0));
        layout.set_logical_position(start_pos2);
        
        // Move left
        let delta2 = Point::<f64, Logical>::from((-200.0, 0.0));
        let new_pos2 = layout.handle_relative_motion(start_pos2, delta2);
        
        // Should be back on Display1
        assert!(new_pos2.x < 1707.0, "Cursor should be back on Display1");
        assert!(new_pos2.x >= 0.0, "Cursor should be within Display1");
    }
    
    #[test]
    fn test_gap_jumping() {
        // test gap jumping between non-adjacent displays with boundary-based logic
        let mut layout = PhysicalLayoutManager::new();
        
        // display 1: Left side
        let display1 = PhysicalDisplay {
            name: "Display1".to_string(),
            pixel_size: Size::from((1920, 1080)),
            physical_size_mm: Size::from((300.0, 200.0)),
            physical_position_mm: Point::from((0.0, 0.0)),
            scale: 1.0,
            transform: Transform::Normal,
            logical_position: Point::from((0, 0)),
            logical_size: Size::from((1920, 1080)),
        };
        
        // display 2: Right side with a gap (100mm gap)
        let display2 = PhysicalDisplay {
            name: "Display2".to_string(),
            pixel_size: Size::from((1920, 1080)),
            physical_size_mm: Size::from((300.0, 200.0)),
            physical_position_mm: Point::from((400.0, 0.0)), // 100mm gap horizontally, same vertical position
            scale: 1.0,
            transform: Transform::Normal,
            logical_position: Point::from((2000, 0)),
            logical_size: Size::from((1920, 1080)),
        };
        
        layout.add_display(display1);
        layout.add_display(display2);
        
        // start at right edge of Display1
        let start_pos = Point::<f64, Logical>::from((1919.0, 540.0));
        layout.set_logical_position(start_pos);
        
        // move right to cross the boundary - should trigger gap jump to Display2
        let delta = Point::<f64, Logical>::from((2.0, 0.0)); // Small delta that crosses the boundary
        let new_pos = layout.handle_relative_motion(start_pos, delta);
        
        // Should have jumped to Display2's left edge
        assert!(new_pos.x >= 2000.0 && new_pos.x < 2100.0, "Cursor should have jumped to Display2's left edge, got x={}", new_pos.x);
        
        // test 2: moving from Display2 back to Display1
        let start_pos2 = Point::<f64, Logical>::from((2001.0, 540.0));
        layout.set_logical_position(start_pos2);
        
        // move left to cross the boundary
        let delta2 = Point::<f64, Logical>::from((-2.0, 0.0));
        let new_pos2 = layout.handle_relative_motion(start_pos2, delta2);
        
        // Should have jumped back to Display1's right edge
        assert!(new_pos2.x < 1920.0 && new_pos2.x > 1800.0, "Cursor should have jumped to Display1's right edge, got x={}", new_pos2.x);
    }
}