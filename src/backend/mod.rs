//! Backend implementations for the compositor

pub mod ascii;
pub mod ascii_integration;
pub mod test_harness;

use smithay::backend::input::InputBackend;
use smithay::backend::renderer::Renderer;

/// Trait for compositor backends
pub trait Backend {
    type InputBackend: InputBackend;
    type Renderer: Renderer;

    /// Get the input backend
    fn input_backend(&mut self) -> &mut Self::InputBackend;

    /// Get the renderer
    fn renderer(&mut self) -> &mut Self::Renderer;
}
