use crate::{
    config::{Command, Direction, LayoutCommand, WorkspaceTarget},
    shell::WindowElement,
    state::{StilchState, Backend},
};
use smithay::{
    backend::input::KeyState,
    desktop::space::SpaceElement,
    input::keyboard::{FilterResult, Keysym, ModifiersState},
    utils::{Logical, Point},
};
use std::process::Command as ProcessCommand;
use tracing::{debug, error, info, warn};

/// Represents a focus target which can be either a window or an empty virtual output
#[derive(Debug, Clone)]
enum FocusTarget {
    Window(WindowElement),
    EmptyVirtualOutput(crate::virtual_output::VirtualOutputId),
}

#[derive(Debug, Clone)]
pub enum KeyAction {
    /// Quit the compositor
    Quit,
    /// Switch to a VT
    VtSwitch(i32),
    /// run a command
    Run(String),
    /// Switch to a workspace
    Workspace(WorkspaceTarget),
    /// Move window to workspace
    MoveToWorkspace(WorkspaceTarget),
    /// Focus window in direction
    Focus(Direction),
    /// Move window in direction
    Move(Direction),
    /// Kill focused window
    Kill,
    /// Toggle fullscreen (default: virtual output)
    Fullscreen,
    /// Toggle container fullscreen
    FullscreenContainer,
    /// Toggle virtual output fullscreen
    FullscreenVirtualOutput,
    /// Toggle physical output fullscreen
    FullscreenPhysicalOutput,
    /// Toggle floating
    FloatingToggle,
    /// Reload config
    Reload,
    /// Scale output up
    ScaleUp,
    /// Scale output down
    ScaleDown,
    /// Rotate output
    RotateOutput,
    /// Toggle window preview
    TogglePreview,
    /// Toggle decorations
    ToggleDecorations,
    /// Switch screen/output (udev specific)
    Screen(usize),
    /// Toggle tint
    ToggleTint,
    /// Do nothing
    None,
    /// Debug command to swap first two windows
    DebugSwapWindows,
    /// Set horizontal split
    SplitHorizontal,
    /// Set vertical split
    SplitVertical,
    /// Set automatic (BSP) split
    SplitAutomatic,
    /// Move workspace to output in direction
    MoveWorkspaceToOutput(Direction),
    /// Layout commands (tabbed, stacking, etc)
    Layout(LayoutCommand),
}

impl<BackendData: Backend> StilchState<BackendData> {
    /// Find the next focus target in the given direction
    fn find_focus_target_in_direction(&self, direction: Direction) -> Option<FocusTarget> {
        debug!("find_focus_target_in_direction: {:?}", direction);

        // Get current focus location - either from focused window or pointer
        let current_location = if let Some(keyboard) = self.seat().get_keyboard() {
            if let Some(current_focus) = keyboard.current_focus() {
                match &current_focus {
                    crate::focus::KeyboardFocusTarget::Window(w) => {
                        // Find the WindowElement that contains this Window
                        if let Some(window_elem) = self.space().elements().find(|elem| &elem.0 == w)
                        {
                            if let Some(loc) = self.space().element_location(window_elem) {
                                let geo = window_elem.geometry();
                                Some(Point::<i32, Logical>::from((
                                    loc.x + geo.size.w / 2,
                                    loc.y + geo.size.h / 2,
                                )))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        };

        // If no focused window, use pointer location
        let current_location = current_location.unwrap_or_else(|| {
            let pointer_loc = self.pointer().current_location();
            Point::from((pointer_loc.x as i32, pointer_loc.y as i32))
        });

        debug!("Current location: {:?}", current_location);

        // Find which virtual output we're currently in
        let current_vo_id = self
            .virtual_output_manager
            .virtual_output_at(current_location)?;
        let current_vo = self.virtual_output_manager.get(current_vo_id)?;
        let current_region = current_vo.logical_region();

        // First, look for windows in the current VO's active workspace
        let active_ws = current_vo.active_workspace();
        let mut best_window = None;
        let mut best_distance = f64::MAX;

        if let Some(active_ws) = active_ws {
            // Get workspace ID from old system index
            let workspace_id = crate::workspace::WorkspaceId::new(active_ws as u8);
            if let Some(workspace) = self.workspace_manager.get_workspace(workspace_id) {
                for window_id in &workspace.windows {
                    // Get the WindowElement from registry
                    if let Some(managed_window) = self.window_registry().get(*window_id) {
                        let window_elem = &managed_window.element;
                        if let Some(window_loc) = self.space().element_location(window_elem) {
                            let window_geo = window_elem.geometry();
                            let window_center = Point::<i32, Logical>::from((
                                window_loc.x + window_geo.size.w / 2,
                                window_loc.y + window_geo.size.h / 2,
                            ));

                            // Check if window is in the correct direction
                            let is_in_direction = match direction {
                                Direction::Left => window_center.x < current_location.x,
                                Direction::Right => window_center.x > current_location.x,
                                Direction::Up => window_center.y < current_location.y,
                                Direction::Down => window_center.y > current_location.y,
                            };

                            if is_in_direction {
                                let dx = (window_center.x - current_location.x) as f64;
                                let dy = (window_center.y - current_location.y) as f64;

                                let distance = match direction {
                                    Direction::Left | Direction::Right => {
                                        dx.abs() + (dy.abs() * 2.0)
                                    }
                                    Direction::Up | Direction::Down => dy.abs() + (dx.abs() * 2.0),
                                };

                                if distance < best_distance {
                                    best_distance = distance;
                                    best_window = Some(window_elem.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(window) = best_window {
            debug!("Found window in current VO");
            return Some(FocusTarget::Window(window));
        }

        // No window found in current VO, look for adjacent VOs
        debug!("No window in current VO, looking for adjacent VOs");

        for (vo_id, vo) in self
            .virtual_output_manager
            .all_virtual_outputs()
            .map(|vo| (vo.id(), vo))
            .collect::<Vec<_>>()
        {
            if vo_id == current_vo_id {
                continue;
            }

            let vo_region = vo.logical_region();

            // Check if this VO is in the direction we're looking
            let is_adjacent = match direction {
                Direction::Left => {
                    // VO is to the left if its right edge touches our left edge
                    vo_region.loc.x + vo_region.size.w <= current_region.loc.x &&
                    vo_region.loc.x + vo_region.size.w >= current_region.loc.x - 10 && // Allow small gap
                    // And there's vertical overlap
                    vo_region.loc.y < current_region.loc.y + current_region.size.h &&
                    vo_region.loc.y + vo_region.size.h > current_region.loc.y
                }
                Direction::Right => {
                    // VO is to the right if its left edge touches our right edge
                    vo_region.loc.x >= current_region.loc.x + current_region.size.w &&
                    vo_region.loc.x <= current_region.loc.x + current_region.size.w + 10 && // Allow small gap
                    // And there's vertical overlap
                    vo_region.loc.y < current_region.loc.y + current_region.size.h &&
                    vo_region.loc.y + vo_region.size.h > current_region.loc.y
                }
                Direction::Up => {
                    // VO is above if its bottom edge touches our top edge
                    vo_region.loc.y + vo_region.size.h <= current_region.loc.y &&
                    vo_region.loc.y + vo_region.size.h >= current_region.loc.y - 10 && // Allow small gap
                    // And there's horizontal overlap
                    vo_region.loc.x < current_region.loc.x + current_region.size.w &&
                    vo_region.loc.x + vo_region.size.w > current_region.loc.x
                }
                Direction::Down => {
                    // VO is below if its top edge touches our bottom edge
                    vo_region.loc.y >= current_region.loc.y + current_region.size.h &&
                    vo_region.loc.y <= current_region.loc.y + current_region.size.h + 10 && // Allow small gap
                    // And there's horizontal overlap
                    vo_region.loc.x < current_region.loc.x + current_region.size.w &&
                    vo_region.loc.x + vo_region.size.w > current_region.loc.x
                }
            };

            if is_adjacent {
                debug!("Found adjacent VO: {:?}", vo.name());

                // Check if there's a window in the adjacent VO's active workspace
                let adjacent_active_ws = vo.active_workspace();
                if let Some(adjacent_active_ws) = adjacent_active_ws {
                    let workspace_id = crate::workspace::WorkspaceId::new(adjacent_active_ws as u8);
                    if let Some(workspace) = self.workspace_manager.get_workspace(workspace_id) {
                        // Use find_next_focus to get the appropriate window
                        // This respects active_child for tabbed containers
                        let next_focus = workspace.layout.find_next_focus();
                        debug!("find_next_focus returned: {:?}", next_focus);
                        if let Some(focus_window_id) = next_focus {
                            if let Some(managed_window) =
                                self.window_registry().get(focus_window_id)
                            {
                                debug!(
                                    "Found window {} in adjacent VO using find_next_focus",
                                    focus_window_id
                                );
                                info!(
                                    "Focusing window {} from adjacent VO (respects active_child)",
                                    focus_window_id
                                );
                                return Some(FocusTarget::Window(managed_window.element.clone()));
                            } else {
                                debug!("Window {} not found in registry", focus_window_id);
                            }
                        } else {
                            debug!("find_next_focus returned None, using fallback");
                        }

                        // Fallback: if find_next_focus didn't find anything, try any window
                        let mut best_adjacent_window = None;
                        let mut best_adjacent_distance = f64::MAX;

                        for window_id in &workspace.windows {
                            if let Some(managed_window) = self.window_registry().get(*window_id) {
                                let window_elem = &managed_window.element;
                                if let Some(window_loc) = self.space().element_location(window_elem)
                                {
                                    let window_geo = window_elem.geometry();
                                    let window_center = Point::<i32, Logical>::from((
                                        window_loc.x + window_geo.size.w / 2,
                                        window_loc.y + window_geo.size.h / 2,
                                    ));

                                    let dx = (window_center.x - current_location.x) as f64;
                                    let dy = (window_center.y - current_location.y) as f64;

                                    let distance = match direction {
                                        Direction::Left | Direction::Right => {
                                            dx.abs() + (dy.abs() * 0.5)
                                        }
                                        Direction::Up | Direction::Down => {
                                            dy.abs() + (dx.abs() * 0.5)
                                        }
                                    };

                                    if distance < best_adjacent_distance {
                                        best_adjacent_distance = distance;
                                        best_adjacent_window = Some(window_elem.clone());
                                    }
                                }
                            }
                        }

                        if let Some(window) = best_adjacent_window {
                            debug!("Found window in adjacent VO");
                            return Some(FocusTarget::Window(window));
                        }
                    }
                }

                // Adjacent VO is empty (no active workspace or no windows), focus it
                debug!("Adjacent VO is empty, will focus it");
                return Some(FocusTarget::EmptyVirtualOutput(vo_id));
            }
        }

        debug!("No focus target found in direction {:?}", direction);
        None
    }

    pub fn process_keybinding(
        &self,
        modifiers: ModifiersState,
        modified_keysym: Keysym,
        raw_keysym: Option<Keysym>,
        state: KeyState,
    ) -> FilterResult<KeyAction> {
        if state != KeyState::Pressed {
            return FilterResult::Forward;
        }

        debug!("process_keybinding: modified_keysym={:?} ({}) raw={:#x}, raw_keysym={:?}, modifiers={:?}",
            modified_keysym,
            xkbcommon::xkb::keysym_get_name(modified_keysym),
            modified_keysym.raw(),
            raw_keysym.map(|k| xkbcommon::xkb::keysym_get_name(k)),
            modifiers
        );

        // Check config keybindings - use raw keysym for matching (if available)
        let keysym_for_binding = raw_keysym.unwrap_or(modified_keysym);

        for binding in &self.config.keybindings {
            if binding.key == keysym_for_binding {
                debug!("Key matches binding: key={:?} ({}), required_modifiers={:?}, current_modifiers={:?}",
                    binding.key,
                    xkbcommon::xkb::keysym_get_name(binding.key),
                    binding.modifiers,
                    modifiers
                );
                if self.modifiers_match(modifiers, binding.modifiers) {
                    debug!("Keybinding matched! Command: {:?}", binding.command);
                    if let Some(action) = self.command_to_action(&binding.command) {
                        return FilterResult::Intercept(action);
                    }
                }
            }
        }

        // Fallback to hardcoded keybindings for essential functions
        if modifiers.ctrl && modifiers.alt && keysym_for_binding == Keysym::BackSpace {
            return FilterResult::Intercept(KeyAction::Quit);
        }

        // VT switching keybindings using XF86Switch_VT keysyms
        // These are generated when pressing Ctrl+Alt+F1-F12
        // Use the modified keysym for VT switching
        #[cfg(feature = "udev")]
        {
            use xkbcommon::xkb::keysyms;
            if (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12)
                .contains(&modified_keysym.raw())
            {
                let vt = (modified_keysym.raw() - keysyms::KEY_XF86Switch_VT_1 + 1) as i32;
                return FilterResult::Intercept(KeyAction::VtSwitch(vt));
            }
        }

        FilterResult::Forward
    }

    fn modifiers_match(&self, current: ModifiersState, required: ModifiersState) -> bool {
        current.ctrl == required.ctrl
            && current.alt == required.alt
            && current.shift == required.shift
            && current.logo == required.logo
    }

    fn command_to_action(&self, command: &Command) -> Option<KeyAction> {
        match command {
            Command::Exec(cmd) => Some(KeyAction::Run(cmd.clone())),
            Command::Kill => Some(KeyAction::Kill),
            Command::Exit => Some(KeyAction::Quit),
            Command::Reload => Some(KeyAction::Reload),
            Command::DebugSwapWindows => Some(KeyAction::DebugSwapWindows),
            Command::SplitHorizontal => Some(KeyAction::SplitHorizontal),
            Command::SplitVertical => Some(KeyAction::SplitVertical),
            Command::SplitAutomatic => Some(KeyAction::SplitAutomatic),
            Command::Workspace(target) => Some(KeyAction::Workspace(target.clone())),
            Command::MoveToWorkspace(target) => Some(KeyAction::MoveToWorkspace(target.clone())),
            Command::Focus(dir) => Some(KeyAction::Focus(*dir)),
            Command::Move(dir) => Some(KeyAction::Move(*dir)),
            Command::Fullscreen => Some(KeyAction::Fullscreen),
            Command::FullscreenContainer => Some(KeyAction::FullscreenContainer),
            Command::FullscreenVirtualOutput => Some(KeyAction::FullscreenVirtualOutput),
            Command::FullscreenPhysicalOutput => Some(KeyAction::FullscreenPhysicalOutput),
            Command::FloatingToggle => Some(KeyAction::FloatingToggle),
            Command::MoveWorkspaceToOutput(dir) => Some(KeyAction::MoveWorkspaceToOutput(*dir)),
            Command::Layout(layout_cmd) => Some(KeyAction::Layout(layout_cmd.clone())),
            _ => None, // Unimplemented commands
        }
    }

    pub fn handle_key_action(&mut self, action: KeyAction) {
        match action {
            KeyAction::Quit => {
                info!("Quitting compositor");
                self.running
                    .store(false, std::sync::atomic::Ordering::SeqCst);
            }

            KeyAction::Run(cmd) => {
                info!(cmd, "Starting program");
                let mut command = ProcessCommand::new("sh");
                command.arg("-c").arg(&cmd);

                if let Some(socket_name) = &self.socket_name {
                    command.env("WAYLAND_DISPLAY", socket_name);
                }

                #[cfg(feature = "xwayland")]
                if let Some(xdisplay) = self.xdisplay {
                    command.env("DISPLAY", format!(":{xdisplay}"));
                }

                // Set virtual output environment variables based on keyboard focus or pointer location
                let focus_location = if let Some(keyboard) = self.seat().get_keyboard() {
                    if let Some(focus) = keyboard.current_focus() {
                        match &focus {
                            crate::focus::KeyboardFocusTarget::Window(w) => self
                                .space()
                                .elements()
                                .find(|elem| &elem.0 == w)
                                .and_then(|elem| self.space().element_location(elem)),
                            _ => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let location = focus_location.unwrap_or_else(|| {
                    let pointer_loc = self.pointer().current_location();
                    Point::from((pointer_loc.x as i32, pointer_loc.y as i32))
                });

                if let Some(vo_id) = self.virtual_output_manager.virtual_output_at(location) {
                    if let Some(vo) = self.virtual_output_manager.get(vo_id) {
                        let region = vo.logical_region();
                        command.env("STILCH_FOCUSED_OUTPUT", vo.name());
                        command.env("STILCH_OUTPUT_X", region.loc.x.to_string());
                        command.env("STILCH_OUTPUT_Y", region.loc.y.to_string());
                        command.env("STILCH_OUTPUT_WIDTH", region.size.w.to_string());
                        command.env("STILCH_OUTPUT_HEIGHT", region.size.h.to_string());
                    }
                }

                if let Err(e) = command.spawn() {
                    error!(cmd, err = %e, "Failed to start program");
                }
            }

            KeyAction::Kill => {
                info!("Kill window requested");
                self.close_focused_window();
            }

            KeyAction::Workspace(target) => {
                info!("Switch to workspace: {:?}", target);

                // Get the current pointer location to determine which virtual output
                let pointer_loc = self.pointer().current_location();
                let pointer_loc_i32 = Point::from((pointer_loc.x as i32, pointer_loc.y as i32));

                if let Some(virtual_output_id) = self
                    .virtual_output_manager
                    .virtual_output_at(pointer_loc_i32)
                {
                    if let Some(virtual_output) =
                        self.virtual_output_manager.get_mut(virtual_output_id)
                    {
                        let workspace_idx = match target {
                            WorkspaceTarget::Number(n) => {
                                if n >= 1 && n <= 10 {
                                    Some((n - 1) as usize)
                                } else {
                                    None
                                }
                            }
                            WorkspaceTarget::Previous => {
                                // TODO: Track previous workspace
                                None
                            }
                            WorkspaceTarget::Next => {
                                virtual_output.active_workspace().and_then(|current| {
                                    if current < 9 {
                                        Some(current + 1)
                                    } else {
                                        None
                                    }
                                })
                            }
                            WorkspaceTarget::Name(_) => {
                                // TODO: Named workspaces
                                None
                            }
                        };

                        if let Some(idx) = workspace_idx {
                            info!(
                                "Switching to workspace {} on virtual output {}",
                                idx + 1,
                                virtual_output.name()
                            );
                            self.switch_to_workspace(virtual_output_id, idx);
                        }
                    }
                }
            }

            KeyAction::MoveToWorkspace(target) => {
                info!("Move window to workspace: {:?}", target);

                if let Some(window_elem) = self.focused_window() {
                    // Find window in registry
                    if let Some(window_id) = self.window_registry().find_by_element(&window_elem) {
                        let workspace_idx = match target {
                            WorkspaceTarget::Number(n) => {
                                if n >= 1 && n <= 10 {
                                    Some((n - 1) as u8)
                                } else {
                                    None
                                }
                            }
                            WorkspaceTarget::Previous => None,
                            WorkspaceTarget::Next => None,
                            WorkspaceTarget::Name(_) => None,
                        };

                        if let Some(idx) = workspace_idx {
                            let target_workspace_id = crate::workspace::WorkspaceId::new(idx);
                            info!("Moving window to workspace {}", idx + 1);
                            self.move_window_to_workspace_by_id(window_id, target_workspace_id);
                        }
                    } else {
                        tracing::warn!("Focused window not found in registry");
                    }
                }
            }

            KeyAction::Focus(dir) => {
                info!("Focus {:?}", dir);

                // Check if current window is in a tabbed container
                let should_switch_tabs = if let Some(focused) = self.focused_window() {
                    if let Some(window_id) = self.window_registry().find_by_element(&focused) {
                        if let Some(managed_window) = self.window_registry().get(window_id) {
                            if let Some(workspace) = self
                                .workspace_manager
                                .get_workspace(managed_window.workspace)
                            {
                                // Check if this window is in a tabbed container
                                workspace.layout.is_window_in_tabbed_container(window_id)
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                // If in tabbed container and direction is left/right, switch tabs
                if should_switch_tabs && matches!(dir, Direction::Left | Direction::Right) {
                    if let Some(focused) = self.focused_window() {
                        if let Some(window_id) = self.window_registry().find_by_element(&focused) {
                            if let Some(managed_window) = self.window_registry().get(window_id) {
                                let workspace_id = managed_window.workspace;
                                let should_escape = if let Some(workspace) =
                                    self.workspace_manager.get_workspace_mut(workspace_id)
                                {
                                    let escape = match dir {
                                        Direction::Right => workspace.layout.next_tab(window_id),
                                        Direction::Left => workspace.layout.prev_tab(window_id),
                                        _ => false,
                                    };
                                    workspace.relayout();
                                    escape
                                } else {
                                    false
                                };

                                // If we should escape, fall through to normal focus behavior
                                if should_escape {
                                    info!("Escaping tabbed container");
                                    // Don't return - fall through to normal focus behavior
                                } else {
                                    // Apply the layout to update visible windows
                                    self.apply_workspace_layout(workspace_id);

                                    // Focus the now-visible tab
                                    let element_to_focus = if let Some(workspace) =
                                        self.workspace_manager.get_workspace(workspace_id)
                                    {
                                        let visible_windows =
                                            workspace.layout.get_visible_geometries();
                                        if let Some((visible_id, _)) = visible_windows.first() {
                                            self.window_registry()
                                                .get(*visible_id)
                                                .map(|m| m.element.clone())
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };

                                    if let Some(element) = element_to_focus {
                                        self.focus_window(&element);
                                        info!("Switched tab");
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    // Only return if we handled the tab switch
                    if !should_switch_tabs {
                        return;
                    }
                }

                // Normal focus behavior
                let target = self.find_focus_target_in_direction(dir);
                match target {
                    Some(FocusTarget::Window(window)) => {
                        debug!("Focusing window in direction {:?}", dir);
                        self.focus_window(&window);
                        self.center_pointer_on_window(&window);
                    }
                    Some(FocusTarget::EmptyVirtualOutput(vo_id)) => {
                        debug!("Focusing empty virtual output in direction {:?}", dir);

                        // Move pointer to center of the empty VO
                        if let Some(vo) = self.virtual_output_manager.get(vo_id) {
                            let region = vo.logical_region();
                            let center = Point::<f64, Logical>::from((
                                (region.loc.x + region.size.w / 2) as f64,
                                (region.loc.y + region.size.h / 2) as f64,
                            ));

                            debug!("Moving pointer to center of VO at {:?}", center);
                            self.pointer().set_location(center);

                            // Clear keyboard focus since there's no window to focus
                            if let Some(keyboard) = self.seat().get_keyboard() {
                                keyboard.set_focus(
                                    self,
                                    None,
                                    smithay::utils::SERIAL_COUNTER.next_serial(),
                                );
                            }

                            // Ensure the VO's active workspace is ready
                            // (This is handled by virtual_output_manager already)
                        }
                    }
                    None => {
                        debug!("No focus target found in direction {:?}", dir);
                    }
                }
            }

            KeyAction::Move(dir) => {
                debug!("Move window {:?}", dir);

                if let Some(window_element) = self.focused_window() {
                    self.move_window_direction(window_element, dir);
                }
            }
            KeyAction::DebugSwapWindows => {
                debug!("Debug: Swapping first two windows");
                self.debug_swap_windows();
            }

            KeyAction::SplitHorizontal => {
                debug!("Setting horizontal split");
                self.set_split_direction(crate::workspace::layout::SplitDirection::Horizontal);
            }

            KeyAction::SplitVertical => {
                debug!("Setting vertical split");
                self.set_split_direction(crate::workspace::layout::SplitDirection::Vertical);
            }

            KeyAction::SplitAutomatic => {
                debug!("Setting automatic (BSP) split");
                // For automatic mode, choose based on pointer location
                self.set_split_direction_automatic();
            }

            KeyAction::Fullscreen => {
                debug!("Toggle fullscreen (default: virtual output)");
                self.toggle_fullscreen(crate::window::FullscreenMode::VirtualOutput);
            }

            KeyAction::FullscreenContainer => {
                debug!("Toggle container fullscreen");
                self.toggle_fullscreen(crate::window::FullscreenMode::Container);
            }

            KeyAction::FullscreenVirtualOutput => {
                debug!("Toggle virtual output fullscreen");
                self.toggle_fullscreen(crate::window::FullscreenMode::VirtualOutput);
            }

            KeyAction::FullscreenPhysicalOutput => {
                debug!("Toggle physical output fullscreen");
                self.toggle_fullscreen(crate::window::FullscreenMode::PhysicalOutput);
            }

            KeyAction::FloatingToggle => {
                debug!("Toggle floating");
                // Get the focused window from the active workspace
                let focused_window = self.space().outputs().find_map(|output| {
                    let vo_ids = self
                        .virtual_output_manager
                        .virtual_outputs_for_physical(output);
                    vo_ids.first().and_then(|vo_id| {
                        let vo = self.virtual_output_manager.get(*vo_id)?;
                        let workspace_idx = vo.active_workspace()?;
                        let workspace_id = crate::workspace::WorkspaceId::new(workspace_idx as u8);
                        let workspace = self.workspace_manager.get_workspace(workspace_id)?;
                        workspace.focused_window
                    })
                });

                if let Some(window_id) = focused_window {
                    // Get info and toggle floating state
                    let (is_floating, workspace_id) = {
                        if let Some(managed_window) = self.window_registry_mut().get_mut(window_id)
                        {
                            // Toggle between tiled and floating
                            let is_now_floating = match &managed_window.layout {
                                crate::window::WindowLayout::Tiled { geometry, .. } => {
                                    // Switch to floating, preserve geometry
                                    managed_window.layout = crate::window::WindowLayout::Floating {
                                        geometry: *geometry,
                                    };
                                    true
                                }
                                crate::window::WindowLayout::Floating { geometry } => {
                                    // Switch to tiled, workspace will assign container
                                    managed_window.layout = crate::window::WindowLayout::Tiled {
                                        container: crate::window::ContainerId::next(), // Temporary
                                        geometry: *geometry,
                                    };
                                    false
                                }
                                crate::window::WindowLayout::Fullscreen { .. } => {
                                    // Don't toggle floating while fullscreen
                                    return;
                                }
                            };
                            (is_now_floating, managed_window.workspace)
                        } else {
                            return; // Window not found
                        }
                    };

                    // Re-apply layout to update the window's position
                    if let Some(vo_id) =
                        self.workspace_manager.find_workspace_location(workspace_id)
                    {
                        if let Some(vo) = self.virtual_output_manager.get(vo_id) {
                            if vo.active_workspace() == Some(workspace_id.get() as usize) {
                                // Apply layout
                                self.apply_workspace_layout(workspace_id);
                            }
                        }
                    }

                    info!("Window {} floating: {}", window_id.get(), is_floating);
                }
            }

            KeyAction::Reload => {
                info!("Reloading config");
                // Config reloading would require re-parsing the config file
                // For now, just log that it's not implemented
                warn!("Config reload not yet implemented - requires config file path tracking");
            }

            KeyAction::ScaleUp => {
                info!("Scale up output");
                if let Some(output) = self.space().outputs().next() {
                    let current_scale = output.current_scale().fractional_scale();
                    let new_scale = (current_scale + 0.25).min(3.0);
                    output.change_current_state(
                        None,
                        None,
                        Some(smithay::output::Scale::Fractional(new_scale)),
                        None,
                    );
                    info!("Output scale changed to: {new_scale}");
                    // Trigger re-render on next frame
                }
            }

            KeyAction::ScaleDown => {
                info!("Scale down output");
                if let Some(output) = self.space().outputs().next() {
                    let current_scale = output.current_scale().fractional_scale();
                    let new_scale = (current_scale - 0.25).max(0.5);
                    output.change_current_state(
                        None,
                        None,
                        Some(smithay::output::Scale::Fractional(new_scale)),
                        None,
                    );
                    info!("Output scale changed to: {new_scale}");
                    // Trigger re-render on next frame
                }
            }

            KeyAction::RotateOutput => {
                info!("Rotate output");
                if let Some(output) = self.space().outputs().next() {
                    use smithay::utils::Transform;
                    let current_transform = output.current_transform();
                    let new_transform = match current_transform {
                        Transform::Normal => Transform::_90,
                        Transform::_90 => Transform::_180,
                        Transform::_180 => Transform::_270,
                        Transform::_270 => Transform::Normal,
                        Transform::Flipped => Transform::Flipped90,
                        Transform::Flipped90 => Transform::Flipped180,
                        Transform::Flipped180 => Transform::Flipped270,
                        Transform::Flipped270 => Transform::Flipped,
                    };
                    output.change_current_state(None, Some(new_transform), None, None);
                    info!("Output rotated to: {:?}", new_transform);
                    // Trigger re-render on next frame
                }
            }

            KeyAction::TogglePreview => {
                self.show_window_preview = !self.show_window_preview;
                debug!("Toggle window preview: {}", self.show_window_preview);
            }

            KeyAction::ToggleDecorations => {
                debug!("Toggle decorations");
                // Get the focused window from the active workspace
                let focused_window = self.space().outputs().find_map(|output| {
                    let vo_ids = self
                        .virtual_output_manager
                        .virtual_outputs_for_physical(output);
                    vo_ids.first().and_then(|vo_id| {
                        let vo = self.virtual_output_manager.get(*vo_id)?;
                        let workspace_idx = vo.active_workspace()?;
                        let workspace_id = crate::workspace::WorkspaceId::new(workspace_idx as u8);
                        let workspace = self.workspace_manager.get_workspace(workspace_id)?;
                        workspace.focused_window
                    })
                });

                if let Some(window_id) = focused_window {
                    if let Some(managed_window) = self.window_registry().get(window_id) {
                        if let Some(toplevel) = managed_window.element.0.toplevel() {
                            toplevel.with_pending_state(|state| {
                                // Toggle server-side decorations
                                let has_decorations = state.decoration_mode == Some(smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode::ServerSide);
                                state.decoration_mode = if has_decorations {
                                    Some(smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode::ClientSide)
                                } else {
                                    Some(smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode::ServerSide)
                                };
                            });
                            toplevel.send_configure();
                            info!("Toggled decorations for window {}", window_id.get());
                        }
                    }
                }
            }

            KeyAction::Screen(idx) => {
                debug!("Switch to screen {idx}");
                // Find the output at the given index
                if let Some(output) = self.space().outputs().nth(idx) {
                    // Move cursor to the center of the output
                    if let Some(geo) = self.space().output_geometry(output) {
                        let center = geo.loc + Point::from((geo.size.w / 2, geo.size.h / 2));
                        self.pointer().set_location(center.to_f64());
                        info!("Switched to screen {} at {:?}", idx, output.name());
                    }
                } else {
                    warn!("Screen index {} out of range", idx);
                }
            }

            KeyAction::ToggleTint => {
                debug!("Toggle tint");
                // Tint effect would need to be implemented in the renderer
                // This could be done by adding a post-processing effect to the render pipeline
                warn!("Tint toggle not yet implemented - requires renderer modifications");
            }

            KeyAction::VtSwitch(vt) => {
                info!("VT switch to {vt}");
                // VT switching is handled by the backend
            }

            KeyAction::MoveWorkspaceToOutput(direction) => {
                info!(
                    "KeyAction::MoveWorkspaceToOutput called with direction: {:?}",
                    direction
                );
                self.move_workspace_to_output(direction);
            }

            KeyAction::Layout(layout_cmd) => {
                self.handle_layout_command(layout_cmd);
            }

            KeyAction::None => {}
        }
    }

    pub fn handle_layout_command(&mut self, layout_cmd: LayoutCommand) {
        use crate::workspace::layout::{ContainerLayout, SplitDirection};

        tracing::info!("handle_layout_command called with {:?}", layout_cmd);

        // Get the current focused window element
        let Some(focused_element) = self.focused_window() else {
            tracing::warn!("No focused window for layout command");
            return;
        };

        // Get the window ID from the element
        let Some(focused_window_id) = self.window_registry().find_by_element(&focused_element) else {
            tracing::warn!("Could not find window ID for focused element");
            return;
        };

        // Find which workspace contains this window
        let workspace_id = self
            .window_registry()
            .get(focused_window_id)
            .map(|w| w.workspace);

        if let Some(workspace_id) = workspace_id {
            if let Some(workspace) = self.workspace_manager.get_workspace_mut(workspace_id) {
                match layout_cmd {
                    LayoutCommand::Tabbed => {
                        tracing::info!(
                            "Setting tabbed layout for container with window {} in workspace {}",
                            focused_window_id,
                            workspace_id
                        );
                        workspace
                            .layout
                            .set_container_layout(focused_window_id, ContainerLayout::Tabbed);
                        workspace.relayout();
                        tracing::info!("Layout set and relayout done");
                    }
                    LayoutCommand::Stacking => {
                        info!(
                            "Setting stacking layout for container with window {}",
                            focused_window_id
                        );
                        workspace
                            .layout
                            .set_container_layout(focused_window_id, ContainerLayout::Stacked);
                        workspace.relayout();
                    }
                    LayoutCommand::ToggleSplit => {
                        info!("Toggling split layout");
                        workspace.layout.toggle_container_split(focused_window_id);
                        workspace.relayout();
                    }
                    LayoutCommand::SplitH => {
                        workspace.next_split = SplitDirection::Horizontal;
                        info!("Next split will be horizontal");
                    }
                    LayoutCommand::SplitV => {
                        workspace.next_split = SplitDirection::Vertical;
                        info!("Next split will be vertical");
                    }
                }
            }

            // Apply the workspace layout to actually update the space
            self.apply_workspace_layout(workspace_id);
        }
    }
}
