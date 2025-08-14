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
   - Diagonal movement becomes broken across boundaries
   - Cursor position doesn't map correctly

3. **Resolution Scaling**: Laptop at 150% scaling next to monitor at 100%
   - Cursor size changes abruptly
   - Movement sensitivity changes

## stilch's Solution

### Physical Layout Manager

stilch implements a PhysicalLayoutManager that tracks display positions and sizes in real-world millimeters, enabling accurate cursor mapping across displays with different scales and DPIs.

### Unified Physical Space

stilch treats all monitors as part of one continuous canvas measured in physical units (millimeters), not pixels:

```
Physical Space Mapping:
┌──────────────────────────────────────┐
│       Unified Physical Canvas        │
│                                      │
│  [Display1]      [Display2]          │
│  ┌─────────┐     ┌──────────┐       │
│  │ 300mm   │ gap │  300mm    │       │
│  │ x       │     │  x        │       │
│  │ 200mm   │     │  200mm    │       │
│  └─────────┘     └──────────┘       │
│                                      │
└──────────────────────────────────────┘

Cursor position tracked in mm, jumps gaps automatically
```

### Key Principles

1. **Physical Continuity**: Cursor maintains physical position across boundaries
2. **Predictable Paths**: Straight lines remain straight across monitors
3. **Gap Jumping**: Intelligent warping across physically separated displays
4. **Boundary-Based Detection**: Direction determined by which display edge was crossed

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

### Implementation Details

The PhysicalLayoutManager tracks:
- Display physical dimensions in millimeters
- Display physical positions in millimeters  
- Logical to physical coordinate mapping
- Normalized position (0.0-1.0) for scale-independent transitions

When the cursor moves, the system:
1. Converts logical position to physical millimeters
2. Checks if the position is within current display bounds
3. If outside bounds, determines which boundary was crossed
4. Searches for displays in that direction
5. If a gap exists, jumps to the nearest valid display
6. Maps the physical position back to logical coordinates for the target display

## Configuration

### Physical Layout Configuration

```bash
# Configure physical size and position in stilch config
output HDMI-A-1 scale 1.6 transform 270 physical_size 521x470mm physical_position 37,0mm
output DP-1 scale 1.0 position 0,1800 physical_size 291x105mm physical_position -9.5,480mm
output DP-2 scale 1.0 position 1920,1800 physical_size 291x105mm physical_position 313.5,480mm
```

The physical layout configuration is done through the output commands in the stilch config file.


## Special Cases

### Gaps Between Monitors

stilch automatically handles physical gaps between monitors using boundary-based gap jumping:

```bash
# Displays with a physical gap between them
output DP-1 physical_size 300x200mm physical_position 0,0mm
output DP-2 physical_size 300x200mm physical_position 400,0mm  # 100mm gap

# The gap is automatically detected and cursor will jump across when
# crossing the boundary in the direction of the other display
```


## Testing

The physical layout with gap jumping can be tested by configuring displays with gaps and moving the cursor between them. The cursor will automatically jump gaps when crossing display boundaries.

## Known Limitations

1. **Application Cursor Warping**: Some applications that warp cursor may conflict with gap jumping
2. **Transform Support**: Rotated displays (transform 90/270) are supported through normalized coordinates