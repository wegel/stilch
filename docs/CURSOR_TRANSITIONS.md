# Physical Cursor Continuity - Detailed Documentation

## Overview

stilch's Physical Cursor Continuity system solves the jarring "cursor jump" problem that occurs when moving between monitors with different DPIs, sizes, or resolutions. By tracking cursor position in physical millimeters and implementing intelligent gap jumping, stilch creates a unified physical space where cursor movement feels natural and continuous across all displays.

## The Problem

### Traditional Cursor Behavior

In most compositors, cursor position is mapped 1:1 to pixel coordinates:

```
1080p Monitor (96 DPI)        4K Monitor (192 DPI)
[1920x1080 pixels]            [3840x2160 pixels]
┌────────────────┐            ┌────────────────┐
│                │            │                │
│      →→→→→→→   │ ─────────→ │ ↓ (cursor jumps│
│                │            │   to different │
└────────────────┘            │   visual pos)  │
                              └────────────────┘

Cursor at x=1920 →  Maps to x=0 on 4K monitor
But visually jumps because pixel density doubled!
```

### Real-World Issues

1. **DPI Mismatch**: 24" 1080p (92 DPI) next to 27" 4K (163 DPI)
   - Cursor appears to teleport when crossing boundary
   - Vertical position shifts unexpectedly

2. **Size Mismatch**: 34" ultrawide next to 24" 16:9
   - Cursor speed feels inconsistent
   - Diagonal movement becomes broken

3. **Resolution Scaling**: Laptop at 150% scaling next to monitor at 100%
   - Cursor size changes abruptly
   - Movement sensitivity changes

## stilch's Solution

### Physical Layout Manager

stilch implements a PhysicalLayoutManager that tracks display positions and sizes in real-world millimeters, enabling accurate cursor mapping across displays with different scales and DPIs.

### Unified Physical Space

stilch treats all monitors as part of one continuous canvas measured in physical units (millimeters), not pixels:

```
Visual Space Mapping:
┌──────────────────────────────────────┐
│         Unified Visual Canvas        │
│                                      │
│  [24" 1080p]      [27" 4K]          │
│  ┌─────────┐     ┌──────────┐       │
│  │ 531mm   │     │  597mm    │       │
│  │ x       │     │  x        │       │
│  │ 298mm   │     │  336mm    │       │
│  └─────────┘     └──────────┘       │
│                                      │
└──────────────────────────────────────┘

Cursor position tracked in mm, converted to pixels per display
```

### Key Principles

1. **Physical Continuity**: Cursor maintains physical position across boundaries
2. **Velocity Preservation**: Movement speed stays consistent  
3. **Predictable Paths**: Straight lines remain straight across monitors
4. **Size Consistency**: Cursor appears same physical size on all displays
5. **Gap Jumping**: Intelligent warping across physically separated displays
6. **Boundary-Based Detection**: Direction determined by which display edge was crossed

## Implementation

### Gap Jumping Algorithm

stilch implements intelligent gap jumping based on boundary crossing detection:

1. **Boundary Detection**: When cursor moves beyond a display's boundary, the system detects which edge was crossed (left, right, top, bottom)
2. **Direction-Based Search**: Only displays in the direction of the crossed boundary are considered for gap jumping
3. **Nearest Display Selection**: Among valid targets, the nearest display that overlaps with the cursor's trajectory is selected
4. **Position Preservation**: The cursor maintains its relative position along the non-crossing axis

Example:
```
Display1 right edge crossed → Look for displays to the right
Display2 found 100mm to the right → Jump cursor to Display2's left edge
Maintain Y position relative to display height
```

### Coordinate Systems

stilch maintains three coordinate systems:

```rust
pub enum CoordinateSpace {
    // Pixel coordinates on specific output
    Physical { 
        output: OutputId,
        position: Point<i32>,
    },
    
    // Unified visual space (millimeters)
    Visual {
        position: Point<f64>,
    },
    
    // Logical coordinates (DPI-independent)
    Logical {
        position: Point<f64>,
    },
}
```

### Conversion Pipeline

```rust
impl CursorManager {
    fn physical_to_visual(&self, physical: Point<i32>, output: &Output) -> Point<f64> {
        let dpi = output.dpi();
        let mm_per_pixel = 25.4 / dpi;
        
        Point {
            x: physical.x as f64 * mm_per_pixel,
            y: physical.y as f64 * mm_per_pixel,
        }
    }
    
    fn visual_to_physical(&self, visual: Point<f64>, output: &Output) -> Point<i32> {
        let dpi = output.dpi();
        let pixels_per_mm = dpi / 25.4;
        
        Point {
            x: (visual.x * pixels_per_mm).round() as i32,
            y: (visual.y * pixels_per_mm).round() as i32,
        }
    }
}
```

### Boundary Crossing

When cursor crosses monitor boundaries:

```rust
pub fn handle_cursor_motion(&mut self, delta: Point<f64>) {
    // Update position in visual space
    self.visual_position += delta;
    
    // Find which output contains cursor
    let output = self.find_output_at_visual(self.visual_position);
    
    // Convert to physical coordinates for that output
    let physical = self.visual_to_physical(self.visual_position, &output);
    
    // Apply to actual cursor
    self.set_cursor_position(output, physical);
}
```

## Configuration

### Physical Layout Configuration

```bash
# Configure physical size and position in stilch config
output HDMI-A-1 scale 1.6 transform 270 physical_size 521x470mm physical_position 37,0mm
output DP-1 scale 1.0 position 0,1800 physical_size 291x105mm physical_position -9.5,480mm
output DP-2 scale 1.0 position 1920,1800 physical_size 291x105mm physical_position 313.5,480mm
```

The physical layout configuration is done through the output commands in the stilch config file.

## Visual Alignment

### Automatic Alignment

stilch automatically aligns monitors based on physical characteristics:

```
Auto-alignment based on physical size:
        ┌─────────┐
        │  24"    │ ← Smaller monitor
┌───────┼─────────┤   aligned to middle
│  27"  │         │
│       └─────────┘
└─────────────────┘

Cursor crosses at matching visual height
```

### Manual Alignment

Override automatic alignment using the stilch config:

```bash
# Position displays with specific physical positions
output DP-1 position 0,0 physical_size 597x336mm physical_position 0,0mm
output HDMI-1 position 2560,0 physical_size 531x298mm physical_position 597,50mm
```

## Special Cases

### Portrait/Landscape Mix

Handle mixed orientations:

```
Landscape + Portrait:
┌────────────┐ ┌──┐
│            │ │  │
│  Landscape │ │P │
│            │ │o │
└────────────┘ │r │
               │t │
               └──┘

Cursor maintains horizontal velocity when entering portrait
```

### Gaps Between Monitors

stilch automatically handles physical gaps between monitors using boundary-based gap jumping:

```bash
# Displays with a physical gap between them
output DP-1 physical_size 300x200mm physical_position 0,0mm
output DP-2 physical_size 300x200mm physical_position 400,0mm  # 100mm gap

# The gap is automatically detected and cursor will jump across when
# crossing the boundary in the direction of the other display
```

### Different Refresh Rates

Synchronize cursor updates across different refresh rates:

```rust
impl CursorManager {
    fn interpolate_position(&self, output: &Output, timestamp: Timestamp) -> Point<f64> {
        let refresh_rate = output.refresh_rate();
        let frame_time = 1.0 / refresh_rate;
        
        // Interpolate between last two positions
        let alpha = (timestamp - self.last_update) / frame_time;
        self.last_position + (self.current_position - self.last_position) * alpha
    }
}
```

## Performance Optimizations

### Caching

Pre-calculate conversion factors:

```rust
pub struct OutputCache {
    dpi: f64,
    mm_per_pixel: f64,
    pixels_per_mm: f64,
    visual_bounds: Rectangle<f64>,
    physical_bounds: Rectangle<i32>,
}
```

### Prediction

Predict cursor position for smooth motion:

```rust
pub fn predict_position(&self, delta_time: f64) -> Point<f64> {
    let velocity = self.velocity_tracker.average();
    self.position + velocity * delta_time
}
```

### Batch Updates

Reduce cursor update calls:

```rust
// Batch cursor updates within same frame
let mut cursor_updates = Vec::new();
for event in input_events {
    if let InputEvent::PointerMotion(delta) = event {
        cursor_updates.push(delta);
    }
}
let total_delta = cursor_updates.iter().sum();
self.update_cursor_once(total_delta);
```

## Input Device Integration

### High-DPI Mice

Handle high-DPI gaming mice correctly:

```rust
pub fn handle_high_dpi_input(&mut self, device: &InputDevice, delta: Point<f64>) {
    let device_dpi = device.dpi().unwrap_or(1000.0);
    
    // Convert device units to visual space
    let visual_delta = delta * (25.4 / device_dpi);
    
    self.update_position(visual_delta);
}
```

### Touchpad Gestures

Maintain gesture continuity across monitors:

```rust
pub fn handle_touchpad_scroll(&mut self, delta: Point<f64>) {
    // Scroll amount independent of monitor DPI
    let visual_scroll = self.to_visual_space(delta);
    
    // Apply to window under cursor
    let window = self.window_at_cursor();
    window.scroll(visual_scroll);
}
```

## Testing & Debugging

### Testing Physical Layout

The physical layout with gap jumping can be tested by configuring displays with gaps and moving the cursor between them. The cursor will automatically jump gaps when crossing display boundaries.

## Known Limitations

1. **Application Cursor Warping**: Some applications that warp cursor may conflict
2. **Gaming**: FPS games that use raw input need special handling
3. **Remote Desktop**: Virtual display boundaries may not map correctly

## Future Enhancements

- **Curve Smoothing**: Bezier curves for even smoother transitions
- **Pressure Sensitivity**: Preserve pen tablet pressure across monitors
- **Multi-Cursor**: Support for multiple simultaneous cursors
- **Gesture Zones**: Special behaviors at monitor boundaries
- **AI Prediction**: Learn user patterns for predictive positioning