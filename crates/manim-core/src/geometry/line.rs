//! Line-based mobjects: [`Line`], [`DashedLine`], [`Arrow`], [`Vector`],
//! [`DoubleArrow`], [`Elbow`], [`Angle`], and [`RightAngle`].

use manim_color::WHITE;
use manim_math::path::{Path, SubPath};
use manim_math::space_ops::{angle_of_vector, line_intersection, normalize_or_zero, rotate_vector};
use manim_math::{Point, ORIGIN, PI, TAU};

use super::polygon_subpath;
use crate::impl_mobject;
use crate::mobject::{Mobject, MobjectData};
use crate::style::Style;

/// manim CE's default arrow tip length.
pub const DEFAULT_ARROW_TIP_LENGTH: f32 = 0.35;

/// The head shape of an arrow tip. Port of manim CE's `ArrowTip` family.
///
/// `filled` (on the arrow builder) fills pointed tips ([`Triangle`](Self::Triangle)
/// / [`Stealth`](Self::Stealth)) as solid heads or draws them as an open outline;
/// [`Circle`](Self::Circle) / [`Square`](Self::Square) heads are always solid
/// (an unfilled round/square head would need per-subpath styling we don't model).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TipShape {
    /// A solid/open triangular head (manim's default `ArrowTriangleFilledTip`).
    #[default]
    Triangle,
    /// A concave dart head (manim's `StealthTip`).
    Stealth,
    /// A round head (`ArrowCircleFilledTip`).
    Circle,
    /// A square head (`ArrowSquareFilledTip`).
    Square,
}

/// Builds the tip head [`SubPath`] with apex at `apex`, pointing along unit
/// `dir`, of length `tl`.
pub(crate) fn tip_subpath(
    apex: Point,
    dir: Point,
    tl: f32,
    shape: TipShape,
    filled: bool,
) -> SubPath {
    let perp = rotate_vector(dir, PI / 2.0);
    let hw = tl * 0.5;
    let base = apex - dir * tl;
    let w1 = base + perp * hw;
    let w2 = base - perp * hw;
    match shape {
        TipShape::Triangle => {
            if filled {
                polygon_subpath(&[apex, w1, w2])
            } else {
                SubPath::from_corners(&[w1, apex, w2])
            }
        }
        TipShape::Stealth => {
            let notch = apex - dir * (tl * 0.55);
            if filled {
                polygon_subpath(&[apex, w1, notch, w2])
            } else {
                SubPath::from_corners(&[w1, apex, notch, w2])
            }
        }
        TipShape::Circle => circle_subpath(apex - dir * hw, hw),
        TipShape::Square => {
            let c = apex - dir * hw;
            polygon_subpath(&[
                c + dir * hw + perp * hw,
                c + dir * hw - perp * hw,
                c - dir * hw - perp * hw,
                c - dir * hw + perp * hw,
            ])
        }
    }
}

/// A closed regular polygon approximating a circle of `radius` at `center`.
fn circle_subpath(center: Point, radius: f32) -> SubPath {
    let n = 16;
    let pts: Vec<Point> = (0..n)
        .map(|k| {
            let a = TAU * k as f32 / n as f32;
            center + Point::new(a.cos(), a.sin(), 0.0) * radius
        })
        .collect();
    polygon_subpath(&pts)
}

/// A straight line segment. Port of manim CE's `Line`.
///
/// ```
/// use manim_core::geometry::Line;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::{Point, RIGHT};
/// let l = Line::new(Point::ZERO, 3.0 * RIGHT);
/// assert!((l.get_length() - 3.0).abs() < 1e-6);
/// assert!((l.get_angle()).abs() < 1e-6); // horizontal
/// ```
#[derive(Clone)]
pub struct Line {
    data: MobjectData,
    start: Point,
    end: Point,
}
impl_mobject!(Line);

impl Line {
    /// A line from `start` to `end`.
    pub fn new(start: Point, end: Point) -> Self {
        Self {
            data: MobjectData::new(
                Path::from_corners(&[start, end], false),
                Style::stroked(WHITE),
            ),
            start,
            end,
        }
    }

    /// The start point.
    pub fn get_start(&self) -> Point {
        self.start
    }

    /// The end point.
    pub fn get_end(&self) -> Point {
        self.end
    }

    /// The direction angle (radians) of `end - start` (manim's `get_angle`).
    pub fn get_angle(&self) -> f32 {
        angle_of_vector(self.end - self.start)
    }

    /// The length of the segment (manim's `get_length`).
    pub fn get_length(&self) -> f32 {
        (self.end - self.start).length()
    }

    /// The unit direction vector from start to end.
    pub fn get_unit_vector(&self) -> Point {
        normalize_or_zero(self.end - self.start)
    }

    /// The orthogonal projection of `point` onto this (infinite) line
    /// (manim's `get_projection`).
    ///
    /// ```
    /// use manim_core::geometry::Line;
    /// use manim_math::{Point, RIGHT, UP};
    /// let l = Line::new(Point::ZERO, 4.0 * RIGHT);
    /// let proj = l.get_projection(Point::new(1.0, 2.0, 0.0));
    /// assert!((proj - RIGHT).length() < 1e-6);
    /// let _ = UP;
    /// ```
    pub fn get_projection(&self, point: Point) -> Point {
        let dir = self.end - self.start;
        let len2 = dir.length_squared();
        if len2 < 1e-12 {
            return self.start;
        }
        let t = (point - self.start).dot(dir) / len2;
        self.start + dir * t
    }

    /// The point at arc-length proportion `alpha` (manim's
    /// `point_from_proportion`).
    pub fn point_from_proportion(&self, alpha: f32) -> Point {
        self.data.path.point_from_proportion(alpha)
    }

    /// Repositions the line's endpoints to `start` and `end`
    /// (manim's `put_start_and_end_on`).
    ///
    /// ```
    /// use manim_core::geometry::Line;
    /// use manim_math::{Point, UP, RIGHT};
    /// let mut l = Line::new(Point::ZERO, RIGHT);
    /// l.put_start_and_end_on(UP, 2.0 * UP);
    /// assert!((l.get_start() - UP).length() < 1e-6);
    /// assert!((l.get_end() - 2.0 * UP).length() < 1e-6);
    /// ```
    pub fn put_start_and_end_on(&mut self, start: Point, end: Point) -> &mut Self {
        self.start = start;
        self.end = end;
        self.data.path = Path::from_corners(&[start, end], false);
        self.data.bump_generation();
        self
    }
}

/// A dashed straight line. Port of manim CE's `DashedLine` — geometrically a
/// single line whose stroke carries a dash pattern.
///
/// ```
/// use manim_core::geometry::DashedLine;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::{Point, RIGHT};
/// let l = DashedLine::new(Point::ZERO, 2.0 * RIGHT);
/// assert!(l.data().style.dash_pattern.is_some());
/// assert!((l.get_length() - 2.0).abs() < 1e-6);
/// ```
#[derive(Clone)]
pub struct DashedLine {
    data: MobjectData,
    start: Point,
    end: Point,
}
impl_mobject!(DashedLine);

/// manim CE's default dash length for `DashedLine`.
pub const DEFAULT_DASH_LENGTH: f32 = 0.05;

impl DashedLine {
    /// A dashed line from `start` to `end` with the default dash pattern.
    pub fn new(start: Point, end: Point) -> Self {
        let mut style = Style::stroked(WHITE);
        style.set_dash(&[DEFAULT_DASH_LENGTH, DEFAULT_DASH_LENGTH]);
        Self {
            data: MobjectData::new(Path::from_corners(&[start, end], false), style),
            start,
            end,
        }
    }

    /// The start point.
    pub fn get_start(&self) -> Point {
        self.start
    }

    /// The end point.
    pub fn get_end(&self) -> Point {
        self.end
    }

    /// The length of the line.
    pub fn get_length(&self) -> f32 {
        (self.end - self.start).length()
    }
}

/// An arrow: a shaft plus a filled triangular tip at the end. Port of manim CE's
/// `Arrow` (the tip is a separate closed subpath of the same mobject).
///
/// ```
/// use manim_core::geometry::Arrow;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::{Point, RIGHT};
/// let a = Arrow::new(Point::ZERO, 4.0 * RIGHT);
/// assert!((a.get_length() - 4.0).abs() < 1e-6);
/// // Shaft subpath + tip subpath.
/// assert_eq!(a.data().path.subpaths.len(), 2);
/// // The tip is filled.
/// assert_eq!(a.data().style.fill_opacity, 1.0);
/// ```
#[derive(Clone)]
pub struct Arrow {
    data: MobjectData,
    start: Point,
    end: Point,
    buff: f32,
    tip_length: f32,
    tip_shape: TipShape,
    tip_filled: bool,
}
impl_mobject!(Arrow);

impl Arrow {
    /// An arrow from `start` to `end` with the default tip length and no buffer.
    pub fn new(start: Point, end: Point) -> Self {
        Self::with_params(start, end, 0.0, DEFAULT_ARROW_TIP_LENGTH)
    }

    /// An arrow with an explicit end `buff` and `tip_length`.
    pub fn with_params(start: Point, end: Point, buff: f32, tip_length: f32) -> Self {
        let mut me = Self {
            data: MobjectData::new(Path::default(), arrow_style()),
            start,
            end,
            buff,
            tip_length,
            tip_shape: TipShape::Triangle,
            tip_filled: true,
        };
        me.rebuild();
        me
    }

    /// Sets the tip head shape (manim's `tip_shape`).
    ///
    /// ```
    /// use manim_core::geometry::{Arrow, TipShape};
    /// use manim_core::mobject::Mobject;
    /// use manim_math::{Point, RIGHT};
    /// let a = Arrow::new(Point::ZERO, 3.0 * RIGHT).tip_shape(TipShape::Stealth);
    /// // Shaft + stealth head (a 4-vertex dart).
    /// assert!(a.data().path.subpaths.len() == 2);
    /// ```
    pub fn tip_shape(mut self, shape: TipShape) -> Self {
        self.tip_shape = shape;
        self.rebuild();
        self
    }

    /// Draws the tip as an open outline rather than a solid head (pointed shapes
    /// only; round/square heads stay solid).
    pub fn open_tip(mut self) -> Self {
        self.tip_filled = false;
        self.rebuild();
        self
    }

    /// Rebuilds the arrow path from its parameters.
    fn rebuild(&mut self) {
        self.data.path = arrow_path(
            self.start,
            self.end,
            self.buff,
            self.tip_length,
            false,
            self.tip_shape,
            self.tip_filled,
        );
        self.data.bump_generation();
    }

    /// The tail point.
    pub fn get_start(&self) -> Point {
        self.start
    }

    /// The tip point.
    pub fn get_end(&self) -> Point {
        self.end
    }

    /// The tip-to-tail length.
    pub fn get_length(&self) -> f32 {
        (self.end - self.start).length()
    }

    /// The direction angle of the arrow.
    pub fn get_angle(&self) -> f32 {
        angle_of_vector(self.end - self.start)
    }

    /// The end buffer distance.
    pub fn buff(&self) -> f32 {
        self.buff
    }

    /// The tip length.
    pub fn tip_length(&self) -> f32 {
        self.tip_length
    }
}

/// A position vector: an [`Arrow`] from the origin to a point. Port of manim
/// CE's `Vector`.
///
/// ```
/// use manim_core::geometry::Vector;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::{Point, UP};
/// let v = Vector::new(2.0 * UP);
/// assert!((v.get_start()).length() < 1e-6);
/// assert!((v.get_end() - 2.0 * UP).length() < 1e-6);
/// ```
#[derive(Clone)]
pub struct Vector {
    data: MobjectData,
    end: Point,
}
impl_mobject!(Vector);

impl Vector {
    /// A vector arrow from the origin to `end`.
    pub fn new(end: Point) -> Self {
        let path = arrow_path(
            ORIGIN,
            end,
            0.0,
            DEFAULT_ARROW_TIP_LENGTH,
            false,
            TipShape::Triangle,
            true,
        );
        Self {
            data: MobjectData::new(path, arrow_style()),
            end,
        }
    }

    /// The tail point (always the origin, before transforms).
    pub fn get_start(&self) -> Point {
        ORIGIN
    }

    /// The tip point.
    pub fn get_end(&self) -> Point {
        self.end
    }
}

/// A double-headed arrow (a tip at each end). Port of manim CE's `DoubleArrow`.
///
/// ```
/// use manim_core::geometry::DoubleArrow;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::{Point, RIGHT};
/// let a = DoubleArrow::new(Point::ZERO, 4.0 * RIGHT);
/// // Shaft + two tips.
/// assert_eq!(a.data().path.subpaths.len(), 3);
/// ```
#[derive(Clone)]
pub struct DoubleArrow {
    data: MobjectData,
    start: Point,
    end: Point,
}
impl_mobject!(DoubleArrow);

impl DoubleArrow {
    /// A double arrow from `start` to `end` with default tip lengths.
    pub fn new(start: Point, end: Point) -> Self {
        let path = arrow_path(
            start,
            end,
            0.0,
            DEFAULT_ARROW_TIP_LENGTH,
            true,
            TipShape::Triangle,
            true,
        );
        Self {
            data: MobjectData::new(path, arrow_style()),
            start,
            end,
        }
    }

    /// The start point.
    pub fn get_start(&self) -> Point {
        self.start
    }

    /// The end point.
    pub fn get_end(&self) -> Point {
        self.end
    }
}

/// A curved arrow: an [`ArcBetweenPoints`](crate::geometry::ArcBetweenPoints)
/// from `start` to `end` with a tip at the end, tangent to the arc. Port of
/// manim CE's `CurvedArrow`.
///
/// ```
/// use manim_core::geometry::CurvedArrow;
/// use manim_core::mobject::Mobject;
/// use manim_math::{Point, RIGHT};
/// let a = CurvedArrow::new(Point::ZERO, 2.0 * RIGHT, 1.0);
/// // Arc shaft + one tip subpath.
/// assert!(a.data().path.subpaths.len() >= 2);
/// ```
#[derive(Clone)]
pub struct CurvedArrow {
    data: MobjectData,
    start: Point,
    end: Point,
}
impl_mobject!(CurvedArrow);

impl CurvedArrow {
    /// A curved arrow from `start` to `end` bowing by `angle` radians, tipped at
    /// the end.
    pub fn new(start: Point, end: Point, angle: f32) -> Self {
        Self {
            data: MobjectData::new(curved_arrow_path(start, end, angle, false), arrow_style()),
            start,
            end,
        }
    }

    /// The start point.
    pub fn get_start(&self) -> Point {
        self.start
    }

    /// The end point (the tipped end).
    pub fn get_end(&self) -> Point {
        self.end
    }
}

/// A curved arrow tipped at **both** ends. Port of manim CE's
/// `CurvedDoubleArrow`.
///
/// ```
/// use manim_core::geometry::CurvedDoubleArrow;
/// use manim_core::mobject::Mobject;
/// use manim_math::{Point, RIGHT};
/// let a = CurvedDoubleArrow::new(Point::ZERO, 2.0 * RIGHT, 1.0);
/// // Arc shaft + two tips.
/// assert!(a.data().path.subpaths.len() >= 3);
/// ```
#[derive(Clone)]
pub struct CurvedDoubleArrow {
    data: MobjectData,
    start: Point,
    end: Point,
}
impl_mobject!(CurvedDoubleArrow);

impl CurvedDoubleArrow {
    /// A double-headed curved arrow from `start` to `end` bowing by `angle`.
    pub fn new(start: Point, end: Point, angle: f32) -> Self {
        Self {
            data: MobjectData::new(curved_arrow_path(start, end, angle, true), arrow_style()),
            start,
            end,
        }
    }

    /// The start point.
    pub fn get_start(&self) -> Point {
        self.start
    }

    /// The end point.
    pub fn get_end(&self) -> Point {
        self.end
    }
}

/// Builds a curved arrow path: an arc plus a tip at the end (and, if `double`,
/// the start), each tangent to the arc.
fn curved_arrow_path(start: Point, end: Point, angle: f32, double: bool) -> Path {
    let mut path = crate::geometry::ArcBetweenPoints::new(start, end, angle)
        .data()
        .path
        .clone();
    let tl = DEFAULT_ARROW_TIP_LENGTH;
    if let Some((p_end, dir_end)) = arc_endpoint_tangent(&path, true) {
        path.subpaths
            .push(tip_subpath(p_end, dir_end, tl, TipShape::Triangle, true));
    }
    if double {
        if let Some((p_start, dir_start)) = arc_endpoint_tangent(&path, false) {
            path.subpaths.push(tip_subpath(
                p_start,
                dir_start,
                tl,
                TipShape::Triangle,
                true,
            ));
        }
    }
    path
}

/// The `(point, outward_unit_tangent)` at the arc's end (`at_end`) or start.
/// Outward means pointing away from the arc body (the arrow direction there).
fn arc_endpoint_tangent(path: &Path, at_end: bool) -> Option<(Point, Point)> {
    let sp = path.subpaths.first()?;
    if at_end {
        let c = sp.curves.last()?;
        let dir = normalize_or_zero(c.p3 - c.p2);
        let dir = if dir == Point::ZERO {
            normalize_or_zero(c.p3 - c.p0)
        } else {
            dir
        };
        Some((c.p3, dir))
    } else {
        let c = sp.curves.first()?;
        let dir = normalize_or_zero(c.p0 - c.p1);
        let dir = if dir == Point::ZERO {
            normalize_or_zero(c.p0 - c.p3)
        } else {
            dir
        };
        Some((c.p0, dir))
    }
}

/// A right-angle "elbow" marker. Port of manim CE's `Elbow`.
///
/// ```
/// use manim_core::geometry::Elbow;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// let e = Elbow::new(0.5, 0.0);
/// // An L of arm length 0.5 spans 0.5 in each axis.
/// assert!((e.bounding_box().width() - 0.5).abs() < 1e-5);
/// ```
#[derive(Clone)]
pub struct Elbow {
    data: MobjectData,
    width: f32,
    angle: f32,
}
impl_mobject!(Elbow);

impl Elbow {
    /// An elbow of the given arm `width`, rotated by `angle` about the origin.
    pub fn new(width: f32, angle: f32) -> Self {
        // Base corners UP → UP+RIGHT → RIGHT, scaled to `width`, rotated.
        let base = [
            Point::new(0.0, 1.0, 0.0),
            Point::new(1.0, 1.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
        ];
        let corners: Vec<Point> = base
            .iter()
            .map(|p| rotate_vector(*p * width, angle))
            .collect();
        Self {
            data: MobjectData::new(Path::from_corners(&corners, false), Style::stroked(WHITE)),
            width,
            angle,
        }
    }

    /// The arm width.
    pub fn width_value(&self) -> f32 {
        self.width
    }

    /// The rotation angle.
    pub fn angle(&self) -> f32 {
        self.angle
    }
}

/// The default radius for [`Angle`] arcs / [`RightAngle`] arm length.
pub const DEFAULT_ANGLE_RADIUS: f32 = 0.5;

/// An arc marking the angle between two lines. Port of manim CE's `Angle`.
///
/// ```
/// use manim_core::geometry::{Angle, Line};
/// use manim_math::{Point, RIGHT, UP};
/// let l1 = Line::new(Point::ZERO, RIGHT);
/// let l2 = Line::new(Point::ZERO, UP);
/// let a = Angle::new(&l1, &l2);
/// // The two lines meet at a right angle.
/// assert!((a.get_value() - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
/// ```
#[derive(Clone)]
pub struct Angle {
    data: MobjectData,
    vertex: Point,
    radius: f32,
    value: f32,
}
impl_mobject!(Angle);

impl Angle {
    /// The angle arc between `line1` and `line2`, at the default radius.
    pub fn new(line1: &Line, line2: &Line) -> Self {
        Self::with_radius(line1, line2, DEFAULT_ANGLE_RADIUS)
    }

    /// The angle arc between `line1` and `line2` at an explicit `radius`.
    pub fn with_radius(line1: &Line, line2: &Line, radius: f32) -> Self {
        let vertex = line_intersection(
            (line1.get_start(), line1.get_end()),
            (line2.get_start(), line2.get_end()),
        )
        .unwrap_or_else(|| line1.get_start());
        let a1 = angle_of_vector(line1.get_end() - line1.get_start());
        let a2 = angle_of_vector(line2.get_end() - line2.get_start());
        // Signed sweep wrapped into (-π, π].
        let mut delta = a2 - a1;
        while delta > PI {
            delta -= manim_math::TAU;
        }
        while delta <= -PI {
            delta += manim_math::TAU;
        }
        let path = super::arc_path(vertex, radius, a1, delta, false);
        Self {
            data: MobjectData::new(path, Style::stroked(WHITE)),
            vertex,
            radius,
            value: delta.abs(),
        }
    }

    /// The (unsigned) measured angle in radians (manim's `get_value`).
    pub fn get_value(&self) -> f32 {
        self.value
    }

    /// The vertex where the two lines meet.
    pub fn vertex(&self) -> Point {
        self.vertex
    }

    /// The arc radius.
    pub fn radius(&self) -> f32 {
        self.radius
    }
}

/// A square right-angle marker between two perpendicular lines. Port of manim
/// CE's `RightAngle`.
///
/// ```
/// use manim_core::geometry::{Line, RightAngle};
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::{Point, RIGHT, UP};
/// let l1 = Line::new(Point::ZERO, RIGHT);
/// let l2 = Line::new(Point::ZERO, UP);
/// let r = RightAngle::new(&l1, &l2);
/// // The elbow marker is a small square corner of the given arm length.
/// assert!((r.bounding_box().width() - 0.5).abs() < 1e-5);
/// ```
#[derive(Clone)]
pub struct RightAngle {
    data: MobjectData,
    vertex: Point,
    length: f32,
}
impl_mobject!(RightAngle);

impl RightAngle {
    /// A right-angle marker at the meeting point of `line1` and `line2`, with
    /// the default arm length.
    pub fn new(line1: &Line, line2: &Line) -> Self {
        Self::with_length(line1, line2, DEFAULT_ANGLE_RADIUS)
    }

    /// A right-angle marker with an explicit arm `length`.
    pub fn with_length(line1: &Line, line2: &Line, length: f32) -> Self {
        let vertex = line_intersection(
            (line1.get_start(), line1.get_end()),
            (line2.get_start(), line2.get_end()),
        )
        .unwrap_or_else(|| line1.get_start());
        let d1 = normalize_or_zero(line1.get_end() - line1.get_start());
        let d2 = normalize_or_zero(line2.get_end() - line2.get_start());
        let corners = [
            vertex + d1 * length,
            vertex + d1 * length + d2 * length,
            vertex + d2 * length,
        ];
        Self {
            data: MobjectData::new(Path::from_corners(&corners, false), Style::stroked(WHITE)),
            vertex,
            length,
        }
    }

    /// The vertex where the two lines meet.
    pub fn vertex(&self) -> Point {
        self.vertex
    }

    /// The arm length of the marker.
    pub fn length(&self) -> f32 {
        self.length
    }
}

/// The style shared by arrows: a stroked shaft and a filled tip.
fn arrow_style() -> Style {
    let mut s = Style::stroked(WHITE);
    s.set_fill(WHITE, 1.0);
    s
}

/// Builds an arrow path: an open shaft subpath plus one (or, if `double`, two)
/// filled triangular tip subpaths.
#[allow(clippy::too_many_arguments)]
fn arrow_path(
    start: Point,
    end: Point,
    buff: f32,
    tip_length: f32,
    double: bool,
    shape: TipShape,
    filled: bool,
) -> Path {
    let dir = normalize_or_zero(end - start);
    if dir == Point::ZERO {
        return Path::from_corners(&[start, end], false);
    }
    let a_start = start + dir * buff;
    let a_end = end - dir * buff;
    let span = (a_end - a_start).length();
    let tl = tip_length.min(span * 0.5).max(0.0);

    let mut subpaths = Vec::new();
    let end_base = a_end - dir * tl;
    // Shaft runs between the (possibly two) tip bases.
    let shaft_start = if double { a_start + dir * tl } else { a_start };
    subpaths.push(SubPath::from_corners(&[shaft_start, end_base]));

    if double {
        subpaths.push(tip_subpath(a_start, -dir, tl, shape, filled));
    }
    subpaths.push(tip_subpath(a_end, dir, tl, shape, filled));
    Path { subpaths }
}

/// A straight line tangent to another mobject's outline at arc-length proportion
/// `alpha`. Port of manim CE's `TangentLine`.
///
/// The direction is estimated by finite difference of the outline around
/// `alpha`; the segment of the requested `length` is centered on the tangent
/// point.
///
/// ```
/// use manim_core::geometry::{Circle, Line, TangentLine};
/// use manim_math::{Point, UP};
/// let circle = Circle::new();
/// // At the rightmost point of the unit circle the tangent is vertical.
/// let t = TangentLine::new(&circle, 0.0, 2.0);
/// assert!((t.get_length() - 2.0).abs() < 1e-3);
/// // The segment is vertical: its endpoints share an x.
/// assert!((t.get_end().x - t.get_start().x).abs() < 1e-2);
/// let _ = UP;
/// ```
#[derive(Clone)]
pub struct TangentLine {
    data: MobjectData,
    start: Point,
    end: Point,
}
impl_mobject!(TangentLine);

impl TangentLine {
    /// A tangent to `mob` at proportion `alpha`, of the given `length`.
    pub fn new(mob: &dyn Mobject, alpha: f32, length: f32) -> Self {
        let path = &mob.data().path;
        let a = alpha.clamp(0.0, 1.0);
        let eps = 1e-3;
        let before = path.point_from_proportion((a - eps).max(0.0));
        let after = path.point_from_proportion((a + eps).min(1.0));
        let center = path.point_from_proportion(a);
        let dir = normalize_or_zero(after - before);
        let half = dir * (length / 2.0);
        let start = center - half;
        let end = center + half;
        Self {
            data: MobjectData::new(
                Path::from_corners(&[start, end], false),
                Style::stroked(WHITE),
            ),
            start,
            end,
        }
    }

    /// The start point.
    pub fn get_start(&self) -> Point {
        self.start
    }

    /// The end point.
    pub fn get_end(&self) -> Point {
        self.end
    }

    /// The length of the tangent segment.
    pub fn get_length(&self) -> f32 {
        (self.end - self.start).length()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::Mobject;
    use manim_math::{RIGHT, UP};

    #[test]
    fn line_length_and_angle() {
        let l = Line::new(ORIGIN, Point::new(3.0, 4.0, 0.0));
        assert!((l.get_length() - 5.0).abs() < 1e-6);
    }

    #[test]
    fn projection_onto_axis() {
        let l = Line::new(ORIGIN, 4.0 * RIGHT);
        let p = l.get_projection(Point::new(1.0, 5.0, 0.0));
        assert!((p - RIGHT).length() < 1e-6);
    }

    #[test]
    fn arrow_has_shaft_and_tip() {
        let a = Arrow::new(ORIGIN, 4.0 * RIGHT);
        assert_eq!(a.data().path.subpaths.len(), 2);
        assert!((a.get_length() - 4.0).abs() < 1e-6);
    }

    #[test]
    fn double_arrow_has_two_tips() {
        let a = DoubleArrow::new(ORIGIN, 4.0 * RIGHT);
        assert_eq!(a.data().path.subpaths.len(), 3);
    }

    #[test]
    fn angle_between_perpendicular_lines() {
        let l1 = Line::new(ORIGIN, RIGHT);
        let l2 = Line::new(ORIGIN, UP);
        let a = Angle::new(&l1, &l2);
        assert!((a.get_value() - PI / 2.0).abs() < 1e-5);
    }
}
