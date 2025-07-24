# Intelligent Cursor Transitions - Detailed Documentation

## Overview

stilch's Intelligent Cursor Transition system solves the jarring "cursor jump" problem that occurs when moving between monitors with different DPIs, sizes, or resolutions. Instead of treating each monitor as an independent coordinate system, stilch creates a unified visual space where cursor movement feels natural and continuous.

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

### Unified Visual Space

stilch treats all monitors as part of one continuous canvas measured in visual units (millimeters), not pixels:

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

1. **Visual Continuity**: Cursor maintains visual position across boundaries
2. **Velocity Preservation**: Movement speed stays consistent
3. **Predictable Paths**: Straight lines remain straight across monitors
4. **Size Consistency**: Cursor appears same physical size on all displays

## Implementation

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

### Basic Settings

```toml
[cursor]
# Enable intelligent transitions
intelligent_transitions = true

# Cursor acceleration profile
acceleration = "adaptive"  # none, linear, adaptive

# Edge resistance (milliseconds to "stick" at edges)
edge_resistance = 50

# Cursor size scaling
unified_size = true  # Keep visual size consistent
```

### Per-Output Configuration

```toml
[[outputs]]
name = "DP-1"
# Override detected DPI if needed
dpi = 96

# Cursor speed multiplier for this output
cursor_speed = 1.0

# Alignment with other outputs
alignment = "top"  # top, middle, bottom

[[outputs]]
name = "HDMI-1"
dpi = 192
cursor_speed = 1.0
alignment = "middle"
```

### Advanced Behavior

```toml
[cursor.transitions]
# Smooth transition time when crossing boundaries (ms)
smoothing = 16

# Prediction for smooth motion
prediction = true

# Snap to edges of windows when moving slowly
edge_snapping = true
snap_distance = 10  # pixels

# Different behavior for different input devices
[[cursor.devices]]
name = "Gaming Mouse"
acceleration = "none"
speed = 1.5

[[cursor.devices]]
name = "Trackpad"
acceleration = "adaptive"
speed = 1.0
natural_scrolling = true
```

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

Override automatic alignment:

```toml
[[outputs]]
name = "DP-1"
position = { x = 0, y = 0 }
physical_size = { width = 597, height = 336 }  # mm

[[outputs]]
name = "HDMI-1"
position = { x = 597, y = 50 }  # 50mm vertical offset
physical_size = { width = 531, height = 298 }
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

Handle physical gaps:

```toml
[[outputs]]
name = "DP-1"
position = { x = 0, y = 0 }

[[outputs]]
name = "DP-2"
position = { x = 600, y = 0 }  # 600mm from origin
physical_gap = 50  # 50mm physical gap

[cursor.gaps]
behavior = "jump"  # jump, resist, or warp
resistance_time = 100  # ms to resist before jumping
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

## Accessibility

### Large Cursor Support

Maintain accessibility settings:

```toml
[accessibility.cursor]
size = "large"  # normal, large, extra-large
high_contrast = true
color = "#FFFF00"  # Yellow for visibility
```

### Cursor Trails

Help track cursor across boundaries:

```toml
[cursor.trails]
enabled = true
length = 5  # Number of trail segments
fade_time = 200  # ms
```

## Testing & Debugging

### Debug Visualization

Enable cursor debug overlay:

```toml
[debug.cursor]
show_position = true  # Show coordinates
show_velocity = true  # Show movement vector
show_boundaries = true  # Highlight monitor boundaries
show_prediction = true  # Show predicted path
```

### Testing Commands

```bash
# Test cursor transition
stilchsg cursor test-transition DP-1 HDMI-1

# Show cursor info
stilchsg cursor info

# Simulate different DPI
stilchsg debug set-dpi DP-1 192
```

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