use smithay::input::keyboard::{Keysym, ModifiersState};
use std::collections::HashMap;
use std::path::Path;

pub mod parser;

#[derive(Debug, Clone)]
pub struct Config {
    /// Variables defined with 'set'
    pub variables: HashMap<String, String>,
    /// Keybindings
    pub keybindings: Vec<Keybinding>,
    /// Output configurations
    pub outputs: Vec<OutputConfig>,
    /// Virtual output configurations
    pub virtual_outputs: Vec<VirtualOutputConfig>,
    /// Workspace configurations
    pub workspaces: Vec<WorkspaceConfig>,
    /// Gap settings
    pub gaps: GapConfig,
    /// Border settings
    pub border: BorderConfig,
    /// Font settings
    pub font: String,
    /// Startup commands (exec without keybinding)
    pub startup_commands: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Keybinding {
    pub modifiers: ModifiersState,
    pub key: Keysym,
    pub command: Command,
}

#[derive(Debug, Clone)]
pub enum Command {
    /// Execute a program
    Exec(String),
    /// Kill focused window
    Kill,
    /// Reload configuration
    Reload,
    /// Exit compositor
    Exit,
    /// Focus window in direction
    Focus(Direction),
    /// Move window in direction
    Move(Direction),
    /// Switch to workspace
    Workspace(WorkspaceTarget),
    /// Move container to workspace
    MoveToWorkspace(WorkspaceTarget),
    /// Layout commands
    Layout(LayoutCommand),
    /// Fullscreen toggle (default: virtual output)
    Fullscreen,
    /// Container fullscreen toggle
    FullscreenContainer,
    /// Virtual output fullscreen toggle
    FullscreenVirtualOutput,
    /// Physical output fullscreen toggle
    FullscreenPhysicalOutput,
    /// Floating toggle
    FloatingToggle,
    /// Focus mode toggle (tiling/floating)
    FocusModeToggle,
    /// Resize mode
    ResizeMode,
    /// Split orientation
    Split(Orientation),
    /// Move workspace to output
    MoveWorkspaceToOutput(Direction),
    /// Scratchpad commands
    Scratchpad(ScratchpadCommand),
    /// Custom/unimplemented command
    Raw(String),
    /// Debug command to swap first two windows
    DebugSwapWindows,
    /// Set horizontal split
    SplitHorizontal,
    /// Set vertical split  
    SplitVertical,
    /// Set automatic (BSP) split
    SplitAutomatic,
    /// Move tab left in tabbed/stacked container
    MoveTabLeft,
    /// Move tab right in tabbed/stacked container
    MoveTabRight,
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone)]
pub enum WorkspaceTarget {
    /// Workspace by number (1-10)
    Number(u8),
    /// Workspace by name
    Name(String),
    /// Previous workspace
    Previous,
    /// Next workspace
    Next,
}

#[derive(Debug, Clone, Copy)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone)]
pub enum LayoutCommand {
    Stacking,
    Tabbed,
    ToggleSplit,
    SplitH,
    SplitV,
}

#[derive(Debug, Clone)]
pub enum ScratchpadCommand {
    Show,
    Move,
}

#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub name: String,
    pub resolution: Option<(i32, i32)>,
    pub position: Option<(i32, i32)>,
    pub scale: Option<f64>,
    pub transform: Option<String>,
    pub background: Option<BackgroundConfig>,
    pub split: Option<(crate::virtual_output::SplitType, usize)>,
}

#[derive(Debug, Clone)]
pub struct VirtualOutputConfig {
    /// Name of the virtual output
    pub name: String,
    /// Physical outputs that make up this virtual output
    pub outputs: Vec<String>,
    /// Optional custom region within the physical outputs
    /// If not specified, uses the full area of all outputs
    pub region: Option<VirtualOutputRegion>,
}

#[derive(Debug, Clone)]
pub struct VirtualOutputRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone)]
pub struct BackgroundConfig {
    pub path: String,
    pub mode: String, // fill, stretch, fit, center, tile
}

#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    pub number: u8,
    pub output: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GapConfig {
    pub inner: Option<i32>,
    pub outer: Option<i32>,
    pub top: Option<i32>,
    pub bottom: Option<i32>,
    pub left: Option<i32>,
    pub right: Option<i32>,
    pub smart: bool,
}

#[derive(Debug, Clone)]
pub struct BorderConfig {
    pub width: i32,
    pub floating_width: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            variables: HashMap::new(),
            keybindings: Vec::new(),
            outputs: Vec::new(),
            virtual_outputs: Vec::new(),
            workspaces: Vec::new(),
            gaps: GapConfig::default(),
            border: BorderConfig::default(),
            font: "monospace 10".to_string(),
            startup_commands: Vec::new(),
        }
    }
}

impl Default for GapConfig {
    fn default() -> Self {
        Self {
            inner: None,
            outer: None,
            top: None,
            bottom: Some(7),
            left: Some(7),
            right: Some(7),
            smart: false,
        }
    }
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            width: 2,
            floating_width: 2,
        }
    }
}

impl Config {
    /// Load config from file
    pub fn load_from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        parser::parse_config(&content)
    }

    /// Get a variable value, expanding nested variables
    pub fn get_variable(&self, name: &str) -> Option<String> {
        self.variables.get(name).cloned()
    }

    /// Get a boolean variable value (compatible with i3/sway: yes/no, true/false, on/off, 1/0)
    pub fn get_bool(&self, name: &str) -> Option<bool> {
        self.get_variable(name).map(|v| {
            let v = v.to_lowercase();
            matches!(v.as_str(), "yes" | "true" | "on" | "1")
        })
    }

    /// Check if focus follows mouse is enabled (default: true)
    pub fn focus_follows_mouse(&self) -> bool {
        self.get_bool("focus_follows_mouse").unwrap_or(true)
    }

    /// Expand variables in a string
    pub fn expand_variables(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (name, value) in &self.variables {
            result = result.replace(&format!("${name}"), value);
        }
        result
    }
}
