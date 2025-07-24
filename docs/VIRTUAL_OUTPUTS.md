# Virtual Outputs - Detailed Documentation

## Overview

Virtual Outputs is stilch's flagship feature that decouples logical display areas from physical monitor boundaries. This allows unprecedented flexibility in multi-monitor setups.

## Concepts

### Physical Output
A physical monitor connected to your system (e.g., DP-1, HDMI-A-1).

### Virtual Output  
A logical display area that can:
- Correspond 1:1 with a physical output (default)
- Be a subdivision of a physical output (split mode)
- Span across multiple physical outputs (merge mode)

### Workspace
A collection of tiled windows. stilch maintains a single global set of workspaces (by default 1-10) that can be displayed on any virtual output. Only one virtual output can show a given workspace at a time.

## Use Cases

### 1. Ultrawide Monitor Split

**Scenario**: You have a 34" ultrawide monitor (3440x1440). You want a main workspace area and a dedicated sidebar for chat/documentation.

**Solution**: Split the ultrawide into main (70%) and sidebar (30%) virtual outputs:

```
Physical: 3440x1440           Virtual: Main + Sidebar

┌──────────────────────┐      ┌──────────────┬────────┐
│                      │      │              │        │
│                      │  →   │      V1      │   V2   │
│   Ultrawide Monitor  │      │   (WS 1)     │ (WS 9) │
│                      │      │   2408x1440  │1032x1440
│                      │      │              │        │
└──────────────────────┘      └──────────────┴────────┘
```

Now you can:
- Keep your main work on V1 (workspace 1)
- Have persistent chat/docs/monitoring on V2 (workspace 9)
- Fullscreen videos/games expand only to V1, keeping sidebar visible
- Different workspace switching behavior for each area

### 2. 4K Monitor Quadrant Split

**Scenario**: You have a 32" 4K monitor. At 4K resolution, windows are either too small (quarter-tiled) or too large (half-tiled).

**Solution**: Split the 4K monitor into a 2x2 grid of 1080p virtual outputs:

```
Physical: 3840x2160          Virtual: Four 1920x1080 outputs

┌────────────────────┐       ┌─────────┬─────────┐
│                    │       │   V1    │   V2    │
│                    │  →    │  (WS 1) │  (WS 2) │
│     4K Monitor     │       ├─────────┼─────────┤
│                    │       │   V3    │   V4    │
│                    │       │  (WS 3) │  (WS 4) │
└────────────────────┘       └─────────┴─────────┘
```

Benefits:
- Display different workspaces on each quadrant
- Tile windows at comfortable 1080p sizes
- Each quadrant acts like an independent monitor

### 3. Unified Multi-Monitor Workspace

**Scenario**: You have two 27" 1080p monitors side-by-side and want windows to tile across both.

**Solution**: Merge both monitors into one virtual output:

```
Physical:                    Virtual:
┌──────────┐ ┌──────────┐   ┌─────────────────────┐
│ HDMI-1   │ │ HDMI-2   │   │         V1          │
│ 1920x1080│ │ 1920x1080│ → │     3840x1080       │
└──────────┘ └──────────┘   │  (showing one WS)   │
                             └─────────────────────┘
```

Benefits:
- Windows can tile across both monitors seamlessly
- Single workspace spans entire width
- No artificial boundary between monitors

### 4. Mixed Setup for Different Tasks

**Scenario**: 34" ultrawide (3440x1440) for coding, 27" 4K (3840x2160) for design work.

**Solution**: Split the 4K for reference materials, keep ultrawide unified:

```
Physical:                         Virtual:
┌─────────────────┐ ┌────────┐   ┌─────────────────┐ ┌────┬────┐
│   Ultrawide     │ │   4K   │   │       V1        │ │ V2 │ V3 │
│   3440x1440     │ │3840x2160│ → │   3440x1440     │ │1920│1920│
└─────────────────┘ └────────┘   │   (e.g. WS 1)   │ │x2160x2160
                                  └─────────────────┘ └────┴────┘
```

Configuration:
- V1: Full ultrawide for IDE/coding
- V2: Design tools
- V3: Reference materials, documentation

## Configuration

In `~/.config/stilch/config` (i3/sway format):

### Basic Virtual Output Setup

```bash
# Set physical output scaling (standard i3/sway)
output DP-2 scale 1.5

# Define virtual outputs using regions
# Format: virtual_output <name> outputs <physical_output> region <x,y,width,height>
virtual_output main outputs DP-2 region 0,0,2880,2160
virtual_output sidebar outputs DP-2 region 2880,0,960,2160
```

This creates two virtual outputs from a single physical monitor:
- `main`: 2880x2160 starting at (0,0)
- `sidebar`: 960x2160 starting at (2880,0)

### Example: Split 4K Monitor into Quadrants

```bash
output DP-1 scale 1.0
virtual_output top_left outputs DP-1 region 0,0,1920,1080
virtual_output top_right outputs DP-1 region 1920,0,1920,1080
virtual_output bottom_left outputs DP-1 region 0,1080,1920,1080
virtual_output bottom_right outputs DP-1 region 1920,1080,1920,1080
```

### Example: Ultrawide Split (70/30)

```bash
output DP-3 scale 1.0
# Main area takes 70% (2408px of 3440px)
virtual_output main outputs DP-3 region 0,0,2408,1440
# Sidebar takes 30% (1032px)
virtual_output sidebar outputs DP-3 region 2408,0,1032,1440
```

## Keybindings

Default keybindings for virtual output control:

```bash
# Cycle through virtual output configurations
bindsym $mod+v virtual_output cycle

# Focus virtual output by direction (stilch extension)
bindsym $mod+Alt+h focus output left
bindsym $mod+Alt+l focus output right
bindsym $mod+Alt+k focus output up
bindsym $mod+Alt+j focus output down

# Move window to virtual output
bindsym $mod+Shift+Alt+h move container to output left
bindsym $mod+Shift+Alt+l move container to output right

# Move workspace to different virtual output (like sway)
bindsym $mod+Ctrl+Alt+h move workspace to output left
bindsym $mod+Ctrl+Alt+l move workspace to output right

# Standard i3/sway output commands work too
bindsym $mod+Alt+1 focus output DP-1
bindsym $mod+Alt+2 focus output HDMI-1
```

## Current Status

### What Works
- Virtual outputs configured via config file using regions
- Each virtual output acts as an independent monitor
- Standard i3/sway commands work with virtual outputs
- Workspaces can be moved between virtual outputs

### Current Limitations
- No runtime IPC commands for changing virtual output configuration
- Virtual outputs must be defined in config file before starting
- Config changes require restart

## Implementation Details

### Coordinate Transformation

Virtual outputs maintain their own coordinate space:

```rust
// Physical coordinate to virtual
let virtual_coord = physical_coord - virtual_output.offset;

// Virtual to physical (for rendering)
let physical_coord = virtual_coord + virtual_output.offset;
```

### Window Constraints

Windows are constrained to their virtual output boundaries:

```rust
impl VirtualOutput {
    pub fn constrain_window(&self, window: &mut Window) {
        let bounds = self.bounds();
        window.geometry = window.geometry.intersection(bounds);
    }
}
```

### Workspace Display

Virtual outputs display workspaces from the global workspace pool:

```rust
pub struct VirtualOutput {
    id: VirtualOutputId,
    bounds: Rectangle,
    active_workspace: Option<WorkspaceId>,  // Currently displayed workspace
    // ...
}

// Global workspace management
pub struct WorkspaceManager {
    workspaces: Vec<Workspace>,  // Global set (1-10)
    output_mapping: HashMap<WorkspaceId, VirtualOutputId>,  // Which output shows which workspace
}
```

## Performance Considerations

- **Rendering**: Each virtual output triggers damage tracking independently
- **Memory**: Fixed workspace count regardless of virtual outputs
- **CPU**: Minimal overhead for coordinate transformation
- **GPU**: No additional overhead - rendering happens at physical output level

## Troubleshooting

### Virtual output configuration not applying

1. Check config file syntax in `~/.config/stilch/config`
2. Verify physical output name matches your hardware
3. Ensure regions don't overlap and are within physical monitor bounds
4. Restart stilch after config changes

### Finding your output names

Run stilch with debug logging to see detected outputs:
```bash
RUST_LOG=stilch=debug stilch
```

## Planned Features

- **Runtime configuration**: IPC commands to change virtual outputs without restart
- **Automatic splits**: Simple `split 2x2` syntax instead of manual regions
- **Per-virtual-output scaling**: Different DPI per virtual area
- **Multiple physical outputs**: Merge multiple monitors into one virtual output