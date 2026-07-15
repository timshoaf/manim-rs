//! [`CoordSystem`]: the shared coordinate math and plotting helpers backing
//! [`Axes`](super::Axes) and the planes.

use std::sync::Arc;

use manim_color::{Color, BLUE};
use manim_math::path::{Path, SubPath};
use manim_math::Point;

use super::functions::{runs_to_path, ImplicitFunction, ScalarFn};
use super::{closed_polygon, sample_runs, FunctionGraph, ParametricFunction};
use crate::geometry::{Line, VMobject};
use crate::style::Style;

/// A linear 2-D coordinate system: data ranges on each axis and the scene-unit
/// scale factors. `Copy`, so it is cheap to embed in [`Axes`](super::Axes) and
/// the planes and to move into sampling closures.
///
/// The range midpoint maps to the scene origin, so the system is centered:
/// `coords_to_point(x_center, y_center) == ORIGIN`.
///
/// ```
/// use manim_core::graphing::CoordSystem;
/// let cs = CoordSystem::new([0.0, 10.0, 1.0], [0.0, 5.0, 1.0], 10.0, 5.0);
/// // 1 data unit on x is x_length / x_span = 10 / 10 = 1 scene unit.
/// let p = cs.coords_to_point(5.0, 2.5); // the range center → origin
/// assert!(p.length() < 1e-5);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoordSystem {
    /// `[min, max, step]` for the x axis.
    pub x_range: [f32; 3],
    /// `[min, max, step]` for the y axis.
    pub y_range: [f32; 3],
    /// Scene units per data unit on x.
    pub x_unit: f32,
    /// Scene units per data unit on y.
    pub y_unit: f32,
}

impl CoordSystem {
    /// Builds a system whose axes have the given data ranges and on-screen
    /// lengths.
    pub fn new(x_range: [f32; 3], y_range: [f32; 3], x_length: f32, y_length: f32) -> Self {
        let x_span = (x_range[1] - x_range[0]).abs().max(1e-9);
        let y_span = (y_range[1] - y_range[0]).abs().max(1e-9);
        Self {
            x_range,
            y_range,
            x_unit: x_length / x_span,
            y_unit: y_length / y_span,
        }
    }

    /// The x data value that maps to the origin.
    pub fn x_center(&self) -> f32 {
        0.5 * (self.x_range[0] + self.x_range[1])
    }

    /// The y data value that maps to the origin.
    pub fn y_center(&self) -> f32 {
        0.5 * (self.y_range[0] + self.y_range[1])
    }

    /// Maps data coordinates `(x, y)` to a scene point (manim's
    /// `coords_to_point` / `c2p`).
    pub fn coords_to_point(&self, x: f32, y: f32) -> Point {
        Point::new(
            (x - self.x_center()) * self.x_unit,
            (y - self.y_center()) * self.y_unit,
            0.0,
        )
    }

    /// Maps a scene point back to data coordinates (manim's `point_to_coords` /
    /// `p2c`); the inverse of [`coords_to_point`](Self::coords_to_point).
    pub fn point_to_coords(&self, p: Point) -> (f32, f32) {
        (
            self.x_center() + p.x / self.x_unit,
            self.y_center() + p.y / self.y_unit,
        )
    }

    /// Plots `y = f(x)` over `x_range` (defaulting to the x axis range), with
    /// adaptive sampling and discontinuity splitting, in this system's
    /// coordinates.
    ///
    /// ```
    /// use manim_core::graphing::CoordSystem;
    /// use manim_core::mobject::Mobject;
    /// let cs = CoordSystem::new([-4.0, 4.0, 1.0], [-2.0, 2.0, 1.0], 8.0, 4.0);
    /// let graph = cs.plot(|x| x.sin(), None);
    /// // 1/x-style discontinuities split into multiple subpaths; sin is one.
    /// assert_eq!(graph.data().path.subpaths.len(), 1);
    /// ```
    pub fn plot(
        &self,
        f: impl Fn(f32) -> f32 + Send + Sync + 'static,
        x_range: Option<[f32; 3]>,
    ) -> FunctionGraph {
        let [x0, x1, step] = x_range.unwrap_or([
            self.x_range[0],
            self.x_range[1],
            (self.x_range[1] - self.x_range[0]).abs() / 50.0,
        ]);
        let step = if step > 0.0 {
            step
        } else {
            (x1 - x0).abs() / 50.0
        };
        let f: ScalarFn = Arc::new(f);
        let yr = (self.y_range[1] - self.y_range[0]).abs().max(1.0);
        let runs = sample_runs(f.as_ref(), x0, x1, step, yr * 0.01, yr * 2.0);
        let path = runs_to_path(&runs, |x, y| self.coords_to_point(x, y));
        FunctionGraph::from_parts(path, f, x0, x1)
    }

    /// Plots a parametric curve `t ↦ (x(t), y(t))` in this system's coordinates.
    pub fn plot_parametric_curve(
        &self,
        f: impl Fn(f32) -> (f32, f32) + Send + Sync + 'static,
        t_min: f32,
        t_max: f32,
        t_step: f32,
    ) -> ParametricFunction {
        let cs = *self;
        ParametricFunction::new(
            move |t| {
                let (x, y) = f(t);
                cs.coords_to_point(x, y)
            },
            t_min,
            t_max,
            t_step,
        )
    }

    /// The point on `graph` at input `x` (manim's `input_to_graph_point`).
    pub fn input_to_graph_point(&self, x: f32, graph: &FunctionGraph) -> Point {
        self.coords_to_point(x, graph.evaluate(x))
    }

    /// The filled area between `graph` and the x-axis over `[x0, x1]`, as a
    /// closed polygon (manim's `get_area`).
    ///
    /// ```
    /// use manim_core::graphing::CoordSystem;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// let cs = CoordSystem::new([0.0, 4.0, 1.0], [0.0, 4.0, 1.0], 4.0, 4.0);
    /// let graph = cs.plot(|x| x, None);
    /// let area = cs.get_area(&graph, 0.0, 4.0, 0.5);
    /// // The area spans the full x extent of the plot.
    /// assert!(area.bounding_box().width() > 3.0);
    /// ```
    pub fn get_area(&self, graph: &FunctionGraph, x0: f32, x1: f32, opacity: f32) -> VMobject {
        let step = (x1 - x0).abs().max(1e-3) / 50.0;
        let mut pts: Vec<Point> = Vec::new();
        let mut x = x0;
        while x < x1 - 1e-6 {
            pts.push(self.coords_to_point(x, graph.evaluate(x)));
            x += step;
        }
        pts.push(self.coords_to_point(x1, graph.evaluate(x1)));
        // Close down to the axis and back.
        pts.push(self.coords_to_point(x1, 0.0));
        pts.push(self.coords_to_point(x0, 0.0));

        let path = Path {
            subpaths: vec![closed_polygon(&pts)],
        };
        let mut style = Style::filled(BLUE);
        style.fill_opacity = opacity;
        VMobject::new(path, style)
    }

    /// Riemann rectangles under `graph` over `[x0, x1]` with width `dx`
    /// (left-endpoint heights), as one filled mobject (manim's
    /// `get_riemann_rectangles`).
    ///
    /// ```
    /// use manim_core::graphing::CoordSystem;
    /// use manim_core::mobject::Mobject;
    /// let cs = CoordSystem::new([0.0, 4.0, 1.0], [0.0, 16.0, 4.0], 4.0, 4.0);
    /// let graph = cs.plot(|x| x * x, None);
    /// let rects = cs.get_riemann_rectangles(&graph, 0.0, 4.0, 1.0, 0.6);
    /// assert_eq!(rects.data().path.subpaths.len(), 4); // (4 - 0) / 1
    /// ```
    pub fn get_riemann_rectangles(
        &self,
        graph: &FunctionGraph,
        x0: f32,
        x1: f32,
        dx: f32,
        opacity: f32,
    ) -> VMobject {
        let dx = if dx.abs() > 1e-9 { dx.abs() } else { 1.0 };
        let n = (((x1 - x0).abs() / dx).round() as i64).max(0);
        let mut subpaths: Vec<SubPath> = Vec::with_capacity(n as usize);
        for i in 0..n {
            let xa = x0 + i as f32 * dx;
            let xb = xa + dx;
            let h = graph.evaluate(xa);
            let rect = [
                self.coords_to_point(xa, 0.0),
                self.coords_to_point(xb, 0.0),
                self.coords_to_point(xb, h),
                self.coords_to_point(xa, h),
            ];
            subpaths.push(closed_polygon(&rect));
        }
        let mut style = Style::filled(BLUE);
        style.fill_opacity = opacity;
        style.stroke_color = Some(manim_color::WHITE);
        style.stroke_opacity = 1.0;
        VMobject::new(Path { subpaths }, style)
    }

    /// A vertical line from the x-axis up to `(x, y)` (manim's
    /// `get_vertical_line`).
    pub fn get_vertical_line(&self, x: f32, y: f32) -> Line {
        Line::new(self.coords_to_point(x, 0.0), self.coords_to_point(x, y))
    }

    /// A horizontal line from the y-axis across to `(x, y)` (manim's
    /// `get_horizontal_line`).
    pub fn get_horizontal_line(&self, x: f32, y: f32) -> Line {
        Line::new(self.coords_to_point(0.0, y), self.coords_to_point(x, y))
    }

    /// Traces the implicit curve `f(x, y) = 0` over the coordinate ranges by
    /// marching squares. `resolution` is the grid divisions per axis (default
    /// [`DEFAULT_IMPLICIT_RESOLUTION`]); the curve is emitted as line-segment
    /// subpaths, so disconnected branches (e.g. a hyperbola) come out naturally.
    /// Port of manim CE's `plot_implicit_curve`.
    pub fn plot_implicit_curve(
        &self,
        f: impl Fn(f32, f32) -> f32,
        resolution: Option<usize>,
    ) -> ImplicitFunction {
        let res = resolution.unwrap_or(DEFAULT_IMPLICIT_RESOLUTION).max(2);
        let segments = marching_squares(&f, self.x_range, self.y_range, res);
        let subpaths = segments
            .into_iter()
            .map(|[a, b]| {
                SubPath::from_corners(&[
                    self.coords_to_point(a.0, a.1),
                    self.coords_to_point(b.0, b.1),
                ])
            })
            .collect();
        ImplicitFunction::from_path(Path { subpaths })
    }

    /// The scene point where the x-axis line sits (data `y` clamped into range),
    /// used when building axis geometry.
    pub(crate) fn x_axis_y(&self) -> f32 {
        0.0_f32.clamp(self.y_range[0], self.y_range[1])
    }

    /// The scene point where the y-axis line sits (data `x` clamped into range).
    pub(crate) fn y_axis_x(&self) -> f32 {
        0.0_f32.clamp(self.x_range[0], self.x_range[1])
    }

    /// Sets a solid color on a mobject-bound helper (kept for callers that want
    /// to recolor a plotted graph).
    pub fn styled_graph(&self, graph: FunctionGraph, color: Color) -> FunctionGraph {
        super::functions::with_color(graph, color)
    }
}

/// Default marching-squares grid resolution per axis for implicit curves.
pub const DEFAULT_IMPLICIT_RESOLUTION: usize = 100;

/// Marching squares over `f = 0` on the `[min,max]` ranges, returning
/// data-space line segments `[(x,y); 2]`.
fn marching_squares(
    f: &dyn Fn(f32, f32) -> f32,
    x_range: [f32; 3],
    y_range: [f32; 3],
    res: usize,
) -> Vec<[(f32, f32); 2]> {
    let (x0, x1) = (x_range[0], x_range[1]);
    let (y0, y1) = (y_range[0], y_range[1]);
    let dx = (x1 - x0) / res as f32;
    let dy = (y1 - y0) / res as f32;
    // Zero crossing on the segment a→b given field values fa, fb of opposite sign.
    let lerp = |t: f32, a: f32, b: f32| a + t * (b - a);
    let mut segs = Vec::new();
    for i in 0..res {
        for j in 0..res {
            let (cx0, cy0) = (x0 + i as f32 * dx, y0 + j as f32 * dy);
            let (cx1, cy1) = (cx0 + dx, cy0 + dy);
            let (bl, br, tr, tl) = (f(cx0, cy0), f(cx1, cy0), f(cx1, cy1), f(cx0, cy1));
            let mut pts: Vec<(f32, f32)> = Vec::new();
            let mut edge = |fa: f32, fb: f32, ax: f32, ay: f32, bx: f32, by: f32| {
                if (fa > 0.0) != (fb > 0.0) && (fa - fb).abs() > 1e-12 {
                    let t = fa / (fa - fb);
                    pts.push((lerp(t, ax, bx), lerp(t, ay, by)));
                }
            };
            edge(bl, br, cx0, cy0, cx1, cy0); // bottom
            edge(br, tr, cx1, cy0, cx1, cy1); // right
            edge(tr, tl, cx1, cy1, cx0, cy1); // top
            edge(tl, bl, cx0, cy1, cx0, cy0); // left
            match pts.len() {
                2 => segs.push([pts[0], pts[1]]),
                4 => {
                    // Saddle: connect the crossings pairwise.
                    segs.push([pts[0], pts[1]]);
                    segs.push([pts[2], pts[3]]);
                }
                _ => {}
            }
        }
    }
    segs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::{Mobject, MobjectExt};
    use manim_math::PI;

    fn unit_cs() -> CoordSystem {
        // 1 scene unit per data unit on both axes.
        CoordSystem::new([-4.0, 4.0, 1.0], [-2.0, 2.0, 1.0], 8.0, 4.0)
    }

    #[test]
    fn implicit_unit_circle_points_lie_on_curve() {
        let cs = CoordSystem::new([-2.0, 2.0, 1.0], [-2.0, 2.0, 1.0], 4.0, 4.0);
        let f = |x: f32, y: f32| x * x + y * y - 1.0;
        let curve = cs.plot_implicit_curve(f, Some(60));
        let path = &curve.data().path;
        assert!(!path.subpaths.is_empty(), "circle should produce segments");
        // Every emitted vertex satisfies |f| < eps (map back to data coords).
        for sp in &path.subpaths {
            for c in &sp.curves {
                for p in [c.p0, c.p3] {
                    let (x, y) = cs.point_to_coords(p);
                    assert!(f(x, y).abs() < 5e-2, "|f|={} at ({x},{y})", f(x, y).abs());
                }
            }
        }
    }

    #[test]
    fn implicit_hyperbola_has_two_branches() {
        // x^2 - y^2 - 1 = 0: two disjoint branches (x <= -1 and x >= 1).
        let cs = CoordSystem::new([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0], 6.0, 6.0);
        let curve = cs.plot_implicit_curve(|x, y| x * x - y * y - 1.0, Some(80));
        let mut left = false;
        let mut right = false;
        for sp in &curve.data().path.subpaths {
            for c in &sp.curves {
                let (x, _) = cs.point_to_coords(c.p0);
                if x < 0.0 {
                    left = true;
                } else {
                    right = true;
                }
            }
        }
        assert!(left && right, "hyperbola should have both branches");
    }

    #[test]
    fn plot_samples_lie_on_the_curve() {
        let cs = unit_cs();
        let g = cs.plot(|x| x.sin(), Some([0.0, PI, 0.1]));
        let sp = &g.data().path.subpaths[0];
        // Endpoints map (0, 0) and (π, 0).
        let start = sp.curves.first().unwrap().p0;
        let end = sp.curves.last().unwrap().p3;
        assert!((start - cs.coords_to_point(0.0, 0.0)).length() < 1e-3);
        assert!((end - cs.coords_to_point(PI, 0.0)).length() < 1e-2);
        // The peak reaches sin = 1 → y == 1 * y_unit.
        let max_y = g
            .data()
            .path
            .subpaths
            .iter()
            .flat_map(|s| s.curves.iter())
            .flat_map(|c| [c.p0.y, c.p3.y])
            .fold(f32::MIN, f32::max);
        assert!((max_y - cs.coords_to_point(0.0, 1.0).y).abs() < 1e-2);
    }

    #[test]
    fn discontinuity_splits_into_branches() {
        let cs = CoordSystem::new([-2.0, 2.0, 1.0], [-4.0, 4.0, 1.0], 4.0, 8.0);
        // 1/x jumps from -∞ to +∞ across 0 → at least two subpaths.
        let g = cs.plot(|x| 1.0 / x, Some([-2.0, 2.0, 0.05]));
        assert!(
            g.data().path.subpaths.len() >= 2,
            "expected a split, got {}",
            g.data().path.subpaths.len()
        );
    }

    #[test]
    fn area_polygon_closes_and_spans_range() {
        let cs = CoordSystem::new([0.0, 4.0, 1.0], [0.0, 4.0, 1.0], 4.0, 4.0);
        let g = cs.plot(|x| x, None);
        let area = cs.get_area(&g, 0.0, 4.0, 0.5);
        let sp = &area.data().path.subpaths[0];
        assert!(sp.closed);
        // Spans the full x extent (0..4 → 4 scene units wide).
        assert!((area.bounding_box().width() - 4.0).abs() < 0.2);
    }

    #[test]
    fn riemann_rectangle_count_and_width() {
        let cs = CoordSystem::new([0.0, 4.0, 1.0], [0.0, 16.0, 4.0], 4.0, 4.0);
        let g = cs.plot(|x| x * x, None);
        let rects = cs.get_riemann_rectangles(&g, 0.0, 4.0, 0.5, 0.6);
        assert_eq!(rects.data().path.subpaths.len(), 8); // 4 / 0.5
                                                         // Each rectangle is one scene-unit-derived width (0.5 data * x_unit 1).
        let sp = &rects.data().path.subpaths[0];
        let xs: Vec<f32> = sp.curves.iter().map(|c| c.p0.x).collect();
        let width = xs.iter().cloned().fold(f32::MIN, f32::max)
            - xs.iter().cloned().fold(f32::MAX, f32::min);
        assert!((width - 0.5).abs() < 1e-4);
    }

    #[test]
    fn input_to_graph_point_matches_c2p() {
        let cs = unit_cs();
        let g = cs.plot(|x| x * x, None);
        let p = cs.input_to_graph_point(1.5, &g);
        assert!((p - cs.coords_to_point(1.5, 2.25)).length() < 1e-5);
    }
}
