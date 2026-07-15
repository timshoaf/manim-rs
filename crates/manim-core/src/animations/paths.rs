//! Transform path functions, ported from manim CE's `utils.paths`.
//!
//! A [`PathFn`] maps `(start, end, alpha)` to a point, describing the curve each
//! control point follows during a [`Transform`](crate::animations::Transform).
//! Wire one in with `.path_fn(...)` or `.path_arc(angle)`.

use std::sync::Arc;

use manim_math::space_ops::rotation_matrix;
use manim_math::{Point, OUT};

use crate::animation::PathFn;

/// Straight-line interpolation (manim's `straight_path`), the default.
///
/// ```
/// use manim_core::animations::paths::straight_path;
/// use manim_math::{Point, RIGHT};
/// let p = straight_path();
/// assert!((p(Point::ZERO, 2.0 * RIGHT, 0.5) - RIGHT).length() < 1e-6);
/// ```
pub fn straight_path() -> PathFn {
    Arc::new(|start: Point, end: Point, alpha: f32| start + (end - start) * alpha)
}

/// Moves each point along a circular arc subtending `arc_angle` radians about
/// the `OUT` axis (manim's `path_along_arc`). A near-zero angle degrades to a
/// straight line.
///
/// ```
/// use manim_core::animations::paths::path_along_arc;
/// use manim_math::{Point, RIGHT, TAU};
/// let p = path_along_arc(TAU / 2.0);
/// // Endpoints are exact regardless of the arc.
/// assert!((p(Point::ZERO, 2.0 * RIGHT, 0.0)).length() < 1e-6);
/// assert!((p(Point::ZERO, 2.0 * RIGHT, 1.0) - 2.0 * RIGHT).length() < 1e-6);
/// ```
pub fn path_along_arc(arc_angle: f32) -> PathFn {
    if arc_angle.abs() < 1e-3 {
        return straight_path();
    }
    Arc::new(move |start: Point, end: Point, alpha: f32| {
        let vect = end - start;
        let mut center = start + vect * 0.5;
        if (arc_angle - std::f32::consts::PI).abs() > 1e-6 {
            // Offset the center perpendicular to the chord.
            let perp = OUT.cross(vect * 0.5);
            center += perp / (arc_angle / 2.0).tan();
        }
        let rot = rotation_matrix(arc_angle * alpha, OUT);
        center + rot * (start - center)
    })
}

/// A clockwise half-turn arc (manim's `clockwise_path`).
///
/// ```
/// use manim_core::animations::paths::clockwise_path;
/// use manim_math::{Point, RIGHT};
/// let _ = clockwise_path()(Point::ZERO, RIGHT, 0.5);
/// ```
pub fn clockwise_path() -> PathFn {
    path_along_arc(-std::f32::consts::PI)
}

/// A counter-clockwise half-turn arc (manim's `counterclockwise_path`).
///
/// ```
/// use manim_core::animations::paths::counterclockwise_path;
/// use manim_math::{Point, RIGHT};
/// let _ = counterclockwise_path()(Point::ZERO, RIGHT, 0.5);
/// ```
pub fn counterclockwise_path() -> PathFn {
    path_along_arc(std::f32::consts::PI)
}

/// A spiral path: the straight interpolation rotated by a decaying angle about
/// the end point (an approximation of manim's `spiral_path`). A near-zero angle
/// degrades to a straight line; endpoints are exact.
///
/// ```
/// use manim_core::animations::paths::spiral_path;
/// use manim_math::{Point, RIGHT, TAU};
/// let p = spiral_path(TAU);
/// assert!((p(Point::ZERO, 2.0 * RIGHT, 0.0)).length() < 1e-6);
/// assert!((p(Point::ZERO, 2.0 * RIGHT, 1.0) - 2.0 * RIGHT).length() < 1e-6);
/// ```
pub fn spiral_path(angle: f32) -> PathFn {
    if angle.abs() < 1e-3 {
        return straight_path();
    }
    Arc::new(move |start: Point, end: Point, alpha: f32| {
        let straight = start + (end - start) * alpha;
        // Rotate the in-flight point about the destination by a vanishing angle.
        let rot = rotation_matrix(angle * (alpha - 1.0), OUT);
        end + rot * (straight - end)
    })
}
