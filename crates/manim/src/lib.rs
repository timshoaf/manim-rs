//! A Rust + WebGPU reimplementation of
//! [Manim Community Edition](https://docs.manim.community): declarative,
//! real-time mathematical animation.
//!
//! Scenes are described by a [`SceneBuilder`](prelude::SceneBuilder) whose
//! `construct` builds a timeline of animations; the timeline can then be
//! played in real time, scrubbed, or rendered offline frame by frame.
//!
//! ```no_run
//! use manim::prelude::*;
//! use manim::render::OffscreenRenderer;
//!
//! struct SquareToCircle;
//!
//! impl SceneBuilder for SquareToCircle {
//!     fn construct(&self, scene: &mut Scene) -> Result<()> {
//!         let square = scene.add(Square::new().with_fill(BLUE, 0.7));
//!         scene.play(square.animate().rotate(PI / 4.0))?;
//!         scene.play(TransformInto::new(square, Circle::new().with_fill(RED, 0.7)))?;
//!         scene.wait(1.0);
//!         Ok(())
//!     }
//! }
//!
//! // One-liner render to MP4 (needs `ffmpeg` on PATH):
//! manim::render(&SquareToCircle, Config::low(), "square_to_circle.mp4")?;
//! # Ok::<(), manim::render::RenderError>(())
//! ```

pub use manim_color as color;
pub use manim_core as core;
pub use manim_math as math;
pub use manim_render as render;

pub use manim_core::animations;
pub use manim_core::error::{CoreError, Result};

/// The browser canvas renderer, re-exported on wasm32 with the `web` feature.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub use manim_render::CanvasSurface;

// The offline `render` and native `preview` entry points below; their shared
// imports are unused on wasm (both are native-only).
#[cfg(not(target_arch = "wasm32"))]
use manim_core::config::Config;
#[cfg(not(target_arch = "wasm32"))]
use manim_core::scene::{Scene, SceneBuilder};
#[cfg(not(target_arch = "wasm32"))]
use manim_render::RenderError;

/// Builds `builder` into a [`Scene`] and renders it to an MP4 at `out`.
///
/// This is the batteries-included offline entry point: it runs the scene's
/// `construct`, then streams every frame through `ffmpeg` (which must be on
/// `PATH`). For a PNG sequence or finer control, use
/// [`render::VideoExporter`](manim_render::export::VideoExporter) directly.
///
/// # Errors
///
/// [`RenderError::Core`](manim_render::RenderError::Core) if `construct` fails,
/// [`RenderError::FfmpegNotFound`](manim_render::RenderError::FfmpegNotFound) if
/// `ffmpeg` is missing, or a GPU/encode error.
///
/// ```no_run
/// use manim::prelude::*;
/// # use manim::animations::Create;
/// struct Demo;
/// impl SceneBuilder for Demo {
///     fn construct(&self, scene: &mut Scene) -> Result<()> {
///         let c = scene.add(Circle::new());
///         scene.play(Create::new(c))?;
///         Ok(())
///     }
/// }
/// manim::render(&Demo, Config::low(), "demo.mp4")?;
/// # Ok::<(), manim::render::RenderError>(())
/// ```
#[cfg(not(target_arch = "wasm32"))]
pub fn render(
    builder: &dyn SceneBuilder,
    config: Config,
    out: impl AsRef<std::path::Path>,
) -> std::result::Result<(), RenderError> {
    let mut scene = Scene::build(builder, config.clone())?;
    manim_render::export::VideoExporter::render_to_mp4(&mut scene, out, &config)
}

/// Builds `builder` into a [`Scene`] and opens a realtime preview window.
///
/// Available with the `preview` feature. Blocks until the user closes the
/// window (Space play/pause, ←/→ seek, R restart, Esc quit).
///
/// # Errors
///
/// [`RenderError::Core`](manim_render::RenderError::Core) if `construct` fails,
/// or a window/GPU-surface error.
///
/// ```no_run
/// use manim::prelude::*;
/// # use manim::animations::Create;
/// # struct Demo;
/// # impl SceneBuilder for Demo {
/// #     fn construct(&self, scene: &mut Scene) -> Result<()> {
/// #         let c = scene.add(Circle::new());
/// #         scene.play(Create::new(c))?;
/// #         Ok(())
/// #     }
/// # }
/// manim::preview(&Demo, Config::low())?;
/// # Ok::<(), manim::render::RenderError>(())
/// ```
#[cfg(feature = "preview")]
pub fn preview(builder: &dyn SceneBuilder, config: Config) -> std::result::Result<(), RenderError> {
    let mut scene = Scene::build(builder, config)?;
    manim_render::RealtimePlayer::new(&mut scene).run()
}

/// Everything you need to build scenes, in one import.
///
/// Re-exports the scene machinery, the geometry catalog, the animation
/// catalog, colors, and the scene-space constants.
///
/// ```
/// use manim::prelude::*;
/// let mut scene = Scene::new(Config::default());
/// let circle = scene.add(Circle::new());
/// scene.play(Create::new(circle)).unwrap();
/// assert!(scene.total_duration() > 0.0);
/// ```
pub mod prelude {
    pub use manim_core::animations::{
        AnimBuilder, Animate, AnimationGroup, Create, DrawBorderThenFill, FadeIn, FadeOut,
        LaggedStart, MoveAlongPath, MoveTo, Rotate, Rotating, SetValue, Shift,
        ShowIncreasingSubsets, ShowSubmobjectsOneByOne, Succession, Transform, TransformInto,
        Uncreate, UpdateFromFunc, ValueTracker,
    };
    pub use manim_core::prelude::*;
}
