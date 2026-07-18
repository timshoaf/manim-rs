//! Potential-well diagrams and the quantum-tunneling scene.
//!
//! Two things live here:
//!
//! - [`potential_well_diagram`] — the textbook figure: the potential curve
//!   `V(x)`, a horizontal energy level at each `Eₙ`, and each eigenfunction drawn
//!   riding on its own level.
//! - [`TunnelingScene`] — a Gaussian wavepacket with positive momentum launched
//!   at a rectangular barrier, evolved with the split-step Schrödinger stepper
//!   ([`Schrodinger1D`]). It measures the transmitted and reflected probability
//!   and hands out per-frame snapshots for animation.
//!
//! Data-space plotting reuses [`PlotTransform`].
//!
//! [`PlotTransform`]: crate::wavefunction::PlotTransform

use manim_core::prelude::*;
use manim_fields::complex::Complex;
use manim_fields::pde::{Complex as PdeComplex, Schrodinger1D};
use manim_math::path::Path;

use crate::wavefunction::{PlotTransform, Wavefunction1D};

/// Samples used to draw each smooth curve (potential, eigenfunctions).
const CURVE_SAMPLES: usize = 240;

/// Adds the canonical potential-well diagram and returns the group holding it.
///
/// The group contains, in order: the potential curve `V(x)` (stroked in
/// `WHITE`), then for each `(Eₙ, ψₙ)` pair a horizontal energy-level line at `Eₙ`
/// (in `YELLOW`) and the eigenfunction `Eₙ + psi_scale · ψₙ(x)` drawn riding on
/// that level (in `BLUE`). Levels and eigenfunctions are zipped, so extra
/// entries in the longer slice are ignored.
///
/// `x_range` is the inclusive `(min, max)` plotting window; `tf` maps data
/// `(x, value)` to scene space; `psi_scale` sets the visual amplitude of the
/// eigenfunctions relative to the energy axis.
///
/// ```
/// use manim_quantum::wells::potential_well_diagram;
/// use manim_quantum::wavefunction::PlotTransform;
/// use manim_core::scene_state::SceneState;
/// use manim_core::prelude::Point;
///
/// // Infinite square well on [0, 1]: E₁ and its ground state sin(πx).
/// let potential = |x: f64| if (0.0..=1.0).contains(&x) { 0.0 } else { 50.0 };
/// let e1 = std::f64::consts::PI.powi(2) / 2.0;
/// let psi1 = |x: f64| (std::f64::consts::PI * x).sin();
/// let eigenfns: [&dyn Fn(f64) -> f64; 1] = [&psi1];
///
/// let mut scene = SceneState::new();
/// let tf = PlotTransform::new(Point::new(0.0, -2.0, 0.0), 4.0, 0.4);
/// let g = potential_well_diagram(&mut scene, potential, &[e1], &eigenfns, (-0.2, 1.2), &tf, 1.0);
/// // group + V-curve + 1 level line + 1 eigenfunction curve.
/// assert_eq!(scene.family(g.erase()).len(), 4);
/// ```
pub fn potential_well_diagram(
    scene: &mut SceneState,
    potential: impl Fn(f64) -> f64,
    energies: &[f64],
    eigenfns: &[&dyn Fn(f64) -> f64],
    x_range: (f64, f64),
    tf: &PlotTransform,
    psi_scale: f64,
) -> MobjectId<VGroup> {
    let xs: Vec<f64> = (0..CURVE_SAMPLES)
        .map(|i| {
            let t = i as f64 / (CURVE_SAMPLES - 1) as f64;
            x_range.0 + t * (x_range.1 - x_range.0)
        })
        .collect();

    let mut members: Vec<AnyId> = Vec::new();

    // The potential curve V(x).
    let v_pts: Vec<Point> = xs.iter().map(|&x| tf.map(x, potential(x))).collect();
    let v_curve = scene
        .add(VMobject::from_path(Path::from_corners(&v_pts, false)).with_stroke(WHITE, 3.0, 1.0));
    members.push(v_curve.erase());

    for (&energy, eigenfn) in energies.iter().zip(eigenfns.iter()) {
        // Horizontal energy level.
        let level = scene.add(
            Line::new(tf.map(x_range.0, energy), tf.map(x_range.1, energy))
                .with_stroke(YELLOW, 2.0, 1.0),
        );
        members.push(level.erase());

        // Eigenfunction riding on the level.
        let psi_pts: Vec<Point> = xs
            .iter()
            .map(|&x| tf.map(x, energy + psi_scale * eigenfn(x)))
            .collect();
        let psi_curve = scene.add(
            VMobject::from_path(Path::from_corners(&psi_pts, false)).with_stroke(BLUE, 3.0, 1.0),
        );
        members.push(psi_curve.erase());
    }

    VGroup::of(scene, members)
}

/// Parameters defining a [`TunnelingScene`].
///
/// The defaults set up the demo the module is built around: a well-resolved
/// Gaussian packet of mean momentum `k0 = 3` (mean energy `4.5`) incident on a
/// rectangular barrier of height `6` and width `0.5`, on a periodic box wide
/// enough that neither the transmitted nor the reflected packet wraps around
/// before the measurement. These give **partial** tunneling — both `T` and `R`
/// safely between 0 and 1.
#[derive(Clone, Copy, Debug)]
pub struct TunnelingParams {
    /// Number of grid points (periodic box length `n · dx`).
    pub n: usize,
    /// Grid spacing.
    pub dx: f64,
    /// Left edge of the box.
    pub x_min: f64,
    /// Particle mass (ħ = 1).
    pub mass: f64,
    /// Initial packet center.
    pub x0: f64,
    /// Initial packet width (position standard deviation).
    pub sigma: f64,
    /// Mean wavenumber (mean momentum, ħ = 1); positive → moving right.
    pub k0: f64,
    /// Barrier height.
    pub v0: f64,
    /// Barrier center.
    pub barrier_center: f64,
    /// Barrier width.
    pub barrier_width: f64,
    /// Integration time step.
    pub dt: f64,
}

impl Default for TunnelingParams {
    fn default() -> Self {
        Self {
            n: 1024,
            dx: 0.1,
            x_min: -51.2,
            mass: 1.0,
            x0: -20.0,
            sigma: 2.0,
            k0: 3.0,
            v0: 6.0,
            barrier_center: 0.0,
            barrier_width: 0.5,
            dt: 0.02,
        }
    }
}

/// A Gaussian wavepacket tunneling through a rectangular barrier.
///
/// Wraps a [`Schrodinger1D`] split-step integrator: [`step`](Self::step) /
/// [`evolve_to`](Self::evolve_to) advance it, [`transmission`](Self::transmission)
/// and [`reflection`](Self::reflection) measure the probability that has crossed
/// past the barrier or bounced back, and [`wavefunction_snapshot`](Self::wavefunction_snapshot)
/// returns a [`Wavefunction1D`] for drawing.
///
/// ```
/// use manim_quantum::wells::{TunnelingParams, TunnelingScene};
/// let mut ts = TunnelingScene::new(TunnelingParams::default());
/// let n0 = ts.total_probability();
/// ts.evolve_to(1.0);
/// // Norm is conserved to machine precision by the unitary stepper.
/// assert!((ts.total_probability() - n0).abs() / n0 < 1e-6);
/// ```
pub struct TunnelingScene {
    params: TunnelingParams,
    sim: Schrodinger1D,
    total0: f64,
    time: f64,
}

impl TunnelingScene {
    /// Builds the scene from `params`: a Gaussian packet
    /// `ψ₀(x) = (2πσ²)^{-1/4} e^{-(x-x₀)²/4σ²} e^{i k₀ x}` on the periodic box,
    /// with the rectangular barrier as the potential.
    pub fn new(params: TunnelingParams) -> Self {
        let bl = params.barrier_center - 0.5 * params.barrier_width;
        let br = params.barrier_center + 0.5 * params.barrier_width;
        let v0 = params.v0;
        let potential = move |x: f64| if x >= bl && x <= br { v0 } else { 0.0 };

        let (x0, sigma, k0) = (params.x0, params.sigma, params.k0);
        let norm = (std::f64::consts::TAU * sigma * sigma).powf(-0.25);
        let psi0 = move |x: f64| {
            let d = x - x0;
            let envelope = norm * (-d * d / (4.0 * sigma * sigma)).exp();
            // rustfft's Complex (the stepper's buffer type): envelope · e^{i k0 x}.
            PdeComplex::from_polar(envelope, k0 * x)
        };

        let sim = Schrodinger1D::from_fn(
            params.n,
            params.x_min,
            params.dx,
            params.mass,
            potential,
            psi0,
        );
        let total0 = sim.norm();
        Self {
            params,
            sim,
            total0,
            time: 0.0,
        }
    }

    /// The parameters this scene was built with.
    pub fn params(&self) -> TunnelingParams {
        self.params
    }

    /// Advances the evolution by one time step.
    pub fn step(&mut self) {
        self.sim.step(self.params.dt);
        self.time += self.params.dt;
    }

    /// Advances until the simulated time reaches at least `t_end`.
    pub fn evolve_to(&mut self, t_end: f64) {
        while self.time < t_end - 1e-9 {
            self.step();
        }
    }

    /// The current simulated time.
    pub fn time(&self) -> f64 {
        self.time
    }

    /// The grid coordinates.
    pub fn xs(&self) -> &[f64] {
        self.sim.x()
    }

    /// The probability density `|ψ(x)|²` at each grid point.
    pub fn probability_density(&self) -> Vec<f64> {
        self.sim.probability_density()
    }

    /// The total probability `∫|ψ|² dx` (conserved by the unitary stepper).
    pub fn total_probability(&self) -> f64 {
        self.sim.norm()
    }

    /// Mean wavenumber `k₀`.
    pub fn k0(&self) -> f64 {
        self.params.k0
    }

    /// Mean kinetic energy `k₀² / 2m` of the incident packet.
    pub fn mean_energy(&self) -> f64 {
        self.params.k0 * self.params.k0 / (2.0 * self.params.mass)
    }

    /// Barrier height `V₀`.
    pub fn barrier_height(&self) -> f64 {
        self.params.v0
    }

    /// Left edge of the barrier.
    pub fn barrier_left(&self) -> f64 {
        self.params.barrier_center - 0.5 * self.params.barrier_width
    }

    /// Right edge of the barrier.
    pub fn barrier_right(&self) -> f64 {
        self.params.barrier_center + 0.5 * self.params.barrier_width
    }

    /// The fraction of probability that has transmitted past the barrier
    /// (`∫_{x > barrier_right} |ψ|² dx`), normalized by the initial total.
    pub fn transmission(&self) -> f64 {
        self.integrate_where(|x| x > self.barrier_right())
    }

    /// The fraction of probability reflected back before the barrier
    /// (`∫_{x < barrier_left} |ψ|² dx`), normalized by the initial total.
    pub fn reflection(&self) -> f64 {
        self.integrate_where(|x| x < self.barrier_left())
    }

    fn integrate_where(&self, keep: impl Fn(f64) -> bool) -> f64 {
        let dens = self.sim.probability_density();
        let acc: f64 = self
            .xs()
            .iter()
            .zip(&dens)
            .filter(|(&x, _)| keep(x))
            .map(|(_, &p)| p)
            .sum();
        acc * self.params.dx / self.total0
    }

    /// A drawable snapshot of the current state as a [`Wavefunction1D`].
    ///
    /// The exact `|ψ|²` comes from the stepper; the phase is the leading-order
    /// carrier `arg ψ ≈ k₀ x − E t` (the stepper keeps its complex buffer
    /// private, so the fast-oscillating carrier is reconstructed analytically —
    /// enough to drive the phase-hue coloring).
    pub fn wavefunction_snapshot(&self) -> Wavefunction1D {
        let dens = self.sim.probability_density();
        let energy = self.mean_energy();
        let k0 = self.params.k0;
        let t = self.time;
        let xs = self.xs().to_vec();
        let psi = xs
            .iter()
            .zip(&dens)
            .map(|(&x, &p)| Complex::from_polar(p.sqrt(), k0 * x - energy * t))
            .collect();
        Wavefunction1D::from_samples(xs, psi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn potential_well_diagram_groups_all_pieces() {
        let potential = |x: f64| if (0.0..=1.0).contains(&x) { 0.0 } else { 50.0 };
        let e1 = std::f64::consts::PI.powi(2) / 2.0;
        let e2 = 4.0 * e1;
        let psi1 = |x: f64| (std::f64::consts::PI * x).sin();
        let psi2 = |x: f64| (2.0 * std::f64::consts::PI * x).sin();
        let eigenfns: [&dyn Fn(f64) -> f64; 2] = [&psi1, &psi2];

        let mut scene = SceneState::new();
        let tf = PlotTransform::new(Point::new(0.0, -2.0, 0.0), 4.0, 0.2);
        let g = potential_well_diagram(
            &mut scene,
            potential,
            &[e1, e2],
            &eigenfns,
            (-0.2, 1.2),
            &tf,
            1.0,
        );
        // group + V-curve + 2 level lines + 2 eigenfunction curves = 6.
        assert_eq!(scene.family(g.erase()).len(), 6);
    }

    #[test]
    fn tunneling_conserves_norm_and_partially_transmits() {
        let mut ts = TunnelingScene::new(TunnelingParams::default());
        let n0 = ts.total_probability();

        // Evolve until both the transmitted and reflected packets have cleared
        // the barrier (group velocity 3, ~7 units of travel to the barrier).
        ts.evolve_to(20.0);

        let t = ts.transmission();
        let r = ts.reflection();
        let drift = (ts.total_probability() - n0).abs() / n0;
        println!(
            "tunneling: T = {t:.4}  R = {r:.4}  T+R = {:.6}  norm drift = {drift:.2e}",
            t + r
        );

        // Unitary evolution conserves norm to machine precision.
        assert!(drift < 1e-6, "norm drifted: {drift:.2e}");
        // Norm conservation: nothing is left inside the barrier region.
        assert!((t + r - 1.0).abs() < 1e-3, "T + R = {} not ~1", t + r);
        // Partial tunneling: neither channel is empty or full.
        assert!(t > 0.01 && t < 0.99, "T = {t} not strictly in (0, 1)");
        assert!(r > 0.01 && r < 0.99, "R = {r} not strictly in (0, 1)");
    }
}
