//! wgpu renderer for `manim_rust`.
//!
//! This crate turns a [`DisplayList`](manim_core::display::DisplayList) — the
//! resolved, z-ordered paths that `manim-core` produces each frame — into
//! pixels. The pipeline is CPU tessellation (lyon) feeding a small
//! premultiplied-alpha wgpu pipeline, rendered offscreen and read back to an
//! [`image::RgbaImage`]. See `docs/design/05-rendering.md`.
//!
//! # Layers
//!
//! - [`tessellate`] — [`DisplayList`](manim_core::display::DisplayList) →
//!   [`FrameMesh`](tessellate::FrameMesh), with a generation-keyed
//!   [`TessellationCache`](tessellate::TessellationCache).
//! - [`camera`] — [`Camera2D`](camera::Camera2D): a scene rectangle → an NDC
//!   view-projection matrix.
//! - [`renderer`] — the wgpu [`GpuContext`](renderer::GpuContext),
//!   [`Pipeline`](renderer::Pipeline), offscreen
//!   [`TextureTarget`](renderer::TextureTarget), and the high-level
//!   [`OffscreenRenderer`](renderer::OffscreenRenderer).
//! - [`export`] — `VideoExporter`: MP4 (via `ffmpeg`) and PNG-sequence output
//!   *(native only)*.
//! - [`layout`] — letterbox math for fitting a fixed-aspect frame in a window.
//! - `preview` *(feature `preview`, native only)* — the winit `RealtimePlayer`
//!   window.
//! - `canvas` *(feature `web`, wasm32 only)* — `CanvasSurface`, rendering into
//!   an HTML `<canvas>`.
//!
//! # Portability
//!
//! Native builds get the full stack (offscreen render, PNG/MP4 export, winit
//! preview). wasm32 builds drop the filesystem/subprocess/window pieces; enable
//! the `web` feature for `CanvasSurface` and construct the GPU context with the
//! async [`GpuContext::new_async`](renderer::GpuContext::new_async).
//!
//! # Quickstart
//!
//! ```no_run
//! use manim_core::config::Config;
//! use manim_core::geometry::Circle;
//! use manim_core::scene_state::SceneState;
//! use manim_render::OffscreenRenderer;
//!
//! let mut scene = SceneState::new();
//! scene.add(Circle::new());
//!
//! let mut renderer = OffscreenRenderer::new(&Config::low())?;
//! renderer.render_to_png(&scene.display_list(), "circle.png")?;
//! # Ok::<(), manim_render::RenderError>(())
//! ```
//!
//! Realtime windowed playback lives in the `preview` module (behind the
//! `preview` feature); it reuses the same [`Pipeline`](renderer::Pipeline),
//! which renders into any [`wgpu::TextureView`] — surface or offscreen.

pub mod camera;
pub mod layout;
pub mod renderer;
pub mod tessellate;

// Offline PNG/MP4 export and the golden harness are native-only: they touch the
// filesystem, an `ffmpeg` subprocess, and blocking pixel readback.
#[cfg(not(target_arch = "wasm32"))]
pub mod export;
#[cfg(not(target_arch = "wasm32"))]
pub mod golden;

// The winit preview is native-only. Asking for it on wasm is a hard error.
#[cfg(all(feature = "preview", target_arch = "wasm32"))]
compile_error!(
    "the `preview` feature is native-only (winit); on wasm enable the `web` \
     feature and render with `CanvasSurface` instead"
);
#[cfg(all(feature = "preview", not(target_arch = "wasm32")))]
pub mod preview;

// Browser canvas rendering, wasm + `web` feature only.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub mod canvas;

pub use camera::Camera2D;
pub use renderer::{GpuContext, Pipeline, RenderError, TextureTarget};
pub use tessellate::{FrameMesh, MeshData, TessellationCache, Vertex};

#[cfg(not(target_arch = "wasm32"))]
pub use export::VideoExporter;
#[cfg(not(target_arch = "wasm32"))]
pub use renderer::OffscreenRenderer;

#[cfg(all(feature = "preview", not(target_arch = "wasm32")))]
pub use preview::RealtimePlayer;

#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub use canvas::CanvasSurface;
