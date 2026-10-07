pub mod atlas;
pub mod materials;
mod particle_shapes;
pub mod particles;
pub mod relief;
pub mod scene3d;
pub mod wind;

#[cfg(target_arch = "wasm32")]
mod gpu;
#[cfg(target_arch = "wasm32")]
mod gpu3d;
#[cfg(target_arch = "wasm32")]
pub use gpu::{DrawRect, GpuBackend, Renderer, TextDraw, WorldTextDraw};
#[cfg(target_arch = "wasm32")]
pub use gpu3d::{Globals3d, GroundVertex, Scene3dFrame, ShapeInstance};
