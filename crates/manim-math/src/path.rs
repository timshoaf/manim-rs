//! Vectorized Bézier paths, the geometric backbone of vectorized mobjects.
//!
//! A [`Path`] is a list of [`SubPath`]s, each a chain of [`CubicBezier`]
//! segments with a `closed` flag. This mirrors manim CE's `VMobject` point
//! layout (anchors and handles forming consecutive cubics) but stores explicit
//! curves for clarity. [`Path::align_with`] ports manim's `align_points`, the
//! prerequisite for `Transform`.

use crate::bezier::{smooth_cubic_handles, CubicBezier};
use crate::{Point, ORIGIN};

/// Curves sampled per segment when estimating arc length for proportion math.
const ARC_SAMPLES: usize = 16;

/// A single connected chain of cubic Bézier segments.
///
/// Consecutive curves share endpoints (`curves[i].p3 == curves[i+1].p0`). The
/// `closed` flag records whether the last anchor should be treated as joined
/// back to the first (a filled loop).
///
/// ```
/// use manim_math::path::SubPath;
/// use manim_math::{Point, RIGHT, UP};
/// let square = SubPath::from_corners(&[Point::ZERO, RIGHT, RIGHT + UP, UP]);
/// assert_eq!(square.n_curves(), 3);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct SubPath {
    /// The cubic segments making up this chain, in order.
    pub curves: Vec<CubicBezier>,
    /// Whether the chain forms a closed loop.
    pub closed: bool,
}

impl SubPath {
    /// Build a subpath of straight segments through `corners` (manim's
    /// `set_points_as_corners`). Fewer than two corners yields an empty chain.
    ///
    /// ```
    /// use manim_math::path::SubPath;
    /// use manim_math::{Point, RIGHT};
    /// let s = SubPath::from_corners(&[Point::ZERO, RIGHT]);
    /// assert_eq!(s.n_curves(), 1);
    /// ```
    pub fn from_corners(corners: &[Point]) -> Self {
        let curves = corners
            .windows(2)
            .map(|w| CubicBezier::line(w[0], w[1]))
            .collect();
        Self {
            curves,
            closed: false,
        }
    }

    /// Build a smooth spline through `anchors` (manim's `set_points_smoothly`).
    ///
    /// When `closed` is set, the loop is closed back to the first anchor and the
    /// handles are solved with periodic boundary conditions.
    ///
    /// ```
    /// use manim_math::path::SubPath;
    /// use manim_math::Point;
    /// let s = SubPath::from_smooth_anchors(
    ///     &[Point::new(-1.0, 0.0, 0.0), Point::ZERO, Point::new(1.0, 1.0, 0.0)],
    ///     false,
    /// );
    /// assert_eq!(s.n_curves(), 2);
    /// // The spline still interpolates its anchors.
    /// assert!((s.curves[0].eval(0.0) - Point::new(-1.0, 0.0, 0.0)).length() < 1e-6);
    /// ```
    pub fn from_smooth_anchors(anchors: &[Point], closed: bool) -> Self {
        if anchors.len() < 2 {
            return Self {
                curves: Vec::new(),
                closed,
            };
        }
        let mut pts = anchors.to_vec();
        if closed && (pts[0] - pts[pts.len() - 1]).length() >= 1e-6 {
            pts.push(pts[0]);
        }
        let handles = smooth_cubic_handles(&pts);
        let curves = handles
            .iter()
            .enumerate()
            .map(|(i, &(h1, h2))| CubicBezier::new(pts[i], h1, h2, pts[i + 1]))
            .collect();
        Self { curves, closed }
    }

    /// The number of cubic segments in this chain.
    ///
    /// ```
    /// use manim_math::path::SubPath;
    /// use manim_math::{Point, RIGHT, UP};
    /// let s = SubPath::from_corners(&[Point::ZERO, RIGHT, UP]);
    /// assert_eq!(s.n_curves(), 2);
    /// ```
    pub fn n_curves(&self) -> usize {
        self.curves.len()
    }

    /// Whether this chain is a closed loop.
    ///
    /// ```
    /// use manim_math::path::SubPath;
    /// use manim_math::{Point, RIGHT, UP};
    /// let mut s = SubPath::from_corners(&[Point::ZERO, RIGHT, UP]);
    /// assert!(!s.is_closed());
    /// s.closed = true;
    /// assert!(s.is_closed());
    /// ```
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// The total approximate arc length of the chain.
    ///
    /// ```
    /// use manim_math::path::SubPath;
    /// use manim_math::{Point, RIGHT, UP};
    /// let s = SubPath::from_corners(&[Point::ZERO, RIGHT, RIGHT + UP]);
    /// assert!((s.arc_length() - 2.0).abs() < 1e-3);
    /// ```
    pub fn arc_length(&self) -> f32 {
        self.curves.iter().map(|c| c.arc_length(ARC_SAMPLES)).sum()
    }

    /// The bounding box of the chain, or `None` if it has no curves.
    ///
    /// ```
    /// use manim_math::path::SubPath;
    /// use manim_math::{Point, RIGHT, UP};
    /// let s = SubPath::from_corners(&[Point::ZERO, RIGHT + UP]);
    /// let (min, max) = s.bounding_box().unwrap();
    /// assert_eq!(min, Point::ZERO);
    /// assert_eq!(max, RIGHT + UP);
    /// ```
    pub fn bounding_box(&self) -> Option<(Point, Point)> {
        let mut iter = self.curves.iter();
        let (mut min, mut max) = iter.next()?.bounding_box();
        for c in iter {
            let (cmin, cmax) = c.bounding_box();
            min = min.min(cmin);
            max = max.max(cmax);
        }
        Some((min, max))
    }

    /// Sample `per_curve` points along each segment (for tessellation/tests).
    ///
    /// ```
    /// use manim_math::path::SubPath;
    /// use manim_math::{Point, RIGHT};
    /// let s = SubPath::from_corners(&[Point::ZERO, RIGHT]);
    /// let pts = s.points(5);
    /// assert_eq!(pts.len(), 5);
    /// assert_eq!(pts[0], Point::ZERO);
    /// ```
    pub fn points(&self, per_curve: usize) -> Vec<Point> {
        let n = per_curve.max(2);
        let mut out = Vec::with_capacity(self.curves.len() * n);
        for c in &self.curves {
            for i in 0..n {
                out.push(c.eval(i as f32 / (n - 1) as f32));
            }
        }
        out
    }

    /// Reverse the direction of travel along the chain.
    ///
    /// ```
    /// use manim_math::path::SubPath;
    /// use manim_math::{Point, RIGHT, UP};
    /// let mut s = SubPath::from_corners(&[Point::ZERO, RIGHT, UP]);
    /// s.reverse();
    /// // Travel now starts where it used to end.
    /// assert_eq!(s.curves[0].p0, UP);
    /// ```
    pub fn reverse(&mut self) {
        self.curves.reverse();
        for c in &mut self.curves {
            std::mem::swap(&mut c.p0, &mut c.p3);
            std::mem::swap(&mut c.p1, &mut c.p2);
        }
    }

    /// Split the longest segments so the chain gains `n` more curves, without
    /// changing its shape.
    ///
    /// ```
    /// use manim_math::path::SubPath;
    /// use manim_math::{Point, RIGHT};
    /// let mut s = SubPath::from_corners(&[Point::ZERO, RIGHT]);
    /// s.insert_n_curves(3);
    /// assert_eq!(s.n_curves(), 4);
    /// ```
    pub fn insert_n_curves(&mut self, n: usize) {
        if self.curves.is_empty() {
            return;
        }
        for _ in 0..n {
            let idx = self.longest_curve_index();
            let (left, right) = self.curves[idx].split(0.5);
            self.curves[idx] = left;
            self.curves.insert(idx + 1, right);
        }
    }

    /// Index of the currently longest segment.
    fn longest_curve_index(&self) -> usize {
        let mut best = 0;
        let mut best_len = f32::NEG_INFINITY;
        for (i, c) in self.curves.iter().enumerate() {
            let len = c.arc_length(ARC_SAMPLES);
            if len > best_len {
                best_len = len;
                best = i;
            }
        }
        best
    }
}

/// A full vectorized path: an ordered collection of subpaths.
///
/// ```
/// use manim_math::path::Path;
/// use manim_math::{Point, RIGHT, UP};
/// let p = Path::from_corners(&[Point::ZERO, RIGHT, RIGHT + UP, UP], true);
/// assert_eq!(p.n_curves(), 3);
/// assert!(p.is_closed());
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    /// The subpaths, drawn in order.
    pub subpaths: Vec<SubPath>,
}

impl Path {
    /// Build a single-subpath path of straight segments through `corners`.
    ///
    /// ```
    /// use manim_math::path::Path;
    /// use manim_math::{Point, RIGHT, UP};
    /// let p = Path::from_corners(&[Point::ZERO, RIGHT, UP], false);
    /// assert_eq!(p.n_curves(), 2);
    /// ```
    pub fn from_corners(corners: &[Point], closed: bool) -> Self {
        let mut sp = SubPath::from_corners(corners);
        sp.closed = closed;
        Self { subpaths: vec![sp] }
    }

    /// Build a single-subpath smooth spline through `anchors`.
    ///
    /// ```
    /// use manim_math::path::Path;
    /// use manim_math::Point;
    /// let p = Path::from_smooth_anchors(
    ///     &[Point::new(-1.0, 0.0, 0.0), Point::ZERO, Point::new(1.0, 0.5, 0.0)],
    ///     false,
    /// );
    /// assert_eq!(p.n_curves(), 2);
    /// ```
    pub fn from_smooth_anchors(anchors: &[Point], closed: bool) -> Self {
        Self {
            subpaths: vec![SubPath::from_smooth_anchors(anchors, closed)],
        }
    }

    /// The total number of cubic segments across all subpaths.
    ///
    /// ```
    /// use manim_math::path::Path;
    /// use manim_math::{Point, RIGHT, UP};
    /// let p = Path::from_corners(&[Point::ZERO, RIGHT, UP], false);
    /// assert_eq!(p.n_curves(), 2);
    /// ```
    pub fn n_curves(&self) -> usize {
        self.subpaths.iter().map(SubPath::n_curves).sum()
    }

    /// Whether the path is non-empty and every subpath is closed.
    ///
    /// ```
    /// use manim_math::path::Path;
    /// use manim_math::{Point, RIGHT, UP};
    /// let p = Path::from_corners(&[Point::ZERO, RIGHT, UP], true);
    /// assert!(p.is_closed());
    /// ```
    pub fn is_closed(&self) -> bool {
        !self.subpaths.is_empty() && self.subpaths.iter().all(SubPath::is_closed)
    }

    /// All segments across all subpaths, flattened in draw order.
    fn all_curves(&self) -> Vec<CubicBezier> {
        self.subpaths
            .iter()
            .flat_map(|s| s.curves.iter().copied())
            .collect()
    }

    /// The point at arc-length proportion `alpha ∈ [0, 1]` along the whole path.
    ///
    /// Ports manim CE's `VMobject.point_from_proportion`: segments are weighted
    /// by arc length, and the residual proportion parameterizes the containing
    /// curve.
    ///
    /// ```
    /// use manim_math::path::Path;
    /// use manim_math::{Point, RIGHT, UP};
    /// let p = Path::from_corners(&[Point::ZERO, RIGHT, RIGHT + UP], false);
    /// // Half of the total arc length lands at the shared corner.
    /// assert!((p.point_from_proportion(0.5) - RIGHT).length() < 1e-3);
    /// ```
    pub fn point_from_proportion(&self, alpha: f32) -> Point {
        let alpha = alpha.clamp(0.0, 1.0);
        let curves = self.all_curves();
        let Some(first) = curves.first() else {
            return ORIGIN;
        };
        let lengths: Vec<f32> = curves.iter().map(|c| c.arc_length(ARC_SAMPLES)).collect();
        let total: f32 = lengths.iter().sum();
        if total <= 1e-9 {
            return first.eval(0.0);
        }
        let target = alpha * total;
        let mut acc = 0.0;
        for (i, (curve, &len)) in curves.iter().zip(&lengths).enumerate() {
            if acc + len >= target || i == curves.len() - 1 {
                let residue = if len > 0.0 {
                    ((target - acc) / len).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                return curve.eval(residue);
            }
            acc += len;
        }
        curves[curves.len() - 1].eval(1.0)
    }

    /// Locate the `(curve index, local parameter)` at path proportion `p`.
    fn locate(curves: &[CubicBezier], lengths: &[f32], total: f32, p: f32) -> (usize, f32) {
        let target = p.clamp(0.0, 1.0) * total;
        let mut acc = 0.0;
        for (i, &len) in lengths.iter().enumerate() {
            if acc + len >= target || i == curves.len() - 1 {
                let local = if len > 0.0 {
                    ((target - acc) / len).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                return (i, local);
            }
            acc += len;
        }
        (curves.len() - 1, 1.0)
    }

    /// The portion of the path between proportions `a` and `b` (with `a ≤ b`),
    /// returned as a fresh open single-subpath path.
    ///
    /// Ports manim CE's `VMobject.get_subcurve`.
    ///
    /// ```
    /// use manim_math::path::Path;
    /// use manim_math::{Point, RIGHT};
    /// let p = Path::from_corners(&[Point::ZERO, RIGHT], false);
    /// let half = p.get_subcurve(0.0, 0.5);
    /// // The subcurve ends at the midpoint of the original.
    /// let end = half.point_from_proportion(1.0);
    /// assert!((end - Point::new(0.5, 0.0, 0.0)).length() < 1e-3);
    /// ```
    pub fn get_subcurve(&self, a: f32, b: f32) -> Path {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        let curves = self.all_curves();
        if curves.is_empty() {
            return Path::default();
        }
        let lengths: Vec<f32> = curves.iter().map(|c| c.arc_length(ARC_SAMPLES)).collect();
        let total: f32 = lengths.iter().sum();
        if total <= 1e-9 {
            return Path {
                subpaths: vec![SubPath {
                    curves: vec![curves[0]],
                    closed: false,
                }],
            };
        }
        let (ia, ta) = Self::locate(&curves, &lengths, total, a);
        let (ib, tb) = Self::locate(&curves, &lengths, total, b);

        let new_curves = if ia == ib {
            vec![curves[ia].partial(ta, tb)]
        } else {
            let mut out = Vec::with_capacity(ib - ia + 1);
            out.push(curves[ia].partial(ta, 1.0));
            out.extend(curves[ia + 1..ib].iter().copied());
            out.push(curves[ib].partial(0.0, tb));
            out
        };
        Path {
            subpaths: vec![SubPath {
                curves: new_curves,
                closed: false,
            }],
        }
    }

    /// Split the longest segments across the path so it gains `n` more curves,
    /// preserving its shape.
    ///
    /// ```
    /// use manim_math::path::Path;
    /// use manim_math::{Point, RIGHT, UP};
    /// let mut p = Path::from_corners(&[Point::ZERO, RIGHT, RIGHT + UP], false);
    /// p.insert_n_curves(4);
    /// assert_eq!(p.n_curves(), 6);
    /// ```
    pub fn insert_n_curves(&mut self, n: usize) {
        if self.n_curves() == 0 {
            return;
        }
        for _ in 0..n {
            let (si, ci) = self.longest_curve();
            let (left, right) = self.subpaths[si].curves[ci].split(0.5);
            self.subpaths[si].curves[ci] = left;
            self.subpaths[si].curves.insert(ci + 1, right);
        }
    }

    /// `(subpath index, curve index)` of the globally longest segment.
    fn longest_curve(&self) -> (usize, usize) {
        let mut best = (0, 0);
        let mut best_len = f32::NEG_INFINITY;
        for (si, sp) in self.subpaths.iter().enumerate() {
            for (ci, c) in sp.curves.iter().enumerate() {
                let len = c.arc_length(ARC_SAMPLES);
                if len > best_len {
                    best_len = len;
                    best = (si, ci);
                }
            }
        }
        best
    }

    /// A degenerate one-point subpath positioned at the path's last anchor,
    /// used to pad subpath counts during alignment.
    fn null_subpath(&self) -> SubPath {
        let anchor = self
            .subpaths
            .iter()
            .rev()
            .find_map(|s| s.curves.last())
            .map(|c| c.p3)
            .unwrap_or(ORIGIN);
        SubPath {
            curves: vec![CubicBezier::new(anchor, anchor, anchor, anchor)],
            closed: false,
        }
    }

    /// Make `self` and `other` structurally alignable: equal subpath counts and,
    /// per index, equal curve counts — without changing either shape.
    ///
    /// Ports manim CE's `VMobject.align_points`, the prerequisite for
    /// `Transform`. Shorter paths gain degenerate point-subpaths; shorter
    /// subpaths gain curves by splitting their longest segments.
    ///
    /// ```
    /// use manim_math::path::Path;
    /// use manim_math::{Point, RIGHT, UP};
    /// let mut a = Path::from_corners(&[Point::ZERO, RIGHT], false);
    /// let mut b = Path::from_corners(&[Point::ZERO, RIGHT, RIGHT + UP, UP], false);
    /// a.align_with(&mut b);
    /// assert_eq!(a.n_curves(), b.n_curves());
    /// assert_eq!(a.subpaths.len(), b.subpaths.len());
    /// ```
    pub fn align_with(&mut self, other: &mut Path) {
        let m = self.subpaths.len().max(other.subpaths.len());
        while self.subpaths.len() < m {
            self.subpaths.push(self.null_subpath());
        }
        while other.subpaths.len() < m {
            other.subpaths.push(other.null_subpath());
        }
        for i in 0..m {
            let ca = self.subpaths[i].n_curves();
            let cb = other.subpaths[i].n_curves();
            let mc = ca.max(cb);
            self.subpaths[i].insert_n_curves(mc - ca);
            other.subpaths[i].insert_n_curves(mc - cb);
        }
    }

    /// The bounding box of the whole path, or `None` if it has no curves.
    ///
    /// ```
    /// use manim_math::path::Path;
    /// use manim_math::{Point, RIGHT, UP};
    /// let p = Path::from_corners(&[Point::ZERO, RIGHT + UP], false);
    /// let (min, max) = p.bounding_box().unwrap();
    /// assert_eq!(min, Point::ZERO);
    /// assert_eq!(max, RIGHT + UP);
    /// ```
    pub fn bounding_box(&self) -> Option<(Point, Point)> {
        let mut result: Option<(Point, Point)> = None;
        for sp in &self.subpaths {
            if let Some((min, max)) = sp.bounding_box() {
                result = Some(match result {
                    Some((rmin, rmax)) => (rmin.min(min), rmax.max(max)),
                    None => (min, max),
                });
            }
        }
        result
    }

    /// Apply `f` to every control point of every segment (manim's `apply_function`).
    ///
    /// ```
    /// use manim_math::path::Path;
    /// use manim_math::{Point, RIGHT};
    /// let mut p = Path::from_corners(&[Point::ZERO, RIGHT], false);
    /// p.apply(|pt| pt + Point::new(0.0, 1.0, 0.0));
    /// assert_eq!(p.point_from_proportion(0.0), Point::new(0.0, 1.0, 0.0));
    /// ```
    pub fn apply<F: Fn(Point) -> Point>(&mut self, f: F) {
        for sp in &mut self.subpaths {
            for c in &mut sp.curves {
                c.p0 = f(c.p0);
                c.p1 = f(c.p1);
                c.p2 = f(c.p2);
                c.p3 = f(c.p3);
            }
        }
    }

    /// Reverse the path's overall direction of travel.
    ///
    /// ```
    /// use manim_math::path::Path;
    /// use manim_math::{Point, RIGHT};
    /// let mut p = Path::from_corners(&[Point::ZERO, RIGHT], false);
    /// p.reverse();
    /// assert_eq!(p.point_from_proportion(0.0), RIGHT);
    /// ```
    pub fn reverse(&mut self) {
        self.subpaths.reverse();
        for sp in &mut self.subpaths {
            sp.reverse();
        }
    }

    /// Sample `per_curve` points along each segment of each subpath.
    ///
    /// ```
    /// use manim_math::path::Path;
    /// use manim_math::{Point, RIGHT, UP};
    /// let p = Path::from_corners(&[Point::ZERO, RIGHT, UP], false);
    /// let pts = p.points(4);
    /// assert_eq!(pts.len(), 8); // 2 curves × 4 samples
    /// ```
    pub fn points(&self, per_curve: usize) -> Vec<Point> {
        self.subpaths
            .iter()
            .flat_map(|s| s.points(per_curve))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RIGHT, UP};
    use approx::assert_relative_eq;

    fn approx_point(a: Point, b: Point, eps: f32) {
        assert_relative_eq!(a.x, b.x, epsilon = eps);
        assert_relative_eq!(a.y, b.y, epsilon = eps);
        assert_relative_eq!(a.z, b.z, epsilon = eps);
    }

    #[test]
    fn point_from_proportion_endpoints() {
        let p = Path::from_corners(&[Point::ZERO, RIGHT, RIGHT + UP], false);
        approx_point(p.point_from_proportion(0.0), Point::ZERO, 1e-4);
        approx_point(p.point_from_proportion(1.0), RIGHT + UP, 1e-4);
        approx_point(p.point_from_proportion(0.5), RIGHT, 1e-3);
    }

    #[test]
    fn get_subcurve_matches_original_samples() {
        let p = Path::from_smooth_anchors(
            &[
                Point::new(-2.0, 0.0, 0.0),
                Point::new(-1.0, 1.5, 0.0),
                Point::new(1.0, -1.0, 0.0),
                Point::new(2.0, 0.5, 0.0),
            ],
            false,
        );
        let sub = p.get_subcurve(0.25, 0.75);
        // Endpoints of the subcurve match the original at 0.25 and 0.75.
        approx_point(
            sub.point_from_proportion(0.0),
            p.point_from_proportion(0.25),
            2e-2,
        );
        approx_point(
            sub.point_from_proportion(1.0),
            p.point_from_proportion(0.75),
            2e-2,
        );
    }

    #[test]
    fn insert_n_curves_preserves_straight_parameterization() {
        // On straight segments, parameter is proportional to arc length, so
        // point_from_proportion is invariant under subdivision.
        let mut p = Path::from_corners(&[Point::ZERO, RIGHT, RIGHT + UP, UP], false);
        let before: Vec<Point> = (0..=20)
            .map(|i| p.point_from_proportion(i as f32 / 20.0))
            .collect();
        p.insert_n_curves(5);
        assert_eq!(p.n_curves(), 8);
        for (i, b) in before.iter().enumerate() {
            approx_point(p.point_from_proportion(i as f32 / 20.0), *b, 1e-4);
        }
    }

    #[test]
    fn insert_n_curves_preserves_geometry() {
        // Splitting curves is exact, so the geometric bounding box is invariant
        // even for curved splines.
        let mut p = Path::from_smooth_anchors(
            &[
                Point::new(-2.0, 0.0, 0.0),
                Point::new(0.0, 1.0, 0.0),
                Point::new(2.0, 0.0, 0.0),
            ],
            false,
        );
        let (min0, max0) = p.bounding_box().unwrap();
        p.insert_n_curves(5);
        assert_eq!(p.n_curves(), 7);
        let (min1, max1) = p.bounding_box().unwrap();
        approx_point(min0, min1, 1e-4);
        approx_point(max0, max1, 1e-4);
    }

    #[test]
    fn align_equalizes_counts() {
        let mut a = Path::from_corners(&[Point::ZERO, RIGHT], false);
        let mut b = Path::from_corners(&[Point::ZERO, RIGHT, RIGHT + UP, UP, Point::ZERO], false);
        a.align_with(&mut b);
        assert_eq!(a.n_curves(), b.n_curves());
        assert_eq!(a.subpaths.len(), b.subpaths.len());
        for i in 0..a.subpaths.len() {
            assert_eq!(a.subpaths[i].n_curves(), b.subpaths[i].n_curves());
        }
    }

    #[test]
    fn align_pads_subpath_counts() {
        let mut a = Path {
            subpaths: vec![
                SubPath::from_corners(&[Point::ZERO, RIGHT]),
                SubPath::from_corners(&[UP, UP + RIGHT]),
            ],
        };
        let mut b = Path::from_corners(&[Point::ZERO, RIGHT, UP], false);
        a.align_with(&mut b);
        assert_eq!(a.subpaths.len(), b.subpaths.len());
        assert_eq!(a.n_curves(), b.n_curves());
    }

    #[test]
    fn reverse_swaps_endpoints() {
        let p0 = Point::new(-1.0, 0.0, 0.0);
        let p1 = Point::new(2.0, 1.0, 0.0);
        let mut p = Path::from_corners(&[p0, p1], false);
        p.reverse();
        approx_point(p.point_from_proportion(0.0), p1, 1e-4);
        approx_point(p.point_from_proportion(1.0), p0, 1e-4);
    }
}
