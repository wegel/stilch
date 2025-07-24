use smithay::backend::renderer::{
    element::{
        memory::MemoryRenderBufferRenderElement,
        solid::{SolidColorBuffer, SolidColorRenderElement},
        Kind,
    },
    ImportAll, ImportMem, Renderer,
};
use smithay::utils::{Logical, Point, Rectangle, Scale, Size};

mod text_render;
use text_render::TabTextCache;

/// Tab bar height in logical pixels
pub const TAB_BAR_HEIGHT: i32 = 30;

/// Colors for tab bar
pub struct TabBarColors {
    pub active_bg: [f32; 4],
    pub inactive_bg: [f32; 4],
    pub active_text: [f32; 4],
    pub inactive_text: [f32; 4],
    pub border: [f32; 4],
}

impl Default for TabBarColors {
    fn default() -> Self {
        Self {
            active_bg: [0.2, 0.2, 0.3, 1.0],     // Dark blue-gray
            inactive_bg: [0.1, 0.1, 0.15, 1.0],  // Darker gray
            active_text: [1.0, 1.0, 1.0, 1.0],   // White
            inactive_text: [0.7, 0.7, 0.7, 1.0], // Light gray
            border: [0.3, 0.3, 0.4, 1.0],        // Medium gray
        }
    }
}

/// Information about a single tab
#[derive(Debug, Clone)]
pub struct TabInfo {
    pub window_id: crate::window::WindowId,
    pub title: String,
    pub app_id: Option<String>,
    pub is_active: bool,
}

/// Tab bar render element using text rendering
pub struct TabBar {
    tabs: Vec<TabInfo>,
    geometry: Rectangle<i32, Logical>,
    _colors: TabBarColors,
    buffers: Vec<SolidColorBuffer>,
    text_cache: TabTextCache,
}

impl TabBar {
    pub fn new(tabs: Vec<TabInfo>, geometry: Rectangle<i32, Logical>) -> Self {
        let colors = TabBarColors::default();
        let mut buffers = Vec::new();

        // Create solid color buffers for each tab
        if !tabs.is_empty() {
            let tab_width = geometry.size.w / tabs.len() as i32;

            for (i, tab) in tabs.iter().enumerate() {
                let color = if tab.is_active {
                    colors.active_bg
                } else {
                    colors.inactive_bg
                };

                let buffer = SolidColorBuffer::new(Size::from((tab_width, TAB_BAR_HEIGHT)), color);
                buffers.push(buffer);

                // Add border buffer between tabs
                if i < tabs.len() - 1 {
                    let border_buffer =
                        SolidColorBuffer::new(Size::from((1, TAB_BAR_HEIGHT)), colors.border);
                    buffers.push(border_buffer);
                }
            }
        }

        Self {
            tabs,
            geometry,
            _colors: colors,
            buffers,
            text_cache: TabTextCache::new(),
        }
    }

    /// Get render elements for the tab bar with text
    pub fn render_elements_with_text<R>(
        &mut self,
        renderer: &mut R,
        scale: Scale<f64>,
    ) -> Vec<MemoryRenderBufferRenderElement<R>>
    where
        R: Renderer + ImportAll + ImportMem,
        R::TextureId: Clone + Send + 'static,
    {
        let mut elements = Vec::new();

        if self.tabs.is_empty() {
            return elements;
        }

        let tab_width = self.geometry.size.w / self.tabs.len() as i32;
        let mut x_offset = 0;

        for tab in &self.tabs {
            // Get or create the rendered tab with text
            if let Ok(buffer) = self.text_cache.get_or_create_tab(
                &tab.title,
                tab_width,
                TAB_BAR_HEIGHT,
                tab.is_active,
                scale,
            ) {
                let location = Point::<i32, Logical>::from((
                    self.geometry.loc.x + x_offset,
                    self.geometry.loc.y,
                ))
                .to_f64()
                .to_physical(scale)
                .to_i32_round::<i32>();

                // Create render element from buffer
                // The buffer already has the correct size and format
                if let Ok(elem) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    location.to_f64(), // location as f64 Point
                    &buffer,
                    None, // No custom src
                    None, // No damage tracking
                    None, // No opaque regions
                    Kind::Unspecified,
                ) {
                    elements.push(elem);
                }
            }

            x_offset += tab_width;
        }

        elements
    }

    /// Get render elements for the tab bar (old solid color version for compatibility)
    pub fn render_elements(&self, scale: Scale<f64>) -> Vec<SolidColorRenderElement> {
        let mut elements = Vec::new();

        if self.tabs.is_empty() {
            return elements;
        }

        let tab_width = self.geometry.size.w / self.tabs.len() as i32;
        let mut x_offset = 0;

        for (i, buffer) in self.buffers.iter().enumerate() {
            let location =
                Point::<i32, Logical>::from((self.geometry.loc.x + x_offset, self.geometry.loc.y))
                    .to_f64()
                    .to_physical(scale)
                    .to_i32_round();

            elements.push(SolidColorRenderElement::from_buffer(
                buffer,
                location,
                scale,
                1.0, // alpha
                Kind::Unspecified,
            ));

            // Update x_offset - tab buffers are tab_width wide, border buffers are 1 pixel wide
            if i % 2 == 0 {
                // This is a tab buffer
                x_offset += tab_width;
            } else {
                // This is a border buffer
                x_offset += 1;
            }
        }

        elements
    }
}

/// Create tab bar render elements with text for a tabbed container
pub fn create_tab_bar_elements_with_text<R>(
    renderer: &mut R,
    tabs: Vec<TabInfo>,
    container_geometry: Rectangle<i32, Logical>,
    scale: Scale<f64>,
) -> Vec<MemoryRenderBufferRenderElement<R>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Clone + Send + 'static,
{
    let tab_bar_geometry = Rectangle {
        loc: container_geometry.loc,
        size: Size::from((container_geometry.size.w, TAB_BAR_HEIGHT)),
    };

    let mut tab_bar = TabBar::new(tabs, tab_bar_geometry);
    tab_bar.render_elements_with_text(renderer, scale)
}

/// Create tab bar render elements for a tabbed container (old solid color version)
pub fn create_tab_bar_elements(
    tabs: Vec<TabInfo>,
    container_geometry: Rectangle<i32, Logical>,
    scale: Scale<f64>,
) -> Vec<SolidColorRenderElement> {
    let tab_bar_geometry = Rectangle {
        loc: container_geometry.loc,
        size: Size::from((container_geometry.size.w, TAB_BAR_HEIGHT)),
    };

    let tab_bar = TabBar::new(tabs, tab_bar_geometry);
    tab_bar.render_elements(scale)
}

/// Calculate the client area for a tabbed container (excluding tab bar)
pub fn calculate_client_area(
    container_geometry: Rectangle<i32, Logical>,
) -> Rectangle<i32, Logical> {
    Rectangle {
        loc: Point::from((
            container_geometry.loc.x,
            container_geometry.loc.y + TAB_BAR_HEIGHT,
        )),
        size: Size::from((
            container_geometry.size.w,
            container_geometry.size.h - TAB_BAR_HEIGHT,
        )),
    }
}
