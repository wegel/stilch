# stilch - A Tiling Wayland Compositor with Virtual Outputs

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-%3E%3D1.70-orange.svg)](https://www.rust-lang.org)
[![Wayland](https://img.shields.io/badge/wayland-native-blue.svg)](https://wayland.freedesktop.org/)

> ⚠️ **Alpha Software**: stilch is under active development and not yet production-ready. Expect bugs, breaking changes, and missing features. Use at your own risk!

stilch is a modern tiling Wayland compositor built with [Smithay](https://github.com/Smithay/smithay) that maintains i3/sway compatibility while introducing unique features for multi-monitor productivity workflows.

## 🌟 Why stilch?

stilch isn't just another tiling compositor - it introduces three groundbreaking features that solve real multi-monitor workflow problems:

### 🖥️ Virtual Outputs - Split & Merge Your Monitors

**Problem**: Traditional compositors tie workspaces to physical monitors. When you have multiple monitors of different sizes/resolutions, managing windows across them becomes cumbersome.

**Solution**: stilch introduces **Virtual Outputs** - logical display areas that can span across or subdivide physical monitors.

- **Split** a 4K monitor into four 1080p virtual outputs, each with independent workspaces
- **Merge** two 1080p monitors into one unified workspace
- **Mix** different splits - e.g., split your 4K monitor in half while keeping side monitors unified
- Workspaces belong to virtual outputs, not physical monitors
- Windows seamlessly tile within virtual output boundaries

```
Physical Setup:          Virtual Configuration:
┌──────────┐            ┌─────┬─────┐
│  4K Mon  │     →      │ V1  │ V2  │  (4K split into 2)
└──────────┘            └─────┴─────┘

┌────┐ ┌────┐           ┌───────────┐
│1080│ │1080│    →      │    V3     │  (Two 1080p merged)
└────┘ └────┘           └───────────┘
```

[📖 Full Virtual Outputs Documentation](docs/VIRTUAL_OUTPUTS.md)

### 🔲 Three-Tier Fullscreen System

**Problem**: Want to fullscreen a video on one monitor while working on another? Traditional fullscreen makes this hard.

**Solution**: stilch offers three intelligent fullscreen modes:

1. **Container Fullscreen** (`Mod+f`) - Fullscreens within the window's container (tile)
2. **Virtual Output Fullscreen** (`Mod+Shift+f`) - Takes over the current workspace only  
3. **Monitor Fullscreen** (`Mod+Ctrl+f`) - Traditional fullscreen across the entire physical's output

Perfect for:
- Watching videos while coding
- Fullscreening reference material in one tile
- Gaming on one monitor without disrupting other workspaces

[📖 Full Fullscreen Modes Documentation](docs/FULLSCREEN_MODES.md)

### 🖱️ Smooth Cursor Transitions

**Problem**: Moving between monitors with different DPIs causes cursor "jumps" - the cursor position suddenly shifts due to pixel density differences and not accounting for the physical position of one screen vs the other.

**Solution**: stilch provides smooth cursor transitions between monitors:

- No position jumps at monitor boundaries
- Cursor position preserved relative to visual space
- Smooth movement between different DPI displays
- Natural feel across multi-monitor setups

```
Traditional:                 stilch:
[1080p]  →  [4K]            [1080p]  →  [4K]
Cursor ─────┐               Cursor ──────────
            ↓ (jumps!)                (smooth)
         ───┘                         ───────
```

[📖 Full Cursor Transitions Documentation](docs/CURSOR_TRANSITIONS.md)

## ✨ Features

### Core Tiling Features
- **i3/sway compatible** configuration and keybindings
- **Dynamic tiling** with configurable gaps
- **Tabbed & stacking** container layouts
- **Floating windows** with proper stacking
- **10 workspaces** by default (configurable)
- **Smart focus** follows mouse or keyboard

### Display & Rendering
- **Multi-GPU support** with buffer sharing
- **Hardware acceleration** via GBM/EGL
- **Damage tracking** for efficient redraws
- **HiDPI support** with fractional scaling
- **Screen capture** via DMA-BUF

### Wayland Protocol Support
- ✅ **XDG Shell** - Native Wayland applications
- ✅ **XWayland** - Legacy X11 application support
- ✅ **Layer Shell** - Panels & overlays (waybar, rofi, etc.)
- ✅ **DMA-BUF** - Zero-copy rendering
- ✅ **Explicit Sync** - Latest synchronization protocol
- ✅ **wp-viewporter** - Viewport scaling
- ✅ **wp-fractional-scale** - Fractional HiDPI scaling

## 🚀 Quick Start

### Prerequisites

```bash
# Debian/Ubuntu
sudo apt install libudev-dev libinput-dev libgbm-dev libxkbcommon-dev \
                 libwayland-dev libsystemd-dev libseat-dev

# Fedora
sudo dnf install systemd-devel libinput-devel libgbm-devel \
                 libxkbcommon-devel wayland-devel libseat-devel

# Arch
sudo pacman -S udev libinput libgbm libxkbcommon wayland seatd
```

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/stilch
cd stilch

# Build in release mode
cargo build --release

# Install to system (optional)
sudo cp target/release/stilch /usr/local/bin/
```

### Running stilch

#### From a TTY (Recommended)
```bash
# Switch to a TTY (Ctrl+Alt+F2)
stilch --tty-udev
```

#### Nested in Another Compositor (Testing)
```bash
# In Wayland
stilch --winit

# In X11
stilch --x11
```

#### With Custom Config
```bash
stilch --config ~/.config/stilch/config
```

## ⚙️ Configuration

stilch uses i3/sway-compatible configuration for familiarity. Default location: `~/.config/stilch/config`

### Basic Configuration

```bash
# Mod key (Mod4 = Super/Windows key)
set $mod Mod4

# Terminal emulator
set $term alacritty

# Application launcher
set $menu rofi -show drun

# Gaps
gaps inner 10
gaps outer 5

# Focus follows mouse
focus_follows_mouse yes

# Default layout
workspace_layout default
```

### Virtual Output Configuration

```bash
# Split a 4K monitor into 2x2 grid
virtual_output DP-1 split 2x2

# Merge two monitors horizontally
virtual_output HDMI-1,HDMI-2 merge horizontal

# Custom virtual output with specific region
virtual_output "MyVirtual" outputs DP-1 region 0,0,1920,1080
```

### Key Bindings

```bash
# Launch terminal
bindsym $mod+Return exec $term

# Launch menu
bindsym $mod+d exec $menu

# Kill focused window
bindsym $mod+q kill

# Change focus (vim keys)
bindsym $mod+h focus left
bindsym $mod+j focus down
bindsym $mod+k focus up
bindsym $mod+l focus right

# Move windows
bindsym $mod+Shift+h move left
bindsym $mod+Shift+j move down
bindsym $mod+Shift+k move up
bindsym $mod+Shift+l move right

# Workspaces
bindsym $mod+1 workspace number 1
bindsym $mod+2 workspace number 2
# ... through 9
bindsym $mod+0 workspace number 10

# Move to workspace
bindsym $mod+Shift+1 move container to workspace number 1
# ... etc

# Fullscreen modes (stilch special)
bindsym $mod+f fullscreen container
bindsym $mod+Shift+f fullscreen workspace
bindsym $mod+Control+f fullscreen global

# Layout modes
bindsym $mod+s layout stacking
bindsym $mod+w layout tabbed
bindsym $mod+e layout toggle split

# Floating
bindsym $mod+Shift+space floating toggle
bindsym $mod+space focus mode_toggle

# Split orientation
bindsym $mod+b splith
bindsym $mod+v splitv

# Resize mode
mode "resize" {
    bindsym h resize shrink width 10 px
    bindsym j resize grow height 10 px
    bindsym k resize shrink height 10 px
    bindsym l resize grow width 10 px
    
    bindsym Escape mode "default"
}
bindsym $mod+r mode "resize"

# Exit
bindsym $mod+Shift+e exit
```

[📖 Full Configuration Guide](docs/CONFIGURATION.md)

## 🏗️ Architecture

stilch is built on a modular architecture leveraging Smithay's compositor framework:

```
┌─────────────────────────────────────┐
│         Wayland Clients             │
└─────────────┬───────────────────────┘
              │ Wayland Protocol
┌─────────────▼───────────────────────┐
│          Protocol Handlers          │
│  (XDG, Layer Shell, DMA-BUF, etc)   │
└─────────────┬───────────────────────┘
              │
┌─────────────▼───────────────────────┐
│        StilchState (Core)            │
│  ┌─────────────────────────────┐    │
│  │   VirtualOutputManager      │    │
│  ├─────────────────────────────┤    │
│  │   WorkspaceManager          │    │
│  ├─────────────────────────────┤    │
│  │   WindowManager             │    │
│  ├─────────────────────────────┤    │
│  │   LayoutTree (Tiling)       │    │
│  └─────────────────────────────┘    │
└─────────────┬───────────────────────┘
              │
┌─────────────▼───────────────────────┐
│     Backend (udev/winit/x11)        │
└─────────────────────────────────────┘
```

Key Components:
- **VirtualOutputManager**: Manages virtual output configuration and mapping
- **WorkspaceManager**: Handles workspace switching and window assignment
- **WindowManager**: Tracks windows and their properties
- **LayoutTree**: Implements tiling algorithms and container management

## 🧪 Development

### Running Tests

```bash
# Run all tests
cargo test

# Run integration tests only
cargo test --test '*'

# Run with logging
RUST_LOG=debug cargo test
```

### Debug Mode

```bash
# Run with debug logging
RUST_LOG=stilch=debug cargo run -- --winit

# Run with trace logging for specific module
RUST_LOG=stilch::virtual_output=trace cargo run -- --winit
```

## 🤝 Contributing

We welcome contributions! stilch is under active development and there are many ways to help.

### Getting Started

1. Fork the repository on GitHub
2. Clone your fork: `git clone https://github.com/yourusername/stilch`
3. Create a branch: `git checkout -b feature/your-feature-name`
4. Make your changes and commit
5. Push and create a Pull Request

### Development Guidelines

- **Code Style**: Run `cargo fmt` and `cargo clippy` before committing
- **Testing**: Add tests for new features, ensure existing tests pass
- **Commits**: Use clear, descriptive commit messages (e.g., "add virtual output splitting")
- **Documentation**: Update docs if you change behavior

### Areas for Contribution

**High Priority:**
- Protocol implementations (additional Wayland protocols)
- Performance optimizations
- Bug fixes (check issue tracker)
- More integration and unit tests

**Feature Ideas:**
- IPC improvements
- Configuration hot-reload
- Animation support
- Touch gesture support
- Additional tiling layouts (spiral, BSP, etc.)

### Project Structure

```
stilch/
├── src/
│   ├── main.rs              # Entry point
│   ├── state/               # Compositor state management
│   ├── shell/               # Window management
│   ├── workspace/           # Workspace and tiling logic
│   ├── virtual_output.rs    # Virtual output system
│   ├── config/              # Configuration parsing
│   ├── handlers/            # Wayland protocol handlers
│   └── window/              # Window tracking
├── tests/                   # Integration tests
└── docs/                    # Documentation
```

### Testing

```bash
# Run all tests
cargo test

# Run with logging
RUST_LOG=debug cargo test

# Test specific feature
cargo test test_virtual_output_split
```

By contributing to stilch, you agree that your contributions will be licensed under the MIT License

## 📊 Project Status

- ✅ **Core tiling functionality** - Complete
- ✅ **Virtual outputs** - Complete
- ✅ **Multi-fullscreen modes** - Complete
- ✅ **i3/sway compatibility** - ~40% complete
- 🚧 **Cursor transitions** - In progress
- 🚧 **IPC interface** - Basic implementation

## 📄 License

stilch is licensed under the MIT License. See [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

- [Smithay](https://github.com/Smithay/smithay) - The Wayland compositor library that makes this possible
- [sway](https://github.com/swaywm/sway) - Inspiration for configuration and behavior
- [niri](https://github.com/YaLTeR/niri) - Reference for modern Smithay usage
- The Wayland community for protocols and specifications

## 📬 Contact

- **Issues**: [GitHub Issues](https://github.com/wegel/stilch/issues)
- **Discussions**: [GitHub Discussions](https://github.com/wegel/stilch/discussions)

---

**stilch** - *stitch + tiling*

Optimize your monitor for your workflow.

