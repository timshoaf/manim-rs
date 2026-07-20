//! `manim-dynamics`: a planar dynamical-systems kit — phase portraits and
//! everything you draw on top of one.
//!
//! A system is anything implementing [`PlanarSystem`], written **once**
//! generically over the AD [`manim_fields::ad::Scalar`] type. Writing it
//! that way is what lets the crate differentiate it exactly: equilibrium
//! classification, Newton refinement, and separatrix directions all read the
//! Jacobian off forward-mode dual numbers, never off a finite difference.
//!
//! - [`phase`] — direction-field arrows and streamlines over an
//!   [`Axes`](manim_core::graphing::Axes) window.
//! - [`equilibria`] — grid seeds → Newton → eigenvalue classification (saddle,
//!   node, spiral, centre) with a marker per class.
//! - [`nullclines`] — marching-squares zero contours of each component.
//! - [`separatrix`] — the four saddle branches, integrated out along the
//!   eigenvectors in both time directions.
//! - [`cycles`] — Poincaré return map, and the limit cycle its fixed point names.
//! - [`bifurcation`] — one-parameter attractor sweeps, with logistic-map and
//!   Hopf presets.
//!
//! ```
//! use manim_dynamics::{jacobian, value, Pendulum, PlanarSystem};
//! let p = Pendulum { damping: 0.0 };
//! // The inverted pendulum is an equilibrium…
//! let f = value(&p, std::f64::consts::PI, 0.0);
//! assert!(f[0].abs() < 1e-15 && f[1].abs() < 1e-15);
//! // …and its Jacobian has eigenvalues ±1: a saddle.
//! let j = jacobian(&p, std::f64::consts::PI, 0.0);
//! assert!((j[1][0] - 1.0).abs() < 1e-12);
//! ```

pub mod bifurcation;
pub mod cycles;
pub mod equilibria;
pub mod nullclines;
pub mod phase;
pub mod separatrix;

use manim_fields::ad::{Dual, Scalar};
use manim_fields::integrate::rk45;

/// A planar autonomous system `(ẋ, ẏ) = f(x, y)`, written generically over the
/// AD scalar so its Jacobian is exact.
///
/// ```
/// use manim_dynamics::{value, PlanarSystem};
/// use manim_fields::ad::Scalar;
/// // The harmonic oscillator ẋ = y, ẏ = −x.
/// struct Sho;
/// impl PlanarSystem for Sho {
///     fn eval<S: Scalar>(&self, x: S, y: S) -> [S; 2] { [y, -x] }
/// }
/// assert_eq!(value(&Sho, 2.0, 3.0), [3.0, -2.0]);
/// ```
pub trait PlanarSystem {
    /// The vector field at `(x, y)`.
    fn eval<S: Scalar>(&self, x: S, y: S) -> [S; 2];
}

/// The field value at `(x, y)`.
///
/// ```
/// use manim_dynamics::{value, VanDerPol};
/// // At the origin the Van der Pol field vanishes.
/// assert_eq!(value(&VanDerPol { mu: 1.0 }, 0.0, 0.0), [0.0, 0.0]);
/// ```
pub fn value<Sy: PlanarSystem + ?Sized>(system: &Sy, x: f64, y: f64) -> [f64; 2] {
    system.eval::<f64>(x, y)
}

/// The exact Jacobian `[[∂ẋ/∂x, ∂ẋ/∂y], [∂ẏ/∂x, ∂ẏ/∂y]]` by forward-mode AD.
///
/// Two dual-number evaluations, one seeded in each coordinate — no step size to
/// choose and no truncation error.
///
/// ```
/// use manim_dynamics::{jacobian, Linear};
/// let j = jacobian(&Linear { a: 1.0, b: 2.0, c: 3.0, d: 4.0 }, 0.0, 0.0);
/// assert_eq!(j, [[1.0, 2.0], [3.0, 4.0]]);
/// ```
pub fn jacobian<Sy: PlanarSystem + ?Sized>(system: &Sy, x: f64, y: f64) -> [[f64; 2]; 2] {
    let dx = system.eval(Dual::var(x), Dual::constant(y));
    let dy = system.eval(Dual::constant(x), Dual::var(y));
    [[dx[0].du, dy[0].du], [dx[1].du, dy[1].du]]
}

/// The trace of a 2×2 matrix.
///
/// ```
/// use manim_dynamics::trace;
/// assert_eq!(trace([[1.0, 2.0], [3.0, 4.0]]), 5.0);
/// ```
pub fn trace(j: [[f64; 2]; 2]) -> f64 {
    j[0][0] + j[1][1]
}

/// The determinant of a 2×2 matrix.
///
/// ```
/// use manim_dynamics::determinant;
/// assert_eq!(determinant([[1.0, 2.0], [3.0, 4.0]]), -2.0);
/// ```
pub fn determinant(j: [[f64; 2]; 2]) -> f64 {
    j[0][0] * j[1][1] - j[0][1] * j[1][0]
}

/// Integrates the system from `start` for `steps` output points spaced `dt`
/// apart, adaptively (Dormand–Prince 5(4)) between them.
///
/// A negative `dt` integrates backwards in time by flipping the field, which is
/// how the stable manifolds in [`separatrix`] are traced.
///
/// ```
/// use manim_dynamics::{trajectory, Linear};
/// // ẋ = −x, ẏ = −y: everything decays to the origin.
/// let path = trajectory(&Linear { a: -1.0, b: 0.0, c: 0.0, d: -1.0 }, [1.0, 1.0], 0.1, 50);
/// assert_eq!(path.len(), 51);
/// assert!(path[50][0].abs() < 0.02);
/// ```
pub fn trajectory<Sy: PlanarSystem + ?Sized>(
    system: &Sy,
    start: [f64; 2],
    dt: f64,
    steps: usize,
) -> Vec<[f64; 2]> {
    let backwards = dt < 0.0;
    let h = dt.abs();
    let f = |_t: f64, y: &[f64]| {
        let v = value(system, y[0], y[1]);
        if backwards {
            vec![-v[0], -v[1]]
        } else {
            vec![v[0], v[1]]
        }
    };
    let mut out = Vec::with_capacity(steps + 1);
    out.push(start);
    let mut y = vec![start[0], start[1]];
    for _ in 0..steps {
        y = rk45(&f, 0.0, &y, h, 1e-9, 1e-9);
        if !y[0].is_finite() || !y[1].is_finite() {
            break;
        }
        out.push([y[0], y[1]]);
    }
    out
}

/// The undamped-or-damped pendulum `θ̈ + b θ̇ + sin θ = 0`, as
/// `(θ̇, ω̇) = (ω, −sin θ − b ω)`.
///
/// Its equilibria are `(kπ, 0)`: the hanging states `(2kπ, 0)` are centres when
/// undamped (stable spirals when damped), the inverted states `((2k+1)π, 0)` are
/// saddles at any damping.
///
/// ```
/// use manim_dynamics::{value, Pendulum};
/// assert_eq!(value(&Pendulum { damping: 0.5 }, 0.0, 2.0), [2.0, -1.0]);
/// ```
pub struct Pendulum {
    /// The linear damping coefficient `b` (0 for the conservative pendulum).
    pub damping: f64,
}

impl PlanarSystem for Pendulum {
    fn eval<S: Scalar>(&self, x: S, y: S) -> [S; 2] {
        [y, -x.sin() - y.scale(self.damping)]
    }
}

/// The Van der Pol oscillator `ẍ − μ(1 − x²)ẋ + x = 0`, as
/// `(ẋ, ẏ) = (y, μ(1 − x²)y − x)`.
///
/// For `μ > 0` the origin is an unstable spiral and every other orbit winds onto
/// a unique limit cycle — the textbook example of self-sustained oscillation.
///
/// ```
/// use manim_dynamics::{value, VanDerPol};
/// assert_eq!(value(&VanDerPol { mu: 1.0 }, 0.0, 1.0), [1.0, 1.0]);
/// ```
pub struct VanDerPol {
    /// The nonlinearity strength `μ`.
    pub mu: f64,
}

impl PlanarSystem for VanDerPol {
    fn eval<S: Scalar>(&self, x: S, y: S) -> [S; 2] {
        [y, (S::constant(1.0) - x * x) * y.scale(self.mu) - x]
    }
}

/// A linear system `(ẋ, ẏ) = (ax + by, cx + dy)` — the local model every
/// hyperbolic equilibrium is conjugate to.
///
/// ```
/// use manim_dynamics::{jacobian, Linear};
/// // A pure rotation: trace 0, determinant 1 — a centre.
/// let rot = Linear { a: 0.0, b: -1.0, c: 1.0, d: 0.0 };
/// assert_eq!(jacobian(&rot, 3.0, -2.0), [[0.0, -1.0], [1.0, 0.0]]);
/// ```
pub struct Linear {
    /// `∂ẋ/∂x`.
    pub a: f64,
    /// `∂ẋ/∂y`.
    pub b: f64,
    /// `∂ẏ/∂x`.
    pub c: f64,
    /// `∂ẏ/∂y`.
    pub d: f64,
}

impl PlanarSystem for Linear {
    fn eval<S: Scalar>(&self, x: S, y: S) -> [S; 2] {
        [
            x.scale(self.a) + y.scale(self.b),
            x.scale(self.c) + y.scale(self.d),
        ]
    }
}

/// The supercritical Hopf normal form in Cartesian coordinates,
/// `ẋ = μx − ωy − x(x²+y²)`, `ẏ = ωx + μy − y(x²+y²)`.
///
/// In polar form this is `ṙ = μr − r³`, `θ̇ = ω`: the origin loses stability at
/// `μ = 0` and a limit cycle of radius `√μ` is born — the Hopf bifurcation, with
/// its onset and amplitude both known in closed form.
///
/// ```
/// use manim_dynamics::{value, HopfNormalForm};
/// // On the invariant circle r = √μ the radial motion stops.
/// let h = HopfNormalForm { mu: 0.25, omega: 1.0 };
/// let v = value(&h, 0.5, 0.0);
/// assert!(v[0].abs() < 1e-15);
/// ```
pub struct HopfNormalForm {
    /// The bifurcation parameter `μ` (cycle radius `√μ` for `μ > 0`).
    pub mu: f64,
    /// The rotation rate `ω`.
    pub omega: f64,
}

impl PlanarSystem for HopfNormalForm {
    fn eval<S: Scalar>(&self, x: S, y: S) -> [S; 2] {
        let r2 = x * x + y * y;
        [
            x.scale(self.mu) - y.scale(self.omega) - x * r2,
            x.scale(self.omega) + y.scale(self.mu) - y * r2,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jacobian_is_exact_for_the_pendulum() {
        let p = Pendulum { damping: 0.3 };
        for &(x, y) in &[(0.0, 0.0), (1.0, -2.0), (std::f64::consts::PI, 0.5)] {
            let j = jacobian(&p, x, y);
            // Analytic: [[0, 1], [−cos x, −b]].
            assert!((j[0][0]).abs() < 1e-15);
            assert!((j[0][1] - 1.0).abs() < 1e-15);
            assert!((j[1][0] + x.cos()).abs() < 1e-14);
            assert!((j[1][1] + 0.3).abs() < 1e-15);
        }
    }

    #[test]
    fn jacobian_is_exact_for_van_der_pol() {
        let v = VanDerPol { mu: 2.0 };
        let (x, y) = (0.7, -1.3);
        let j = jacobian(&v, x, y);
        // Analytic: [[0, 1], [−2μxy − 1, μ(1 − x²)]].
        assert!((j[1][0] - (-2.0 * 2.0 * x * y - 1.0)).abs() < 1e-13);
        assert!((j[1][1] - 2.0 * (1.0 - x * x)).abs() < 1e-13);
    }

    #[test]
    fn trajectory_matches_the_analytic_flow_of_a_rotation() {
        // ẋ = −y, ẏ = x rotates at unit rate: after time t, angle t.
        let rot = Linear {
            a: 0.0,
            b: -1.0,
            c: 1.0,
            d: 0.0,
        };
        let path = trajectory(&rot, [1.0, 0.0], 0.05, 126); // ≈ 2π
        for (i, p) in path.iter().enumerate() {
            let t = i as f64 * 0.05;
            assert!((p[0] - t.cos()).abs() < 1e-6, "step {i}");
            assert!((p[1] - t.sin()).abs() < 1e-6, "step {i}");
            // The rotation is an isometry: the radius never drifts.
            assert!(((p[0] * p[0] + p[1] * p[1]).sqrt() - 1.0).abs() < 1e-7);
        }
    }

    #[test]
    fn backward_integration_undoes_forward_integration() {
        let v = VanDerPol { mu: 0.6 };
        let fwd = trajectory(&v, [0.4, 0.1], 0.02, 200);
        let end = *fwd.last().unwrap();
        let back = trajectory(&v, end, -0.02, 200);
        let home = *back.last().unwrap();
        assert!((home[0] - 0.4).abs() < 1e-5, "x back to {}", home[0]);
        assert!((home[1] - 0.1).abs() < 1e-5, "y back to {}", home[1]);
    }

    #[test]
    fn hopf_circle_is_invariant() {
        let h = HopfNormalForm {
            mu: 0.36,
            omega: 1.5,
        };
        let r = 0.6; // √0.36
        let path = trajectory(&h, [r, 0.0], 0.05, 200);
        for p in &path {
            let rad = (p[0] * p[0] + p[1] * p[1]).sqrt();
            assert!((rad - r).abs() < 1e-6, "radius drifted to {rad}");
        }
    }
}
