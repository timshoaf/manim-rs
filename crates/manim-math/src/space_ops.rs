//! Spatial operations, ported from `manim.utils.space_ops`.
//!
//! Rotation and angle helpers, vector intersections, and regular-polygon
//! vertex generation. Rotations are expressed with [`glam::Mat3`]; angles are
//! measured in radians in manim's convention (`+x` is angle 0, counter-clockwise
//! positive).

use crate::{Point, OUT, RIGHT, TAU};
use glam::Mat3;

/// A rotation matrix of `angle` radians about `axis`.
///
/// Ports manim CE's `rotation_matrix`. The axis is normalized internally; a
/// zero axis yields the identity.
///
/// ```
/// use manim_math::space_ops::rotation_matrix;
/// use manim_math::{OUT, RIGHT, UP};
/// use std::f32::consts::FRAC_PI_2;
/// let m = rotation_matrix(FRAC_PI_2, OUT);
/// // Rotating RIGHT by 90° about OUT gives UP.
/// assert!((m * RIGHT - UP).length() < 1e-6);
/// ```
pub fn rotation_matrix(angle: f32, axis: Point) -> Mat3 {
    let normalized = axis.normalize_or_zero();
    if normalized == Point::ZERO {
        Mat3::IDENTITY
    } else {
        Mat3::from_axis_angle(normalized, angle)
    }
}

/// Rotate `vector` by `angle` radians about the `OUT` (`+z`) axis.
///
/// Ports manim CE's `rotate_vector` with its default `axis = OUT`.
///
/// ```
/// use manim_math::space_ops::rotate_vector;
/// use manim_math::{RIGHT, UP};
/// use std::f32::consts::FRAC_PI_2;
/// assert!((rotate_vector(RIGHT, FRAC_PI_2) - UP).length() < 1e-6);
/// ```
pub fn rotate_vector(vector: Point, angle: f32) -> Point {
    rotation_matrix(angle, OUT) * vector
}

/// The polar angle of `vector` projected onto the xy-plane, in `(-π, π]`.
///
/// Ports manim CE's `angle_of_vector` (`atan2(y, x)`).
///
/// ```
/// use manim_math::space_ops::angle_of_vector;
/// use manim_math::UP;
/// use std::f32::consts::FRAC_PI_2;
/// assert!((angle_of_vector(UP) - FRAC_PI_2).abs() < 1e-6);
/// ```
pub fn angle_of_vector(vector: Point) -> f32 {
    vector.y.atan2(vector.x)
}

/// The (unsigned) angle between two vectors, in `[0, π]`.
///
/// Ports manim CE's `angle_between_vectors`, using the numerically stable
/// `2 * atan2(‖n1 - n2‖, ‖n1 + n2‖)` form on the normalized inputs.
///
/// ```
/// use manim_math::space_ops::angle_between_vectors;
/// use manim_math::{RIGHT, UP};
/// use std::f32::consts::FRAC_PI_2;
/// assert!((angle_between_vectors(RIGHT, UP) - FRAC_PI_2).abs() < 1e-6);
/// ```
pub fn angle_between_vectors(a: Point, b: Point) -> f32 {
    let na = normalize_or_zero(a);
    let nb = normalize_or_zero(b);
    2.0 * (na - nb).length().atan2((na + nb).length())
}

/// Normalize `v` to unit length, returning zero for a zero-length input.
///
/// Ports manim CE's `normalize` (with the default zero fall-back).
///
/// ```
/// use manim_math::space_ops::normalize_or_zero;
/// use manim_math::Point;
/// assert_eq!(normalize_or_zero(Point::new(3.0, 0.0, 0.0)), Point::new(1.0, 0.0, 0.0));
/// assert_eq!(normalize_or_zero(Point::ZERO), Point::ZERO);
/// ```
pub fn normalize_or_zero(v: Point) -> Point {
    v.normalize_or_zero()
}

/// The 2D cross product (`z`-component of the 3D cross) of `a` and `b`.
///
/// Ports manim CE's `cross2d`.
///
/// ```
/// use manim_math::space_ops::cross2d;
/// use manim_math::{RIGHT, UP};
/// assert_eq!(cross2d(RIGHT, UP), 1.0);
/// ```
pub fn cross2d(a: Point, b: Point) -> f32 {
    a.x * b.y - a.y * b.x
}

/// The midpoint of `a` and `b`.
///
/// Ports manim CE's `midpoint`.
///
/// ```
/// use manim_math::space_ops::midpoint;
/// use manim_math::Point;
/// assert_eq!(
///     midpoint(Point::ZERO, Point::new(2.0, 4.0, 0.0)),
///     Point::new(1.0, 2.0, 0.0),
/// );
/// ```
pub fn midpoint(a: Point, b: Point) -> Point {
    (a + b) * 0.5
}

/// The intersection of two lines, each given by a pair of points on it.
///
/// Ports manim CE's `line_intersection` (an xy-plane cross-product algorithm).
/// Returns `None` when the lines are parallel instead of raising.
///
/// ```
/// use manim_math::space_ops::line_intersection;
/// use manim_math::Point;
/// let l1 = (Point::new(-1.0, 0.0, 0.0), Point::new(1.0, 0.0, 0.0)); // x-axis
/// let l2 = (Point::new(0.0, -1.0, 0.0), Point::new(0.0, 1.0, 0.0)); // y-axis
/// assert_eq!(line_intersection(l1, l2), Some(Point::ZERO));
/// ```
pub fn line_intersection(l1: (Point, Point), l2: (Point, Point)) -> Option<Point> {
    let homog = |p: Point| Point::new(p.x, p.y, 1.0);
    let line1 = homog(l1.0).cross(homog(l1.1));
    let line2 = homog(l2.0).cross(homog(l2.1));
    let inter = line1.cross(line2);
    if inter.z.abs() < 1e-9 {
        None
    } else {
        Some(Point::new(inter.x / inter.z, inter.y / inter.z, 0.0))
    }
}

/// The intersection of two rays given in point-direction form.
///
/// Ports manim CE's `find_intersection`: ray `i` passes through `p_i` with
/// direction `v_i`. Returns `None` when the directions are (near) parallel.
///
/// ```
/// use manim_math::space_ops::find_intersection;
/// use manim_math::{Point, RIGHT, UP};
/// let hit = find_intersection(Point::new(0.0, -2.0, 0.0), UP, Point::new(-2.0, 0.0, 0.0), RIGHT);
/// assert_eq!(hit, Some(Point::ZERO));
/// ```
pub fn find_intersection(p0: Point, v0: Point, p1: Point, v1: Point) -> Option<Point> {
    let cross = v0.cross(v1);
    if cross.length_squared() < 1e-12 {
        return None;
    }
    let normal = v1.cross(cross);
    let denom = v0.dot(normal);
    if denom.abs() < 1e-9 {
        return None;
    }
    Some(p0 + v0 * ((p1 - p0).dot(normal) / denom))
}

/// The perpendicular bisector of the segment `line`, as two points spanning it.
///
/// Ports manim CE's `perpendicular_bisector` (bisecting in the xy-plane).
///
/// ```
/// use manim_math::space_ops::perpendicular_bisector;
/// use manim_math::Point;
/// let (a, b) = perpendicular_bisector((Point::new(-1.0, 0.0, 0.0), Point::new(1.0, 0.0, 0.0)));
/// // The bisector of a horizontal segment is vertical and passes through the origin.
/// assert!((a.x).abs() < 1e-6 && (b.x).abs() < 1e-6);
/// assert!((a + b).length() < 1e-6);
/// ```
pub fn perpendicular_bisector(line: (Point, Point)) -> (Point, Point) {
    let (p1, p2) = line;
    let direction = (p1 - p2).cross(OUT);
    let m = midpoint(p1, p2);
    (m + direction, m - direction)
}

/// `n` unit-length directions spaced evenly around the circle, starting at
/// `start`.
///
/// Ports manim CE's `compass_directions`.
///
/// ```
/// use manim_math::space_ops::compass_directions;
/// use manim_math::RIGHT;
/// let dirs = compass_directions(4, RIGHT);
/// assert_eq!(dirs.len(), 4);
/// // The four cardinal directions, starting at RIGHT.
/// assert!((dirs[1] - manim_math::UP).length() < 1e-6);
/// ```
pub fn compass_directions(n: usize, start: Point) -> Vec<Point> {
    let angle = TAU / n as f32;
    (0..n)
        .map(|k| rotate_vector(start, k as f32 * angle))
        .collect()
}

/// The vertices of a regular `n`-gon of the given `radius`, plus the resolved
/// starting angle.
///
/// Ports manim CE's `regular_vertices`. When `start_angle` is `None` it defaults
/// as manim does: `0` for even `n`, `τ/4` (pointing up) for odd `n`.
///
/// ```
/// use manim_math::space_ops::regular_vertices;
/// let (verts, start) = regular_vertices(3, 1.0, None);
/// assert_eq!(verts.len(), 3);
/// // An odd polygon points straight up by default.
/// assert!((verts[0] - manim_math::UP).length() < 1e-6);
/// assert!((start - manim_math::TAU / 4.0).abs() < 1e-6);
/// ```
pub fn regular_vertices(n: usize, radius: f32, start_angle: Option<f32>) -> (Vec<Point>, f32) {
    let start_angle = start_angle.unwrap_or(if n % 2 == 0 { 0.0 } else { TAU / 4.0 });
    let start_vector = rotate_vector(RIGHT * radius, start_angle);
    (compass_directions(n, start_vector), start_angle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PI, UP};
    use approx::assert_relative_eq;

    #[test]
    fn rotation_matrix_is_orthonormal() {
        let m = rotation_matrix(0.9, Point::new(1.0, 2.0, 3.0));
        // Columns are orthonormal and determinant is +1.
        assert_relative_eq!(m.determinant(), 1.0, epsilon = 1e-5);
        let prod = m * m.transpose();
        let diff = prod - Mat3::IDENTITY;
        let max_off = diff
            .x_axis
            .abs()
            .max_element()
            .max(diff.y_axis.abs().max_element())
            .max(diff.z_axis.abs().max_element());
        assert_relative_eq!(max_off, 0.0, epsilon = 1e-5);
    }

    #[test]
    fn angle_helpers_match_glam() {
        for i in 0..8 {
            let a = i as f32 * PI / 4.0;
            let v = rotate_vector(RIGHT, a);
            assert_relative_eq!(
                angle_of_vector(v).rem_euclid(TAU),
                a.rem_euclid(TAU),
                epsilon = 1e-5
            );
        }
        assert_relative_eq!(angle_between_vectors(RIGHT, UP), PI / 2.0, epsilon = 1e-6);
        assert_relative_eq!(angle_between_vectors(RIGHT, RIGHT), 0.0, epsilon = 1e-6);
    }

    #[test]
    fn parallel_lines_have_no_intersection() {
        let l1 = (Point::new(0.0, 0.0, 0.0), Point::new(1.0, 0.0, 0.0));
        let l2 = (Point::new(0.0, 1.0, 0.0), Point::new(1.0, 1.0, 0.0));
        assert_eq!(line_intersection(l1, l2), None);
    }

    #[test]
    fn regular_vertices_even_lies_on_axis() {
        let (verts, start) = regular_vertices(4, 2.0, None);
        assert_relative_eq!(start, 0.0);
        assert_relative_eq!(verts[0].x, 2.0, epsilon = 1e-6);
        assert_relative_eq!(verts[0].y, 0.0, epsilon = 1e-6);
        for v in &verts {
            assert_relative_eq!(v.length(), 2.0, epsilon = 1e-5);
        }
    }
}
