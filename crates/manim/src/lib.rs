//! A Rust + WebGPU reimplementation of
//! [Manim Community Edition](https://docs.manim.community): declarative,
//! real-time mathematical animation.
//!
//! Scenes are described by a [`SceneBuilder`] whose
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
pub use manim_text as text;

/// The linear-algebra types the mesh API speaks in, re-exported so scene authors
/// need not depend on `glam` directly.
///
/// [`prelude`] pulls [`Vec3`](glam::Vec3) and [`Mat4`](glam::Mat4) out of here:
/// mesh geometry is `Vec3`-valued and an
/// [`Instance`](manim_core::mesh::Instance) transform is a `Mat4`.
pub use glam;

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
/// [`RenderError::Core`] if `construct` fails,
/// [`RenderError::FfmpegNotFound`] if
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
/// Re-exports the scene machinery, the geometry catalog, the animation and
/// coordinate catalogs, the text/TeX/matrix/table mobjects, colors, and the
/// scene-space constants — so a scene author rarely path-qualifies anything.
///
/// ```
/// use manim::prelude::*;
/// let mut scene = Scene::new(Config::default());
/// let circle = scene.add(Circle::new());
/// scene.play(Create::new(circle)).unwrap();
/// assert!(scene.total_duration() > 0.0);
/// ```
///
/// The text, matrix, table, and label mobjects from `manim-text` are here too;
/// this compiles purely by naming them through the prelude (no rendering):
///
/// ```
/// use manim::prelude::*;
/// fn _prelude_types_resolve() {
///     // Core additions (geometry/boolean/svg/image).
///     let _ = std::any::type_name::<Brace>();
///     let _ = std::any::type_name::<SVGMobject>();
///     let _ = std::any::type_name::<ImageMobject>();
///     let _ = std::any::type_name::<Union>();
///     let _ = std::any::type_name::<AnimatedBoundary>();
///     // manim-text surface the gallery used to path-qualify.
///     let _ = std::any::type_name::<DecimalNumber>();
///     let _ = std::any::type_name::<Integer>();
///     let _ = std::any::type_name::<Variable>();
///     let _ = std::any::type_name::<Matrix>();
///     let _ = std::any::type_name::<IntegerMatrix>();
///     let _ = std::any::type_name::<Table>();
///     let _ = std::any::type_name::<MathTable>();
///     let _ = std::any::type_name::<LabeledDot>();
///     let _ = std::any::type_name::<BulletedList>();
///     let _ = std::any::type_name::<Title>();
///     let _ = std::any::type_name::<MarkupText>();
///     let _ = std::any::type_name::<TransformMatchingTex>();
///     // The depth-tested mesh path.
///     let _ = std::any::type_name::<Mesh>();
///     let _ = std::any::type_name::<Surface3D>();
///     let _ = std::any::type_name::<InstancedMesh>();
///     let _ = std::any::type_name::<HeightField>();
///     let _ = std::any::type_name::<TriMesh>();
///     let _ = std::any::type_name::<MeshMaterial>();
///     let _ = std::any::type_name::<Shading>();
///     let _ = std::any::type_name::<Instance>();
///     let _ = std::any::type_name::<MorphMesh>();
///     let _ = std::any::type_name::<MorphSurface>();
/// }
/// // The label helpers are extension traits (they add `.plot_label(..)` etc.);
/// // they resolve as bounds through the prelude too.
/// fn _prelude_traits_resolve<T>()
/// where
///     T: AxesLabels + CoordinateLabels + GraphLabel + BarChartLabels,
/// {
/// }
/// ```
pub mod prelude {
    pub use manim_core::animations::{
        AnimBuilder, Animate, AnimationGroup, Create, DrawBorderThenFill, FadeIn, FadeOut,
        LaggedStart, MoveAlongPath, MoveTo, MoveToTarget, Rotate, Rotating, SetValue, Shift,
        ShowIncreasingSubsets, ShowSubmobjectsOneByOne, Succession, Transform, TransformInto,
        Uncreate, UpdateFromFunc, ValueTracker,
    };
    pub use manim_core::prelude::*;

    /// The linear algebra the mesh API speaks in: geometry is `Vec3`-valued and
    /// an [`Instance`] transform is a `Mat4`. (`Point` is an alias of `Vec3`.)
    pub use glam::{Mat4, Vec3};
    /// The depth-tested mesh path (`docs/design/12-mesh-pipeline.md`) — a
    /// *second* path alongside the project-and-sort `threed` mobjects
    /// (`Surface`, `Cube`, …), which stay in the prelude and keep working. See
    /// the migration guide for which to reach for. `MeshPayload`/`MeshMobject`
    /// are for authors implementing their own mesh mobject; reach those via
    /// `manim::core::mesh`.
    pub use manim_core::mesh::{
        HeightField, Instance, InstancedMesh, Mesh, MeshMaterial, MorphMesh, MorphSurface, Shading,
        Surface3D, TriMesh,
    };

    // Text, TeX, numbers, matrices, tables, labels, and label-aware animations.
    // Tuning constants (font sizes, buffs) and low-level helpers (match_glyphs,
    // MatchResult) are intentionally left out of the prelude to keep the
    // namespace scene-author-facing; reach them via `manim::text::...`.
    pub use manim_text::{
        AddTextLetterByLetter, Alignment, AxesLabels, BarChartLabels, BraceLabel, BulletedList,
        ChangeDecimalToValue, ChangingDecimal, CoordinateLabels, DecimalMatrix, DecimalNumber,
        DecimalTable, GraphLabel, Integer, IntegerMatrix, LabeledArrow, LabeledDot, LabeledLine,
        MarkupText, MathTable, MathTex, Matrix, MobjectMatrix, Paragraph, RemoveTextLetterByLetter,
        Slant, Table, Tex, Text, Title, TransformMatchingTex, Typst, Unwrite, Variable, Weighting,
        Write,
    };
}
