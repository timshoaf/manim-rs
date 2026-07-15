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
//! - [`export`] — [`VideoExporter`](export::VideoExporter): MP4 (via `ffmpeg`)
//!   and PNG-sequence output.
//! - [`layout`] — letterbox math for fitting a fixed-aspect frame in a window.
//! - [`preview`] *(feature `preview`)* — the winit
//!   [`RealtimePlayer`](preview::RealtimePlayer) window.
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
//! Realtime windowed playback lives in [`preview`] (behind the `preview`
//! feature); it reuses the same [`Pipeline`](renderer::Pipeline), which renders
//! into any [`wgpu::TextureView`] — surface or offscreen.

pub mod camera;
pub mod export;
pub mod golden;
pub mod layout;
pub mod renderer;
pub mod tessellate;

#[cfg(feature = "preview")]
pub mod preview;

pub use camera::Camera2D;
pub use export::VideoExporter;
pub use renderer::{GpuContext, OffscreenRenderer, Pipeline, RenderError, TextureTarget};
pub use tessellate::{FrameMesh, MeshData, TessellationCache, Vertex};

#[cfg(feature = "preview")]
pub use preview::RealtimePlayer;
