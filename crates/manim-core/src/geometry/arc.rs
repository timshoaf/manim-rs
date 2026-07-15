//! Arc-based mobjects: [`Arc`], [`ArcBetweenPoints`], [`Circle`], [`Dot`],
//! [`Ellipse`], [`Annulus`], [`Sector`], and [`AnnularSector`].

use manim_color::{RED, WHITE};
use manim_math::bezier::CubicBezier;
use manim_math::path::{Path, SubPath};
use manim_math::space_ops::perpendicular_bisector;
use manim_math::{Point, ORIGIN, TAU};

use super::{arc_path, point_on_circle, put_start_and_end_on};
use crate::impl_mobject;
use crate::mobject::{BoundingBox, MobjectData, MobjectExt};
use crate::style::Style;

/// A circular arc of a given `radius`, `start_angle`, and sweep `angle`,
/// centered at the origin. Port of manim CE's `Arc`.
///
/// ```
/// use manim_core::geometry::Arc;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::{TAU, Point};
/// // A quarter arc of unit radius from angle 0 sweeps from (1,0) to (0,1).
/// let a = Arc::new(1.0, 0.0, TAU / 4.0);
/// assert!((a.point_at_angle(0.0) - Point::new(1.0, 0.0, 0.0)).length() < 1e-4);
/// assert!((a.point_at_angle(TAU / 4.0) - Point::new(0.0, 1.0, 0.0)).length() < 1e-4);
/// ```
#[derive(Clone)]
pub struct Arc {
    data: MobjectData,
    radius: f32,
    start_angle: f32,
    angle: f32,
    arc_center: Point,
}
impl_mobject!(Arc);

impl Arc {
    /// Builds an arc of `radius` starting at `start_angle`, sweeping `angle`
    /// radians (positive = counter-clockwise), centered at the origin.
    pub fn new(radius: f32, start_angle: f32, angle: f32) -> Self {
        let path = arc_path(ORIGIN, radius, start_angle, angle, false);
        Self {
            data: MobjectData::new(path, Style::stroked(WHITE)),
            radius,
            start_angle,
            angle,
            arc_center: ORIGIN,
        }
    }

    /// The arc's radius.
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// The arc's center.
    pub fn arc_center(&self) -> Point {
        self.arc_center
    }

    /// The arc's starting angle in radians.
    pub fn start_angle(&self) -> f32 {
        self.start_angle
    }

    /// The arc's swept angle in radians.
    pub fn angle(&self) -> f32 {
        self.angle
    }

    /// The point at absolute `angle` radians on the arc's circle.
    pub fn point_at_angle(&self, angle: f32) -> Point {
        point_on_circle(self.arc_center, self.radius, angle)
    }
}

/// A circular arc between two points subtending a given central `angle`. Port of
/// manim CE's `ArcBetweenPoints`.
///
/// ```
/// use manim_core::geometry::ArcBetweenPoints;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::{Point, TAU};
/// let start = Point::new(-1.0, 0.0, 0.0);
/// let end = Point::new(1.0, 0.0, 0.0);
/// let arc = ArcBetweenPoints::new(start, end, TAU / 4.0);
/// // The arc still starts and ends on the given points.
/// assert!((arc.get_start() - start).length() < 1e-3);
/// assert!((arc.get_end() - end).length() < 1e-3);
/// ```
#[derive(Clone)]
pub struct ArcBetweenPoints {
    data: MobjectData,
    start: Point,
    end: Point,
    angle: f32,
}
impl_mobject!(ArcBetweenPoints);

impl ArcBetweenPoints {
    /// Builds the arc from `start` to `end` subtending `angle` radians. An angle
    /// of zero yields a straight line.
    pub fn new(start: Point, end: Point, angle: f32) -> Self {
        let path = arc_between(start, end, angle);
        Self {
            data: MobjectData::new(path, Style::stroked(WHITE)),
            start,
            end,
            angle,
        }
    }

    /// The arc's start point.
    pub fn get_start(&self) -> Point {
        self.start
    }

    /// The arc's end point.
    pub fn get_end(&self) -> Point {
        self.end
    }

    /// The subtended central angle.
    pub fn angle(&self) -> f32 {
        self.angle
    }
}

/// Builds the arc path from `start` to `end` for a central `angle`.
fn arc_between(start: Point, end: Point, angle: f32) -> Path {
    if angle.abs() < 1e-6 {
        return Path::from_corners(&[start, end], false);
    }
    let mut p = arc_path(ORIGIN, 1.0, 0.0, angle, false);
    put_start_and_end_on(&mut p, start, end);
    p
}

/// A circle. Port of manim CE's `Circle` (default radius `1.0`, stroke `RED`).
///
/// ```
/// use manim_core::geometry::Circle;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// // A radius-r circle has bounding-box width 2r.
/// let c = Circle::new().radius(2.0);
/// assert!((c.bounding_box().width() - 4.0).abs() < 1e-4);
/// assert_eq!(c.data().style.stroke_color, Some(manim_color::RED));
/// ```
#[derive(Clone)]
pub struct Circle {
    data: MobjectData,
    radius: f32,
}
impl_mobject!(Circle);

impl Circle {
    /// A unit circle (radius `1.0`) centered at the origin.
    pub fn new() -> Self {
        let path = arc_path(ORIGIN, 1.0, 0.0, TAU, true);
        Self {
            data: MobjectData::new(path, Style::stroked(RED)),
            radius: 1.0,
        }
    }

    /// Sets the radius (construction-time builder), rebuilt about the current
    /// center.
    pub fn radius(mut self, radius: f32) -> Self {
        let center = self.get_center();
        self.radius = radius;
        self.data.path = arc_path(center, radius, 0.0, TAU, true);
        self.data.bump_generation();
        self
    }

    /// A circle through three points (its circumscribed circle). Falls back to a
    /// unit circle at the centroid if the points are collinear.
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
    /// use manim_math::{Point, RIGHT, UP, LEFT};
    /// let c = Circle::from_three_points(RIGHT, UP, LEFT);
    /// // These three lie on the unit circle about the origin.
    /// assert!(c.get_center().length() < 1e-4);
    /// assert!((c.radius_value() - 1.0).abs() < 1e-4);
    /// ```
    pub fn from_three_points(a: Point, b: Point, c: Point) -> Self {
        let bis1 = perpendicular_bisector((a, b));
        let bis2 = perpendicular_bisector((b, c));
        let center = manim_math::space_ops::line_intersection(bis1, bis2)
            .unwrap_or_else(|| (a + b + c) / 3.0);
        let radius = (a - center).length();
        Self::new().radius(radius).with_center(center)
    }

    /// The current radius.
    pub fn radius_value(&self) -> f32 {
        self.radius
    }

    /// Resizes and repositions the circle to enclose `bbox`, scaled by `buffer`.
    /// Port of manim CE's `Circle.surround`.
    ///
    /// ```
    /// use manim_core::geometry::{Circle, Square};
    /// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
    /// let sq = Square::new(); // 2 × 2, diagonal ≈ 2.83
    /// let mut c = Circle::new();
    /// c.surround(sq.bounding_box(), 1.0);
    /// // Radius covers half the box diagonal.
    /// assert!((c.radius_value() - 2.0_f32.sqrt()).abs() < 1e-4);
    /// ```
    pub fn surround(&mut self, bbox: BoundingBox, buffer: f32) -> &mut Self {
        let radius = 0.5 * bbox.width().hypot(bbox.height()) * buffer;
        self.radius = radius;
        self.data.path = arc_path(bbox.center(), radius, 0.0, TAU, true);
        self.data.bump_generation();
        self
    }

    /// Consuming helper used by builders to recenter the circle at `center`.
    fn with_center(mut self, center: Point) -> Self {
        self.move_to(center);
        self
    }
}

impl Default for Circle {
    fn default() -> Self {
        Self::new()
    }
}

/// A small filled dot. Port of manim CE's `Dot` (radius `0.08`, filled `WHITE`).
///
/// ```
/// use manim_core::geometry::Dot;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// let d = Dot::new();
/// assert!((d.bounding_box().width() - 0.16).abs() < 1e-4);
/// assert_eq!(d.data().style.fill_opacity, 1.0);
/// ```
#[derive(Clone)]
pub struct Dot {
    data: MobjectData,
    radius: f32,
}
impl_mobject!(Dot);

/// manim CE's default `Dot` radius.
pub const DEFAULT_DOT_RADIUS: f32 = 0.08;

impl Dot {
    /// A dot at the origin.
    pub fn new() -> Self {
        Self::at(ORIGIN)
    }

    /// A dot centered at `point`.
    pub fn at(point: Point) -> Self {
        let path = arc_path(point, DEFAULT_DOT_RADIUS, 0.0, TAU, true);
        Self {
            data: MobjectData::new(path, Style::filled(WHITE)),
            radius: DEFAULT_DOT_RADIUS,
        }
    }

    /// Sets the dot radius (construction-time builder).
    pub fn radius(mut self, radius: f32) -> Self {
        let center = self.get_center();
        self.radius = radius;
        self.data.path = arc_path(center, radius, 0.0, TAU, true);
        self.data.bump_generation();
        self
    }
}

impl Default for Dot {
    fn default() -> Self {
        Self::new()
    }
}

/// An axis-aligned ellipse. Port of manim CE's `Ellipse` (width `2`, height `1`).
///
/// ```
/// use manim_core::geometry::Ellipse;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// let e = Ellipse::new();
/// assert!((e.bounding_box().width() - 2.0).abs() < 1e-4);
/// assert!((e.bounding_box().height() - 1.0).abs() < 1e-4);
/// ```
#[derive(Clone)]
pub struct Ellipse {
    data: MobjectData,
    width: f32,
    height: f32,
}
impl_mobject!(Ellipse);

impl Ellipse {
    /// An ellipse of the given `width` and `height`, centered at the origin.
    pub fn with_size(width: f32, height: f32) -> Self {
        let mut path = arc_path(ORIGIN, 1.0, 0.0, TAU, true);
        path.apply(|p| Point::new(p.x * width / 2.0, p.y * height / 2.0, p.z));
        Self {
            data: MobjectData::new(path, Style::stroked(WHITE)),
            width,
            height,
        }
    }

    /// The manim CE default ellipse: width `2`, height `1`.
    pub fn new() -> Self {
        Self::with_size(2.0, 1.0)
    }

    /// The ellipse width.
    pub fn width_value(&self) -> f32 {
        self.width
    }

    /// The ellipse height.
    pub fn height_value(&self) -> f32 {
        self.height
    }
}

impl Default for Ellipse {
    fn default() -> Self {
        Self::new()
    }
}

/// A filled ring between two radii. Port of manim CE's `Annulus`
/// (inner `1`, outer `2`, filled `WHITE`).
///
/// ```
/// use manim_core::geometry::Annulus;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// let a = Annulus::new();
/// // Outer radius 2 → width 4.
/// assert!((a.bounding_box().width() - 4.0).abs() < 1e-4);
/// // Two subpaths: outer ring and inner hole.
/// assert_eq!(a.data().path.subpaths.len(), 2);
/// ```
#[derive(Clone)]
pub struct Annulus {
    data: MobjectData,
    inner_radius: f32,
    outer_radius: f32,
}
impl_mobject!(Annulus);

impl Annulus {
    /// An annulus with the given `inner_radius` and `outer_radius`, centered at
    /// the origin.
    pub fn with_radii(inner_radius: f32, outer_radius: f32) -> Self {
        let outer = arc_path(ORIGIN, outer_radius, 0.0, TAU, true).subpaths;
        let mut inner = arc_path(ORIGIN, inner_radius, 0.0, TAU, true);
        inner.reverse(); // opposite winding punches the hole
        let subpaths = outer.into_iter().chain(inner.subpaths).collect();
        Self {
            data: MobjectData::new(Path { subpaths }, Style::filled(WHITE)),
            inner_radius,
            outer_radius,
        }
    }

    /// The manim CE default annulus: inner `1`, outer `2`.
    pub fn new() -> Self {
        Self::with_radii(1.0, 2.0)
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

impl Default for Annulus {
    fn default() -> Self {
        Self::new()
    }
}

/// A filled annular sector (a wedge of an [`Annulus`]). Port of manim CE's
/// `AnnularSector`.
///
/// ```
/// use manim_core::geometry::AnnularSector;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::TAU;
/// let s = AnnularSector::new(0.5, 1.0, 0.0, TAU / 4.0);
/// assert_eq!(s.data().style.fill_opacity, 1.0);
/// ```
#[derive(Clone)]
pub struct AnnularSector {
    data: MobjectData,
    inner_radius: f32,
    outer_radius: f32,
    start_angle: f32,
    angle: f32,
}
impl_mobject!(AnnularSector);

impl AnnularSector {
    /// An annular sector with the given radii, `start_angle`, and sweep `angle`,
    /// centered at the origin.
    pub fn new(inner_radius: f32, outer_radius: f32, start_angle: f32, angle: f32) -> Self {
        let path = annular_sector_path(ORIGIN, inner_radius, outer_radius, start_angle, angle);
        Self {
            data: MobjectData::new(path, Style::filled(WHITE)),
            inner_radius,
            outer_radius,
            start_angle,
            angle,
        }
    }

    /// The inner radius.
    pub fn inner_radius(&self) -> f32 {
        self.inner_radius
    }

    /// The outer radius.
    pub fn outer_radius(&self) -> f32 {
        self.outer_radius
    }

    /// The starting angle in radians.
    pub fn start_angle(&self) -> f32 {
        self.start_angle
    }

    /// The swept angle in radians.
    pub fn angle(&self) -> f32 {
        self.angle
    }
}

/// A filled circular sector (a pie slice). Port of manim CE's `Sector`
/// (inner radius `0`, outer radius `1`, quarter turn by default).
///
/// ```
/// use manim_core::geometry::Sector;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// let s = Sector::new();
/// assert_eq!(s.data().style.fill_opacity, 1.0);
/// ```
#[derive(Clone)]
pub struct Sector {
    data: MobjectData,
    outer_radius: f32,
    start_angle: f32,
    angle: f32,
}
impl_mobject!(Sector);

impl Sector {
    /// A sector of `radius`, `start_angle`, and sweep `angle`, centered at the
    /// origin.
    pub fn with_params(radius: f32, start_angle: f32, angle: f32) -> Self {
        let path = annular_sector_path(ORIGIN, 0.0, radius, start_angle, angle);
        Self {
            data: MobjectData::new(path, Style::filled(WHITE)),
            outer_radius: radius,
            start_angle,
            angle,
        }
    }

    /// The manim CE default sector: radius `1`, quarter turn from angle `0`.
    pub fn new() -> Self {
        Self::with_params(1.0, 0.0, TAU / 4.0)
    }

    /// The outer radius.
    pub fn outer_radius(&self) -> f32 {
        self.outer_radius
    }

    /// The starting angle in radians.
    pub fn start_angle(&self) -> f32 {
        self.start_angle
    }

    /// The swept angle in radians.
    pub fn angle(&self) -> f32 {
        self.angle
    }
}

impl Default for Sector {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds the closed path of an annular sector.
fn annular_sector_path(
    center: Point,
    inner_radius: f32,
    outer_radius: f32,
    start_angle: f32,
    angle: f32,
) -> Path {
    let inner = super::arc_subpath(center, inner_radius, start_angle, angle, false);
    let mut outer = super::arc_subpath(center, outer_radius, start_angle, angle, false);
    outer.reverse();
    let inner_end = point_on_circle(center, inner_radius, start_angle + angle);
    let outer_start = point_on_circle(center, outer_radius, start_angle + angle);
    let mut curves = inner.curves;
    curves.push(CubicBezier::line(inner_end, outer_start));
    curves.extend(outer.curves);
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
    use crate::mobject::Mobject;
    use manim_math::{RIGHT, UP};

    #[test]
    fn circle_width_is_two_radius() {
        let c = Circle::new().radius(3.0);
        assert!((c.bounding_box().width() - 6.0).abs() < 1e-4);
    }

    #[test]
    fn circle_default_color_is_red() {
        assert_eq!(Circle::new().data().style.stroke_color, Some(RED));
    }

    #[test]
    fn from_three_points_unit_circle() {
        let c = Circle::from_three_points(RIGHT, UP, -RIGHT);
        assert!(c.get_center().length() < 1e-4);
        assert!((c.radius_value() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn arc_between_hits_endpoints() {
        let start = Point::new(-1.0, 0.5, 0.0);
        let end = Point::new(2.0, -1.0, 0.0);
        let arc = ArcBetweenPoints::new(start, end, TAU / 3.0);
        assert!((arc.get_start() - start).length() < 1e-3);
        assert!((arc.get_end() - end).length() < 1e-3);
    }

    #[test]
    fn annulus_has_two_subpaths() {
        assert_eq!(Annulus::new().data().path.subpaths.len(), 2);
    }
}
