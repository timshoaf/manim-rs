//! Coordinate systems and plotting: number lines, axes, planes, and graphs.
//!
//! Port of manim CE's `mobject.graphing`. Everything here is **geometry only** —
//! the shapes of axes, ticks, grids, and plotted curves. Numeric *labels* on
//! ticks and axes require `DecimalNumber` / text (M4), so this module builds the
//! label **attachment points** (e.g. [`NumberLine::number_label_point`]) but
//! defers rendering the text itself. That is the one broad deferral; see each
//! type's docs.
//!
//! # Contents
//!
//! - [`NumberLine`] — a 1-D axis with ticks and an optional tip.
//! - [`Axes`] — a 2-D coordinate system with `coords_to_point` / `plot` / area /
//!   Riemann rectangles.
//! - [`NumberPlane`], [`ComplexPlane`], [`PolarPlane`] — axes with a background
//!   grid / polar grid.
//! - [`ParametricFunction`], [`FunctionGraph`] — plotted curves as mobjects.
//! - [`CoordSystem`] — the shared coordinate math (`c2p` / `p2c` / plotting)
//!   backing [`Axes`] and the planes.
//!
//! ```
//! use manim_core::graphing::Axes;
//! use manim_core::mobject::Mobject;
//! let axes = Axes::new([-5.0, 5.0, 1.0], [-3.0, 3.0, 1.0]);
//! // Data coordinates map to scene points and back.
//! let p = axes.coords_to_point(2.0, 1.0);
//! let (x, y) = axes.point_to_coords(p);
//! assert!((x - 2.0).abs() < 1e-4 && (y - 1.0).abs() < 1e-4);
//! let _ = axes.data(); // it is a mobject
//! ```

use manim_math::path::SubPath;
use manim_math::Point;

mod axes;
mod bar_chart;
mod coord;
mod functions;
mod number_line;
mod plane;

pub use axes::Axes;
pub use bar_chart::{default_bar_colors, BarChart};
pub use coord::{CoordSystem, DEFAULT_IMPLICIT_RESOLUTION};
pub use functions::{FunctionGraph, ImplicitFunction, ParametricFunction};
pub use number_line::NumberLine;
pub use plane::{ComplexPlane, NumberPlane, PolarPlane};

/// A closed polygon subpath through `points`.
pub(crate) fn closed_polygon(points: &[Point]) -> SubPath {
    let mut sp = SubPath::from_corners(points);
    sp.closed = true;
    sp
}

/// Recursively refines the interval `[x0, x1]` of `f`, inserting the midpoint
/// (in order) wherever the curve bows away from the chord by more than `tol`.
#[allow(clippy::too_many_arguments)]
fn refine(
    f: &dyn Fn(f32) -> f32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    depth: u32,
    tol: f32,
    out: &mut Vec<(f32, f32)>,
) {
    if depth == 0 {
        return;
    }
    let xm = 0.5 * (x0 + x1);
    let ym = f(xm);
    let chord_mid = 0.5 * (y0 + y1);
    if ym.is_finite() && (chord_mid - ym).abs() > tol {
        refine(f, x0, y0, xm, ym, depth - 1, tol, out);
        out.push((xm, ym));
        refine(f, xm, ym, x1, y1, depth - 1, tol, out);
    }
}

/// Adaptively samples `f` over `[x_min, x_max]` and splits the result into
/// continuous runs, breaking wherever the value is non-finite or jumps by more
/// than `jump` (a discontinuity like `1/x`). Returned points are in the domain's
/// own units.
///
/// `step` sets the base grid; each cell is then refined by [`refine`] up to
/// `MAX_DEPTH` levels where curvature is high (adaptive sampling).
pub(crate) fn sample_runs(
    f: &dyn Fn(f32) -> f32,
    x_min: f32,
    x_max: f32,
    step: f32,
    tol: f32,
    jump: f32,
) -> Vec<Vec<(f32, f32)>> {
    const MAX_DEPTH: u32 = 6;
    let step = if step > 0.0 {
        step
    } else {
        (x_max - x_min).abs().max(1e-3)
    };
    let n = (((x_max - x_min) / step).ceil() as i64).max(1);

    let mut pts: Vec<(f32, f32)> = Vec::new();
    let y0 = f(x_min);
    pts.push((x_min, y0));
    let (mut px, mut py) = (x_min, y0);
    for i in 1..=n {
        let x = (x_min + i as f32 * step).min(x_max);
        let y = f(x);
        refine(f, px, py, x, y, MAX_DEPTH, tol, &mut pts);
        pts.push((x, y));
        px = x;
        py = y;
        if x >= x_max {
            break;
        }
    }

    let mut runs: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    let mut last_y: Option<f32> = None;
    for (x, y) in pts {
        if !y.is_finite() {
            if !cur.is_empty() {
                runs.push(std::mem::take(&mut cur));
            }
            last_y = None;
            continue;
        }
        if let Some(ly) = last_y {
            if (y - ly).abs() > jump {
                runs.push(std::mem::take(&mut cur));
            }
        }
        cur.push((x, y));
        last_y = Some(y);
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs
}
