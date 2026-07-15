//! Cubic Bézier curves and interpolation, ported from `manim.utils.bezier`.
//!
//! The [`CubicBezier`] type mirrors manim CE's per-curve operations
//! (`partial_bezier_points`, splitting, arc length), and the free functions
//! [`interpolate`], [`inverse_interpolate`], [`match_interpolate`], and
//! [`integer_interpolate`] mirror manim's interpolation helpers.
//! [`smooth_cubic_handles`] ports `get_smooth_cubic_bezier_handle_points`.

use crate::Point;
use core::ops::{Add, Mul};

/// Linearly interpolate between `start` and `end` by `alpha`.
///
/// Ports manim CE's `interpolate`: returns `(1 - alpha) * start + alpha * end`.
/// Generic over anything scalable by `f32` and addable, i.e. both `f32` and
/// [`Point`].
///
/// ```
/// use manim_math::bezier::interpolate;
/// use manim_math::Point;
/// assert_eq!(interpolate(0.0_f32, 10.0, 0.25), 2.5);
/// assert_eq!(
///     interpolate(Point::ZERO, Point::new(4.0, 0.0, 0.0), 0.5),
///     Point::new(2.0, 0.0, 0.0),
/// );
/// ```
pub fn interpolate<T>(start: T, end: T, alpha: f32) -> T
where
    T: Copy + Add<Output = T> + Mul<f32, Output = T>,
{
    start * (1.0 - alpha) + end * alpha
}

/// Inverse of [`interpolate`]: the `alpha` that maps `start`..`end` to `value`.
///
/// Ports manim CE's `inverse_interpolate` for scalar values.
///
/// ```
/// use manim_math::bezier::inverse_interpolate;
/// assert_eq!(inverse_interpolate(0.0, 10.0, 2.5), 0.25);
/// ```
pub fn inverse_interpolate(start: f32, end: f32, value: f32) -> f32 {
    (value - start) / (end - start)
}

/// Remap `old_value` from the range `old_start`..`old_end` onto the range
/// `new_start`..`new_end`.
///
/// Ports manim CE's `match_interpolate`.
///
/// ```
/// use manim_math::bezier::match_interpolate;
/// // 5 sits halfway in 0..10, so it maps to halfway in 100..200.
/// assert_eq!(match_interpolate(100.0, 200.0, 0.0, 10.0, 5.0), 150.0);
/// ```
pub fn match_interpolate(
    new_start: f32,
    new_end: f32,
    old_start: f32,
    old_end: f32,
    old_value: f32,
) -> f32 {
    interpolate(
        new_start,
        new_end,
        inverse_interpolate(old_start, old_end, old_value),
    )
}

/// Interpolate over integer indices, returning the integer part and residue.
///
/// Ports manim CE's `integer_interpolate`, used to index into submobject
/// families. Returns `(value, residue)` where `value` is the integer reached
/// and `residue` is the fractional progress toward the next integer.
///
/// ```
/// use manim_math::bezier::integer_interpolate;
/// let (value, residue) = integer_interpolate(0, 10, 0.55);
/// assert_eq!(value, 5);
/// assert!((residue - 0.5).abs() < 1e-6);
/// ```
pub fn integer_interpolate(start: i32, end: i32, alpha: f32) -> (i32, f32) {
    if alpha >= 1.0 {
        return (end - 1, 1.0);
    }
    if alpha <= 0.0 {
        return (start, 0.0);
    }
    let value = interpolate(start as f32, end as f32, alpha) as i32;
    let residue = ((end - start) as f32 * alpha).rem_euclid(1.0);
    (value, residue)
}

/// A cubic Bézier curve defined by an anchor, two handles, and an anchor.
///
/// The control points follow manim's convention: `p0` and `p3` are on-curve
/// anchors, `p1` and `p2` are off-curve handles.
///
/// ```
/// use manim_math::bezier::CubicBezier;
/// use manim_math::Point;
/// let curve = CubicBezier::line(Point::ZERO, Point::new(3.0, 0.0, 0.0));
/// assert_eq!(curve.eval(0.5), Point::new(1.5, 0.0, 0.0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    /// The starting on-curve anchor.
    pub p0: Point,
    /// The first off-curve handle.
    pub p1: Point,
    /// The second off-curve handle.
    pub p2: Point,
    /// The ending on-curve anchor.
    pub p3: Point,
}

impl CubicBezier {
    /// Construct a curve directly from its four control points.
    ///
    /// ```
    /// use manim_math::bezier::CubicBezier;
    /// use manim_math::Point;
    /// let c = CubicBezier::new(Point::ZERO, Point::X, Point::Y, Point::ONE);
    /// assert_eq!(c.p0, Point::ZERO);
    /// ```
    pub fn new(p0: Point, p1: Point, p2: Point, p3: Point) -> Self {
        Self { p0, p1, p2, p3 }
    }

    /// Construct the cubic representing the straight segment `a`..`b`, with
    /// handles placed at the one-third and two-thirds points (manim's
    /// convention for `set_points_as_corners`).
    ///
    /// ```
    /// use manim_math::bezier::CubicBezier;
    /// use manim_math::Point;
    /// let c = CubicBezier::line(Point::ZERO, Point::new(3.0, 0.0, 0.0));
    /// assert_eq!(c.p1, Point::new(1.0, 0.0, 0.0));
    /// assert_eq!(c.p2, Point::new(2.0, 0.0, 0.0));
    /// ```
    pub fn line(a: Point, b: Point) -> Self {
        Self {
            p0: a,
            p1: interpolate(a, b, 1.0 / 3.0),
            p2: interpolate(a, b, 2.0 / 3.0),
            p3: b,
        }
    }

    /// Evaluate the curve at parameter `t` (typically in `[0, 1]`).
    ///
    /// ```
    /// use manim_math::bezier::CubicBezier;
    /// use manim_math::Point;
    /// let c = CubicBezier::line(Point::ZERO, Point::new(4.0, 0.0, 0.0));
    /// assert_eq!(c.eval(0.0), Point::ZERO);
    /// assert_eq!(c.eval(1.0), Point::new(4.0, 0.0, 0.0));
    /// ```
    pub fn eval(&self, t: f32) -> Point {
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;
        let t2 = t * t;
        let t3 = t2 * t;
        self.p0 * mt3 + self.p1 * (3.0 * t * mt2) + self.p2 * (3.0 * t2 * mt) + self.p3 * t3
    }

    /// The first derivative (velocity) of the curve at parameter `t`.
    ///
    /// ```
    /// use manim_math::bezier::CubicBezier;
    /// use manim_math::Point;
    /// // A straight unit segment has constant velocity equal to its length.
    /// let c = CubicBezier::line(Point::ZERO, Point::new(3.0, 0.0, 0.0));
    /// assert_eq!(c.derivative(0.5), Point::new(3.0, 0.0, 0.0));
    /// ```
    pub fn derivative(&self, t: f32) -> Point {
        let mt = 1.0 - t;
        let d0 = self.p1 - self.p0;
        let d1 = self.p2 - self.p1;
        let d2 = self.p3 - self.p2;
        d0 * (3.0 * mt * mt) + d1 * (6.0 * mt * t) + d2 * (3.0 * t * t)
    }

    /// The portion of this curve over the parameter interval `[a, b]`, itself a
    /// cubic Bézier. Ports manim CE's `partial_bezier_points` (cubic case).
    ///
    /// ```
    /// use manim_math::bezier::CubicBezier;
    /// use manim_math::Point;
    /// let c = CubicBezier::line(Point::ZERO, Point::new(4.0, 0.0, 0.0));
    /// // partial(0, 1) reproduces the original curve.
    /// assert_eq!(c.partial(0.0, 1.0), c);
    /// // The endpoints of the sub-curve match evaluations of the original.
    /// let sub = c.partial(0.25, 0.75);
    /// assert!((sub.p0 - c.eval(0.25)).length() < 1e-6);
    /// assert!((sub.p3 - c.eval(0.75)).length() < 1e-6);
    /// ```
    pub fn partial(&self, a: f32, b: f32) -> Self {
        if a == 1.0 {
            return Self::new(self.p3, self.p3, self.p3, self.p3);
        }
        if b == 0.0 {
            return Self::new(self.p0, self.p0, self.p0, self.p0);
        }
        let (ma, mb) = (1.0 - a, 1.0 - b);
        let (a2, b2, ma2, mb2) = (a * a, b * b, ma * ma, mb * mb);
        let (a3, b3, ma3, mb3) = (a2 * a, b2 * b, ma2 * ma, mb2 * mb);
        let p = [self.p0, self.p1, self.p2, self.p3];

        let combine = |c0: f32, c1: f32, c2: f32, c3: f32| -> Point {
            p[0] * c0 + p[1] * c1 + p[2] * c2 + p[3] * c3
        };

        Self {
            p0: combine(ma3, 3.0 * ma2 * a, 3.0 * ma * a2, a3),
            p1: combine(
                ma2 * mb,
                2.0 * ma * a * mb + ma2 * b,
                a2 * mb + 2.0 * ma * a * b,
                a2 * b,
            ),
            p2: combine(
                ma * mb2,
                a * mb2 + 2.0 * ma * mb * b,
                2.0 * a * mb * b + ma * b2,
                a * b2,
            ),
            p3: combine(mb3, 3.0 * mb2 * b, 3.0 * mb * b2, b3),
        }
    }

    /// Split the curve at parameter `t` into two cubics that together reproduce
    /// the original.
    ///
    /// ```
    /// use manim_math::bezier::CubicBezier;
    /// use manim_math::Point;
    /// let c = CubicBezier::new(
    ///     Point::ZERO,
    ///     Point::new(1.0, 2.0, 0.0),
    ///     Point::new(2.0, -1.0, 0.0),
    ///     Point::new(3.0, 0.0, 0.0),
    /// );
    /// let (left, right) = c.split(0.4);
    /// // The two halves meet at the split point.
    /// assert!((left.p3 - right.p0).length() < 1e-6);
    /// assert!((left.p3 - c.eval(0.4)).length() < 1e-6);
    /// ```
    pub fn split(&self, t: f32) -> (Self, Self) {
        (self.partial(0.0, t), self.partial(t, 1.0))
    }

    /// Approximate arc length by sampling `n_samples` segments (minimum 1).
    ///
    /// ```
    /// use manim_math::bezier::CubicBezier;
    /// use manim_math::Point;
    /// let c = CubicBezier::line(Point::ZERO, Point::new(3.0, 4.0, 0.0));
    /// // A straight segment's length is exact regardless of sampling.
    /// assert!((c.arc_length(4) - 5.0).abs() < 1e-4);
    /// ```
    pub fn arc_length(&self, n_samples: usize) -> f32 {
        let n = n_samples.max(1);
        let mut total = 0.0;
        let mut prev = self.eval(0.0);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let cur = self.eval(t);
            total += (cur - prev).length();
            prev = cur;
        }
        total
    }

    /// The axis-aligned bounding box of the curve, as `(min, max)` corners.
    ///
    /// Computed exactly from the curve's endpoints and the roots of its
    /// per-axis derivative, so the box tightly contains the curve.
    ///
    /// ```
    /// use manim_math::bezier::CubicBezier;
    /// use manim_math::Point;
    /// let c = CubicBezier::line(Point::new(-1.0, 0.0, 0.0), Point::new(2.0, 3.0, 0.0));
    /// let (min, max) = c.bounding_box();
    /// assert_eq!(min, Point::new(-1.0, 0.0, 0.0));
    /// assert_eq!(max, Point::new(2.0, 3.0, 0.0));
    /// ```
    pub fn bounding_box(&self) -> (Point, Point) {
        let mut min = self.p0.min(self.p3);
        let mut max = self.p0.max(self.p3);
        for axis in 0..3 {
            let d0 = self.p1[axis] - self.p0[axis];
            let d1 = self.p2[axis] - self.p1[axis];
            let d2 = self.p3[axis] - self.p2[axis];
            // Derivative/3 = a t^2 + b t + c.
            let a = d0 - 2.0 * d1 + d2;
            let b = 2.0 * (d1 - d0);
            let c = d0;
            for &t in &roots_in_unit_interval(a, b, c) {
                let v = self.eval(t)[axis];
                min[axis] = min[axis].min(v);
                max[axis] = max[axis].max(v);
            }
        }
        (min, max)
    }
}

/// Real roots of `a t^2 + b t + c` that lie strictly inside `(0, 1)`.
fn roots_in_unit_interval(a: f32, b: f32, c: f32) -> smallvec::SmallVec<[f32; 2]> {
    let mut out = smallvec::SmallVec::new();
    let mut push = |t: f32| {
        if t > 0.0 && t < 1.0 {
            out.push(t);
        }
    };
    if a.abs() < 1e-9 {
        if b.abs() > 1e-9 {
            push(-c / b);
        }
        return out;
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return out;
    }
    let sq = disc.sqrt();
    push((-b + sq) / (2.0 * a));
    push((-b - sq) / (2.0 * a));
    out
}

/// Whether the anchor list forms a closed loop (first and last coincide).
fn is_closed(anchors: &[Point]) -> bool {
    anchors.len() >= 2 && (anchors[0] - anchors[anchors.len() - 1]).length() < 1e-6
}

/// Smooth first/second handle pairs for a cubic spline through `anchors`.
///
/// Ports manim CE's `get_smooth_cubic_bezier_handle_points`: returns one
/// `(h1, h2)` handle pair per curve (so `anchors.len() - 1` pairs). Detects
/// closed loops automatically; falls back to thirds for two anchors.
///
/// ```
/// use manim_math::bezier::{smooth_cubic_handles, CubicBezier};
/// use manim_math::Point;
/// let anchors = [
///     Point::new(-2.0, 0.0, 0.0),
///     Point::new(0.0, 1.0, 0.0),
///     Point::new(2.0, 0.0, 0.0),
/// ];
/// let handles = smooth_cubic_handles(&anchors);
/// assert_eq!(handles.len(), 2);
/// // Reconstructed curves pass through the anchors.
/// let c0 = CubicBezier::new(anchors[0], handles[0].0, handles[0].1, anchors[1]);
/// assert!((c0.eval(0.0) - anchors[0]).length() < 1e-6);
/// assert!((c0.eval(1.0) - anchors[1]).length() < 1e-6);
/// ```
pub fn smooth_cubic_handles(anchors: &[Point]) -> Vec<(Point, Point)> {
    let n = anchors.len();
    if n <= 1 {
        return Vec::new();
    }
    if n == 2 {
        return vec![(
            interpolate(anchors[0], anchors[1], 1.0 / 3.0),
            interpolate(anchors[0], anchors[1], 2.0 / 3.0),
        )];
    }
    let (h1, h2) = if is_closed(anchors) {
        smooth_closed_handles(anchors)
    } else {
        smooth_open_handles(anchors)
    };
    h1.into_iter().zip(h2).collect()
}

/// Solve the open (non-looping) smooth-spline tridiagonal system.
fn smooth_open_handles(a: &[Point]) -> (Vec<Point>, Vec<Point>) {
    let big_n = a.len() - 1; // number of curves
                             // cp[i] = 1 / (4 - cp[i-1]), cp[0] = 0.5; length N-1.
    let mut cp = vec![0.0_f32; big_n - 1];
    cp[0] = 0.5;
    for i in 1..big_n - 1 {
        cp[i] = 1.0 / (4.0 - cp[i - 1]);
    }

    let mut dp = vec![Point::ZERO; big_n];
    dp[0] = a[0] * 0.5 + a[1];
    for i in 1..big_n - 1 {
        let aux = a[i] * 4.0 + a[i + 1] * 2.0;
        dp[i] = (aux - dp[i - 1]) * cp[i];
    }
    dp[big_n - 1] =
        (a[big_n - 1] * 8.0 + a[big_n] - dp[big_n - 2] * 2.0) * (1.0 / (7.0 - 2.0 * cp[big_n - 2]));

    let mut h1 = dp;
    for i in (0..big_n - 1).rev() {
        h1[i] = h1[i] - h1[i + 1] * cp[i];
    }

    let mut h2 = vec![Point::ZERO; big_n];
    for i in 0..big_n - 1 {
        h2[i] = a[i + 1] * 2.0 - h1[i + 1];
    }
    h2[big_n - 1] = (a[big_n] + h1[big_n - 1]) * 0.5;

    (h1, h2)
}

/// Solve the closed (looping) smooth-spline cyclic tridiagonal system.
fn smooth_closed_handles(a: &[Point]) -> (Vec<Point>, Vec<Point>) {
    let big_n = a.len() - 1;
    let mut cp = vec![0.0_f32; big_n - 1];
    let mut up = vec![0.0_f32; big_n - 1];
    cp[0] = 1.0 / 3.0;
    up[0] = 1.0 / 3.0;
    for i in 1..big_n - 1 {
        cp[i] = 1.0 / (4.0 - cp[i - 1]);
        up[i] = -cp[i] * up[i - 1];
    }

    let cp_last = 1.0 / (3.0 - cp[big_n - 2]);
    let up_last = cp_last * (1.0 - up[big_n - 2]);

    let mut q = vec![0.0_f32; big_n];
    q[big_n - 1] = up_last;
    for i in (0..big_n - 1).rev() {
        q[i] = up[i] - cp[i] * q[i + 1];
    }

    let aux = |i: usize| a[i] * 4.0 + a[i + 1] * 2.0;
    let mut dp = vec![Point::ZERO; big_n];
    dp[0] = aux(0) * (1.0 / 3.0);
    for i in 1..big_n - 1 {
        dp[i] = (aux(i) - dp[i - 1]) * cp[i];
    }
    dp[big_n - 1] = (aux(big_n - 1) - dp[big_n - 2]) * cp_last;

    let mut y = dp;
    for i in (0..big_n - 1).rev() {
        y[i] = y[i] - y[i + 1] * cp[i];
    }

    let factor = 1.0 / (1.0 + q[0] + q[big_n - 1]);
    let y_ends = y[0] + y[big_n - 1];
    let mut h1 = vec![Point::ZERO; big_n];
    for i in 0..big_n {
        h1[i] = y[i] - y_ends * (factor * q[i]);
    }

    let mut h2 = vec![Point::ZERO; big_n];
    for i in 0..big_n - 1 {
        h2[i] = a[i + 1] * 2.0 - h1[i + 1];
    }
    h2[big_n - 1] = a[big_n] * 2.0 - h1[0];

    (h1, h2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn approx_point(a: Point, b: Point) {
        assert_relative_eq!(a.x, b.x, epsilon = 1e-4);
        assert_relative_eq!(a.y, b.y, epsilon = 1e-4);
        assert_relative_eq!(a.z, b.z, epsilon = 1e-4);
    }

    #[test]
    fn interpolate_scalar_and_point() {
        assert_relative_eq!(interpolate(2.0_f32, 4.0, 0.5), 3.0);
        approx_point(
            interpolate(Point::ZERO, Point::new(2.0, 4.0, 6.0), 0.25),
            Point::new(0.5, 1.0, 1.5),
        );
    }

    #[test]
    fn integer_interpolate_endpoints() {
        assert_eq!(integer_interpolate(0, 5, 0.0), (0, 0.0));
        assert_eq!(integer_interpolate(0, 5, 1.0), (4, 1.0));
        let (v, _) = integer_interpolate(0, 4, 0.5);
        assert_eq!(v, 2);
    }

    #[test]
    fn partial_identity_is_curve() {
        let c = CubicBezier::new(
            Point::ZERO,
            Point::new(1.0, 2.0, 0.0),
            Point::new(2.0, -1.0, 0.0),
            Point::new(3.0, 0.0, 0.0),
        );
        let p = c.partial(0.0, 1.0);
        approx_point(p.p0, c.p0);
        approx_point(p.p1, c.p1);
        approx_point(p.p2, c.p2);
        approx_point(p.p3, c.p3);
    }

    #[test]
    fn split_join_agree_with_original() {
        let c = CubicBezier::new(
            Point::new(-1.0, 0.0, 0.0),
            Point::new(0.0, 3.0, 0.0),
            Point::new(2.0, 3.0, 0.0),
            Point::new(3.0, 0.0, 0.0),
        );
        let (l, r) = c.split(0.3);
        for i in 0..=10 {
            let s = i as f32 / 10.0;
            approx_point(l.eval(s), c.eval(0.3 * s));
            approx_point(r.eval(s), c.eval(0.3 + 0.7 * s));
        }
    }

    #[test]
    fn smooth_handles_pass_through_open_anchors() {
        let anchors = [
            Point::new(-3.0, 0.0, 0.0),
            Point::new(-1.0, 2.0, 0.0),
            Point::new(1.0, -1.0, 0.0),
            Point::new(3.0, 1.0, 0.0),
        ];
        let handles = smooth_cubic_handles(&anchors);
        assert_eq!(handles.len(), 3);
        for (i, &(h1, h2)) in handles.iter().enumerate() {
            let c = CubicBezier::new(anchors[i], h1, h2, anchors[i + 1]);
            approx_point(c.eval(0.0), anchors[i]);
            approx_point(c.eval(1.0), anchors[i + 1]);
        }
    }

    #[test]
    fn smooth_handles_closed_loop() {
        let anchors = [
            Point::new(-2.0, 0.0, 0.0),
            Point::new(0.0, 2.0, 0.0),
            Point::new(2.0, 0.0, 0.0),
            Point::new(0.0, -2.0, 0.0),
            Point::new(-2.0, 0.0, 0.0),
        ];
        assert!(is_closed(&anchors));
        let handles = smooth_cubic_handles(&anchors);
        assert_eq!(handles.len(), 4);
        for (i, &(h1, h2)) in handles.iter().enumerate() {
            let c = CubicBezier::new(anchors[i], h1, h2, anchors[i + 1]);
            approx_point(c.eval(0.0), anchors[i]);
            approx_point(c.eval(1.0), anchors[i + 1]);
        }
    }

    #[test]
    fn bounding_box_contains_samples() {
        let c = CubicBezier::new(
            Point::ZERO,
            Point::new(0.0, 4.0, 0.0),
            Point::new(4.0, 4.0, 0.0),
            Point::new(4.0, 0.0, 0.0),
        );
        let (min, max) = c.bounding_box();
        for i in 0..=20 {
            let p = c.eval(i as f32 / 20.0);
            assert!(p.x >= min.x - 1e-5 && p.x <= max.x + 1e-5);
            assert!(p.y >= min.y - 1e-5 && p.y <= max.y + 1e-5);
        }
        // The peak of this arch sits above both anchors.
        assert!(max.y > 2.0);
    }
}
