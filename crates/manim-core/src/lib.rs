//! Renderer-agnostic core of `manim_rust`: the scene graph, mobjects, and the
//! display-list contract to renderers.
//!
//! This crate is a headless, GPU-free port of manim CE's object model. Mobjects
//! live in an arena ([`SceneState`]); users hold cheap, `Copy`, typed handles
//! ([`MobjectId`]). Every mobject shares a [`MobjectData`] (geometry + style +
//! hierarchy) and implements the tiny [`Mobject`] trait, while the rich shared
//! behavior — transforms, positioning, sizing, styling — lives on the
//! blanket-implemented [`MobjectExt`] extension trait. The scene extracts a flat
//! [`DisplayList`] that a renderer consumes. See `docs/design/03-mobject-model.md`
//! and `docs/design/01-architecture.md`.
//!
//! # Quickstart
//!
//! ```
//! use manim_core::prelude::*;
//!
//! let mut scene = SceneState::new();
//! let circle = scene.add(Circle::new().with_fill(BLUE, 0.5));
//! let square = scene.add(Square::new().with_shift(2.0 * RIGHT));
//! // Group and move them together.
//! let group = VGroup::of(&mut scene, [circle.erase(), square.erase()]);
//! scene.shift(group.erase(), UP);
//!
//! let display = scene.display_list();
//! assert_eq!(display.len(), 2);
//! ```
//!
//! # Modules
//!
//! - [`style`] — fill/stroke [`Style`].
//! - [`mobject`] — [`MobjectData`], the [`Mobject`] trait, [`MobjectExt`], typed
//!   handles ([`MobjectId`] / [`AnyId`]), and [`BoundingBox`].
//! - [`scene_state`] — the [`SceneState`] arena and family-aware transforms.
//! - [`display`] — the [`DisplayList`] core→render contract.
//! - [`mesh`] — depth-tested triangle meshes ([`Mesh`], [`Surface3D`]).
//! - [`geometry`] — the concrete shape catalog (Circle, Square, Line, Arrow, …).
//! - [`config`] — scene [`Config`].

pub mod animated_boundary;
pub mod animation;
pub mod animations;
pub mod boolean;
pub mod camera;
pub mod config;
pub mod display;
pub mod error;
pub mod geometry;
pub mod graphing;
pub mod image_mobject;
pub mod mesh;
pub mod mobject;
pub mod network;
pub mod scene;
pub mod scene_state;
pub mod style;
pub mod svg;
pub mod threed;
pub mod timeline;
pub mod vector_field;
pub mod vector_space;

/// The full color library, re-exported so downstream crates reach the whole
/// palette without adding their own `manim-color` dependency.
///
/// The [`prelude`] carries only the handful of colors most scenes use; anything
/// else — the rest of the named palette, color spaces, gradients — lives here.
///
/// ```
/// use manim_core::manim_color::TEAL;
/// use manim_core::prelude::*;
/// let mut scene = SceneState::new();
/// let _ = scene.add(Circle::new().with_fill(TEAL, 1.0));
/// ```
pub use manim_color;

pub use animation::{AnimConfig, Animation, IntoAnimations};
pub use camera::{Camera2D, CameraFrame};
pub use config::Config;
pub use display::{DisplayList, DrawItem, Fill, MeshItem, Stroke};
pub use error::{CoreError, Result};
pub use mesh::{Mesh, MeshMaterial, Shading, Surface3D, TriMesh};
pub use mobject::{AnyId, BoundingBox, Buildable, Mobject, MobjectData, MobjectExt, MobjectId};
pub use scene::{Frame, Scene, SceneBuilder};
pub use scene_state::{SceneState, UpdaterCtx};
pub use style::Style;
pub use timeline::Section;

/// Curated re-exports for `use manim_core::prelude::*;`.
///
/// Pulls in the scene, the shared mobject API traits, the geometry catalog, and
/// the most-used math constants and colors.
///
/// ```
/// use manim_core::prelude::*;
/// let mut scene = SceneState::new();
/// let _ = scene.add(Circle::new());
/// ```
///
/// Only a handful of colors are re-exported here. For the rest of the palette,
/// reach through the [`manim_color`] re-export rather than
/// depending on that crate directly:
///
/// ```
/// use manim_core::manim_color::{MAROON, TEAL};
/// use manim_core::prelude::*;
/// let mut scene = SceneState::new();
/// let _ = scene.add(Square::new().with_fill(MAROON, 1.0));
/// let _ = scene.add(Circle::new().with_stroke(TEAL, 2.0, 1.0));
/// ```
pub mod prelude {
    pub use crate::animated_boundary::AnimatedBoundary;
    pub use crate::animation::{AnimConfig, Animation, IntoAnimations};
    pub use crate::animations::{Animate, TransformMatchingShapes};
    pub use crate::boolean::{Cutout, Difference, Exclusion, Intersection, Union};
    pub use crate::camera::{Camera2D, CameraFrame};
    pub use crate::config::Config;
    pub use crate::display::{DisplayList, DrawItem, Fill, Stroke};
    pub use crate::error::{CoreError, Result};
    pub use crate::geometry::*;
    pub use crate::graphing::{
        Axes, BarChart, ComplexPlane, CoordSystem, FunctionGraph, ImplicitFunction, NumberLine,
        NumberPlane, ParametricFunction, PolarPlane,
    };
    pub use crate::image_mobject::ImageMobject;
    pub use crate::mobject::{
        AnyId, BoundingBox, Buildable, Mobject, MobjectData, MobjectExt, MobjectId, RefTarget,
    };
    pub use crate::network::{DiGraph, Graph, GraphLayout};
    pub use crate::scene::{Frame, Scene, SceneBuilder};
    pub use crate::scene_state::{SceneState, UpdaterCtx};
    pub use crate::style::Style;
    pub use crate::svg::SVGMobject;
    pub use crate::threed::{
        Arrow3D, Cone, Cube, Cylinder, Dot3D, Line3D, Prism, Sphere, Surface, ThreeDAxes, Torus,
    };
    pub use crate::timeline::Section;
    pub use crate::vector_field::{ArrowVectorField, StreamLines, VectorField};
    pub use crate::vector_space::{add_axes, add_plane, add_vector, LinearTransformationScene};

    pub use manim_color::{Color, BLACK, BLUE, GREEN, ORANGE, PINK, PURPLE, RED, WHITE, YELLOW};
    pub use manim_math::rate_functions::RateFn;
    pub use manim_math::{
        Point, DEGREES, DL, DOWN, DR, IN, LARGE_BUFF, LEFT, MED_LARGE_BUFF, MED_SMALL_BUFF, ORIGIN,
        OUT, PI, RIGHT, SMALL_BUFF, TAU, UL, UP, UR,
    };
}
