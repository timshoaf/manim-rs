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
//! let mut scene = Scene::build(&SquareToCircle, Config::low())?;
//! let mut renderer = OffscreenRenderer::new(scene.config())?;
//! for (t, list) in scene.frames() {
//!     let _image = renderer.render_display_list(&list)?;
//!     let _ = t; // encode, save, or stream the frame
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub use manim_color as color;
pub use manim_core as core;
pub use manim_math as math;
pub use manim_render as render;

pub use manim_core::animations;
pub use manim_core::error::{CoreError, Result};

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
