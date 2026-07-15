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
//! Windowed rendering (a `SurfaceTarget`) is deferred to FE-95; the
//! [`Pipeline`](renderer::Pipeline) already renders into any
//! [`wgpu::TextureView`], so it slots in without a redesign.

pub mod camera;
pub mod golden;
pub mod renderer;
pub mod tessellate;

pub use camera::Camera2D;
pub use renderer::{GpuContext, OffscreenRenderer, Pipeline, RenderError, TextureTarget};
pub use tessellate::{FrameMesh, MeshData, TessellationCache, Vertex};
