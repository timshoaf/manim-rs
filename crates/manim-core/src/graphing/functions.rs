//! Plotted curves as mobjects: [`ParametricFunction`] and [`FunctionGraph`].

use std::sync::Arc;

use manim_color::{Color, WHITE};
use manim_math::path::{Path, SubPath};
use manim_math::Point;

use crate::impl_mobject;
use crate::mobject::MobjectData;
use crate::style::Style;

/// A `t → Point` sampler, shared so the mobject stays `Clone`.
pub(crate) type PointFn = Arc<dyn Fn(f32) -> Point + Send + Sync>;
/// A `x → y` sampler.
pub(crate) type ScalarFn = Arc<dyn Fn(f32) -> f32 + Send + Sync>;

/// A parametric curve `t ↦ (x(t), y(t))` sampled into a path. Port of manim
/// CE's `ParametricFunction`.
///
/// ```
/// use manim_core::graphing::ParametricFunction;
/// use manim_math::Point;
/// // A unit circle.
/// let circle = ParametricFunction::new(
///     |t| Point::new(t.cos(), t.sin(), 0.0),
///     0.0,
///     std::f32::consts::TAU,
///     0.05,
/// );
/// // A sampled point lands on the curve.
/// assert!((circle.evaluate(0.0) - Point::new(1.0, 0.0, 0.0)).length() < 1e-6);
/// ```
#[derive(Clone)]
pub struct ParametricFunction {
    data: MobjectData,
    func: PointFn,
    t_min: f32,
    t_max: f32,
}
impl_mobject!(ParametricFunction);

impl ParametricFunction {
    /// Samples `func` over `[t_min, t_max]` at spacing `t_step` into an open
    /// polyline path.
    pub fn new(
        func: impl Fn(f32) -> Point + Send + Sync + 'static,
        t_min: f32,
        t_max: f32,
        t_step: f32,
    ) -> Self {
        let func: PointFn = Arc::new(func);
        let path = sample_path(&func, t_min, t_max, t_step);
        Self {
            data: MobjectData::new(path, Style::stroked(WHITE)),
            func,
            t_min,
            t_max,
        }
    }

    /// The curve point at parameter `t`.
    pub fn evaluate(&self, t: f32) -> Point {
        (self.func)(t)
    }

    /// The parameter range `(t_min, t_max)`.
    pub fn t_range(&self) -> (f32, f32) {
        (self.t_min, self.t_max)
    }
}

/// Samples `func` uniformly (endpoints included) into one open subpath.
fn sample_path(func: &PointFn, t_min: f32, t_max: f32, t_step: f32) -> Path {
    let step = if t_step > 0.0 {
        t_step
    } else {
        (t_max - t_min).abs().max(1e-3)
    };
    let n = (((t_max - t_min) / step).ceil() as i64).max(1);
    let mut pts = Vec::with_capacity(n as usize + 1);
    for i in 0..=n {
        let t = (t_min + i as f32 * step).min(t_max);
        pts.push(func(t));
        if t >= t_max {
            break;
        }
    }
    Path {
        subpaths: vec![SubPath::from_corners(&pts)],
    }
}

/// The graph of a scalar function `y = f(x)` in raw scene coordinates (each
/// data point `(x, f(x))` is a scene point). Port of manim CE's `FunctionGraph`.
///
/// [`Axes::plot`](crate::graphing::Axes::plot) produces a `FunctionGraph` whose
/// path is instead mapped through the axes' coordinate system.
///
/// ```
/// use manim_core::graphing::FunctionGraph;
/// let parabola = FunctionGraph::new(|x| x * x, -2.0, 2.0, 0.05);
/// assert!((parabola.evaluate(2.0) - 4.0).abs() < 1e-6);
/// ```
#[derive(Clone)]
pub struct FunctionGraph {
    data: MobjectData,
    func: ScalarFn,
    x_min: f32,
    x_max: f32,
}
impl_mobject!(FunctionGraph);

impl FunctionGraph {
    /// Graphs `func` over `[x_min, x_max]` at spacing `x_step`, plotting the
    /// point `(x, f(x))` directly in scene space.
    pub fn new(
        func: impl Fn(f32) -> f32 + Send + Sync + 'static,
        x_min: f32,
        x_max: f32,
        x_step: f32,
    ) -> Self {
        let func: ScalarFn = Arc::new(func);
        let runs = super::sample_runs(
            &|x| func(x),
            x_min,
            x_max,
            x_step,
            f32::INFINITY, // raw graphs: no curvature refinement threshold
            f32::INFINITY, // and no discontinuity splitting
        );
        let path = runs_to_path(&runs, |x, y| Point::new(x, y, 0.0));
        Self::from_parts(path, func, x_min, x_max)
    }

    /// Builds a graph from a pre-mapped `path` (used by `Axes::plot`, which maps
    /// through its coordinate system) while retaining the underlying function.
    pub(crate) fn from_parts(path: Path, func: ScalarFn, x_min: f32, x_max: f32) -> Self {
        Self {
            data: MobjectData::new(path, Style::stroked(WHITE)),
            func,
            x_min,
            x_max,
        }
    }

    /// The value `f(x)` of the underlying function.
    pub fn evaluate(&self, x: f32) -> f32 {
        (self.func)(x)
    }

    /// A clone of the underlying function handle.
    pub fn function(&self) -> ScalarFn {
        Arc::clone(&self.func)
    }

    /// The input range `(x_min, x_max)`.
    pub fn x_range(&self) -> (f32, f32) {
        (self.x_min, self.x_max)
    }
}

/// Maps runs of `(x, y)` samples through `map` into a multi-subpath open path.
pub(crate) fn runs_to_path(runs: &[Vec<(f32, f32)>], map: impl Fn(f32, f32) -> Point) -> Path {
    let subpaths = runs
        .iter()
        .filter(|r| r.len() >= 2)
        .map(|r| {
            let pts: Vec<Point> = r.iter().map(|&(x, y)| map(x, y)).collect();
            SubPath::from_corners(&pts)
        })
        .collect();
    Path { subpaths }
}

/// Applies the graph's default stroke color.
pub(crate) fn with_color(mut mobj: FunctionGraph, color: Color) -> FunctionGraph {
    mobj.data.style.stroke_color = Some(color);
    mobj
}

/// The zero set of `f(x, y) = 0`, traced by marching squares into line-segment
/// subpaths. Port of manim CE's `ImplicitFunction`.
#[derive(Clone)]
pub struct ImplicitFunction {
    data: MobjectData,
}
impl_mobject!(ImplicitFunction);

impl ImplicitFunction {
    /// Builds an implicit-curve mobject from an already-traced segment path.
    pub(crate) fn from_path(path: Path) -> Self {
        Self {
            data: MobjectData::new(path, Style::stroked(WHITE)),
        }
    }
}
