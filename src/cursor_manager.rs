use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::input::pointer::{CursorIcon, CursorImageStatus};
use smithay::utils::Transform;
use tracing::warn;
use xcursor::{
    parser::{parse_xcursor, Image},
    CursorTheme,
};

static FALLBACK_CURSOR_DATA: &[u8] = include_bytes!("../resources/cursor.rgba");

/// Manages cursor loading and caching for different cursor shapes
#[derive(Debug)]
pub struct CursorManager {
    theme: CursorTheme,
    size: u32,
    /// Cache of loaded cursors by icon and scale
    cache: HashMap<(CursorIcon, u32), Arc<CursorData>>,
    /// Cache of memory buffers by (icon, scale, frame_index)
    buffer_cache: HashMap<(CursorIcon, u32, usize), MemoryRenderBuffer>,
    /// Current cursor image status
    current_status: CursorImageStatus,
}

#[derive(Debug, Clone)]
struct CursorData {
    images: Vec<Image>,
}

impl CursorData {
    fn is_animated(&self) -> bool {
        self.images.len() > 1 || self.images.first().map_or(false, |img| img.delay > 0)
    }

    fn get_frame_index(&self, size: u32, time: Duration) -> usize {
        if !self.is_animated() {
            return 0;
        }

        let nearest = nearest_images(size, &self.images);
        if nearest.is_empty() {
            return 0;
        }

        let total_delay: u32 = nearest.iter().map(|img| img.delay).sum();
        if total_delay == 0 {
            return 0;
        }

        let mut millis = (time.as_millis() as u32) % total_delay;

        for (index, img) in nearest.iter().enumerate() {
            if millis < img.delay {
                return index;
            }
            millis -= img.delay;
        }

        0
    }

    fn get_image(&self, size: u32, time: Duration) -> Image {
        // Note: 'size' here is the target size in pixels, not a scale factor
        frame(time.as_millis() as u32, size, &self.images)
    }

    fn to_memory_buffer(&self, size: u32, _scale: u32, time: Duration) -> MemoryRenderBuffer {
        let image = self.get_image(size, time);
        MemoryRenderBuffer::from_slice(
            &image.pixels_rgba,
            Fourcc::Abgr8888,
            (image.width as i32, image.height as i32),
            1, // Cursor pixels are already at the desired size
            Transform::Normal,
            None,
        )
    }
}

impl CursorManager {
    pub fn new() -> Self {
        let theme_name = std::env::var("XCURSOR_THEME")
            .ok()
            .unwrap_or_else(|| "default".into());
        let size = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(24);

        let theme = CursorTheme::load(&theme_name);

        let mut manager = Self {
            theme,
            size,
            cache: HashMap::new(),
            buffer_cache: HashMap::new(),
            current_status: CursorImageStatus::default_named(),
        };

        // Ensure we have at least the default cursor
        if manager.load_cursor(CursorIcon::Default, 1).is_none() {
            warn!(
                "Failed to load default cursor from theme '{}', using built-in fallback",
                theme_name
            );
            // Add the fallback cursor to cache
            let fallback = Self::create_fallback_cursor();
            manager
                .cache
                .insert((CursorIcon::Default, 1), Arc::new(fallback.clone()));
            manager
                .cache
                .insert((CursorIcon::Default, 2), Arc::new(fallback.clone()));
            manager
                .cache
                .insert((CursorIcon::Default, 3), Arc::new(fallback));
        }

        manager
    }

    /// Set the current cursor image status
    pub fn set_cursor_image(&mut self, status: CursorImageStatus) {
        // Clear buffer cache when cursor changes
        if !matches!(&self.current_status, current if std::mem::discriminant(current) == std::mem::discriminant(&status))
        {
            self.buffer_cache.clear();
        }
        self.current_status = status;
    }

    /// Get the current cursor image status
    pub fn cursor_image(&self) -> &CursorImageStatus {
        &self.current_status
    }

    /// Get the base cursor size
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Get a memory buffer for the current cursor at the given scale and time
    pub fn get_current_cursor_buffer(
        &mut self,
        scale: u32,
        time: Duration,
    ) -> Option<MemoryRenderBuffer> {
        // Extract icon first to avoid borrow checker issues
        let icon = match &self.current_status {
            CursorImageStatus::Hidden => return None,
            CursorImageStatus::Surface(_) => return None, // Surface cursors are handled elsewhere
            CursorImageStatus::Named(icon) => *icon,
        };

        let size = self.size * scale;
        let cursor = self.get_cursor(icon, scale)?;

        // Calculate frame index for animated cursors
        let frame_index = if cursor.is_animated() {
            cursor.get_frame_index(size, time)
        } else {
            0
        };

        // Check cache first
        let cache_key = (icon, scale, frame_index);
        if let Some(buffer) = self.buffer_cache.get(&cache_key) {
            return Some(buffer.clone());
        }

        // Create new buffer and cache it
        let buffer = cursor.to_memory_buffer(size, scale, time);
        self.buffer_cache.insert(cache_key, buffer.clone());
        Some(buffer)
    }

    /// Check if the current cursor is animated
    pub fn is_current_cursor_animated(&mut self, scale: u32) -> bool {
        match &self.current_status {
            CursorImageStatus::Hidden => false,
            CursorImageStatus::Surface(_) => false,
            CursorImageStatus::Named(icon) => self
                .get_cursor(*icon, scale)
                .map(|cursor| cursor.is_animated())
                .unwrap_or(false),
        }
    }

    /// Get raw cursor data for XWayland
    /// Returns (pixels_rgba, width, height, xhot, yhot)
    pub fn get_current_cursor_for_xwayland(
        &mut self,
        scale: u32,
        time: Duration,
    ) -> Option<(Vec<u8>, u16, u16, u16, u16)> {
        match &self.current_status {
            CursorImageStatus::Named(icon) => {
                let size = self.size * scale;
                let cursor = self.get_cursor(*icon, scale)?;
                let image = cursor.get_image(size, time);
                Some((
                    image.pixels_rgba,
                    image.width as u16,
                    image.height as u16,
                    image.xhot as u16,
                    image.yhot as u16,
                ))
            }
            _ => None,
        }
    }

    /// Get the current cursor hotspot
    pub fn get_current_cursor_hotspot(&mut self, scale: u32, time: Duration) -> Option<(i32, i32)> {
        match &self.current_status {
            CursorImageStatus::Named(icon) => {
                let size = self.size * scale;
                let cursor = self.get_cursor(*icon, scale)?;
                let image = cursor.get_image(size, time);
                Some((image.xhot as i32, image.yhot as i32))
            }
            _ => None,
        }
    }

    /// Get a cursor for the given icon, loading it if necessary
    fn get_cursor(&mut self, icon: CursorIcon, scale: u32) -> Option<Arc<CursorData>> {
        let key = (icon, scale);

        if let Some(cursor) = self.cache.get(&key) {
            return Some(cursor.clone());
        }

        // Try to load the cursor
        let cursor_data = self.load_cursor(icon, scale)?;
        let cursor_arc = Arc::new(cursor_data);
        self.cache.insert(key, cursor_arc.clone());
        Some(cursor_arc)
    }

    /// Load a cursor from the theme
    fn load_cursor(&self, icon: CursorIcon, scale: u32) -> Option<CursorData> {
        let size = self.size * scale;

        // Try the primary name first
        if let Ok(cursor) = self.load_cursor_with_name(icon.name(), size) {
            return Some(cursor);
        }

        // Try alternative names
        for alt_name in icon.alt_names() {
            if let Ok(cursor) = self.load_cursor_with_name(alt_name, size) {
                return Some(cursor);
            }
        }

        warn!("Failed to load cursor '{}' at size {}", icon.name(), size);

        // For the default cursor, use the fallback
        if icon == CursorIcon::Default {
            Some(self.fallback_cursor())
        } else {
            // Try to fall back to default cursor
            self.load_cursor(CursorIcon::Default, scale)
        }
    }

    fn load_cursor_with_name(&self, name: &str, _size: u32) -> Result<CursorData, Error> {
        let icon_path = self.theme.load_icon(name).ok_or(Error::CursorNotFound)?;

        let mut cursor_file = std::fs::File::open(icon_path)?;
        let mut cursor_data = Vec::new();
        cursor_file.read_to_end(&mut cursor_data)?;

        let images = parse_xcursor(&cursor_data).ok_or(Error::ParseError)?;

        Ok(CursorData { images })
    }

    fn fallback_cursor(&self) -> CursorData {
        Self::create_fallback_cursor()
    }

    fn create_fallback_cursor() -> CursorData {
        CursorData {
            images: vec![Image {
                size: 32,
                width: 64,
                height: 64,
                xhot: 1,
                yhot: 1,
                delay: 1,
                pixels_rgba: Vec::from(FALLBACK_CURSOR_DATA),
                pixels_argb: vec![],
            }],
        }
    }
}

fn nearest_images(size: u32, images: &[Image]) -> Vec<&Image> {
    let nearest_image = match images
        .iter()
        .min_by_key(|image| (size as i32 - image.size as i32).abs())
    {
        Some(img) => img,
        None => return Vec::new(),
    };

    images
        .iter()
        .filter(|image| image.width == nearest_image.width && image.height == nearest_image.height)
        .collect()
}

fn frame(mut millis: u32, size: u32, images: &[Image]) -> Image {
    let nearest = nearest_images(size, images);
    let total = nearest.iter().fold(0, |acc, image| acc + image.delay);

    if total == 0 {
        return nearest
            .first()
            .map(|img| (*img).clone())
            .unwrap_or_else(|| Image {
                size: 24,
                width: 24,
                height: 24,
                xhot: 0,
                yhot: 0,
                delay: 0,
                pixels_rgba: vec![],
                pixels_argb: vec![],
            });
    }

    millis %= total;

    for img in &nearest {
        if millis < img.delay {
            return (*img).clone();
        }
        millis -= img.delay;
    }

    unreachable!()
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Cursor not found in theme")]
    CursorNotFound,
    #[error("Failed to read cursor file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse cursor file")]
    ParseError,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_manager_creation() {
        let manager = CursorManager::new();
        assert!(matches!(
            manager.cursor_image(),
            CursorImageStatus::Named(_)
        ));
    }

    #[test]
    fn test_cursor_loading() {
        let mut manager = CursorManager::new();

        // Test loading default cursor
        let buffer = manager.get_current_cursor_buffer(1, Duration::from_secs(0));
        assert!(buffer.is_some(), "Should load default cursor");

        // Test setting different cursor
        manager.set_cursor_image(CursorImageStatus::Named(CursorIcon::Pointer));
        let buffer = manager.get_current_cursor_buffer(1, Duration::from_secs(0));
        assert!(buffer.is_some(), "Should load pointer cursor");
    }

    #[test]
    fn test_cursor_caching() {
        let mut manager = CursorManager::new();

        // Load cursor twice with same parameters
        manager.set_cursor_image(CursorImageStatus::Named(CursorIcon::Text));
        let _ = manager.get_current_cursor_buffer(1, Duration::from_secs(0));
        let cache_size_before = manager.cache.len();

        let _ = manager.get_current_cursor_buffer(1, Duration::from_secs(0));
        let cache_size_after = manager.cache.len();

        assert_eq!(
            cache_size_before, cache_size_after,
            "Should use cached cursor"
        );
    }

    #[test]
    fn test_hidden_cursor() {
        let mut manager = CursorManager::new();
        manager.set_cursor_image(CursorImageStatus::Hidden);

        let buffer = manager.get_current_cursor_buffer(1, Duration::from_secs(0));
        assert!(buffer.is_none(), "Hidden cursor should return None");
    }

    #[test]
    fn test_fallback_cursor() {
        let manager = CursorManager::new();
        let fallback = manager.fallback_cursor();
        assert_eq!(fallback.images.len(), 1);
        assert_eq!(fallback.images[0].width, 64);
        assert_eq!(fallback.images[0].height, 64);
    }
}
