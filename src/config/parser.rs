use super::*;
use smithay::input::keyboard::{keysyms, Keysym, ModifiersState};

/// Parse a sway config file
pub fn parse_config(content: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let mut config = Config::default();

    for (_line_num, line) in content.lines().enumerate() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse the line
        if let Err(e) = parse_line(&mut config, line) {
            eprintln!("Warning: Failed to parse config line '{}': {}", line, e);
        }
    }

    Ok(config)
}

fn parse_line(config: &mut Config, line: &str) -> Result<(), Box<dyn std::error::Error>> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let first_part = parts.first().ok_or("Empty command line")?;

    match *first_part {
        "set" => parse_set(config, &parts[1..])?,
        "bindsym" => parse_bindsym(config, &parts[1..])?,
        "exec" => parse_exec(config, &parts[1..])?,
        "output" => parse_output(config, &parts[1..])?,
        "virtual_output" => parse_virtual_output(config, &parts[1..])?,
        "workspace" => parse_workspace(config, &parts[1..])?,
        "gaps" => parse_gaps(config, &parts[1..])?,
        "default_border" => parse_border(config, &parts[1..])?,
        "font" => parse_font(config, &parts[1..])?,
        "input" => parse_input(config, line)?,
        _ => {
            // Ignore unrecognized commands for now
        }
    }

    Ok(())
}

fn parse_set(config: &mut Config, parts: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    if parts.len() < 2 {
        return Err("set requires variable name and value".into());
    }

    let var_name = parts.first().ok_or("Missing variable name")?;
    let value = parts[1..].join(" ");

    // Remove leading $ from variable name if present
    let var_name = if var_name.starts_with('$') {
        &var_name[1..]
    } else {
        var_name
    };

    // Expand any variables in the value
    let expanded_value = config.expand_variables(&value);
    config
        .variables
        .insert(var_name.to_string(), expanded_value);

    Ok(())
}

fn parse_bindsym(config: &mut Config, parts: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    if parts.len() < 2 {
        return Err("bindsym requires key combination and command".into());
    }

    let key_combo = parts.first().ok_or("Missing key combination")?;
    let command_parts = &parts[1..];

    // Parse modifiers and key
    let (modifiers, key) = parse_key_combo(config, key_combo)?;

    // Parse command
    let command = parse_command(config, command_parts)?;

    config.keybindings.push(Keybinding {
        modifiers,
        key,
        command,
    });

    Ok(())
}

fn parse_exec(config: &mut Config, parts: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    if parts.is_empty() {
        return Err("exec requires a command".into());
    }

    let command = parts.join(" ");
    let expanded_command = config.expand_variables(&command);
    config.startup_commands.push(expanded_command);

    Ok(())
}

fn parse_key_combo(
    config: &Config,
    combo: &str,
) -> Result<(ModifiersState, Keysym), Box<dyn std::error::Error>> {
    let parts: Vec<&str> = combo.split('+').collect();
    if parts.is_empty() {
        return Err("Empty key combination".into());
    }

    let mut modifiers = ModifiersState::default();
    let key_part = parts
        .last()
        // SAFETY: split always produces at least one part
        .expect("split always produces at least one part");

    // Parse modifiers
    for part in &parts[..parts.len() - 1] {
        let modifier_name = if part.starts_with('$') {
            // Variable reference
            config
                .get_variable(&part[1..])
                .ok_or_else(|| format!("Unknown variable: {part}"))?
        } else {
            part.to_string()
        };

        match modifier_name.as_str() {
            "Mod1" | "Alt" => modifiers.alt = true,
            "Mod4" | "Super" | "Logo" => modifiers.logo = true,
            "Ctrl" | "Control" => modifiers.ctrl = true,
            "Shift" => modifiers.shift = true,
            _ => return Err(format!("Unknown modifier: {modifier_name}").into()),
        }
    }

    // Parse key
    let key = parse_key(key_part)?;

    Ok((modifiers, key))
}

fn parse_key(key: &str) -> Result<Keysym, Box<dyn std::error::Error>> {
    // Handle common key names
    let keysym = match key {
        // Letters
        "a" | "A" => keysyms::KEY_a,
        "b" | "B" => keysyms::KEY_b,
        "c" | "C" => keysyms::KEY_c,
        "d" | "D" => keysyms::KEY_d,
        "e" | "E" => keysyms::KEY_e,
        "f" | "F" => keysyms::KEY_f,
        "g" | "G" => keysyms::KEY_g,
        "h" | "H" => keysyms::KEY_h,
        "i" | "I" => keysyms::KEY_i,
        "j" | "J" => keysyms::KEY_j,
        "k" | "K" => keysyms::KEY_k,
        "l" | "L" => keysyms::KEY_l,
        "m" | "M" => keysyms::KEY_m,
        "n" | "N" => keysyms::KEY_n,
        "o" | "O" => keysyms::KEY_o,
        "p" | "P" => keysyms::KEY_p,
        "q" | "Q" => keysyms::KEY_q,
        "r" | "R" => keysyms::KEY_r,
        "s" | "S" => keysyms::KEY_s,
        "t" | "T" => keysyms::KEY_t,
        "u" | "U" => keysyms::KEY_u,
        "v" | "V" => keysyms::KEY_v,
        "w" | "W" => keysyms::KEY_w,
        "x" | "X" => keysyms::KEY_x,
        "y" | "Y" => keysyms::KEY_y,
        "z" | "Z" => keysyms::KEY_z,

        // Numbers (handle both shifted and unshifted)
        "1" | "exclam" => keysyms::KEY_1,
        "2" | "at" => keysyms::KEY_2,
        "3" | "numbersign" => keysyms::KEY_3,
        "4" | "dollar" => keysyms::KEY_4,
        "5" | "percent" => keysyms::KEY_5,
        "6" | "asciicircum" => keysyms::KEY_6,
        "7" | "ampersand" => keysyms::KEY_7,
        "8" | "asterisk" => keysyms::KEY_8,
        "9" | "parenleft" => keysyms::KEY_9,
        "0" | "parenright" => keysyms::KEY_0,

        // Special keys
        "Return" | "Enter" => keysyms::KEY_Return,
        "space" => keysyms::KEY_space,
        "Tab" => keysyms::KEY_Tab,
        "Escape" | "Esc" => keysyms::KEY_Escape,
        "BackSpace" => keysyms::KEY_BackSpace,
        "Delete" => keysyms::KEY_Delete,

        // Arrow keys
        "Left" => keysyms::KEY_Left,
        "Right" => keysyms::KEY_Right,
        "Up" => keysyms::KEY_Up,
        "Down" => keysyms::KEY_Down,
        "Home" => keysyms::KEY_Home,
        "End" => keysyms::KEY_End,
        "Page_Up" | "Prior" => keysyms::KEY_Page_Up,
        "Page_Down" | "Next" => keysyms::KEY_Page_Down,
        "F1" => keysyms::KEY_F1,
        "F2" => keysyms::KEY_F2,
        "F3" => keysyms::KEY_F3,
        "F4" => keysyms::KEY_F4,
        "F5" => keysyms::KEY_F5,
        "F6" => keysyms::KEY_F6,
        "F7" => keysyms::KEY_F7,
        "F8" => keysyms::KEY_F8,
        "F9" => keysyms::KEY_F9,
        "F10" => keysyms::KEY_F10,
        "F11" => keysyms::KEY_F11,
        "F12" => keysyms::KEY_F12,
        "Print" => keysyms::KEY_Print,

        // Media keys
        "XF86AudioMute" => keysyms::KEY_XF86AudioMute,
        "XF86AudioRaiseVolume" => keysyms::KEY_XF86AudioRaiseVolume,
        "XF86AudioLowerVolume" => keysyms::KEY_XF86AudioLowerVolume,
        "XF86AudioPlay" => keysyms::KEY_XF86AudioPlay,
        "XF86AudioNext" => keysyms::KEY_XF86AudioNext,
        "XF86AudioPrev" => keysyms::KEY_XF86AudioPrev,

        _ => return Err(format!("Unknown key: {key}").into()),
    };

    Ok(Keysym::new(keysym))
}

fn parse_command(config: &Config, parts: &[&str]) -> Result<Command, Box<dyn std::error::Error>> {
    if parts.is_empty() {
        return Err("Empty command".into());
    }

    let cmd = match parts.first().ok_or("Empty command")?.as_ref() {
        "exec" => {
            let program = parts[1..].join(" ");
            Command::Exec(config.expand_variables(&program))
        }
        "kill" => Command::Kill,
        "reload" => Command::Reload,
        "exit" => Command::Exit,
        "debugswapwindows" => Command::DebugSwapWindows,
        "splith" => Command::SplitHorizontal,
        "splitv" => Command::SplitVertical,
        "splitauto" => Command::SplitAutomatic,
        "movetableft" => Command::MoveTabLeft,
        "movetabright" => Command::MoveTabRight,
        "focus" => {
            if parts.len() < 2 {
                return Err("focus requires direction".into());
            }
            Command::Focus(parse_direction(
                parts.get(1).ok_or("Missing direction for focus")?,
            )?)
        }
        "move" => {
            if parts.len() < 2 {
                return Err("move requires argument".into());
            }
            match parts.get(1).ok_or("Missing move target")?.as_ref() {
                "left" | "right" | "up" | "down" => {
                    Command::Move(parse_direction(parts.get(1).ok_or("Missing direction")?)?)
                }
                "container" | "window" => {
                    if parts.len() >= 4 && parts[2] == "to" && parts[3] == "workspace" {
                        Command::MoveToWorkspace(parse_workspace_target(&parts[4..])?)
                    } else {
                        Command::Raw(parts.join(" "))
                    }
                }
                "workspace" => {
                    if parts.len() >= 5 && parts[2] == "to" && parts[3] == "output" {
                        Command::MoveWorkspaceToOutput(parse_direction(parts[4])?)
                    } else {
                        Command::Raw(parts.join(" "))
                    }
                }
                "scratchpad" => Command::Scratchpad(ScratchpadCommand::Move),
                _ => Command::Raw(parts.join(" ")),
            }
        }
        "workspace" => Command::Workspace(parse_workspace_target(&parts[1..])?),
        "fullscreen" => {
            if parts.len() >= 2 {
                match parts[1] {
                    "container" => Command::FullscreenContainer,
                    "virtual" | "virtual_output" => Command::FullscreenVirtualOutput,
                    "physical" | "physical_output" => Command::FullscreenPhysicalOutput,
                    _ => Command::Fullscreen,
                }
            } else {
                Command::Fullscreen
            }
        }
        "floating" => {
            if parts.len() >= 2 && parts[1] == "toggle" {
                Command::FloatingToggle
            } else {
                Command::Raw(parts.join(" "))
            }
        }
        "layout" => {
            if parts.len() < 2 {
                return Err("layout requires argument".into());
            }
            Command::Layout(match parts[1] {
                "stacking" => LayoutCommand::Stacking,
                "tabbed" => LayoutCommand::Tabbed,
                "toggle" => {
                    if parts.len() >= 3 && parts[2] == "split" {
                        LayoutCommand::ToggleSplit
                    } else {
                        return Err("Unknown layout toggle command".into());
                    }
                }
                _ => return Err(format!("Unknown layout: {}", parts[1]).into()),
            })
        }
        "scratchpad" => {
            if parts.len() >= 2 && parts[1] == "show" {
                Command::Scratchpad(ScratchpadCommand::Show)
            } else {
                Command::Raw(parts.join(" "))
            }
        }
        "mode" => {
            if parts.len() >= 2 && parts[1] == "toggle" {
                Command::FocusModeToggle
            } else if parts.len() >= 2 && parts[1] == "\"resize\"" {
                Command::ResizeMode
            } else {
                Command::Raw(parts.join(" "))
            }
        }
        _ => Command::Raw(parts.join(" ")),
    };

    Ok(cmd)
}

fn parse_direction(dir: &str) -> Result<Direction, Box<dyn std::error::Error>> {
    match dir {
        "left" => Ok(Direction::Left),
        "right" => Ok(Direction::Right),
        "up" => Ok(Direction::Up),
        "down" => Ok(Direction::Down),
        _ => Err(format!("Unknown direction: {dir}").into()),
    }
}

fn parse_workspace_target(parts: &[&str]) -> Result<WorkspaceTarget, Box<dyn std::error::Error>> {
    let first = parts.first().ok_or("Workspace target required")?;

    match *first {
        "1" => Ok(WorkspaceTarget::Number(1)),
        "2" => Ok(WorkspaceTarget::Number(2)),
        "3" => Ok(WorkspaceTarget::Number(3)),
        "4" => Ok(WorkspaceTarget::Number(4)),
        "5" => Ok(WorkspaceTarget::Number(5)),
        "6" => Ok(WorkspaceTarget::Number(6)),
        "7" => Ok(WorkspaceTarget::Number(7)),
        "8" => Ok(WorkspaceTarget::Number(8)),
        "9" => Ok(WorkspaceTarget::Number(9)),
        "10" => Ok(WorkspaceTarget::Number(10)),
        "next" | "next_on_output" => Ok(WorkspaceTarget::Next),
        "prev" | "previous" | "prev_on_output" => Ok(WorkspaceTarget::Previous),
        name => Ok(WorkspaceTarget::Name(name.to_string())),
    }
}

fn parse_output(config: &mut Config, parts: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    // Format: output <name> [scale <value>] [resolution <WxH>] [position <x,y>] [transform <value>] [split <horizontal|vertical|grid> <count>]
    // Note: resolution and position are in physical pixels
    // Note: parts[0] is the output name since we're called with &parts[1..]
    // Transform values match sway: normal, 90, 180, 270, flipped, flipped-90, flipped-180, flipped-270
    if parts.len() < 1 {
        return Err("output requires at least a name".into());
    }

    let output_name = parts.first().ok_or("Missing output name")?.to_string();
    let mut output_config = OutputConfig {
        name: output_name.clone(),
        scale: None,
        resolution: None,
        position: None,
        transform: None,
        background: None,
        split: None,
    };

    let mut i = 1; // Start at 1 since parts[0] is the output name
    while i < parts.len() {
        match parts[i] {
            "scale" if i + 1 < parts.len() => {
                let scale: f64 = parts[i + 1]
                    .parse()
                    .map_err(|_| format!("Invalid scale value: {}", parts[i + 1]))?;
                if scale < 0.5 || scale > 4.0 {
                    return Err("Scale must be between 0.5 and 4.0".into());
                }
                output_config.scale = Some(scale);
                i += 2;
            }
            "resolution" if i + 1 < parts.len() => {
                let res_parts: Vec<&str> = parts[i + 1].split('x').collect();
                if res_parts.len() != 2 {
                    return Err(format!("Invalid resolution format: {}", parts[i + 1]).into());
                }
                let width_str = res_parts.first().ok_or("Missing width in resolution")?;
                let height_str = res_parts.get(1).ok_or("Missing height in resolution")?;
                let width: i32 = width_str
                    .parse()
                    .map_err(|_| format!("Invalid width: {width_str}"))?;
                let height: i32 = height_str
                    .parse()
                    .map_err(|_| format!("Invalid height: {height_str}"))?;
                output_config.resolution = Some((width, height));
                i += 2;
            }
            "position" if i + 1 < parts.len() => {
                let pos_parts: Vec<&str> = parts[i + 1].split(',').collect();
                if pos_parts.len() != 2 {
                    return Err(format!("Invalid position format: {}", parts[i + 1]).into());
                }
                let x_str = pos_parts.first().ok_or("Missing x position")?;
                let y_str = pos_parts.get(1).ok_or("Missing y position")?;
                let x: i32 = x_str
                    .parse()
                    .map_err(|_| format!("Invalid x position: {x_str}"))?;
                let y: i32 = y_str
                    .parse()
                    .map_err(|_| format!("Invalid y position: {y_str}"))?;
                output_config.position = Some((x, y));
                i += 2;
            }
            "transform" if i + 1 < parts.len() => {
                // Parse transform values like "90", "180", "270", "flipped", "flipped-90", etc.
                let transform_str = parts[i + 1].to_lowercase();
                let transform = match transform_str.as_str() {
                    "normal" | "0" => "normal",
                    "90" => "90",
                    "180" => "180", 
                    "270" => "270",
                    "flipped" => "flipped",
                    "flipped-90" => "flipped-90",
                    "flipped-180" => "flipped-180",
                    "flipped-270" => "flipped-270",
                    _ => return Err(format!("Invalid transform value: {}. Valid values are: normal, 90, 180, 270, flipped, flipped-90, flipped-180, flipped-270", parts[i + 1]).into()),
                };
                output_config.transform = Some(transform.to_string());
                i += 2;
            }
            "split" if i + 2 < parts.len() => {
                match parts[i + 1] {
                    "horizontal" | "h" => {
                        let count: usize = parts[i + 2]
                            .parse()
                            .map_err(|_| format!("Invalid split count: {}", parts[i + 2]))?;
                        if count < 2 || count > 10 {
                            return Err("Split count must be between 2 and 10".into());
                        }
                        output_config.split =
                            Some((crate::virtual_output::SplitType::Horizontal, count));
                        i += 3;
                    }
                    "vertical" | "v" => {
                        let count: usize = parts[i + 2]
                            .parse()
                            .map_err(|_| format!("Invalid split count: {}", parts[i + 2]))?;
                        if count < 2 || count > 10 {
                            return Err("Split count must be between 2 and 10".into());
                        }
                        output_config.split =
                            Some((crate::virtual_output::SplitType::Vertical, count));
                        i += 3;
                    }
                    "grid" | "g" if i + 3 < parts.len() => {
                        // Grid expects rows and columns (e.g., "grid 2 3" for 2x3 grid)
                        let rows: usize = parts[i + 2]
                            .parse()
                            .map_err(|_| format!("Invalid grid rows: {}", parts[i + 2]))?;
                        let cols: usize = parts[i + 3]
                            .parse()
                            .map_err(|_| format!("Invalid grid columns: {}", parts[i + 3]))?;
                        if rows < 1 || rows > 5 || cols < 1 || cols > 5 {
                            return Err("Grid dimensions must be between 1 and 5".into());
                        }
                        let total = rows * cols;
                        output_config.split =
                            Some((crate::virtual_output::SplitType::Grid(rows, cols), total));
                        i += 4;
                    }
                    _ => return Err(format!("Invalid split type: {}", parts[i + 1]).into()),
                }
            }
            _ => {
                return Err(format!("Unknown output parameter: {}", parts[i]).into());
            }
        }
    }

    // Store or update the output configuration
    if let Some(existing) = config.outputs.iter_mut().find(|o| o.name == output_name) {
        *existing = output_config;
    } else {
        config.outputs.push(output_config);
    }

    Ok(())
}

fn parse_virtual_output(
    config: &mut Config,
    parts: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    // Format: virtual_output <name> outputs <output1,output2,...> [region <x,y,width,height>]
    // Note: region coordinates are in physical pixels and will be converted to logical based on output scale
    if parts.len() < 4 {
        return Err("virtual_output requires name and outputs list".into());
    }

    let name = parts
        .get(0)
        .ok_or("Missing virtual output name")?
        .to_string();

    if parts.get(1) != Some(&"outputs") {
        return Err("virtual_output second parameter must be 'outputs'".into());
    }

    let outputs: Vec<String> = parts
        .get(2)
        .ok_or("Missing outputs list")?
        .split(',')
        .map(|s| s.to_string())
        .collect();
    if outputs.is_empty() {
        return Err("virtual_output requires at least one output".into());
    }

    let mut virtual_config = VirtualOutputConfig {
        name,
        outputs,
        region: None,
    };

    // Check for optional region parameter
    if parts.len() >= 5 && parts.get(3) == Some(&"region") {
        let region_str = parts.get(4).ok_or("Missing region specification")?;
        let region_parts: Vec<&str> = region_str.split(',').collect();
        if region_parts.len() != 4 {
            return Err("region requires x,y,width,height format".into());
        }

        let x_str = region_parts
            .get(0)
            .ok_or("Missing x coordinate in region")?;
        let y_str = region_parts
            .get(1)
            .ok_or("Missing y coordinate in region")?;
        let width_str = region_parts.get(2).ok_or("Missing width in region")?;
        let height_str = region_parts.get(3).ok_or("Missing height in region")?;

        let x: i32 = x_str
            .parse()
            .map_err(|_| format!("Invalid x coordinate: {x_str}"))?;
        let y: i32 = y_str
            .parse()
            .map_err(|_| format!("Invalid y coordinate: {y_str}"))?;
        let width: i32 = width_str
            .parse()
            .map_err(|_| format!("Invalid width: {width_str}"))?;
        let height: i32 = height_str
            .parse()
            .map_err(|_| format!("Invalid height: {height_str}"))?;

        virtual_config.region = Some(VirtualOutputRegion {
            x,
            y,
            width,
            height,
        });
    }

    config.virtual_outputs.push(virtual_config);
    Ok(())
}

fn parse_workspace(
    _config: &mut Config,
    _parts: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    // Workspace configuration would be parsed here
    // For now, workspaces are created dynamically as needed
    Ok(())
}

fn parse_gaps(config: &mut Config, parts: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    if parts.len() < 2 {
        return Ok(());
    }

    let value: i32 = parts.get(1).ok_or("Missing gap value")?.parse()?;

    match parts.first().ok_or("Missing gap type")?.as_ref() {
        "inner" => config.gaps.inner = Some(value),
        "outer" => config.gaps.outer = Some(value),
        "top" => config.gaps.top = Some(value),
        "bottom" => config.gaps.bottom = Some(value),
        "left" => config.gaps.left = Some(value),
        "right" => config.gaps.right = Some(value),
        _ => {}
    }

    Ok(())
}

fn parse_border(config: &mut Config, parts: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    if parts.len() >= 2 && parts.first() == Some(&"pixel") {
        config.border.width = parts.get(1).ok_or("Missing border width")?.parse()?;
    }
    Ok(())
}

fn parse_font(config: &mut Config, parts: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    if parts.len() >= 2 && parts.first() == Some(&"pango:") {
        config.font = parts[1..].join(" ");
    }
    Ok(())
}

fn parse_input(config: &mut Config, line: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Input lines are special - they have the format:
    // input <identifier> { <settings> }
    // We need to parse this differently than other commands

    // First, extract the identifier and the block content
    let input_start = line.find("input").unwrap() + 5;
    let rest = line[input_start..].trim_start();

    // Find the opening brace
    let brace_pos = rest
        .find('{')
        .ok_or("Missing opening brace for input block")?;
    let identifier = rest[..brace_pos].trim().to_string();

    // For now, we'll just store the input config if it's in a single line
    // In a real implementation, we'd need to handle multi-line blocks
    if let Some(closing_brace) = rest.rfind('}') {
        let content = &rest[brace_pos + 1..closing_brace];

        let mut input_config = InputConfig {
            identifier,
            repeat_delay: None,
            repeat_rate: None,
            xkb_layout: None,
            xkb_variant: None,
            xkb_model: None,
            xkb_options: None,
            accel_speed: None,
            accel_profile: None,
            natural_scroll: None,
            tap: None,
            tap_button_map: None,
            scroll_method: None,
            left_handed: None,
            middle_emulation: None,
        };

        // Parse each setting
        for setting in content.split_whitespace().collect::<Vec<_>>().chunks(2) {
            if setting.len() == 2 {
                match setting[0] {
                    "repeat_delay" => {
                        input_config.repeat_delay = Some(setting[1].parse()?);
                    }
                    "repeat_rate" => {
                        input_config.repeat_rate = Some(setting[1].parse()?);
                    }
                    "xkb_layout" => {
                        input_config.xkb_layout = Some(setting[1].to_string());
                    }
                    "xkb_variant" => {
                        input_config.xkb_variant = Some(setting[1].to_string());
                    }
                    "xkb_model" => {
                        input_config.xkb_model = Some(setting[1].to_string());
                    }
                    "xkb_options" => {
                        input_config.xkb_options = Some(setting[1].to_string());
                    }
                    "accel_speed" => {
                        input_config.accel_speed = Some(setting[1].parse()?);
                    }
                    "accel_profile" => {
                        input_config.accel_profile = match setting[1] {
                            "flat" => Some(AccelProfile::Flat),
                            "adaptive" => Some(AccelProfile::Adaptive),
                            _ => None,
                        };
                    }
                    "natural_scroll" => {
                        input_config.natural_scroll = match setting[1] {
                            "enabled" | "yes" | "true" | "on" => Some(true),
                            "disabled" | "no" | "false" | "off" => Some(false),
                            _ => None,
                        };
                    }
                    "tap" => {
                        input_config.tap = match setting[1] {
                            "enabled" | "yes" | "true" | "on" => Some(true),
                            "disabled" | "no" | "false" | "off" => Some(false),
                            _ => None,
                        };
                    }
                    "tap_button_map" => {
                        input_config.tap_button_map = match setting[1] {
                            "lrm" => Some(TapButtonMap::Lrm),
                            "lmr" => Some(TapButtonMap::Lmr),
                            _ => None,
                        };
                    }
                    "scroll_method" => {
                        input_config.scroll_method = match setting[1] {
                            "two_finger" => Some(ScrollMethod::TwoFinger),
                            "edge" => Some(ScrollMethod::Edge),
                            "on_button_down" => Some(ScrollMethod::OnButtonDown),
                            _ => None,
                        };
                    }
                    "left_handed" => {
                        input_config.left_handed = match setting[1] {
                            "enabled" | "yes" | "true" | "on" => Some(true),
                            "disabled" | "no" | "false" | "off" => Some(false),
                            _ => None,
                        };
                    }
                    "middle_emulation" => {
                        input_config.middle_emulation = match setting[1] {
                            "enabled" | "yes" | "true" | "on" => Some(true),
                            "disabled" | "no" | "false" | "off" => Some(false),
                            _ => None,
                        };
                    }
                    _ => {}
                }
            }
        }

        config.input_configs.push(input_config);
    }

    Ok(())
}
