use pangocairo::cairo::{self, ImageSurface};
use pangocairo::pango::{Alignment, EllipsizeMode, FontDescription};
use smithay::{
    backend::allocator::Fourcc,
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Scale, Transform},
};
use std::collections::HashMap;

/// Cache key for rendered tab text
#[derive(Hash, Eq, PartialEq, Clone)]
struct TabTextKey {
    title: String,
    width: i32,
    height: i32,
    is_active: bool,
    scale: u64, // Scale as fixed point (multiply by 1000)
}

/// Cache for rendered tab textures
/// Now compositor-scoped instead of global for better resource management
pub struct TabTextCache {
    cache: HashMap<TabTextKey, MemoryRenderBuffer>,
}

impl TabTextCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Get or create a rendered tab with text
    pub fn get_or_create_tab(
        &mut self,
        title: &str,
        width: i32,
        height: i32,
        is_active: bool,
        scale: Scale<f64>,
    ) -> Result<MemoryRenderBuffer, Box<dyn std::error::Error>> {
        let key = TabTextKey {
            title: title.to_string(),
            width,
            height,
            is_active,
            scale: (scale.x * 1000.0) as u64,
        };

        if let Some(buffer) = self.cache.get(&key) {
            return Ok(buffer.clone());
        }

        let buffer = render_tab_text(title, width, height, is_active, scale)?;
        self.cache.insert(key.clone(), buffer.clone());
        Ok(buffer)
    }

    /// Clear the cache (e.g., when theme changes)
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Remove old entries to prevent unbounded growth
    /// Call this periodically or when tabs are closed
    #[allow(dead_code)]
    pub fn prune(&mut self, max_entries: usize) {
        if self.cache.len() > max_entries {
            // Simple strategy: clear everything if we exceed the limit
            // A more sophisticated approach would track LRU
            self.cache.clear();
        }
    }
}

/// Render tab text to a memory buffer
fn render_tab_text(
    title: &str,
    width: i32,
    height: i32,
    is_active: bool,
    _scale: Scale<f64>, // Currently unused, but kept for future HiDPI support
) -> Result<MemoryRenderBuffer, Box<dyn std::error::Error>> {
    // Don't scale dimensions - keep them logical
    // The compositor will handle scaling for the output
    let physical_width = width;
    let physical_height = height;

    // Create Cairo surface
    let surface = ImageSurface::create(cairo::Format::ARgb32, physical_width, physical_height)?;
    let cr = cairo::Context::new(&surface)?;

    // Background color
    if is_active {
        // Active tab - darker background
        cr.set_source_rgba(0.2, 0.2, 0.2, 1.0);
    } else {
        // Inactive tab - lighter background
        cr.set_source_rgba(0.15, 0.15, 0.15, 1.0);
    }
    cr.paint()?;

    // Set up font
    // Don't scale font size - we're rendering at logical size
    let font_size = 14.0;
    let font = FontDescription::from_string(&format!("sans {}px", font_size));

    // Create Pango layout
    let layout = pangocairo::functions::create_layout(&cr);
    layout.set_font_description(Some(&font));
    layout.set_width(physical_width * pango::SCALE);
    layout.set_height(physical_height * pango::SCALE);
    layout.set_alignment(Alignment::Center);
    layout.set_ellipsize(EllipsizeMode::End);

    // Set the text
    layout.set_text(title);

    // Text color
    if is_active {
        cr.set_source_rgba(1.0, 1.0, 1.0, 1.0); // White for active
    } else {
        cr.set_source_rgba(0.7, 0.7, 0.7, 1.0); // Gray for inactive
    }

    // Center the text vertically
    let (_ink_rect, logical_rect) = layout.extents();
    let text_height = logical_rect.height() / pango::SCALE;
    let y_offset = (physical_height - text_height) / 2;

    // Draw the text
    cr.move_to(8.0, y_offset as f64); // Small left padding
    pangocairo::functions::show_layout(&cr, &layout);

    // Draw bottom border for active tab
    if is_active {
        cr.set_source_rgba(0.4, 0.6, 1.0, 1.0); // Blue accent
        cr.set_line_width(2.0);
        cr.move_to(0.0, physical_height as f64 - 1.0);
        cr.line_to(physical_width as f64, physical_height as f64 - 1.0);
        cr.stroke()?;
    }

    // Draw right border (tab separator)
    cr.set_source_rgba(0.3, 0.3, 0.3, 1.0);
    cr.set_line_width(1.0);
    cr.move_to(physical_width as f64 - 0.5, 0.0);
    cr.line_to(physical_width as f64 - 0.5, physical_height as f64);
    cr.stroke()?;

    drop(cr);

    // Get the pixel data
    let data = surface.take_data().map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::Other, "Failed to take surface data")
    })?;

    // Create memory buffer using from_slice like smithay does
    let buffer = MemoryRenderBuffer::from_slice(
        &data,
        Fourcc::Argb8888,
        (physical_width, physical_height),
        1, // Buffer scale is 1:1 since we're rendering at logical size
        Transform::Normal,
        None, // No damage tracking
    );

    Ok(buffer)
}
