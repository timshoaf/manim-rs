//! [`NumberPlane`], [`ComplexPlane`], and [`PolarPlane`]: coordinate systems
//! with a background grid.

use manim_color::{BLUE, BLUE_D};
use manim_math::path::{Path, SubPath};
use manim_math::{Point, ORIGIN, TAU};

use super::axes::tick_values;
use super::CoordSystem;
use crate::impl_mobject;
use crate::mobject::MobjectData;
use crate::style::Style;

/// Grid stroke opacity (faded, as in manim).
const GRID_OPACITY: f32 = 0.45;

/// Builds the Cartesian grid (one line per tick on each axis, plus the two axis
/// lines) for `cs`.
fn cartesian_grid(cs: &CoordSystem) -> Path {
    let mut subpaths = Vec::new();
    // Vertical grid lines.
    for x in tick_values(cs.x_range) {
        subpaths.push(SubPath::from_corners(&[
            cs.coords_to_point(x, cs.y_range[0]),
            cs.coords_to_point(x, cs.y_range[1]),
        ]));
    }
    // Horizontal grid lines.
    for y in tick_values(cs.y_range) {
        subpaths.push(SubPath::from_corners(&[
            cs.coords_to_point(cs.x_range[0], y),
            cs.coords_to_point(cs.x_range[1], y),
        ]));
    }
    Path { subpaths }
}

/// A faded grid style in `color`.
fn grid_style(color: manim_color::Color) -> Style {
    let mut s = Style::stroked(color);
    s.stroke_opacity = GRID_OPACITY;
    s.stroke_width = 2.0;
    s
}

/// A Cartesian grid over a coordinate system (manim CE's `NumberPlane`).
///
/// Rendered as one faded mobject (grid + axis lines). Per-line color
/// differentiation (brighter axes, fainter sub-lines) needs child grouping and
/// is deferred; the coordinate math (`c2p`/`p2c`) is exact.
///
/// ```
/// use manim_core::graphing::NumberPlane;
/// use manim_core::mobject::Mobject;
/// let plane = NumberPlane::new([-4.0, 4.0, 1.0], [-3.0, 3.0, 1.0]);
/// // 9 vertical + 7 horizontal grid lines.
/// assert_eq!(plane.data().path.subpaths.len(), 16);
/// let (x, y) = plane.point_to_coords(plane.coords_to_point(2.0, -1.0));
/// assert!((x - 2.0).abs() < 1e-4 && (y + 1.0).abs() < 1e-4);
/// ```
#[derive(Clone)]
pub struct NumberPlane {
    data: MobjectData,
    coords: CoordSystem,
}
impl_mobject!(NumberPlane);

impl NumberPlane {
    /// A plane over the given ranges, one scene unit per data unit.
    pub fn new(x_range: [f32; 3], y_range: [f32; 3]) -> Self {
        let x_len = (x_range[1] - x_range[0]).abs();
        let y_len = (y_range[1] - y_range[0]).abs();
        let coords = CoordSystem::new(x_range, y_range, x_len, y_len);
        Self {
            data: MobjectData::new(cartesian_grid(&coords), grid_style(BLUE_D)),
            coords,
        }
    }

    /// The embedded coordinate system.
    pub fn coords(&self) -> CoordSystem {
        self.coords
    }

    /// Maps data `(x, y)` to a scene point.
    pub fn coords_to_point(&self, x: f32, y: f32) -> Point {
        self.coords.coords_to_point(x, y)
    }

    /// Maps a scene point back to data `(x, y)`.
    pub fn point_to_coords(&self, p: Point) -> (f32, f32) {
        self.coords.point_to_coords(p)
    }
}

/// The complex plane: a [`NumberPlane`] addressed by `(re, im)` (manim CE's
/// `ComplexPlane`). No external complex-number type is needed — a complex value
/// is the tuple `(re, im)`.
///
/// ```
/// use manim_core::graphing::ComplexPlane;
/// let plane = ComplexPlane::new([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0]);
/// let p = plane.number_to_point((1.0, 2.0)); // 1 + 2i
/// let (re, im) = plane.point_to_number(p);
/// assert!((re - 1.0).abs() < 1e-4 && (im - 2.0).abs() < 1e-4);
/// ```
#[derive(Clone)]
pub struct ComplexPlane {
    data: MobjectData,
    coords: CoordSystem,
}
impl_mobject!(ComplexPlane);

impl ComplexPlane {
    /// A complex plane over the given real/imaginary ranges.
    pub fn new(re_range: [f32; 3], im_range: [f32; 3]) -> Self {
        let x_len = (re_range[1] - re_range[0]).abs();
        let y_len = (im_range[1] - im_range[0]).abs();
        let coords = CoordSystem::new(re_range, im_range, x_len, y_len);
        Self {
            data: MobjectData::new(cartesian_grid(&coords), grid_style(BLUE_D)),
            coords,
        }
    }

    /// The scene point for complex number `(re, im)` (manim's `number_to_point`
    /// / `n2p`).
    pub fn number_to_point(&self, z: (f32, f32)) -> Point {
        self.coords.coords_to_point(z.0, z.1)
    }

    /// The complex number at scene point `p` (manim's `point_to_number` /
    /// `p2n`).
    pub fn point_to_number(&self, p: Point) -> (f32, f32) {
        self.coords.point_to_coords(p)
    }

    /// The embedded coordinate system.
    pub fn coords(&self) -> CoordSystem {
        self.coords
    }
}

/// A basic polar grid: concentric azimuth circles and radial spokes (manim CE's
/// `PolarPlane`). Circles are approximated as regular polygons.
///
/// ```
/// use manim_core::graphing::PolarPlane;
/// use manim_math::Point;
/// let plane = PolarPlane::new(3.0, 3, 12);
/// // r = 0 is the pole; angle 0 points along +x.
/// assert!(plane.polar_to_point(0.0, 0.0).length() < 1e-6);
/// let p = plane.polar_to_point(2.0, 0.0);
/// assert!((p - Point::new(2.0, 0.0, 0.0)).length() < 1e-5);
/// ```
#[derive(Clone)]
pub struct PolarPlane {
    data: MobjectData,
    max_radius: f32,
    radius_step: f32,
    azimuth_divisions: usize,
}
impl_mobject!(PolarPlane);

impl PolarPlane {
    /// A polar grid out to `max_radius`, with `radius_rings` concentric circles
    /// and `azimuth_divisions` radial spokes.
    pub fn new(max_radius: f32, radius_rings: usize, azimuth_divisions: usize) -> Self {
        let radius_step = if radius_rings > 0 {
            max_radius / radius_rings as f32
        } else {
            max_radius
        };
        let plane = Self {
            data: MobjectData::new(Path::default(), grid_style(BLUE)),
            max_radius,
            radius_step,
            azimuth_divisions,
        };
        Self {
            data: MobjectData::new(plane.build_grid(), grid_style(BLUE)),
            ..plane
        }
    }

    /// The scene point at polar `(r, theta)` — the pole is the origin, `theta`
    /// measured from `+x`.
    pub fn polar_to_point(&self, r: f32, theta: f32) -> Point {
        ORIGIN + Point::new(r * theta.cos(), r * theta.sin(), 0.0)
    }

    /// The polar coordinates `(r, theta)` of scene point `p`.
    pub fn point_to_polar(&self, p: Point) -> (f32, f32) {
        ((p.x * p.x + p.y * p.y).sqrt(), p.y.atan2(p.x))
    }

    /// Builds the concentric circles and radial spokes.
    fn build_grid(&self) -> Path {
        const CIRCLE_SEGMENTS: usize = 64;
        let mut subpaths = Vec::new();

        let rings = if self.radius_step > 0.0 {
            (self.max_radius / self.radius_step).round() as usize
        } else {
            0
        };
        for k in 1..=rings {
            let r = k as f32 * self.radius_step;
            let pts: Vec<Point> = (0..CIRCLE_SEGMENTS)
                .map(|i| {
                    let a = i as f32 / CIRCLE_SEGMENTS as f32 * TAU;
                    self.polar_to_point(r, a)
                })
                .collect();
            let mut circle = SubPath::from_corners(&pts);
            circle.closed = true;
            subpaths.push(circle);
        }

        for i in 0..self.azimuth_divisions {
            let a = i as f32 / self.azimuth_divisions as f32 * TAU;
            subpaths.push(SubPath::from_corners(&[
                ORIGIN,
                self.polar_to_point(self.max_radius, a),
            ]));
        }

        Path { subpaths }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::Mobject;

    #[test]
    fn number_plane_line_count() {
        let plane = NumberPlane::new([-2.0, 2.0, 1.0], [-1.0, 1.0, 1.0]);
        // 5 vertical (x = -2..2) + 3 horizontal (y = -1..1).
        assert_eq!(plane.data().path.subpaths.len(), 8);
    }

    #[test]
    fn complex_plane_round_trip() {
        let plane = ComplexPlane::new([-4.0, 4.0, 1.0], [-4.0, 4.0, 1.0]);
        let (re, im) = plane.point_to_number(plane.number_to_point((2.5, -1.5)));
        assert!((re - 2.5).abs() < 1e-4 && (im + 1.5).abs() < 1e-4);
    }

    #[test]
    fn polar_plane_has_rings_and_spokes() {
        let plane = PolarPlane::new(3.0, 3, 8);
        // 3 rings + 8 spokes.
        assert_eq!(plane.data().path.subpaths.len(), 11);
        // Round-trip a polar point.
        let p = plane.polar_to_point(2.0, 1.0);
        let (r, t) = plane.point_to_polar(p);
        assert!((r - 2.0).abs() < 1e-5 && (t - 1.0).abs() < 1e-5);
    }
}
