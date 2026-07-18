//! `manim-sci`: scientific visualizers bridging [`manim_fields`] (fields, maps,
//! numerics — all `f64`) to [`manim_core`] mobjects (paths — all `f32`).
//!
//! - [`deform`] — [`ApplyMap`](deform::ApplyMap) / [`FlowMap`](deform::FlowMap)
//!   animations that deform a mobject by a [`SpaceMap`](manim_fields::map::SpaceMap)
//!   or vector-field flow, and [`DeformationGrid`](deform::DeformationGrid), an
//!   ambient grid whose lines subdivide adaptively where the map distorts most.
//! - [`complex_viz`] — a complex-analysis kit: conformal grid images, zero/pole
//!   markers, branch-cut indicators, and a [`RiemannSphere`](complex_viz::RiemannSphere).
//!
//! # The f64 ↔ f32 boundary
//!
//! Fields and maps compute in `f64`; mobject geometry is `f32`. Convert with
//! [`to_field`] (mobject → field space) and [`to_scene`] (field → mobject space)
//! at every crossing — they are the only place precision is dropped.
//!
//! ```
//! use manim_sci::{to_field, to_scene};
//! use manim_core::prelude::Point;
//! let p = Point::new(1.5, -2.0, 0.0);
//! // Round-trips within f32 precision.
//! assert!((to_scene(to_field(p)) - p).length() < 1e-6);
//! ```

pub mod complex_viz;
pub mod curveviz;
pub mod deform;
pub mod diffgeo;
pub mod geodesics;
pub mod isosurface;
pub mod material_quad;

use manim_core::prelude::Point;

/// Lifts a scene-space `f32` point into field space (`f64`).
///
/// ```
/// use manim_sci::to_field;
/// use manim_core::prelude::Point;
/// let d = to_field(Point::new(2.0, 3.0, 4.0));
/// assert_eq!((d.x, d.y, d.z), (2.0, 3.0, 4.0));
/// ```
#[inline]
pub fn to_field(p: Point) -> manim_fields::Point {
    p.as_dvec3()
}

/// Drops a field-space `f64` point back to scene-space `f32`.
///
/// ```
/// use manim_sci::to_scene;
/// use manim_fields::Point;
/// let p = to_scene(Point::new(2.0, 3.0, 4.0));
/// assert_eq!((p.x, p.y, p.z), (2.0, 3.0, 4.0));
/// ```
#[inline]
pub fn to_scene(p: manim_fields::Point) -> Point {
    p.as_vec3()
}
