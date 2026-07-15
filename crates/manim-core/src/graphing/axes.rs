//! [`Axes`]: a 2-D coordinate system mobject with plotting helpers.

use manim_color::WHITE;
use manim_math::path::{Path, SubPath};
use manim_math::{Point, DOWN, LEFT};

use super::{CoordSystem, FunctionGraph, ParametricFunction};
use crate::geometry::{Line, VMobject};
use crate::impl_mobject;
use crate::mobject::MobjectData;
use crate::style::Style;

/// Tick length for axes (scene units).
pub const AXIS_TICK_SIZE: f32 = 0.1;

/// Generates the tick values of `[min, max, step]`, aligned to multiples of the
/// step and clamped to the range.
pub(crate) fn tick_values(range: [f32; 3]) -> Vec<f32> {
    let [min, max, step] = range;
    let mut out = Vec::new();
    if step <= 0.0 {
        return out;
    }
    let mut i = (min / step).ceil() as i64;
    loop {
        let v = i as f32 * step;
        if v > max + 1e-6 {
            break;
        }
        if v >= min - 1e-6 {
            out.push(v);
        }
        i += 1;
    }
    out
}

/// A pair of perpendicular number lines forming a 2-D coordinate system. Port of
/// manim CE's `Axes`.
///
/// Coordinate conversion, plotting, area, and Riemann helpers are delegated to
/// the embedded [`CoordSystem`]. Numeric axis labels need `DecimalNumber` (M4)
/// and are deferred; tick geometry and [`CoordSystem`] anchors are provided now.
///
/// ```
/// use manim_core::graphing::Axes;
/// use manim_core::mobject::Mobject;
/// let axes = Axes::new([-5.0, 5.0, 1.0], [-3.0, 3.0, 1.0]);
/// let graph = axes.plot(|x| x.sin(), None);
/// assert!(!graph.data().path.subpaths.is_empty());
/// ```
#[derive(Clone)]
pub struct Axes {
    data: MobjectData,
    coords: CoordSystem,
    include_ticks: bool,
    tick_size: f32,
}
impl_mobject!(Axes);

impl Axes {
    /// Axes over the given `[min, max, step]` ranges, one scene unit per data
    /// unit (on-screen length equals the data span).
    pub fn new(x_range: [f32; 3], y_range: [f32; 3]) -> Self {
        let x_len = (x_range[1] - x_range[0]).abs();
        let y_len = (y_range[1] - y_range[0]).abs();
        Self::with_lengths(x_range, y_range, x_len, y_len)
    }

    /// Axes with explicit on-screen lengths.
    pub fn with_lengths(
        x_range: [f32; 3],
        y_range: [f32; 3],
        x_length: f32,
        y_length: f32,
    ) -> Self {
        let coords = CoordSystem::new(x_range, y_range, x_length, y_length);
        let mut axes = Self {
            data: MobjectData::new(Path::default(), Style::stroked(WHITE)),
            coords,
            include_ticks: true,
            tick_size: AXIS_TICK_SIZE,
        };
        axes.rebuild();
        axes
    }

    /// The embedded coordinate system (for plotting/area helpers).
    pub fn coords(&self) -> CoordSystem {
        self.coords
    }

    /// Maps data `(x, y)` to a scene point (`c2p`).
    pub fn coords_to_point(&self, x: f32, y: f32) -> Point {
        self.coords.coords_to_point(x, y)
    }

    /// Alias for [`coords_to_point`](Self::coords_to_point).
    pub fn c2p(&self, x: f32, y: f32) -> Point {
        self.coords.coords_to_point(x, y)
    }

    /// Maps a scene point to data `(x, y)` (`p2c`).
    pub fn point_to_coords(&self, p: Point) -> (f32, f32) {
        self.coords.point_to_coords(p)
    }

    /// Alias for [`point_to_coords`](Self::point_to_coords).
    pub fn p2c(&self, p: Point) -> (f32, f32) {
        self.coords.point_to_coords(p)
    }

    /// Plots `y = f(x)` in these axes (see [`CoordSystem::plot`]).
    pub fn plot(
        &self,
        f: impl Fn(f32) -> f32 + Send + Sync + 'static,
        x_range: Option<[f32; 3]>,
    ) -> FunctionGraph {
        self.coords.plot(f, x_range)
    }

    /// Plots a parametric curve in these axes.
    pub fn plot_parametric_curve(
        &self,
        f: impl Fn(f32) -> (f32, f32) + Send + Sync + 'static,
        t_min: f32,
        t_max: f32,
        t_step: f32,
    ) -> ParametricFunction {
        self.coords.plot_parametric_curve(f, t_min, t_max, t_step)
    }

    /// The point on `graph` at input `x`.
    pub fn input_to_graph_point(&self, x: f32, graph: &FunctionGraph) -> Point {
        self.coords.input_to_graph_point(x, graph)
    }

    /// The filled area between `graph` and the x-axis over `[x0, x1]`.
    pub fn get_area(&self, graph: &FunctionGraph, x0: f32, x1: f32, opacity: f32) -> VMobject {
        self.coords.get_area(graph, x0, x1, opacity)
    }

    /// Riemann rectangles under `graph` over `[x0, x1]` with width `dx`.
    pub fn get_riemann_rectangles(
        &self,
        graph: &FunctionGraph,
        x0: f32,
        x1: f32,
        dx: f32,
        opacity: f32,
    ) -> VMobject {
        self.coords
            .get_riemann_rectangles(graph, x0, x1, dx, opacity)
    }

    /// A vertical line from the x-axis up to `(x, y)`.
    pub fn get_vertical_line(&self, x: f32, y: f32) -> Line {
        self.coords.get_vertical_line(x, y)
    }

    /// A horizontal line from the y-axis across to `(x, y)`.
    pub fn get_horizontal_line(&self, x: f32, y: f32) -> Line {
        self.coords.get_horizontal_line(x, y)
    }

    /// The anchor point for a numeric label on the x-axis at value `x` (below
    /// the axis). Text is deferred (M4).
    pub fn x_label_point(&self, x: f32) -> Point {
        self.coords.coords_to_point(x, self.coords.x_axis_y()) + DOWN * (self.tick_size + 0.15)
    }

    /// The anchor point for a numeric label on the y-axis at value `y` (left of
    /// the axis). Text is deferred (M4).
    pub fn y_label_point(&self, y: f32) -> Point {
        self.coords.coords_to_point(self.coords.y_axis_x(), y) + LEFT * (self.tick_size + 0.15)
    }

    /// Rebuilds the axis geometry: two axis lines plus their ticks.
    fn rebuild(&mut self) {
        let cs = self.coords;
        let mut subpaths = Vec::new();

        // X axis at the (clamped) data y = 0 line.
        let xy = cs.x_axis_y();
        subpaths.push(SubPath::from_corners(&[
            cs.coords_to_point(cs.x_range[0], xy),
            cs.coords_to_point(cs.x_range[1], xy),
        ]));
        // Y axis at the (clamped) data x = 0 line.
        let yx = cs.y_axis_x();
        subpaths.push(SubPath::from_corners(&[
            cs.coords_to_point(yx, cs.y_range[0]),
            cs.coords_to_point(yx, cs.y_range[1]),
        ]));

        if self.include_ticks {
            let h = self.tick_size / 2.0;
            for x in tick_values(cs.x_range) {
                let p = cs.coords_to_point(x, xy);
                subpaths.push(SubPath::from_corners(&[
                    p + Point::new(0.0, -h, 0.0),
                    p + Point::new(0.0, h, 0.0),
                ]));
            }
            for y in tick_values(cs.y_range) {
                let p = cs.coords_to_point(yx, y);
                subpaths.push(SubPath::from_corners(&[
                    p + Point::new(-h, 0.0, 0.0),
                    p + Point::new(h, 0.0, 0.0),
                ]));
            }
        }

        self.data.path = Path { subpaths };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::Mobject;

    #[test]
    fn c2p_p2c_round_trip() {
        let axes = Axes::new([-5.0, 5.0, 1.0], [-3.0, 3.0, 1.0]);
        for (x, y) in [(0.0, 0.0), (2.0, 1.0), (-4.5, 2.5), (5.0, -3.0)] {
            let (rx, ry) = axes.point_to_coords(axes.coords_to_point(x, y));
            assert!((rx - x).abs() < 1e-4, "x {rx} vs {x}");
            assert!((ry - y).abs() < 1e-4, "y {ry} vs {y}");
        }
    }

    #[test]
    fn origin_maps_to_range_center() {
        // Asymmetric x range: its center (4.5) maps to the scene origin.
        let axes = Axes::new([-1.0, 10.0, 1.0], [0.0, 6.0, 1.0]);
        let p = axes.coords_to_point(4.5, 3.0);
        assert!(p.length() < 1e-5);
    }

    #[test]
    fn axes_has_two_axis_lines_plus_ticks() {
        let axes = Axes::new([-2.0, 2.0, 1.0], [-2.0, 2.0, 1.0]);
        // 2 axis lines + 5 x ticks + 5 y ticks.
        assert_eq!(axes.data().path.subpaths.len(), 12);
    }
}
