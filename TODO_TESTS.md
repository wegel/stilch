# TODO: Comprehensive Test Suite for TunaWM

## ⚠️ CRITICAL: Test Code Must Use Production Code Paths

**NEVER REIMPLEMENT FUNCTIONALITY IN TEST CODE!**

All test commands MUST use the exact same code paths as the production compositor. Any reimplementation of compositor functionality in test code is a bug that:
- Defeats the purpose of testing
- Can hide real bugs (like the window close crash with Chromium)
- Creates maintenance burden with duplicate code
- Gives false confidence in test coverage

### Example of what NOT to do:
```rust
// BAD: Test reimplements window closing logic
if let Some(toplevel) = window.0.toplevel() {
    toplevel.send_close();
}
```

### Example of what TO do:
```rust
// GOOD: Test uses the same method as production
state.close_window(&window_elem);
```

Every test command should call the appropriate methods on AnvilState or other production components. If a needed method doesn't exist, CREATE IT in the production code first, then use it in both production and test code.

---

This document outlines all the tests we need to implement for comprehensive coverage of window manager functionality. Tests marked with ✅ are completed.

## Window Management Tests

### Basic Window Operations
- [x] Create single window and verify geometry
- [x] Create multiple windows and verify non-overlapping layout
- [x] Close window and verify remaining windows reflow  
- [x] Close last window on workspace (workspace becomes empty)
- [x] Window killed externally (process crash/kill handling)
- [ ] Window with minimum size constraints
- [ ] Window with maximum size constraints
- [ ] Window with fixed aspect ratio
- [ ] Window requesting specific initial size
- [ ] Window requesting fullscreen on creation
- [ ] Urgent window notification handling

### Window Focus
- [ ] Focus window by clicking
- [x] Focus window by ID via IPC
- [ ] Focus next window in workspace (Tab)
- [ ] Focus previous window in workspace (Shift+Tab)
- [ ] Focus window in direction (left/right/up/down)
- [ ] Focus follows mouse
- [ ] Focus wrapping at workspace edges
- [x] Focus transfers to next window when focused window closes
- [ ] Focus urgent window
- [ ] Focus window on different workspace (switches workspace)

### Window Movement
- [ ] Move window left/right/up/down within workspace
- [x] Move window to specific workspace by number
- [ ] Move window to next/previous workspace
- [ ] Move window and follow (switch to target workspace)
- [ ] Move window to output (multi-monitor)
- [ ] Move floating window with mouse
- [ ] Move floating window with keyboard (by increments)
- [ ] Swap two windows positions
- [ ] Move window to scratchpad
- [ ] Move window from scratchpad

### Window Resizing
- [ ] Resize window with mouse drag
- [ ] Resize window with keyboard (grow/shrink by increments)
- [ ] Resize constraints (min/max size)
- [ ] Resize floating window from any edge/corner
- [ ] Resize tiled window (adjusts split ratios)
- [ ] Reset window size to default
- [ ] Equalize all windows in workspace

## Layout Tests

### Tiling Layouts
- [ ] Horizontal split (new window appears to the right)
- [ ] Vertical split (new window appears below)
- [ ] Nested splits (H split inside V split, etc.)
- [ ] Toggle split direction for container
- [ ] Default layout for new windows
- [ ] Spiral layout
- [ ] Binary tree layout
- [ ] Master-stack layout (one large, others stacked)
- [ ] Columns layout
- [ ] Rows layout
- [ ] Grid layout

### Layout Manipulation
- [ ] Change layout of current container
- [ ] Promote window to parent container
- [ ] Demote window to child container
- [ ] Flatten container (remove unnecessary nesting)
- [ ] Tab container (multiple windows in tabs)
- [ ] Stack container (multiple windows stacked)
- [ ] Toggle between horizontal/vertical split
- [ ] Layout persistence across workspace switches
- [ ] Layout restoration after window removal

### Floating Windows
- [ ] Toggle window between tiling and floating
- [ ] Float window on creation (window hints)
- [ ] Floating window always on top
- [ ] Floating window z-order management
- [ ] Center floating window
- [ ] Floating window stays within screen bounds
- [ ] Dialog windows float by default
- [ ] Popup windows float by default
- [ ] Floating window on all workspaces (sticky)

### Fullscreen
- [ ] Toggle fullscreen for focused window
- [ ] Fullscreen window covers entire output
- [ ] Fullscreen with multiple monitors
- [ ] Fullscreen window on top of floating windows
- [ ] Exit fullscreen on window close
- [ ] Exit fullscreen on workspace switch
- [ ] Fullscreen inhibits idle/lock screen
- [ ] Multiple fullscreen windows on different workspaces

## Workspace Tests

### Basic Workspace Operations
- [x] Switch to workspace by number (0-9)
- [ ] Switch to next/previous workspace
- [ ] Switch to last active workspace (back-and-forth)
- [ ] Create workspace on demand (when moving window to it)
- [ ] Workspace with custom name
- [ ] Workspace with custom layout
- [ ] Empty workspace cleanup
- [ ] Workspace-specific wallpaper

### Workspace Window Management
- [ ] Move all windows from workspace A to B
- [ ] Swap workspaces
- [ ] Workspace groups (related workspaces)
- [ ] Workspace templates (predefined layouts)
- [ ] Save/restore workspace layout
- [ ] Workspace-specific rules (all windows float, etc.)

### Multi-Monitor Workspaces
- [ ] Workspace per monitor
- [x] Move workspace to different monitor/output
- [ ] Workspace spanning multiple monitors
- [ ] Primary monitor designation
- [ ] Monitor-specific workspace assignment

### Move Workspace to Output
- [x] Move workspace to output in direction (left/right/up/down)
- [x] Move workspace with all its windows
- [x] Workspace association persists after move
- [x] No-op when no output exists in direction
- [x] Multi-directional movement (2x2 grid test)
- [x] Focus follows workspace to new output

## Container Tests

### Container Hierarchy
- [ ] Create nested containers
- [ ] Maximum nesting depth enforcement
- [ ] Container focus vs window focus
- [ ] Container selection
- [ ] Container border highlighting
- [ ] Kill entire container (all windows within)

### Container Operations
- [ ] Split container horizontally
- [ ] Split container vertically
- [ ] Change container layout (tabbed/stacked/split)
- [ ] Move container to workspace
- [ ] Resize container
- [ ] Container marks (named containers)
- [ ] Jump to marked container

## Multi-Monitor Tests

### Monitor Management
- [ ] Detect monitor connection
- [ ] Detect monitor disconnection
- [ ] Handle monitor configuration change
- [ ] Primary monitor designation
- [ ] Monitor arrangement (left/right/above/below)
- [ ] Monitor with different resolutions
- [ ] Monitor with different DPI
- [ ] Mirror mode
- [ ] Extended desktop mode

### Cross-Monitor Operations
- [ ] Move window to monitor by direction
- [ ] Move window to monitor by name/number
- [ ] Focus monitor by direction
- [ ] Focus monitor by name/number
- [ ] Mouse cursor crosses monitor boundaries
- [ ] Window snap to monitor edges
- [ ] Fullscreen per monitor
- [ ] Workspace bar per monitor

## Virtual Output Tests

### Basic Virtual Output Operations
- [ ] Create virtual output from physical output (1:1 mapping)
- [ ] Split physical output into multiple virtual outputs
- [ ] Virtual output with custom geometry/region
- [ ] Remove/destroy virtual output
- [ ] List all virtual outputs
- [ ] Get virtual output properties (geometry, physical backing)
- [ ] Virtual output persistence across restarts

### Virtual Output Workspace Management
- [ ] Assign workspace to virtual output
- [ ] Move workspace between virtual outputs
- [ ] Show different workspaces on split virtual outputs simultaneously
- [ ] Virtual output with no workspace (empty state)
- [ ] Switch workspaces within a virtual output
- [ ] Workspace follows virtual output on reconfiguration

### Window Management on Virtual Outputs
- [ ] Window constrained to virtual output boundaries
- [ ] Move window between virtual outputs
- [ ] Window spanning multiple virtual outputs (edge case)
- [ ] Floating window respects virtual output boundaries
- [ ] Focus follows virtual output boundaries
- [ ] Mouse warping at virtual output edges

### Virtual Output Splitting Scenarios
- [ ] Split ultrawide monitor horizontally (2 virtual outputs)
- [ ] Split ultrawide monitor horizontally (3 virtual outputs)
- [ ] Split 4K monitor into quadrants (2x2 grid)
- [ ] Asymmetric splits (e.g., 70/30 split)
- [ ] Nested virtual outputs (split within split)
- [ ] Dynamic resizing of virtual output boundaries

### Virtual Output Merging
- [ ] Merge two adjacent virtual outputs on same physical display
- [ ] Merge virtual outputs across physical monitor boundaries
- [ ] Create virtual output spanning two physical monitors
- [ ] Create virtual output spanning three or more monitors
- [ ] Merge non-adjacent virtual outputs (should fail)
- [ ] Merge with different sized physical monitors
- [ ] Merge with different resolution monitors
- [ ] Unmerge/split previously merged virtual output
- [ ] Window behavior when virtual outputs merge
- [ ] Workspace assignment on merged virtual outputs
- [ ] Focus behavior across merged virtual output
- [ ] Fullscreen on merged cross-monitor virtual output

### Virtual Output Edge Cases
- [ ] Virtual output smaller than minimum window size
- [ ] Overlapping virtual outputs (should be prevented)
- [ ] Gaps between virtual outputs (should be prevented)
- [ ] Virtual output larger than physical output
- [ ] Zero-size virtual output (should be prevented)
- [ ] Virtual output with negative coordinates
- [ ] Virtual output partially off-screen
- [ ] Hotplug monitor with existing virtual output config

## Fullscreen Mode Tests

### Basic Fullscreen Operations
- [ ] Toggle fullscreen (default: virtual output mode)
- [ ] Enter fullscreen from tiled window
- [ ] Enter fullscreen from floating window
- [ ] Exit fullscreen to previous layout
- [ ] Fullscreen window has correct geometry
- [ ] Fullscreen window covers panels/bars
- [ ] Multiple fullscreen windows on different workspaces

### Container Fullscreen Mode
- [ ] Toggle container fullscreen mode
- [ ] Container fullscreen respects container boundaries
- [ ] Container fullscreen with single window (fills container)
- [ ] Container fullscreen with split container
- [ ] Container fullscreen preserves gaps
- [ ] Exit container fullscreen restores original size
- [ ] Container fullscreen in nested containers
- [ ] Switch between container and other fullscreen modes

### Virtual Output Fullscreen Mode
- [ ] Toggle virtual output fullscreen mode
- [ ] Virtual output fullscreen on single virtual output
- [ ] Virtual output fullscreen with split physical monitor
- [ ] Virtual output fullscreen respects virtual boundaries
- [ ] Move fullscreen window to different virtual output
- [ ] Fullscreen window on each virtual output simultaneously
- [ ] Exit virtual output fullscreen restores tiling

### Physical Display Fullscreen Mode
- [ ] Toggle physical display fullscreen mode
- [ ] Physical fullscreen spans entire monitor
- [ ] Physical fullscreen with multiple virtual outputs
- [ ] Physical fullscreen ignores virtual output boundaries
- [ ] Physical fullscreen on multi-monitor setup
- [ ] Move physical fullscreen window to different monitor
- [ ] Exit physical fullscreen restores previous mode

### Fullscreen Mode Transitions
- [ ] Switch from container to virtual output fullscreen
- [ ] Switch from container to physical fullscreen
- [ ] Switch from virtual to physical fullscreen
- [ ] Switch from physical to container fullscreen
- [ ] Switch from physical to virtual fullscreen
- [ ] Switch from virtual to container fullscreen
- [ ] Cycle through all fullscreen modes
- [ ] Direct transition vs exit-then-enter behavior

### Fullscreen with Special Windows
- [ ] Fullscreen video player behavior
- [ ] Fullscreen game behavior
- [ ] Fullscreen browser (F11 vs WM fullscreen)
- [ ] Fullscreen terminal emulator
- [ ] Fullscreen Electron app
- [ ] Fullscreen popup/dialog (should be prevented)
- [ ] Fullscreen with window decorations

### Fullscreen Edge Cases
- [ ] Fullscreen window loses focus
- [ ] Fullscreen window minimized
- [ ] Fullscreen window closed
- [ ] Fullscreen window moved to scratchpad
- [ ] Fullscreen window workspace switch
- [ ] New window created while in fullscreen
- [ ] Fullscreen with sticky window
- [ ] Multiple monitors with different fullscreen modes
- [ ] Fullscreen during workspace layout change

## IPC Tests

### Command Tests
- [ ] Execute all IPC commands
- [ ] Command with invalid parameters
- [ ] Command rate limiting
- [ ] Command batching
- [ ] Command transaction (all or nothing)
- [ ] Async command with callback
- [ ] Subscribe to events
- [ ] Unsubscribe from events

### Event Tests
- [ ] Window created event
- [ ] Window destroyed event
- [ ] Window focus changed event
- [ ] Workspace changed event
- [ ] Layout changed event
- [ ] Monitor changed event
- [ ] Mode changed event
- [ ] Binding triggered event
- [ ] Configuration reloaded event

### Query Tests
- [ ] Get window tree
- [ ] Get workspace list
- [ ] Get monitor list
- [ ] Get binding list
- [ ] Get configuration
- [ ] Get version info
- [ ] Get current mode
- [ ] Get marks
- [ ] Get bar configuration

## Configuration Tests

### Configuration Loading
- [x] Load configuration file (via --config flag and env var)
- [ ] Reload configuration without restart
- [ ] Configuration syntax error handling
- [ ] Configuration validation
- [ ] Default configuration fallback
- [ ] Configuration includes
- [ ] Configuration variables
- [ ] Environment variable expansion

### Runtime Configuration
- [ ] Change key bindings at runtime
- [ ] Change mouse bindings at runtime
- [ ] Change workspace names at runtime
- [ ] Change colors at runtime
- [ ] Change fonts at runtime
- [ ] Change gaps at runtime
- [ ] Change borders at runtime

### Gap Configuration
- [x] Inner gaps between windows working correctly
- [x] No gaps configuration (gaps = 0)
- [x] Gap configuration loaded from config file
- [ ] Outer gaps from screen edges
- [ ] Different gaps per workspace
- [ ] Dynamic gap adjustment at runtime

## Window Rules and Criteria

### Window Matching
- [ ] Match window by class
- [ ] Match window by instance
- [ ] Match window by title
- [ ] Match window by role
- [ ] Match window by type
- [ ] Match with regex
- [ ] Match with multiple criteria (AND)
- [ ] Match with alternative criteria (OR)

### Rule Actions
- [ ] Assign window to workspace
- [ ] Float window
- [ ] Set window border
- [ ] Set window size
- [ ] Set window position
- [ ] Focus on creation
- [ ] Move to scratchpad
- [ ] Make sticky
- [ ] Set opacity

## Input Tests

### Keyboard
- [ ] Key press triggers binding
- [ ] Key combinations (Mod+Shift+Key)
- [ ] Key repeat handling
- [ ] Mode-specific bindings
- [ ] Binding with arguments
- [ ] Binding to shell command
- [ ] Binding to IPC command
- [ ] Escape special keys in bindings
- [ ] International keyboard layouts

### Mouse
- [ ] Mouse binding on window
- [ ] Mouse binding on titlebar
- [ ] Mouse binding on border
- [ ] Mouse binding on root window
- [ ] Mouse drag to move window
- [ ] Mouse drag to resize window
- [ ] Mouse scroll on titlebar
- [ ] Mouse scroll on border
- [ ] Disable mouse warping

## Special Features

### Marks
- [ ] Mark window with identifier
- [ ] Jump to marked window
- [ ] Move marked window
- [ ] Unmark window
- [ ] List all marks
- [ ] Mark conflicts (duplicate marks)

### Modes
- [ ] Enter resize mode
- [ ] Enter move mode
- [ ] Custom user modes
- [ ] Mode-specific bindings
- [ ] Mode timeout
- [ ] Mode indicator
- [ ] Nested modes

### Gaps and Borders
- [ ] Inner gaps between windows
- [ ] Outer gaps to screen edges
- [ ] Smart gaps (hide when one window)
- [ ] Border styles (normal/pixel/none)
- [ ] Border colors (focused/unfocused/urgent)
- [ ] Title bar formatting
- [ ] Hide borders at screen edge

## Session Management

### State Persistence
- [ ] Save session on exit
- [ ] Restore session on start
- [ ] Save workspace layouts
- [ ] Restore window positions
- [ ] Restore window sizes
- [ ] Restore floating state
- [ ] Restore marks
- [ ] Restore scratchpad

### Restart and Reload
- [ ] In-place restart preserves windows
- [ ] Configuration reload
- [ ] Restart with different config
- [ ] Emergency restart
- [ ] Clean shutdown
- [ ] Crash recovery

## Performance Tests

### Stress Tests
- [ ] Create 100+ windows
- [ ] Rapid workspace switching
- [ ] Rapid window creation/destruction
- [ ] Large number of workspaces
- [ ] Deep nesting levels
- [ ] Many floating windows
- [ ] Animated wallpapers
- [ ] High-frequency IPC commands

### Resource Tests
- [ ] Memory leak detection
- [ ] CPU usage monitoring
- [ ] GPU usage monitoring
- [ ] File descriptor limits
- [ ] Window count limits
- [ ] Workspace count limits

## Integration Tests

### Application Compatibility
- [ ] Terminal emulators
- [ ] Web browsers
- [ ] Electron applications
- [ ] GTK applications
- [ ] Qt applications
- [ ] Java applications
- [ ] Games (fullscreen/windowed)
- [ ] Video players
- [ ] Screen sharing applications

### Desktop Environment Integration
- [ ] System tray support
- [ ] Notification daemon
- [ ] Application launcher
- [ ] Desktop widgets
- [ ] Panel/bar integration
- [ ] Compositor integration
- [ ] Screen locker
- [ ] Power management
- [ ] Media keys

### Protocol Support
- [ ] X11 compatibility layer (Xwayland)
- [ ] XDG shell protocol
- [ ] Layer shell protocol
- [ ] Input method protocol
- [ ] Drag and drop
- [ ] Clipboard/selection
- [ ] Screen capture protocol
- [ ] Remote desktop protocol

## Error Handling Tests

### Graceful Degradation
- [ ] Window creation failure
- [ ] Out of memory
- [ ] Display connection lost
- [ ] Monitor disconnected while in use
- [ ] Invalid configuration
- [ ] Corrupted state file
- [ ] Permission denied
- [ ] Resource exhaustion

### Recovery Tests
- [ ] Recover from crashed window
- [ ] Recover from IPC disconnect
- [ ] Recover from render error
- [ ] Recover from input device error
- [ ] Automatic workspace cleanup
- [ ] Orphaned window handling

## Accessibility Tests

### Keyboard Navigation
- [ ] Full keyboard control (no mouse required)
- [ ] Vim-like navigation
- [ ] Screen reader support
- [ ] High contrast mode
- [ ] Large text mode
- [ ] Reduced motion mode

### Visual Aids
- [ ] Window focus indicators
- [ ] Active workspace indicators
- [ ] Flash window on focus
- [ ] Window previews
- [ ] Workspace previews
- [ ] Magnifier integration

## Security Tests

### Sandboxing
- [ ] Window isolation
- [ ] Input isolation
- [ ] Screen capture permissions
- [ ] Clipboard access control
- [ ] IPC authentication
- [ ] Privileged operations

### Input Validation
- [ ] IPC command injection
- [ ] Configuration injection
- [ ] Window title sanitization
- [ ] Size/position bounds checking
- [ ] Resource limit enforcement

---

## Test Infrastructure

### Test Utilities Needed
- [ ] Window creation helper
- [ ] IPC client library
- [ ] Layout assertion helpers
- [ ] Visual diff tools for ASCII/screenshots
- [ ] Performance profiling tools
- [ ] Test fixture management
- [ ] Parallel test execution
- [ ] Test coverage reporting
- [ ] Continuous integration setup

### Test Organization
- Tests should be organized by feature area
- Each test should be independent and idempotent
- Tests should clean up after themselves
- Fast tests should run first
- Slow/stress tests in separate suite
- Visual tests should output ASCII art for verification
