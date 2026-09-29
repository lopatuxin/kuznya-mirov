pub mod atlas;
pub mod scene3d;

#[cfg(target_arch = "wasm32")]
mod gpu;
#[cfg(target_arch = "wasm32")]
mod gpu3d;
#[cfg(target_arch = "wasm32")]
pub use gpu::{DrawRect, GpuBackend, Renderer, TextDraw, WorldTextDraw};
#[cfg(target_arch = "wasm32")]
pub use gpu3d::{Globals3d, GroundRect, Scene3dFrame, ShapeInstance};
