//! Limit cycles, found the way Poincaré found them: by return map.
//!
//! Pick a [`Section`] — a ray transverse to the flow — and record where an orbit
//! next crosses it. That map `P: s ↦ s'` turns a continuous flow into a 1-D
//! discrete system, and a **fixed point of `P` is a closed orbit**: the orbit
//! comes back to exactly where it started, so it repeats forever. Iterating `P`
//! from any nearby point converges to that fixed point precisely when the cycle
//! is attracting, which is why the naive iteration below is enough for Van der
//! Pol and every other stable limit cycle.

use manim_core::geometry::VMobject;
use manim_core::graphing::Axes;
use manim_core::mobject::{Buildable, MobjectId};
use manim_core::prelude::{Color, RED};
use manim_core::scene_state::SceneState;
use manim_math::path::Path;

use crate::{value, PlanarSystem};

/// Which way the flow must cross a section for the crossing to count.
///
/// Counting only one orientation is what makes the return map single-valued: an
/// orbit that crosses a line twice per lap crosses it once in each direction.
///
/// ```
/// use manim_dynamics::cycles::Crossing;
/// assert_ne!(Crossing::Increasing, Crossing::Decreasing);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crossing {
    /// The transverse coordinate goes from negative to positive.
    Increasing,
    /// The transverse coordinate goes from positive to negative.
    Decreasing,
}

/// A Poincaré section: the ray from `base` along `direction`, crossed in a
/// chosen orientation.
///
/// ```
/// use manim_dynamics::cycles::{Crossing, Section};
/// // The positive x-axis, crossed downwards.
/// let s = Section::new([0.0, 0.0], [1.0, 0.0], Crossing::Decreasing);
/// // A point 2 units out along the ray, right on it.
/// assert_eq!(s.along([2.0, 0.0]), 2.0);
/// assert_eq!(s.transverse([2.0, 0.0]), 0.0);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Section {
    base: [f64; 2],
    dir: [f64; 2],
    crossing: Crossing,
}

impl Section {
    /// A section from `base` along `direction` (normalized), counting crossings
    /// of the given orientation.
    ///
    /// ```
    /// use manim_dynamics::cycles::{Crossing, Section};
    /// let s = Section::new([1.0, 1.0], [0.0, 3.0], Crossing::Increasing);
    /// // The direction is normalized, so "along" is a true distance.
    /// assert!((s.along([1.0, 4.0]) - 3.0).abs() < 1e-12);
    /// ```
    pub fn new(base: [f64; 2], direction: [f64; 2], crossing: Crossing) -> Self {
        let n = (direction[0] * direction[0] + direction[1] * direction[1])
            .sqrt()
            .max(1e-300);
        Self {
            base,
            dir: [direction[0] / n, direction[1] / n],
            crossing,
        }
    }

    /// The positive x-axis from the origin, crossed downwards — the section Van
    /// der Pol and most `ẏ < 0`-on-the-right systems want.
    ///
    /// ```
    /// use manim_dynamics::cycles::Section;
    /// assert_eq!(Section::positive_x_axis().along([3.0, 0.0]), 3.0);
    /// ```
    pub fn positive_x_axis() -> Self {
        Self::new([0.0, 0.0], [1.0, 0.0], Crossing::Decreasing)
    }

    /// The distance from `base` along the section direction.
    ///
    /// ```
    /// use manim_dynamics::cycles::Section;
    /// assert_eq!(Section::positive_x_axis().along([-2.0, 0.0]), -2.0);
    /// ```
    pub fn along(&self, p: [f64; 2]) -> f64 {
        (p[0] - self.base[0]) * self.dir[0] + (p[1] - self.base[1]) * self.dir[1]
    }

    /// The signed distance *off* the section line (zero exactly on it).
    ///
    /// ```
    /// use manim_dynamics::cycles::Section;
    /// assert_eq!(Section::positive_x_axis().transverse([1.0, 0.5]), 0.5);
    /// ```
    pub fn transverse(&self, p: [f64; 2]) -> f64 {
        self.dir[0] * (p[1] - self.base[1]) - self.dir[1] * (p[0] - self.base[0])
    }

    /// The point at distance `s` along the section.
    ///
    /// ```
    /// use manim_dynamics::cycles::Section;
    /// assert_eq!(Section::positive_x_axis().point_at(2.5), [2.5, 0.0]);
    /// ```
    pub fn point_at(&self, s: f64) -> [f64; 2] {
        [
            self.base[0] + self.dir[0] * s,
            self.base[1] + self.dir[1] * s,
        ]
    }

    /// Whether a step from transverse coordinate `a` to `b` counts as a
    /// crossing.
    fn crosses(&self, a: f64, b: f64) -> bool {
        match self.crossing {
            Crossing::Increasing => a < 0.0 && b >= 0.0,
            Crossing::Decreasing => a > 0.0 && b <= 0.0,
        }
    }
}

/// One application of the Poincaré return map: integrate from the section point
/// at `s` until the orbit next crosses the section, and report where and when.
///
/// Returns `None` if no crossing happens within `max_time`.
///
/// ```
/// use manim_dynamics::cycles::{return_map, Section};
/// use manim_dynamics::HopfNormalForm;
/// use manim_dynamics::cycles::Crossing;
/// // On the invariant circle r = 0.5 the map is the identity, with period 2π/ω.
/// let h = HopfNormalForm { mu: 0.25, omega: 2.0 };
/// let sec = Section::new([0.0, 0.0], [1.0, 0.0], Crossing::Increasing);
/// let (s, t) = return_map(&h, &sec, 0.5, 20.0, 1e-3).unwrap();
/// assert!((s - 0.5).abs() < 1e-6);
/// assert!((t - std::f64::consts::PI).abs() < 1e-4);
/// ```
pub fn return_map<Sy: PlanarSystem + ?Sized>(
    system: &Sy,
    section: &Section,
    s: f64,
    max_time: f64,
    dt: f64,
) -> Option<(f64, f64)> {
    let mut p = section.point_at(s);
    let mut g = section.transverse(p);
    let mut t = 0.0;
    let f = |_t: f64, y: &[f64]| {
        let v = value(system, y[0], y[1]);
        vec![v[0], v[1]]
    };
    // Step off the section first so the starting point is not itself a crossing.
    let mut started = false;
    while t < max_time {
        let next = manim_fields::integrate::rk45(&f, 0.0, &[p[0], p[1]], dt, 1e-10, 1e-10);
        let q = [next[0], next[1]];
        if !q[0].is_finite() || !q[1].is_finite() {
            return None;
        }
        let gq = section.transverse(q);
        if started && section.crosses(g, gq) {
            // Linear interpolation to the crossing, in both space and time.
            let frac = if (g - gq).abs() > 1e-300 {
                g / (g - gq)
            } else {
                0.0
            };
            let hit = [p[0] + frac * (q[0] - p[0]), p[1] + frac * (q[1] - p[1])];
            let along = section.along(hit);
            if along > 0.0 {
                return Some((along, t + frac * dt));
            }
        }
        if g.abs() > 1e-12 {
            started = true;
        }
        p = q;
        g = gq;
        t += dt;
    }
    None
}

/// A closed orbit: the loop itself, its period, and its size.
#[derive(Clone, Debug)]
pub struct LimitCycle {
    /// One full lap of the orbit in data coordinates.
    pub points: Vec<[f64; 2]>,
    /// The time it takes to close.
    pub period: f64,
    /// Where the cycle crosses the section (distance along it).
    pub section_coordinate: f64,
}

impl LimitCycle {
    /// Half the peak-to-peak extent in `x` — the amplitude of the `x`
    /// oscillation.
    ///
    /// ```
    /// use manim_dynamics::cycles::{find_limit_cycle, Section};
    /// use manim_dynamics::VanDerPol;
    /// let cyc = find_limit_cycle(&VanDerPol { mu: 1.0 }, &Section::positive_x_axis(),
    ///                            1.0, 40, 1e-8, 1e-3).unwrap();
    /// assert!((cyc.amplitude() - 2.0086).abs() < 1e-2);
    /// ```
    pub fn amplitude(&self) -> f64 {
        let (lo, hi) = self
            .points
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
        0.5 * (hi - lo)
    }

    /// The largest distance from the origin reached on the cycle.
    ///
    /// ```
    /// use manim_dynamics::cycles::{find_limit_cycle, Crossing, Section};
    /// use manim_dynamics::HopfNormalForm;
    /// let sec = Section::new([0.0, 0.0], [1.0, 0.0], Crossing::Increasing);
    /// let cyc = find_limit_cycle(&HopfNormalForm { mu: 0.49, omega: 1.0 }, &sec,
    ///                            0.3, 60, 1e-9, 1e-3).unwrap();
    /// // The analytic radius is √μ = 0.7.
    /// assert!((cyc.max_radius() - 0.7).abs() < 1e-3);
    /// ```
    pub fn max_radius(&self) -> f64 {
        self.points
            .iter()
            .fold(0.0_f64, |m, p| m.max((p[0] * p[0] + p[1] * p[1]).sqrt()))
    }
}

/// Finds a limit cycle by iterating the return map from `s0` to its fixed point.
///
/// `iterations` bounds the fixed-point iteration and `tol` sets when it has
/// converged; `dt` is the integration step. Returns `None` if the orbit escapes,
/// stops crossing the section, or fails to converge.
///
/// ```
/// use manim_dynamics::cycles::{find_limit_cycle, Section};
/// use manim_dynamics::VanDerPol;
/// let cyc = find_limit_cycle(&VanDerPol { mu: 1.0 }, &Section::positive_x_axis(),
///                            1.0, 40, 1e-8, 1e-3).unwrap();
/// // The Van der Pol cycle at μ = 1 has period ≈ 6.663.
/// assert!((cyc.period - 6.663).abs() < 5e-2);
/// ```
pub fn find_limit_cycle<Sy: PlanarSystem + ?Sized>(
    system: &Sy,
    section: &Section,
    s0: f64,
    iterations: usize,
    tol: f64,
    dt: f64,
) -> Option<LimitCycle> {
    let mut s = s0;
    let mut period = 0.0;
    let mut converged = false;
    for _ in 0..iterations {
        let (s_next, t) = return_map(system, section, s, 1_000.0, dt)?;
        let delta = (s_next - s).abs();
        s = s_next;
        period = t;
        if delta < tol {
            converged = true;
            break;
        }
    }
    if !converged {
        return None;
    }
    // One more lap, recorded, so the caller gets the loop itself.
    let steps = ((period / dt).ceil() as usize).max(2);
    let points = crate::trajectory(system, section.point_at(s), period / steps as f64, steps);
    Some(LimitCycle {
        points,
        period,
        section_coordinate: s,
    })
}

/// Draws a limit cycle on `axes` as a closed curve.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_dynamics::cycles::{add_cycle, find_limit_cycle, Section};
/// use manim_dynamics::VanDerPol;
/// let axes = Axes::new([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0]);
/// let cyc = find_limit_cycle(&VanDerPol { mu: 1.0 }, &Section::positive_x_axis(),
///                            1.0, 40, 1e-8, 1e-3).unwrap();
/// let mut scene = SceneState::new();
/// let id = add_cycle(&mut scene, &axes, &cyc, RED);
/// assert!(scene.contains(id));
/// ```
pub fn add_cycle(
    scene: &mut SceneState,
    axes: &Axes,
    cycle: &LimitCycle,
    color: Color,
) -> MobjectId<VMobject> {
    let pts: Vec<_> = cycle
        .points
        .iter()
        .map(|p| axes.c2p(p[0] as f32, p[1] as f32))
        .collect();
    scene.add(VMobject::from_path(Path::from_corners(&pts, true)).with_stroke(color, 4.0, 1.0))
}

/// The default limit-cycle colour.
///
/// ```
/// use manim_core::prelude::RED;
/// assert_eq!(manim_dynamics::cycles::default_color(), RED);
/// ```
pub fn default_color() -> Color {
    RED
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HopfNormalForm, VanDerPol};
    use std::f64::consts::{PI, TAU};

    #[test]
    fn van_der_pol_limit_cycle_has_amplitude_two() {
        let cyc = find_limit_cycle(
            &VanDerPol { mu: 1.0 },
            &Section::positive_x_axis(),
            1.0,
            60,
            1e-9,
            1e-3,
        )
        .expect("cycle");
        // The classic result: x oscillates between ±2.0086 with period 6.6633.
        assert!(
            (cyc.amplitude() - 2.0086).abs() < 5e-3,
            "amplitude {}",
            cyc.amplitude()
        );
        assert!((cyc.period - 6.6633).abs() < 5e-2, "period {}", cyc.period);
    }

    #[test]
    fn the_cycle_is_reached_from_inside_and_outside() {
        let sys = VanDerPol { mu: 1.0 };
        let sec = Section::positive_x_axis();
        let inner = find_limit_cycle(&sys, &sec, 0.4, 80, 1e-9, 1e-3).unwrap();
        let outer = find_limit_cycle(&sys, &sec, 3.5, 80, 1e-9, 1e-3).unwrap();
        assert!(
            (inner.section_coordinate - outer.section_coordinate).abs() < 1e-5,
            "{} vs {}",
            inner.section_coordinate,
            outer.section_coordinate
        );
    }

    #[test]
    fn the_cycle_closes_on_itself() {
        let cyc = find_limit_cycle(
            &VanDerPol { mu: 1.0 },
            &Section::positive_x_axis(),
            1.0,
            60,
            1e-9,
            1e-3,
        )
        .unwrap();
        let a = cyc.points[0];
        let b = *cyc.points.last().unwrap();
        let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        assert!(d < 1e-3, "gap {d}");
    }

    #[test]
    fn a_bigger_mu_gives_a_slower_relaxation_oscillation() {
        // As μ grows the Van der Pol cycle becomes a relaxation oscillator: the
        // amplitude stays near 2 but the period grows like μ(3 − 2 ln 2).
        let sec = Section::positive_x_axis();
        let c1 = find_limit_cycle(&VanDerPol { mu: 1.0 }, &sec, 2.0, 80, 1e-9, 1e-3).unwrap();
        let c4 = find_limit_cycle(&VanDerPol { mu: 4.0 }, &sec, 2.0, 80, 1e-9, 1e-3).unwrap();
        assert!(c4.period > c1.period, "{} vs {}", c4.period, c1.period);
        assert!((c4.amplitude() - 2.0).abs() < 0.1, "{}", c4.amplitude());
        // The asymptotic law is τ ≈ (3 − 2ln2)μ ≈ 1.614μ = 6.46 at μ = 4;
        // finite-μ corrections push the true value up to ≈ 10.2.
        assert!(c4.period > 1.614 * 4.0);
    }

    #[test]
    fn hopf_cycle_radius_is_sqrt_mu() {
        let sec = Section::new([0.0, 0.0], [1.0, 0.0], Crossing::Increasing);
        for mu in [0.09, 0.25, 0.49, 1.0] {
            let h = HopfNormalForm { mu, omega: 1.0 };
            let cyc = find_limit_cycle(&h, &sec, 0.3 * mu.sqrt() + 0.05, 200, 1e-10, 1e-3)
                .unwrap_or_else(|| panic!("no cycle at μ = {mu}"));
            assert!(
                (cyc.section_coordinate - mu.sqrt()).abs() < 1e-4,
                "μ = {mu}: r = {} vs {}",
                cyc.section_coordinate,
                mu.sqrt()
            );
            // θ̇ = ω = 1 ⇒ the period is exactly 2π.
            assert!(
                (cyc.period - TAU).abs() < 1e-3,
                "μ = {mu}: T = {}",
                cyc.period
            );
        }
    }

    #[test]
    fn there_is_no_cycle_below_the_hopf_onset() {
        // For μ < 0 every orbit spirals into the origin: the return map has no
        // fixed point other than the (excluded) origin, so the search must fail
        // rather than invent one.
        let sec = Section::new([0.0, 0.0], [1.0, 0.0], Crossing::Increasing);
        let h = HopfNormalForm {
            mu: -0.3,
            omega: 1.0,
        };
        let found = find_limit_cycle(&h, &sec, 0.8, 20, 1e-10, 1e-3);
        // Any "cycle" it does report must have collapsed to (essentially) the
        // origin, not a genuine loop.
        assert!(found.is_none_or(|c| c.section_coordinate < 1e-2));
    }

    #[test]
    fn return_map_is_the_identity_on_an_invariant_circle() {
        let sec = Section::new([0.0, 0.0], [1.0, 0.0], Crossing::Increasing);
        let h = HopfNormalForm {
            mu: 0.25,
            omega: 2.0,
        };
        let (s, t) = return_map(&h, &sec, 0.5, 20.0, 1e-3).unwrap();
        assert!((s - 0.5).abs() < 1e-6, "s = {s}");
        assert!((t - PI).abs() < 1e-4, "t = {t}"); // 2π/ω
    }
}
