//! ASCII renderer backend for testing
//!
//! This backend renders the compositor state as ASCII art and provides
//! a command interface for programmatic testing.

use crate::window::WindowId;
use smithay::utils::{Logical, Point, Rectangle, Size};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::error;

/// ASCII grid dimensions
const DEFAULT_WIDTH: usize = 80;
const DEFAULT_HEIGHT: usize = 24;

/// Box drawing characters for different window states
mod box_chars {
    // Normal window (single line)
    pub const NORMAL_TL: char = '┌';
    pub const NORMAL_TR: char = '┐';
    pub const NORMAL_BL: char = '└';
    pub const NORMAL_BR: char = '┘';
    pub const NORMAL_H: char = '─';
    pub const NORMAL_V: char = '│';

    // Focused window (double line)
    pub const FOCUS_TL: char = '╔';
    pub const FOCUS_TR: char = '╗';
    pub const FOCUS_BL: char = '╚';
    pub const FOCUS_BR: char = '╝';
    pub const FOCUS_H: char = '═';
    pub const FOCUS_V: char = '║';

    // Floating window (rounded)
    pub const FLOAT_TL: char = '╭';
    pub const FLOAT_TR: char = '╮';
    pub const FLOAT_BL: char = '╰';
    pub const FLOAT_BR: char = '╯';
    pub const FLOAT_H: char = '─';
    pub const FLOAT_V: char = '│';

    // Fullscreen window (heavy/thick)
    pub const FULL_TL: char = '┏';
    pub const FULL_TR: char = '┓';
    pub const FULL_BL: char = '┗';
    pub const FULL_BR: char = '┛';
    pub const FULL_H: char = '━';
    pub const FULL_V: char = '┃';

    // Virtual output boundaries
    #[allow(dead_code)]
    pub const OUTPUT_H: char = '━';
    #[allow(dead_code)]
    pub const OUTPUT_V: char = '┃';
}

/// Window state for rendering
#[derive(Debug, Clone)]
pub struct AsciiWindow {
    pub id: WindowId,
    pub bounds: Rectangle<i32, Logical>,
    pub focused: bool,
    pub floating: bool,
    pub fullscreen: bool,
    pub urgent: bool,
    /// If this window is in a tabbed container, this contains tab info
    pub tab_info: Option<TabInfo>,
}

/// Information about a window's tab container
#[derive(Debug, Clone)]
pub struct TabInfo {
    /// Total number of tabs in the container
    pub total_tabs: usize,
    /// This window's position in the tab list (0-indexed)
    pub tab_index: usize,
    /// Whether this is the active (visible) tab
    pub is_active: bool,
}

/// ASCII renderer backend
pub struct AsciiBackend {
    /// Grid dimensions
    width: usize,
    height: usize,

    /// Character grid
    grid: Vec<Vec<char>>,

    /// Window positions
    pub windows: Arc<Mutex<HashMap<WindowId, AsciiWindow>>>,

    /// Currently focused window
    focused_window: Option<WindowId>,

    /// Logical size of the output
    logical_size: Size<i32, Logical>,

    /// Scale factor for converting logical coordinates to ASCII grid
    scale_x: f64,
    scale_y: f64,
}

impl AsciiBackend {
    /// Create a new ASCII backend with the given dimensions
    pub fn new(width: usize, height: usize, logical_size: Size<i32, Logical>) -> Self {
        let grid = vec![vec![' '; width]; height];

        Self {
            width,
            height,
            grid,
            windows: Arc::new(Mutex::new(HashMap::new())),
            focused_window: None,
            logical_size,
            scale_x: width as f64 / logical_size.w as f64,
            scale_y: height as f64 / logical_size.h as f64,
        }
    }

    /// Get the width of the ASCII grid
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get the height of the ASCII grid
    pub fn height(&self) -> usize {
        self.height
    }

    /// Create with default dimensions
    pub fn default() -> Self {
        Self::new(DEFAULT_WIDTH, DEFAULT_HEIGHT, Size::from((3840, 2160)))
    }

    /// Add or update a window
    pub fn update_window(&mut self, window: AsciiWindow) {
        let id = window.id;
        match self.windows.lock() {
            Ok(mut windows) => { windows.insert(id, window); }
            Err(e) => tracing::error!("Windows lock poisoned: {e}"),
        }
    }

    /// Remove a window
    pub fn remove_window(&mut self, id: WindowId) {
        match self.windows.lock() {
            Ok(mut windows) => { windows.remove(&id); }
            Err(e) => tracing::error!("Windows lock poisoned: {e}"),
        }
    }

    /// Set the focused window
    pub fn set_focus(&mut self, id: Option<WindowId>) {
        self.focused_window = id;
        if let Some(id) = id {
            if let Ok(mut windows) = self.windows.lock() {
                if let Some(window) = windows.get_mut(&id) {
                    window.focused = true;
                }
            }
        }
        // Clear focus on other windows
        if let Ok(mut windows) = self.windows.lock() {
            for (wid, window) in windows.iter_mut() {
                if Some(*wid) != id {
                    window.focused = false;
                }
            }
        }
    }

    /// Update the total size to accommodate multiple outputs
    pub fn update_total_size(&mut self, total_width: i32, total_height: i32) {
        // Convert logical coordinates to ASCII grid coordinates
        let new_width =
            ((total_width as f64 / self.logical_size.w as f64) * self.width as f64) as usize;
        let new_height =
            ((total_height as f64 / self.logical_size.h as f64) * self.height as f64) as usize;

        // Resize grid if needed
        if new_width > self.width || new_height > self.height {
            self.width = new_width.max(self.width);
            self.height = new_height.max(self.height);
            self.grid = vec![vec![' '; self.width]; self.height];

            // Update logical size to match
            self.logical_size = Size::from((total_width, total_height));

            // Recalculate scale factors
            self.scale_x = self.width as f64 / self.logical_size.w as f64;
            self.scale_y = self.height as f64 / self.logical_size.h as f64;
        }
    }

    /// Convert logical coordinates to grid coordinates
    fn to_grid_coords(&self, logical: Point<i32, Logical>) -> (usize, usize) {
        let x = (logical.x as f64 * self.scale_x) as usize;
        let y = (logical.y as f64 * self.scale_y) as usize;
        (x.min(self.width - 1), y.min(self.height - 1))
    }

    /// Convert logical rectangle to grid rectangle
    fn to_grid_rect(&self, rect: Rectangle<i32, Logical>) -> (usize, usize, usize, usize) {
        let (x1, y1) = self.to_grid_coords(rect.loc);
        let (x2, y2) = self.to_grid_coords(Point::from((
            rect.loc.x + rect.size.w,
            rect.loc.y + rect.size.h,
        )));
        (x1, y1, x2.min(self.width - 1), y2.min(self.height - 1))
    }

    /// Clear the grid
    fn clear_grid(&mut self) {
        for row in &mut self.grid {
            row.fill(' ');
        }
    }

    /// Draw a box on the grid
    fn draw_box(
        &mut self,
        x1: usize,
        y1: usize,
        x2: usize,
        y2: usize,
        tl: char,
        tr: char,
        bl: char,
        br: char,
        h: char,
        v: char,
    ) {
        // Ensure bounds are valid
        if x2 <= x1 || y2 <= y1 {
            return;
        }

        // Top border
        self.grid[y1][x1] = tl;
        for x in (x1 + 1)..x2 {
            self.grid[y1][x] = h;
        }
        self.grid[y1][x2] = tr;

        // Side borders
        for y in (y1 + 1)..y2 {
            self.grid[y][x1] = v;
            self.grid[y][x2] = v;
        }

        // Bottom border
        if y2 < self.height {
            self.grid[y2][x1] = bl;
            for x in (x1 + 1)..x2 {
                self.grid[y2][x] = h;
            }
            self.grid[y2][x2] = br;
        }
    }

    /// Draw a window on the grid
    fn draw_window(&mut self, window: &AsciiWindow) {
        let (x1, y1, x2, y2) = self.to_grid_rect(window.bounds);

        // Choose box characters based on state
        let (tl, tr, bl, br, h, v) = if window.fullscreen {
            (
                box_chars::FULL_TL,
                box_chars::FULL_TR,
                box_chars::FULL_BL,
                box_chars::FULL_BR,
                box_chars::FULL_H,
                box_chars::FULL_V,
            )
        } else if window.floating {
            (
                box_chars::FLOAT_TL,
                box_chars::FLOAT_TR,
                box_chars::FLOAT_BL,
                box_chars::FLOAT_BR,
                box_chars::FLOAT_H,
                box_chars::FLOAT_V,
            )
        } else if window.focused {
            (
                box_chars::FOCUS_TL,
                box_chars::FOCUS_TR,
                box_chars::FOCUS_BL,
                box_chars::FOCUS_BR,
                box_chars::FOCUS_H,
                box_chars::FOCUS_V,
            )
        } else {
            (
                box_chars::NORMAL_TL,
                box_chars::NORMAL_TR,
                box_chars::NORMAL_BL,
                box_chars::NORMAL_BR,
                box_chars::NORMAL_H,
                box_chars::NORMAL_V,
            )
        };

        self.draw_box(x1, y1, x2, y2, tl, tr, bl, br, h, v);

        // Draw tab indicator if this is a tabbed window
        if let Some(tab_info) = &window.tab_info {
            if y1 > 0 && x1 + 2 < x2 {
                // Draw tab bar above the window
                let tab_y = y1.saturating_sub(1);

                // Draw tab indicators like [1*][2][3] where * marks active
                let mut tab_x = x1 + 1;
                for i in 0..tab_info.total_tabs.min(10) {
                    if tab_x + 3 >= x2 {
                        break;
                    }

                    self.grid[tab_y][tab_x] = '[';
                    self.grid[tab_y][tab_x + 1] =
                        char::from_digit((i + 1) as u32, 10).unwrap_or('?');
                    if i == tab_info.tab_index && tab_info.is_active {
                        self.grid[tab_y][tab_x + 2] = '*';
                        tab_x += 1;
                    }
                    self.grid[tab_y][tab_x + 2] = ']';
                    tab_x += 3;
                }
            }
        }

        // Draw window ID and status in top-left corner
        if y1 + 1 < self.height && x1 + 2 < x2 {
            let id_str = format!("{}", window.id.get());
            let mut x = x1 + 2;
            for ch in id_str.chars() {
                if x >= x2 {
                    break;
                }
                self.grid[y1 + 1][x] = ch;
                x += 1;
            }

            // Add status indicators
            if window.focused && x + 4 < x2 {
                self.grid[y1 + 1][x] = ' ';
                self.grid[y1 + 1][x + 1] = '[';
                self.grid[y1 + 1][x + 2] = 'F';
                self.grid[y1 + 1][x + 3] = ']';
            }
            if window.fullscreen && x + 5 < x2 {
                x += 4;
                self.grid[y1 + 1][x] = ' ';
                self.grid[y1 + 1][x + 1] = '[';
                self.grid[y1 + 1][x + 2] = 'F';
                self.grid[y1 + 1][x + 3] = 'S';
                self.grid[y1 + 1][x + 4] = ']';
            }
            if window.urgent && x + 4 < x2 {
                x += 5;
                self.grid[y1 + 1][x] = ' ';
                self.grid[y1 + 1][x + 2] = '!';
                self.grid[y1 + 1][x + 3] = ']';
            }
        }
    }

    /// Render the current state to ASCII
    pub fn render(&mut self) -> String {
        self.clear_grid();

        // Sort windows by z-order (floating windows last)
        let mut windows: Vec<_> = match self.windows.lock() {
            Ok(w) => w.values().cloned().collect(),
            Err(e) => {
                tracing::error!("Windows lock poisoned: {e}");
                vec![]
            }
        };
        windows.sort_by_key(|w| w.floating);

        // Draw all windows
        for window in &windows {
            self.draw_window(window);
        }

        // Convert grid to string
        let mut output = String::new();
        for row in &self.grid {
            for &ch in row {
                output.push(ch);
            }
            output.push('\n');
        }

        output
    }

    /// Get a list of windows with their positions
    pub fn get_windows(&self) -> Vec<(WindowId, Rectangle<i32, Logical>)> {
        match self.windows.lock() {
            Ok(windows) => windows
                .iter()
                .map(|(id, w)| (*id, w.bounds))
                .collect(),
            Err(e) => {
                error!("Windows lock poisoned: {e}");
                vec![]
            }
        }
    }
}

/// Command for controlling the ASCII backend
#[derive(Debug, Clone)]
pub enum AsciiCommand {
    CreateWindow {
        id: WindowId,
        size: Size<i32, Logical>,
    },
    DestroyWindow {
        id: WindowId,
    },
    FocusWindow {
        id: WindowId,
    },
    SetLayout {
        layout: String,
    },
    MoveWindow {
        id: WindowId,
        workspace: usize,
    },
    Fullscreen {
        id: WindowId,
        mode: String,
    },
    GetState,
    GetWindows,
}

impl AsciiCommand {
    /// Parse a command string
    pub fn parse(input: &str) -> Option<Self> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        match parts[0] {
            "CREATE_WINDOW" => {
                // Parse id=N size=WxH
                let mut id = None;
                let mut size = None;

                for part in &parts[1..] {
                    if let Some(val) = part.strip_prefix("id=") {
                        id = val.parse().ok().map(WindowId::new);
                    } else if let Some(val) = part.strip_prefix("size=") {
                        let dims: Vec<&str> = val.split('x').collect();
                        if dims.len() == 2 {
                            if let (Ok(w), Ok(h)) = (dims[0].parse(), dims[1].parse()) {
                                size = Some(Size::from((w, h)));
                            }
                        }
                    }
                }

                if let (Some(id), Some(size)) = (id, size) {
                    Some(AsciiCommand::CreateWindow { id, size })
                } else {
                    None
                }
            }
            "DESTROY_WINDOW" => parts
                .get(1)
                .and_then(|s| s.strip_prefix("id="))
                .and_then(|s| s.parse().ok())
                .map(|id| AsciiCommand::DestroyWindow {
                    id: WindowId::new(id),
                }),
            "FOCUS_WINDOW" => parts
                .get(1)
                .and_then(|s| s.strip_prefix("id="))
                .and_then(|s| s.parse().ok())
                .map(|id| AsciiCommand::FocusWindow {
                    id: WindowId::new(id),
                }),
            "SET_LAYOUT" => parts.get(1).map(|layout| AsciiCommand::SetLayout {
                layout: layout.to_string(),
            }),
            "GET_STATE" => Some(AsciiCommand::GetState),
            "GET_WINDOWS" => Some(AsciiCommand::GetWindows),
            _ => None,
        }
    }
}
