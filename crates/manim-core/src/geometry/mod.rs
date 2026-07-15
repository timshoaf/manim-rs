//! Concrete mobjects: arcs, polygons, lines, arrows, and groups.
//!
//! Every constructor returns a plain struct that embeds a
//! [`MobjectData`](crate::mobject::MobjectData) plus its own semantic parameters
//! (so `Circle` remembers its radius, `Line` its endpoints), matching how manim
//! CE's subclasses carry semantics over raw points. Colors and defaults mirror
//! manim CE.
//!
//! | manim CE | here |
//! | --- | --- |
//! | `Arc`, `ArcBetweenPoints` | [`Arc`], [`ArcBetweenPoints`] |
//! | `Circle`, `Dot`, `Ellipse` | [`Circle`], [`Dot`], [`Ellipse`] |
//! | `Annulus`, `Sector`, `AnnularSector` | [`Annulus`], [`Sector`], [`AnnularSector`] |
//! | `Polygon`, `RegularPolygon`, `Triangle` | [`Polygon`], [`RegularPolygon`], [`Triangle`] |
//! | `Rectangle`, `Square`, `RoundedRectangle` | [`Rectangle`], [`Square`], [`RoundedRectangle`] |
//! | `Star`, `Polygram` | [`Star`], [`Polygram`] |
//! | `Line`, `DashedLine`, `Arrow`, `Vector` | [`Line`], [`DashedLine`], [`Arrow`], [`Vector`] |
//! | `DoubleArrow`, `Elbow`, `Angle`, `RightAngle` | [`DoubleArrow`], [`Elbow`], [`Angle`], [`RightAngle`] |
//! | `VGroup` | [`VGroup`] |

mod arc;
mod brace;
mod group;
mod line;
mod polygram;
mod vectorized;

pub use arc::{AnnularSector, Annulus, Arc, ArcBetweenPoints, Circle, Dot, Ellipse, Sector};
pub use brace::Brace;
pub use group::VGroup;
pub use line::{
    Angle, Arrow, DashedLine, DoubleArrow, Elbow, Line, RightAngle, TangentLine, Vector,
};
pub use polygram::{
    Polygon, Polygram, Rectangle, RegularPolygon, RegularPolygram, RoundedRectangle, Square, Star,
    Triangle,
};
pub use vectorized::{
    CurvesAsSubmobjects, DashedVMobject, TracedPath, VDict, VMobject, VectorizedPoint,
    DEFAULT_DASHED_RATIO, DEFAULT_NUM_DASHES,
};

use manim_math::bezier::CubicBezier;
use manim_math::path::{Path, SubPath};
use manim_math::{Point, PI};

/// A single cubic Bézier approximating the circular arc from `a0` to `a1`
/// (radians) on a circle of `radius` about `center`.
///
/// Uses the standard kappa handle length `k = 4/3 · tan(θ/4)`, exact at the
/// endpoints and near-exact between for spans up to a quarter turn.
fn arc_segment(center: Point, radius: f32, a0: f32, a1: f32) -> CubicBezier {
    let theta = a1 - a0;
    let k = 4.0 / 3.0 * (theta / 4.0).tan();
    let on = |ang: f32| center + Point::new(ang.cos(), ang.sin(), 0.0) * radius;
    let tangent = |ang: f32| Point::new(-ang.sin(), ang.cos(), 0.0) * (radius * k);
    let p0 = on(a0);
    let p3 = on(a1);
    CubicBezier::new(p0, p0 + tangent(a0), p3 - tangent(a1), p3)
}

/// The subpath of a circular arc, split into ≤ quarter-turn cubic segments.
fn arc_subpath(center: Point, radius: f32, start_angle: f32, angle: f32, closed: bool) -> SubPath {
    let n = (angle.abs() / (PI / 2.0)).ceil().max(1.0) as usize;
    let seg = angle / n as f32;
    let curves = (0..n)
        .map(|i| {
            let a0 = start_angle + seg * i as f32;
            arc_segment(center, radius, a0, a0 + seg)
        })
        .collect();
    SubPath { curves, closed }
}

/// A single-subpath [`Path`] tracing a circular arc.
///
/// ```
/// use manim_core::geometry::arc_path;
/// use manim_math::{Point, TAU};
/// // A full unit circle about the origin.
/// let p = arc_path(Point::ZERO, 1.0, 0.0, TAU, true);
/// let (min, max) = p.bounding_box().unwrap();
/// assert!((max.x - 1.0).abs() < 1e-4 && (min.x + 1.0).abs() < 1e-4);
/// ```
pub fn arc_path(center: Point, radius: f32, start_angle: f32, angle: f32, closed: bool) -> Path {
    Path {
        subpaths: vec![arc_subpath(center, radius, start_angle, angle, closed)],
    }
}

/// The point at `angle` radians on a circle of `radius` about `center`.
fn point_on_circle(center: Point, radius: f32, angle: f32) -> Point {
    center + Point::new(angle.cos(), angle.sin(), 0.0) * radius
}

/// A closed polygon subpath through `vertices` (straight edges).
fn polygon_subpath(vertices: &[Point]) -> SubPath {
    if vertices.len() < 2 {
        return SubPath {
            curves: Vec::new(),
            closed: true,
        };
    }
    let mut corners = vertices.to_vec();
    corners.push(vertices[0]);
    let mut sp = SubPath::from_corners(&corners);
    sp.closed = true;
    sp
}

/// A closed single-subpath polygon [`Path`] through `vertices`.
fn polygon_path(vertices: &[Point]) -> Path {
    Path {
        subpaths: vec![polygon_subpath(vertices)],
    }
}

/// Re-maps a path so its current endpoints move onto `start` and `end`, via a
/// rotate + uniform-scale + translate (manim's `put_start_and_end_on`).
///
/// The "start" is the first anchor of the first subpath and the "end" is the
/// last anchor of the last subpath. A degenerate (zero-length) current span is
/// left untranslated except for the shift onto `start`.
fn put_start_and_end_on(path: &mut Path, start: Point, end: Point) {
    let (Some(cur_start), Some(cur_end)) = (first_anchor(path), last_anchor(path)) else {
        return;
    };
    let cur = cur_end - cur_start;
    let target = end - start;
    let cur_len = cur.length();
    if cur_len < 1e-9 {
        path.apply(|p| p + (start - cur_start));
        return;
    }
    let scale = target.length() / cur_len;
    let angle = manim_math::space_ops::angle_of_vector(target)
        - manim_math::space_ops::angle_of_vector(cur);
    let rot = manim_math::space_ops::rotation_matrix(angle, manim_math::OUT);
    path.apply(|p| start + rot * ((p - cur_start) * scale));
}

/// The first on-curve anchor of a path, if any.
fn first_anchor(path: &Path) -> Option<Point> {
    path.subpaths
        .iter()
        .find_map(|s| s.curves.first())
        .map(|c| c.p0)
}

/// The last on-curve anchor of a path, if any.
fn last_anchor(path: &Path) -> Option<Point> {
    path.subpaths
        .iter()
        .rev()
        .find_map(|s| s.curves.last())
        .map(|c| c.p3)
}
