//! Property-based tests for `manim-math` invariants.

use approx::assert_relative_eq;
use manim_math::bezier::CubicBezier;
use manim_math::path::Path;
use manim_math::rate_functions as rf;
use manim_math::Point;
use proptest::prelude::*;

/// A point strategy within a modest scene-space box.
fn point() -> impl Strategy<Value = Point> {
    (-8.0f32..8.0, -8.0f32..8.0).prop_map(|(x, y)| Point::new(x, y, 0.0))
}

/// A cubic Bézier with control points in scene space.
fn cubic() -> impl Strategy<Value = CubicBezier> {
    (point(), point(), point(), point()).prop_map(|(a, b, c, d)| CubicBezier::new(a, b, c, d))
}

proptest! {
    /// Splitting a curve and evaluating the halves agrees with the original.
    #[test]
    fn split_join_round_trip(c in cubic(), t in 0.05f32..0.95) {
        let (left, right) = c.split(t);
        for i in 0..=8 {
            let s = i as f32 / 8.0;
            let l = left.eval(s);
            prop_assert!((l - c.eval(t * s)).length() < 1e-3);
            let r = right.eval(s);
            prop_assert!((r - c.eval(t + (1.0 - t) * s)).length() < 1e-3);
        }
    }

    /// `partial(0, 1)` reproduces the curve exactly.
    #[test]
    fn partial_full_is_identity(c in cubic()) {
        let p = c.partial(0.0, 1.0);
        prop_assert!((p.p0 - c.p0).length() < 1e-4);
        prop_assert!((p.p1 - c.p1).length() < 1e-4);
        prop_assert!((p.p2 - c.p2).length() < 1e-4);
        prop_assert!((p.p3 - c.p3).length() < 1e-4);
    }

    /// `partial(a, b)` endpoints match evaluations of the original curve.
    #[test]
    fn partial_endpoints_match_eval(c in cubic(), a in 0.0f32..1.0, b in 0.0f32..1.0) {
        prop_assume!((a - b).abs() > 1e-3);
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        let sub = c.partial(a, b);
        prop_assert!((sub.p0 - c.eval(a)).length() < 1e-3);
        prop_assert!((sub.p3 - c.eval(b)).length() < 1e-3);
    }
}

/// Rate functions with the standard `f(0)=0, f(1)=1` endpoint contract.
const MONOTONE: &[fn(f32) -> f32] = &[
    rf::linear,
    rf::smooth,
    rf::smoothstep,
    rf::smootherstep,
    rf::smoothererstep,
    rf::rush_into,
    rf::rush_from,
    rf::slow_into,
    rf::double_smooth,
    rf::running_start,
    rf::lingering,
    rf::ease_in_sine,
    rf::ease_out_sine,
    rf::ease_in_out_sine,
    rf::ease_in_quad,
    rf::ease_out_quad,
    rf::ease_in_out_quad,
    rf::ease_in_cubic,
    rf::ease_out_cubic,
    rf::ease_in_out_cubic,
    rf::ease_in_quart,
    rf::ease_out_quart,
    rf::ease_in_out_quart,
    rf::ease_in_quint,
    rf::ease_out_quint,
    rf::ease_in_out_quint,
    rf::ease_in_expo,
    rf::ease_out_expo,
    rf::ease_in_out_expo,
    rf::ease_in_circ,
    rf::ease_out_circ,
    rf::ease_in_out_circ,
    rf::ease_in_back,
    rf::ease_out_back,
    rf::ease_in_out_back,
    rf::ease_in_bounce,
    rf::ease_out_bounce,
    rf::ease_in_out_bounce,
];

#[test]
fn monotone_rate_fn_endpoints() {
    for f in MONOTONE {
        assert_relative_eq!(f(0.0), 0.0, epsilon = 1e-4);
        assert_relative_eq!(f(1.0), 1.0, epsilon = 1e-4);
    }
}

#[test]
fn there_and_back_family_endpoints() {
    for f in [rf::there_and_back, rf::there_and_back_with_pause, rf::wiggle] {
        assert_relative_eq!(f(0.0), 0.0, epsilon = 1e-4);
        assert_relative_eq!(f(1.0), 0.0, epsilon = 1e-4);
    }
}

proptest! {
    /// Rate functions stay finite across the unit interval.
    #[test]
    fn rate_fns_finite_on_unit_interval(t in 0.0f32..=1.0) {
        for f in MONOTONE {
            prop_assert!(f(t).is_finite());
        }
    }
}

proptest! {
    /// `align_with` equalizes curve/subpath counts and preserves both shapes'
    /// sampled points along their arc-length parameterization.
    #[test]
    fn align_preserves_shape_and_equalizes_counts(
        anchors_a in prop::collection::vec(point(), 2..6),
        anchors_b in prop::collection::vec(point(), 2..6),
    ) {
        let mut a = Path::from_corners(&anchors_a, false);
        let mut b = Path::from_corners(&anchors_b, false);

        // Skip degenerate zero-length paths, whose proportion math is undefined.
        prop_assume!(a.point_from_proportion(0.0).distance(a.point_from_proportion(1.0)) > 0.1);
        prop_assume!(b.point_from_proportion(0.0).distance(b.point_from_proportion(1.0)) > 0.1);

        let before_a: Vec<Point> = (0..=10).map(|i| a.point_from_proportion(i as f32 / 10.0)).collect();
        let before_b: Vec<Point> = (0..=10).map(|i| b.point_from_proportion(i as f32 / 10.0)).collect();

        a.align_with(&mut b);

        prop_assert_eq!(a.n_curves(), b.n_curves());
        prop_assert_eq!(a.subpaths.len(), b.subpaths.len());
        for i in 0..a.subpaths.len() {
            prop_assert_eq!(a.subpaths[i].n_curves(), b.subpaths[i].n_curves());
        }

        for (i, p) in before_a.iter().enumerate() {
            let after = a.point_from_proportion(i as f32 / 10.0);
            prop_assert!((after - *p).length() < 0.05, "a shape drifted: {:?} vs {:?}", after, p);
        }
        for (i, p) in before_b.iter().enumerate() {
            let after = b.point_from_proportion(i as f32 / 10.0);
            prop_assert!((after - *p).length() < 0.05, "b shape drifted: {:?} vs {:?}", after, p);
        }
    }
}
