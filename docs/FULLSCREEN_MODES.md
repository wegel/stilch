# Fullscreen Modes - Detailed Documentation

## Overview

stilch provides three distinct fullscreen modes, solving the common problem of wanting to fullscreen content without losing access to other windows or workspaces.

## The Problem with Traditional Fullscreen

In most window managers, fullscreen is binary - a window either:
- Is normal (tiled/floating)
- Takes over the entire monitor

This creates workflow problems:
- Can't watch a video while coding
- Can't fullscreen reference material in one area
- Gaming takes over entire workspace
- Multiple monitors feel disconnected

## stilch's Three-Tier Fullscreen System

### 1. Container Fullscreen

**What it does**: Maximizes the window within its current container. When there's only one container in the workspace, this is equivalent to Virtual Output fullscreen.

**Use cases**:
- Temporarily focus on one window in a split layout
- Hide UI chrome while keeping other windows visible
- Quick maximize without affecting other containers

**Behavior with multiple containers**:
```
Before:                        Container Fullscreen:
┌─────────┬─────────┐         ┌─────────┬─────────┐
│ Video   │  Code   │   →     │ ██████  │  Code   │
│         │         │         │ ██████  │         │
└─────────┴─────────┘         └─────────┴─────────┘
                              Video fills its container only
```

**Behavior with single container** (same as Virtual Output):
```
Before:                        Container Fullscreen:
┌───────────────────┐         ┌───────────────────┐
│      Video        │   →     │   ████████████    │
└───────────────────┘         └───────────────────┘
                              Fills entire virtual output
```

**Key points**:
- With multiple containers: stays within container boundaries
- With single container: expands to full virtual output
- Other containers remain visible and interactive
- Useful for temporary focus without disrupting layout

### 2. Virtual Output Fullscreen

**What it does**: Expands window to fill the current virtual output.

**Use cases**:
- Gaming on one virtual output while keeping others for chat/browser
- Fullscreen video on main virtual output, work on sidebar
- Using split monitors effectively

**Behavior**:
```
Split ultrawide (main + sidebar):
┌──────────────┬────────┐     ┌──────────────┬────────┐
│    Main      │Sidebar │     │              │Sidebar │
│   (WS 1)     │ (WS 9) │ →   │  Video Full  │ (WS 9) │
│              │        │     │              │        │
└──────────────┴────────┘     └──────────────┴────────┘
```

**Key points**:
- Takes over current virtual output only
- Other virtual outputs remain unchanged
- Respects virtual output boundaries
- Can switch workspaces on other virtual outputs

### 3. Physical Output Fullscreen

**What it does**: Traditional fullscreen - takes over the entire physical output.

**Use cases**:
- Gaming with maximum immersion
- Professional video/photo editing
- Presentations on projectors
- Compatibility with applications expecting traditional fullscreen

**Behavior**:
```
All Virtual Outputs:           Physical Output Fullscreen:
┌──────────────┬────────┐     ┌────────────────────────┐
│     V1       │   V2   │     │                        │
│              │        │ →   │     Window Full        │
│              │        │     │                        │
└──────────────┴────────┘     └────────────────────────┘
```

**Key points**:
- Covers entire physical monitor
- Ignores virtual output boundaries
- Hides panels/bars
- Other physical monitors remain interactive

## Configuration

### Keybindings

In `~/.config/stilch/config` (i3/sway format):

```bash
# Container fullscreen (within tile)
bindsym $mod+f fullscreen container

# Virtual output fullscreen
bindsym $mod+Shift+f fullscreen virtual

# Physical output fullscreen
bindsym $mod+Ctrl+f fullscreen output

# Toggle through modes
bindsym $mod+Alt+f fullscreen toggle

# Exit fullscreen
bindsym Escape fullscreen disable
```

## Interaction with Virtual Outputs

Fullscreen modes respect virtual output boundaries:

### Split Physical Output
```
Ultrawide split into main + sidebar:
┌──────────────┬────────┐
│    Main      │Sidebar │
│     V1       │   V2   │
└──────────────┴────────┘

Virtual output fullscreen in V1:
┌──────────────┬────────┐
│   ████████   │   V2   │  <- Only V1 affected
└──────────────┴────────┘

Physical output fullscreen:
┌────────────────────────┐
│     ████████████       │  <- Entire monitor
└────────────────────────┘
```

## State Management

### Fullscreen State

The window manager tracks fullscreen state per window:

```rust
pub enum FullscreenMode {
    None,
    Container,      // Within container boundaries
    VirtualOutput,  // Within virtual output
    PhysicalOutput, // Entire physical monitor
}
```

### State Preservation

When exiting fullscreen, windows return to their previous position and size. The geometry is saved before entering fullscreen and restored on exit.

## Multi-Monitor Behavior

### Independent Fullscreen per Output

Each output (physical or virtual) maintains independent fullscreen state:

```
Monitor 1: Virtual output fullscreen (video)
Monitor 2: Normal tiling (work)
Monitor 3: Physical output fullscreen (game)
```

### Focus Behavior

- **Container/Virtual fullscreen**: Focus can move to other outputs
- **Physical fullscreen**: Focus typically stays on fullscreen window

## Current Implementation Status

### What Works
- Three fullscreen modes (Container, Virtual Output, Physical Output)
- State preservation when exiting fullscreen
- Independent fullscreen per output
- Basic keybinding support

### Limitations
- No per-application rules yet
- No automatic fullscreen detection for videos
- No presentation mode
- No direct scanout optimization
- Limited IPC control

## Troubleshooting

### Window won't fullscreen

1. Check if the window supports fullscreen (some popup windows don't)
2. Verify keybinding is correctly set in config
3. Try different fullscreen modes

### Fullscreen not respecting virtual output boundaries

- Ensure you're using virtual output fullscreen, not physical
- Check virtual output configuration is correct

### Performance in fullscreen

- Currently no special optimizations for fullscreen
- Performance should be similar to normal windowed mode

## Future Enhancements

- **Direct scanout**: Zero-copy rendering for fullscreen windows
- **Per-app rules**: Configure default fullscreen behavior per application
- **Video detection**: Automatically use appropriate fullscreen for videos
- **Fullscreen animations**: Smooth transitions between modes
