//! Polygon-based mobjects: [`Polygon`], [`RegularPolygon`], [`Triangle`],
//! [`Rectangle`], [`Square`], [`RoundedRectangle`], [`Star`], and [`Polygram`].

use manim_color::WHITE;
use manim_math::bezier::CubicBezier;
use manim_math::path::{Path, SubPath};
use manim_math::space_ops::regular_vertices;
use manim_math::{Point, PI, TAU};

use super::{arc_subpath, polygon_path};
use crate::impl_mobject;
use crate::mobject::{MobjectData, MobjectExt};
use crate::style::Style;

/// A closed polygon through the given vertices. Port of manim CE's `Polygon`
/// (default stroke `WHITE`).
///
/// ```
/// use manim_core::geometry::Polygon;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::{Point, RIGHT, UP};
/// let tri = Polygon::new(&[Point::ZERO, 2.0 * RIGHT, 2.0 * UP]);
/// assert!((tri.bounding_box().width() - 2.0).abs() < 1e-4);
/// ```
#[derive(Clone)]
pub struct Polygon {
    data: MobjectData,
    vertices: Vec<Point>,
}
impl_mobject!(Polygon);

impl Polygon {
    /// A closed polygon through `vertices`.
    pub fn new(vertices: &[Point]) -> Self {
        Self {
            data: MobjectData::new(polygon_path(vertices), Style::stroked(WHITE)),
            vertices: vertices.to_vec(),
        }
    }

    /// The polygon's vertices.
    pub fn vertices(&self) -> &[Point] {
        &self.vertices
    }
}

/// A regular `n`-gon inscribed in the unit circle. Port of manim CE's
/// `RegularPolygon`.
///
/// ```
/// use manim_core::geometry::RegularPolygon;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// let hexagon = RegularPolygon::new(6);
/// assert_eq!(hexagon.vertices().len(), 6);
/// // Inscribed in the unit circle → width 2.
/// assert!((hexagon.bounding_box().width() - 2.0).abs() < 1e-4);
/// ```
#[derive(Clone)]
pub struct RegularPolygon {
    data: MobjectData,
    vertices: Vec<Point>,
    n: usize,
}
impl_mobject!(RegularPolygon);

impl RegularPolygon {
    /// A regular `n`-gon of circumradius `1.0`, centered at the origin.
    pub fn new(n: usize) -> Self {
        let (verts, _) = regular_vertices(n, 1.0, None);
        Self {
            data: MobjectData::new(polygon_path(&verts), Style::stroked(WHITE)),
            vertices: verts,
            n,
        }
    }

    /// The polygon's vertices.
    pub fn vertices(&self) -> &[Point] {
        &self.vertices
    }

    /// The number of sides.
    pub fn n(&self) -> usize {
        self.n
    }
}

/// An equilateral triangle (a [`RegularPolygon`] with three sides). Port of
/// manim CE's `Triangle`.
///
/// ```
/// use manim_core::geometry::Triangle;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// let t = Triangle::new();
/// assert_eq!(t.vertices().len(), 3);
/// ```
#[derive(Clone)]
pub struct Triangle {
    data: MobjectData,
    vertices: Vec<Point>,
}
impl_mobject!(Triangle);

impl Triangle {
    /// An equilateral triangle inscribed in the unit circle, pointing up.
    pub fn new() -> Self {
        let (verts, _) = regular_vertices(3, 1.0, None);
        Self {
            data: MobjectData::new(polygon_path(&verts), Style::stroked(WHITE)),
            vertices: verts,
        }
    }

    /// The triangle's vertices.
    pub fn vertices(&self) -> &[Point] {
        &self.vertices
    }
}

impl Default for Triangle {
    fn default() -> Self {
        Self::new()
    }
}

/// An axis-aligned rectangle. Port of manim CE's `Rectangle`
/// (default width `4`, height `2`, stroke `WHITE`).
///
/// ```
/// use manim_core::geometry::Rectangle;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// let r = Rectangle::new();
/// assert!((r.bounding_box().width() - 4.0).abs() < 1e-4);
/// assert!((r.bounding_box().height() - 2.0).abs() < 1e-4);
/// ```
#[derive(Clone)]
pub struct Rectangle {
    data: MobjectData,
    width: f32,
    height: f32,
}
impl_mobject!(Rectangle);

impl Rectangle {
    /// A rectangle of the given `width` and `height`, centered at the origin.
    pub fn with_size(width: f32, height: f32) -> Self {
        Self {
            data: MobjectData::new(rect_path(width, height), Style::stroked(WHITE)),
            width,
            height,
        }
    }

    /// The manim CE default rectangle: width `4`, height `2`.
    pub fn new() -> Self {
        Self::with_size(4.0, 2.0)
    }

    /// The rectangle width.
    pub fn width_value(&self) -> f32 {
        self.width
    }

    /// The rectangle height.
    pub fn height_value(&self) -> f32 {
        self.height
    }
}

impl Default for Rectangle {
    fn default() -> Self {
        Self::new()
    }
}

/// A square. Port of manim CE's `Square` (default side length `2`, stroke
/// `WHITE`).
///
/// ```
/// use manim_core::geometry::Square;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// let s = Square::new().side_length(3.0);
/// assert!((s.bounding_box().width() - 3.0).abs() < 1e-4);
/// assert!((s.bounding_box().height() - 3.0).abs() < 1e-4);
/// ```
#[derive(Clone)]
pub struct Square {
    data: MobjectData,
    side_length: f32,
}
impl_mobject!(Square);

impl Square {
    /// A square of side length `2.0`, centered at the origin.
    pub fn new() -> Self {
        Self::with_side(2.0)
    }

    /// A square of the given `side_length`, centered at the origin.
    pub fn with_side(side_length: f32) -> Self {
        Self {
            data: MobjectData::new(rect_path(side_length, side_length), Style::stroked(WHITE)),
            side_length,
        }
    }

    /// Sets the side length (construction-time builder), rebuilt about the
    /// current center.
    pub fn side_length(mut self, side_length: f32) -> Self {
        let center = self.get_center();
        self.side_length = side_length;
        self.data.path = rect_path(side_length, side_length);
        self.data.bump_generation();
        self.move_to(center);
        self
    }

    /// The current side length.
    pub fn side_length_value(&self) -> f32 {
        self.side_length
    }
}

impl Default for Square {
    fn default() -> Self {
        Self::new()
    }
}

/// A rectangle with rounded corners. Port of manim CE's `RoundedRectangle`
/// (default corner radius `0.5`).
///
/// ```
/// use manim_core::geometry::RoundedRectangle;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// let r = RoundedRectangle::new();
/// // Outer bounding box still matches the base rectangle (width 4, height 2).
/// assert!((r.bounding_box().width() - 4.0).abs() < 1e-3);
/// assert!((r.bounding_box().height() - 2.0).abs() < 1e-3);
/// ```
#[derive(Clone)]
pub struct RoundedRectangle {
    data: MobjectData,
    width: f32,
    height: f32,
    corner_radius: f32,
}
impl_mobject!(RoundedRectangle);

impl RoundedRectangle {
    /// A rounded rectangle of the given `width`, `height`, and `corner_radius`
    /// (clamped to at most half the shorter side), centered at the origin.
    pub fn with_params(width: f32, height: f32, corner_radius: f32) -> Self {
        let r = corner_radius.min(width / 2.0).min(height / 2.0).max(0.0);
        Self {
            data: MobjectData::new(rounded_rect_path(width, height, r), Style::stroked(WHITE)),
            width,
            height,
            corner_radius: r,
        }
    }

    /// The manim CE default: width `4`, height `2`, corner radius `0.5`.
    pub fn new() -> Self {
        Self::with_params(4.0, 2.0, 0.5)
    }

    /// The corner radius.
    pub fn corner_radius(&self) -> f32 {
        self.corner_radius
    }

    /// The rectangle width.
    pub fn width_value(&self) -> f32 {
        self.width
    }

    /// The rectangle height.
    pub fn height_value(&self) -> f32 {
        self.height
    }
}

impl Default for RoundedRectangle {
    fn default() -> Self {
        Self::new()
    }
}

/// A star polygon. Port of manim CE's `Star`
/// (default `n = 5`, outer radius `1`, density `2`).
///
/// ```
/// use manim_core::geometry::Star;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// let s = Star::new(5);
/// // A star has 2n vertices (outer + inner).
/// assert_eq!(s.data().path.subpaths[0].n_curves(), 10);
/// // The pentagram inner/outer ratio is ≈ 0.382.
/// assert!((s.inner_radius() - 0.382).abs() < 1e-2);
/// ```
#[derive(Clone)]
pub struct Star {
    data: MobjectData,
    n: usize,
    inner_radius: f32,
    outer_radius: f32,
}
impl_mobject!(Star);

impl Star {
    /// An `n`-pointed star with density `2`, outer radius `1`, pointing up.
    pub fn new(n: usize) -> Self {
        let outer_radius = 1.0;
        let density = 2usize;
        let inner_radius = outer_radius * (PI * density as f32 / n as f32).cos()
            / (PI * (density as f32 - 1.0) / n as f32).cos();
        Self::with_params(n, inner_radius, outer_radius, TAU / 4.0)
    }

    /// A star with explicit `inner_radius`, `outer_radius`, and `start_angle`.
    pub fn with_params(n: usize, inner_radius: f32, outer_radius: f32, start_angle: f32) -> Self {
        let step = TAU / n as f32;
        let mut verts = Vec::with_capacity(2 * n);
        for k in 0..n {
            let a = start_angle + step * k as f32;
            verts.push(Point::new(a.cos(), a.sin(), 0.0) * outer_radius);
            let b = a + step / 2.0;
            verts.push(Point::new(b.cos(), b.sin(), 0.0) * inner_radius);
        }
        Self {
            data: MobjectData::new(polygon_path(&verts), Style::stroked(WHITE)),
            n,
            inner_radius,
            outer_radius,
        }
    }

    /// The number of points.
    pub fn n(&self) -> usize {
        self.n
    }

    /// The inner radius.
    pub fn inner_radius(&self) -> f32 {
        self.inner_radius
    }

    /// The outer radius.
    pub fn outer_radius(&self) -> f32 {
        self.outer_radius
    }
}

/// A multi-outline polygon. Port of manim CE's `Polygram` — one closed subpath
/// per vertex loop.
///
/// ```
/// use manim_core::geometry::Polygram;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::{Point, RIGHT, UP};
/// let two = Polygram::new(&[
///     &[Point::ZERO, RIGHT, UP],
///     &[2.0 * RIGHT, 3.0 * RIGHT, 2.0 * RIGHT + UP],
/// ]);
/// assert_eq!(two.data().path.subpaths.len(), 2);
/// ```
#[derive(Clone)]
pub struct Polygram {
    data: MobjectData,
}
impl_mobject!(Polygram);

impl Polygram {
    /// A polygram with one closed subpath per vertex loop.
    pub fn new(loops: &[&[Point]]) -> Self {
        let subpaths = loops
            .iter()
            .map(|verts| super::polygon_subpath(verts))
            .collect();
        Self {
            data: MobjectData::new(Path { subpaths }, Style::stroked(WHITE)),
        }
    }
}

/// A closed axis-aligned rectangle path of the given size, centered at the
/// origin.
fn rect_path(width: f32, height: f32) -> Path {
    let hw = width / 2.0;
    let hh = height / 2.0;
    polygon_path(&[
        Point::new(hw, hh, 0.0),
        Point::new(-hw, hh, 0.0),
        Point::new(-hw, -hh, 0.0),
        Point::new(hw, -hh, 0.0),
    ])
}

/// A closed rounded-rectangle path of the given size and corner radius.
fn rounded_rect_path(width: f32, height: f32, r: f32) -> Path {
    let hw = width / 2.0;
    let hh = height / 2.0;
    // Corner arc centers, per quadrant, and the quarter-turn start angle.
    let corners = [
        (Point::new(hw - r, hh - r, 0.0), 0.0),       // top-right
        (Point::new(-hw + r, hh - r, 0.0), PI / 2.0), // top-left
        (Point::new(-hw + r, -hh + r, 0.0), PI),      // bottom-left
        (Point::new(hw - r, -hh + r, 0.0), 3.0 * PI / 2.0), // bottom-right
    ];
    let mut curves: Vec<CubicBezier> = Vec::new();
    let mut prev_end: Option<Point> = None;
    for (center, start_angle) in corners {
        let arc = arc_subpath(center, r, start_angle, PI / 2.0, false);
        if let (Some(prev), Some(first)) = (prev_end, arc.curves.first()) {
            curves.push(CubicBezier::line(prev, first.p0));
        }
        prev_end = arc.curves.last().map(|c| c.p3);
        curves.extend(arc.curves);
    }
    Path {
        subpaths: vec![SubPath {
            curves,
            closed: true,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::Buildable;

    #[test]
    fn square_default_side_two() {
        let s = Square::new();
        assert!((s.bounding_box().width() - 2.0).abs() < 1e-4);
    }

    #[test]
    fn rectangle_defaults() {
        let r = Rectangle::new();
        assert!((r.bounding_box().width() - 4.0).abs() < 1e-4);
        assert!((r.bounding_box().height() - 2.0).abs() < 1e-4);
    }

    #[test]
    fn regular_polygon_inscribed_unit() {
        let p = RegularPolygon::new(5);
        for v in p.vertices() {
            assert!((v.length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn star_pentagram_ratio() {
        let s = Star::new(5);
        assert!((s.inner_radius() - 0.382).abs() < 1e-2);
    }

    #[test]
    fn rounded_rectangle_bbox_matches_base() {
        let r = RoundedRectangle::new();
        assert!((r.bounding_box().width() - 4.0).abs() < 1e-3);
        assert!((r.bounding_box().height() - 2.0).abs() < 1e-3);
    }

    #[test]
    fn square_center_preserved_by_builder() {
        use manim_math::RIGHT;
        let s = Square::new().with_shift(2.0 * RIGHT).side_length(4.0);
        assert!((s.get_center() - 2.0 * RIGHT).length() < 1e-5);
    }
}
