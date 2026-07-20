//! Bifurcation diagrams: what the attractor does as a parameter moves.
//!
//! Sweep one parameter, and for each value throw away a transient and plot what
//! is left. A single dot per parameter means a fixed point; two means a
//! period-2 orbit; a smear means chaos. The logistic map's cascade — period 1
//! until `r = 3`, period 2 until `r ≈ 3.4495`, then 4, 8, … accumulating at
//! `r ≈ 3.5699` — comes out of [`logistic`] with no special-casing, and
//! [`period_doubling_parameter`] locates those thresholds by bisection on the
//! measured period.

use manim_core::geometry::{Dot, VGroup};
use manim_core::graphing::Axes;
use manim_core::mobject::{AnyId, Buildable, MobjectId};
use manim_core::prelude::{Color, YELLOW};
use manim_core::scene_state::SceneState;

/// The logistic map `x ↦ r·x·(1 − x)`.
///
/// ```
/// use manim_dynamics::bifurcation::logistic_map;
/// assert_eq!(logistic_map(4.0, 0.5), 1.0);
/// ```
pub fn logistic_map(r: f64, x: f64) -> f64 {
    r * x * (1.0 - x)
}

/// The attractor samples of a 1-D map at one parameter value: iterate from `x0`,
/// discard `transient` iterations, keep the next `samples`.
///
/// ```
/// use manim_dynamics::bifurcation::{attractor_samples, logistic_map};
/// // r = 2.5 has a stable fixed point at 1 − 1/r = 0.6.
/// let s = attractor_samples(|x| logistic_map(2.5, x), 0.3, 500, 8);
/// assert!(s.iter().all(|v| (v - 0.6).abs() < 1e-9));
/// ```
pub fn attractor_samples(
    map: impl Fn(f64) -> f64,
    x0: f64,
    transient: usize,
    samples: usize,
) -> Vec<f64> {
    let mut x = x0;
    for _ in 0..transient {
        x = map(x);
        if !x.is_finite() {
            return Vec::new();
        }
    }
    (0..samples)
        .map(|_| {
            x = map(x);
            x
        })
        .collect()
}

/// The period of an attractor sample set: the number of distinct values, up to
/// `tol`, or `None` if it exceeds `max_period` (chaos, for practical purposes).
///
/// ```
/// use manim_dynamics::bifurcation::{attractor_samples, logistic_map, period_of};
/// let s = attractor_samples(|x| logistic_map(3.2, x), 0.3, 2000, 64);
/// assert_eq!(period_of(&s, 1e-6, 32), Some(2));
/// ```
pub fn period_of(samples: &[f64], tol: f64, max_period: usize) -> Option<usize> {
    let mut distinct: Vec<f64> = Vec::new();
    for &v in samples {
        if !distinct.iter().any(|d| (d - v).abs() <= tol) {
            distinct.push(v);
            if distinct.len() > max_period {
                return None;
            }
        }
    }
    Some(distinct.len())
}

/// Bisects on the parameter to find where the period first exceeds `period`.
///
/// `family(r)` supplies the map at parameter `r`. The bracket `(lo, hi)` must
/// straddle the transition: at `lo` the period is at most `period`, at `hi` it
/// is more.
///
/// ```
/// use manim_dynamics::bifurcation::{logistic_map, period_doubling_parameter};
/// // The first period doubling of the logistic map is at exactly r = 3.
/// let r = period_doubling_parameter(|r| move |x| logistic_map(r, x), 1, (2.5, 3.3), 60);
/// assert!((r - 3.0).abs() < 1e-3, "r = {r}");
/// ```
pub fn period_doubling_parameter<M, F>(
    family: F,
    period: usize,
    bracket: (f64, f64),
    steps: usize,
) -> f64
where
    M: Fn(f64) -> f64,
    F: Fn(f64) -> M,
{
    let (mut lo, mut hi) = bracket;
    for _ in 0..steps {
        let mid = 0.5 * (lo + hi);
        let s = attractor_samples(family(mid), 0.3, 20_000, 4 * period.max(1) + 8);
        let p = period_of(&s, 1e-7, 4 * period.max(1)).unwrap_or(usize::MAX);
        if p > period {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    0.5 * (lo + hi)
}

/// A bifurcation diagram: `(parameter, attractor value)` samples.
///
/// ```
/// use manim_dynamics::bifurcation::logistic;
/// let d = logistic((2.8, 4.0), 200, 500, 40);
/// assert!(!d.points().is_empty());
/// assert_eq!(d.parameter_range(), (2.8, 4.0));
/// ```
#[derive(Clone, Debug)]
pub struct BifurcationDiagram {
    points: Vec<(f64, f64)>,
    parameter_range: (f64, f64),
}

impl BifurcationDiagram {
    /// The scatter points.
    ///
    /// ```
    /// use manim_dynamics::bifurcation::logistic;
    /// assert!(logistic((3.0, 3.1), 4, 100, 2).points().len() <= 8);
    /// ```
    pub fn points(&self) -> &[(f64, f64)] {
        &self.points
    }

    /// The parameter interval swept.
    ///
    /// ```
    /// use manim_dynamics::bifurcation::logistic;
    /// assert_eq!(logistic((3.0, 3.5), 4, 10, 2).parameter_range(), (3.0, 3.5));
    /// ```
    pub fn parameter_range(&self) -> (f64, f64) {
        self.parameter_range
    }

    /// The range of attractor values in the diagram.
    ///
    /// ```
    /// use manim_dynamics::bifurcation::logistic;
    /// let (lo, hi) = logistic((2.8, 4.0), 100, 500, 30).value_range();
    /// assert!(lo >= 0.0 && hi <= 1.0);
    /// ```
    pub fn value_range(&self) -> (f64, f64) {
        self.points
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), &(_, v)| (a.min(v), b.max(v)))
    }

    /// The attractor values recorded at the parameter nearest `r`.
    ///
    /// ```
    /// use manim_dynamics::bifurcation::logistic;
    /// let d = logistic((2.0, 2.0), 1, 2000, 16);
    /// // r = 2 has a stable fixed point at 0.5.
    /// assert!(d.values_near(2.0).iter().all(|v| (v - 0.5).abs() < 1e-9));
    /// ```
    pub fn values_near(&self, r: f64) -> Vec<f64> {
        let Some(best) = self.points.iter().map(|&(p, _)| p).min_by(|a, b| {
            (a - r)
                .abs()
                .partial_cmp(&(b - r).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        }) else {
            return Vec::new();
        };
        self.points
            .iter()
            .filter(|&&(p, _)| (p - best).abs() < 1e-12)
            .map(|&(_, v)| v)
            .collect()
    }

    /// Draws the diagram as a dot scatter on `axes`.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_dynamics::bifurcation::logistic;
    /// let axes = Axes::new([2.8, 4.0, 0.2], [0.0, 1.0, 0.25]);
    /// let mut scene = SceneState::new();
    /// let d = logistic((2.8, 4.0), 40, 300, 20);
    /// let g = d.add_to(&mut scene, &axes, YELLOW, 0.012);
    /// assert!(scene.contains(g));
    /// ```
    pub fn add_to(
        &self,
        scene: &mut SceneState,
        axes: &Axes,
        color: Color,
        dot_radius: f32,
    ) -> MobjectId<VGroup> {
        let members: Vec<AnyId> = self
            .points
            .iter()
            .map(|&(r, v)| {
                scene
                    .add(
                        Dot::at(axes.c2p(r as f32, v as f32))
                            .radius(dot_radius)
                            .with_fill(color, 0.9),
                    )
                    .erase()
            })
            .collect();
        VGroup::of(scene, members)
    }
}

/// Sweeps a one-parameter family of 1-D maps into a diagram.
///
/// ```
/// use manim_dynamics::bifurcation::{sweep, logistic_map};
/// let d = sweep(|r| move |x| logistic_map(r, x), (3.4, 3.6), 20, 0.3, 500, 16);
/// assert_eq!(d.parameter_range(), (3.4, 3.6));
/// ```
pub fn sweep<M, F>(
    family: F,
    parameter_range: (f64, f64),
    n_parameters: usize,
    x0: f64,
    transient: usize,
    samples: usize,
) -> BifurcationDiagram
where
    M: Fn(f64) -> f64,
    F: Fn(f64) -> M,
{
    let n = n_parameters.max(1);
    let mut points = Vec::with_capacity(n * samples);
    for i in 0..n {
        let r = if n == 1 {
            parameter_range.0
        } else {
            parameter_range.0 + (parameter_range.1 - parameter_range.0) * i as f64 / (n - 1) as f64
        };
        for v in attractor_samples(family(r), x0, transient, samples) {
            points.push((r, v));
        }
    }
    BifurcationDiagram {
        points,
        parameter_range,
    }
}

/// The logistic-map bifurcation diagram — the period-doubling cascade.
///
/// ```
/// use manim_dynamics::bifurcation::logistic;
/// let d = logistic((2.9, 3.6), 100, 1000, 30);
/// // Below r = 3 the attractor is a single point.
/// assert_eq!(d.values_near(2.9).len(), 30);
/// ```
pub fn logistic(
    r_range: (f64, f64),
    n_parameters: usize,
    transient: usize,
    samples: usize,
) -> BifurcationDiagram {
    sweep(
        |r| move |x| logistic_map(r, x),
        r_range,
        n_parameters,
        0.3,
        transient,
        samples,
    )
}

/// The Hopf amplitude diagram: the radius the flow settles onto as `μ` sweeps.
///
/// Measured, not assumed — the radial equation `ṙ = μr − r³` is integrated to
/// steady state at each `μ` — so it reproduces `√μ` above onset and `0` below it
/// only because that is what the dynamics do.
///
/// ```
/// use manim_dynamics::bifurcation::hopf;
/// let d = hopf((-0.5, 1.0), 31);
/// // Below onset the amplitude is zero; above it, √μ.
/// assert!(d.values_near(-0.4)[0] < 1e-6);
/// assert!((d.values_near(1.0)[0] - 1.0).abs() < 1e-4);
/// ```
pub fn hopf(mu_range: (f64, f64), n_parameters: usize) -> BifurcationDiagram {
    let n = n_parameters.max(1);
    let mut points = Vec::with_capacity(n);
    for i in 0..n {
        let mu = if n == 1 {
            mu_range.0
        } else {
            mu_range.0 + (mu_range.1 - mu_range.0) * i as f64 / (n - 1) as f64
        };
        // ṙ = μr − r³ from a small seed, run long past any transient.
        let f = |_t: f64, y: &[f64]| vec![mu * y[0] - y[0] * y[0] * y[0]];
        let r = manim_fields::integrate::rk45(&f, 0.0, &[0.1], 200.0, 1e-12, 1e-12);
        points.push((mu, r[0].max(0.0)));
    }
    BifurcationDiagram {
        points,
        parameter_range: mu_range,
    }
}

/// The default diagram colour.
///
/// ```
/// use manim_core::prelude::YELLOW;
/// assert_eq!(manim_dynamics::bifurcation::default_color(), YELLOW);
/// ```
pub fn default_color() -> Color {
    YELLOW
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logistic_period(r: f64) -> Option<usize> {
        let s = attractor_samples(|x| logistic_map(r, x), 0.3, 50_000, 128);
        period_of(&s, 1e-7, 64)
    }

    #[test]
    fn logistic_periods_match_the_known_windows() {
        assert_eq!(logistic_period(2.5), Some(1));
        assert_eq!(logistic_period(2.9), Some(1));
        assert_eq!(logistic_period(3.2), Some(2));
        assert_eq!(logistic_period(3.5), Some(4));
        assert_eq!(logistic_period(3.55), Some(8));
        // Past the accumulation point r∞ ≈ 3.5699 the orbit is aperiodic.
        assert_eq!(logistic_period(3.9), None);
        // …but the period-3 window at r ≈ 3.83 is a genuine periodic island.
        assert_eq!(logistic_period(3.83), Some(3));
    }

    #[test]
    fn period_doubling_thresholds_are_the_textbook_values() {
        let family = |r: f64| move |x: f64| logistic_map(r, x);
        let r1 = period_doubling_parameter(family, 1, (2.5, 3.4), 50);
        let r2 = period_doubling_parameter(family, 2, (3.1, 3.5), 50);
        let r3 = period_doubling_parameter(family, 4, (3.44, 3.55), 50);
        assert!((r1 - 3.0).abs() < 1e-3, "r1 = {r1}");
        assert!((r2 - 3.449_489_74).abs() < 1e-3, "r2 = {r2}");
        assert!((r3 - 3.544_090).abs() < 2e-3, "r3 = {r3}");
        // The Feigenbaum ratio: (r2 − r1)/(r3 − r2) ≈ 4.669.
        let delta = (r2 - r1) / (r3 - r2);
        assert!((delta - 4.669).abs() < 0.15, "δ = {delta}");
    }

    #[test]
    fn the_fixed_point_below_onset_is_one_minus_one_over_r() {
        for r in [1.5, 2.0, 2.8] {
            let s = attractor_samples(|x| logistic_map(r, x), 0.3, 20_000, 4);
            for v in s {
                assert!((v - (1.0 - 1.0 / r)).abs() < 1e-9, "r = {r}: x = {v}");
            }
        }
    }

    #[test]
    fn the_period_two_orbit_matches_its_closed_form() {
        // For 3 < r < 1+√6 the 2-cycle is x± = (r + 1 ± √((r−3)(r+1)))/(2r).
        let r = 3.2;
        let s = attractor_samples(|x| logistic_map(r, x), 0.3, 50_000, 8);
        let disc = ((r - 3.0) * (r + 1.0)).sqrt();
        let (a, b) = ((r + 1.0 - disc) / (2.0 * r), (r + 1.0 + disc) / (2.0 * r));
        for v in s {
            assert!(
                (v - a).abs() < 1e-9 || (v - b).abs() < 1e-9,
                "x = {v} is neither {a} nor {b}"
            );
        }
    }

    #[test]
    fn the_diagram_thickens_as_the_cascade_proceeds() {
        let d = logistic((2.9, 3.9), 5, 20_000, 64);
        let widths: Vec<usize> = [2.9, 3.15, 3.4, 3.65, 3.9]
            .iter()
            .map(|&r| {
                let vals = d.values_near(r);
                period_of(&vals, 1e-7, 64).unwrap_or(64)
            })
            .collect();
        assert_eq!(widths[0], 1);
        assert!(widths[1] >= 2);
        assert!(widths.last().unwrap() > &widths[1]);
    }

    #[test]
    fn hopf_amplitude_is_zero_below_onset_and_sqrt_mu_above() {
        let d = hopf((-0.5, 1.0), 61);
        for &(mu, r) in d.points() {
            // Below onset r decays like e^{μt}; at t = 200 that is under 1e-6
            // once μ is comfortably negative (nearer onset the decay is slow,
            // which is the critical slowing-down the diagram is showing).
            if mu < -0.1 {
                assert!(r < 1e-6, "μ = {mu}: r = {r}");
            } else if mu > 0.05 {
                assert!(
                    (r - mu.sqrt()).abs() < 1e-5,
                    "μ = {mu}: {r} vs {}",
                    mu.sqrt()
                );
            }
        }
    }

    #[test]
    fn sweep_covers_the_requested_parameter_grid() {
        let d = sweep(
            |r| move |x| logistic_map(r, x),
            (3.0, 4.0),
            11,
            0.3,
            1_000,
            5,
        );
        let params: Vec<f64> = d
            .points()
            .iter()
            .map(|&(r, _)| r)
            .fold(Vec::new(), |mut acc, r| {
                if !acc.iter().any(|v: &f64| (v - r).abs() < 1e-12) {
                    acc.push(r);
                }
                acc
            });
        assert_eq!(params.len(), 11);
        assert!((params[0] - 3.0).abs() < 1e-12);
        assert!((params[10] - 4.0).abs() < 1e-12);
    }
}
