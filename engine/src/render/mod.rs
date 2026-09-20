pub mod atlas;

#[cfg(target_arch = "wasm32")]
mod gpu;
#[cfg(target_arch = "wasm32")]
pub use gpu::{DrawRect, GpuBackend, Renderer, TextDraw};
