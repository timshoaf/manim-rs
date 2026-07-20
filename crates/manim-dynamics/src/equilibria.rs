//! Finding equilibria and naming them.
//!
//! Newton's method from a grid of seeds finds the points where `f(x, y) = 0`;
//! the exact Jacobian there classifies each one. For a planar system the whole
//! classification is a function of two numbers, the trace `τ` and determinant
//! `Δ` of the Jacobian:
//!
//! | condition | type |
//! |---|---|
//! | `Δ < 0` | saddle |
//! | `Δ > 0`, `τ² > 4Δ`, `τ < 0` | stable node |
//! | `Δ > 0`, `τ² > 4Δ`, `τ > 0` | unstable node |
//! | `Δ > 0`, `τ² < 4Δ`, `τ < 0` | stable spiral |
//! | `Δ > 0`, `τ² < 4Δ`, `τ > 0` | unstable spiral |
//! | `Δ > 0`, `τ = 0` | centre (linearly) |
//!
//! A linear centre is the one verdict the linearisation cannot settle — the
//! nonlinear terms decide whether it is a true centre or a slow spiral — so
//! [`Equilibrium::kind`] reports [`EquilibriumKind::Center`] as what the
//! *linearisation* says, no more.

use manim_core::geometry::{Circle, Dot, Line, VGroup};
use manim_core::graphing::Axes;
use manim_core::mobject::{AnyId, Buildable, MobjectId};
use manim_core::prelude::{Color, BLUE, GREEN, RED, WHITE, YELLOW};
use manim_core::scene_state::SceneState;

use crate::{determinant, jacobian, trace, value, PlanarSystem};

/// How an equilibrium behaves, as read off the Jacobian's eigenvalues.
///
/// ```
/// use manim_dynamics::equilibria::{classify, EquilibriumKind};
/// // ẋ = x, ẏ = −y — the canonical saddle.
/// assert_eq!(classify([[1.0, 0.0], [0.0, -1.0]]), EquilibriumKind::Saddle);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquilibriumKind {
    /// Real eigenvalues of opposite sign: attracting along one direction,
    /// repelling along the other.
    Saddle,
    /// Real, both negative: everything nearby flows straight in.
    StableNode,
    /// Real, both positive: everything nearby flows straight out.
    UnstableNode,
    /// Complex with negative real part: spiralling in.
    StableSpiral,
    /// Complex with positive real part: spiralling out.
    UnstableSpiral,
    /// Purely imaginary: the linearisation predicts closed orbits, and the
    /// nonlinear terms decide.
    Center,
    /// A zero eigenvalue — the linearisation says nothing.
    Degenerate,
}

impl EquilibriumKind {
    /// Whether nearby orbits converge to this equilibrium.
    ///
    /// ```
    /// use manim_dynamics::equilibria::EquilibriumKind;
    /// assert!(EquilibriumKind::StableSpiral.is_attracting());
    /// assert!(!EquilibriumKind::Saddle.is_attracting());
    /// ```
    pub fn is_attracting(self) -> bool {
        matches!(self, Self::StableNode | Self::StableSpiral)
    }

    /// The conventional marker colour for this class: attractors green,
    /// repellers red, saddles yellow, centres blue, degenerate white.
    ///
    /// ```
    /// use manim_core::prelude::GREEN;
    /// use manim_dynamics::equilibria::EquilibriumKind;
    /// assert_eq!(EquilibriumKind::StableNode.color(), GREEN);
    /// ```
    pub fn color(self) -> Color {
        match self {
            Self::StableNode | Self::StableSpiral => GREEN,
            Self::UnstableNode | Self::UnstableSpiral => RED,
            Self::Saddle => YELLOW,
            Self::Center => BLUE,
            Self::Degenerate => WHITE,
        }
    }
}

/// Classifies a 2×2 Jacobian by its trace and determinant.
///
/// ```
/// use manim_dynamics::equilibria::{classify, EquilibriumKind};
/// // A pure rotation is a linear centre.
/// assert_eq!(classify([[0.0, -1.0], [1.0, 0.0]]), EquilibriumKind::Center);
/// // Complex eigenvalues with negative real part spiral in.
/// assert_eq!(classify([[-0.5, -1.0], [1.0, -0.5]]), EquilibriumKind::StableSpiral);
/// ```
pub fn classify(j: [[f64; 2]; 2]) -> EquilibriumKind {
    let (tau, delta) = (trace(j), determinant(j));
    let scale = j
        .iter()
        .flatten()
        .fold(0.0_f64, |m, v| m.max(v.abs()))
        .max(1e-12);
    // Tolerances carry the matrix's units: the trace scales like `scale`, the
    // determinant like `scale²`.
    let tau_eps = 1e-9 * scale;
    if delta.abs() < 1e-9 * scale * scale {
        return EquilibriumKind::Degenerate;
    }
    if delta < 0.0 {
        return EquilibriumKind::Saddle;
    }
    let disc = tau * tau - 4.0 * delta;
    if disc >= 0.0 {
        if tau < 0.0 {
            EquilibriumKind::StableNode
        } else {
            EquilibriumKind::UnstableNode
        }
    } else if tau.abs() < tau_eps {
        EquilibriumKind::Center
    } else if tau < 0.0 {
        EquilibriumKind::StableSpiral
    } else {
        EquilibriumKind::UnstableSpiral
    }
}

/// The real eigenvalue pair of a 2×2 matrix, smaller first, or `None` when the
/// eigenvalues are complex.
///
/// ```
/// use manim_dynamics::equilibria::real_eigenvalues;
/// let e = real_eigenvalues([[1.0, 0.0], [0.0, -2.0]]).unwrap();
/// assert!((e[0] + 2.0).abs() < 1e-12 && (e[1] - 1.0).abs() < 1e-12);
/// assert!(real_eigenvalues([[0.0, -1.0], [1.0, 0.0]]).is_none());
/// ```
pub fn real_eigenvalues(j: [[f64; 2]; 2]) -> Option<[f64; 2]> {
    let (tau, delta) = (trace(j), determinant(j));
    let disc = tau * tau - 4.0 * delta;
    if disc < 0.0 {
        return None;
    }
    let s = disc.sqrt();
    Some([(tau - s) / 2.0, (tau + s) / 2.0])
}

/// A unit eigenvector of `j` for the real eigenvalue `lambda`.
///
/// ```
/// use manim_dynamics::equilibria::eigenvector;
/// // [[1,0],[0,-1]] has eigenvector (1,0) for λ = 1.
/// let v = eigenvector([[1.0, 0.0], [0.0, -1.0]], 1.0);
/// assert!((v[0].abs() - 1.0).abs() < 1e-12 && v[1].abs() < 1e-12);
/// ```
pub fn eigenvector(j: [[f64; 2]; 2], lambda: f64) -> [f64; 2] {
    let scale = j
        .iter()
        .flatten()
        .fold(0.0_f64, |m, v| m.max(v.abs()))
        .max(1e-12);
    // (J − λI)v = 0: read a null vector off whichever row is non-degenerate.
    let candidates = [
        [j[0][1], lambda - j[0][0]],
        [lambda - j[1][1], j[1][0]],
        [1.0, 0.0],
    ];
    for v in candidates {
        let n = (v[0] * v[0] + v[1] * v[1]).sqrt();
        if n > 1e-9 * scale {
            return [v[0] / n, v[1] / n];
        }
    }
    [1.0, 0.0]
}

/// An equilibrium: where it is, what it is, and the local linear data.
#[derive(Clone, Copy, Debug)]
pub struct Equilibrium {
    /// The fixed point `(x, y)`.
    pub point: [f64; 2],
    /// Its class.
    pub kind: EquilibriumKind,
    /// The Jacobian there.
    pub jacobian: [[f64; 2]; 2],
}

impl Equilibrium {
    /// The trace of the local Jacobian.
    ///
    /// ```
    /// use manim_dynamics::equilibria::find_equilibria;
    /// use manim_dynamics::Linear;
    /// let eqs = find_equilibria(&Linear { a: -1.0, b: 0.0, c: 0.0, d: -2.0 },
    ///                          (-1.0, 1.0), (-1.0, 1.0), 5);
    /// assert!((eqs[0].trace() + 3.0).abs() < 1e-9);
    /// ```
    pub fn trace(&self) -> f64 {
        trace(self.jacobian)
    }

    /// The determinant of the local Jacobian.
    ///
    /// ```
    /// use manim_dynamics::equilibria::find_equilibria;
    /// use manim_dynamics::Linear;
    /// let eqs = find_equilibria(&Linear { a: -1.0, b: 0.0, c: 0.0, d: -2.0 },
    ///                          (-1.0, 1.0), (-1.0, 1.0), 5);
    /// assert!((eqs[0].determinant() - 2.0).abs() < 1e-9);
    /// ```
    pub fn determinant(&self) -> f64 {
        determinant(self.jacobian)
    }

    /// The stable and unstable eigen-directions of a saddle, as unit vectors
    /// `(stable, unstable)` — `None` for any other class.
    ///
    /// ```
    /// use manim_dynamics::equilibria::{find_equilibria, EquilibriumKind};
    /// use manim_dynamics::Linear;
    /// let saddle = Linear { a: 1.0, b: 0.0, c: 0.0, d: -1.0 };
    /// let eq = find_equilibria(&saddle, (-1.0, 1.0), (-1.0, 1.0), 5)[0];
    /// assert_eq!(eq.kind, EquilibriumKind::Saddle);
    /// let (s, u) = eq.saddle_directions().unwrap();
    /// // Stable along y, unstable along x.
    /// assert!(s[0].abs() < 1e-9 && u[1].abs() < 1e-9);
    /// ```
    pub fn saddle_directions(&self) -> Option<([f64; 2], [f64; 2])> {
        if self.kind != EquilibriumKind::Saddle {
            return None;
        }
        let [lo, hi] = real_eigenvalues(self.jacobian)?;
        Some((
            eigenvector(self.jacobian, lo),
            eigenvector(self.jacobian, hi),
        ))
    }
}

/// Refines a seed towards a nearby equilibrium with damped Newton iterations on
/// the exact Jacobian, returning `None` if it fails to converge.
///
/// ```
/// use manim_dynamics::equilibria::newton_refine;
/// use manim_dynamics::Pendulum;
/// let p = newton_refine(&Pendulum { damping: 0.2 }, [3.0, 0.4]).unwrap();
/// assert!((p[0] - std::f64::consts::PI).abs() < 1e-10 && p[1].abs() < 1e-10);
/// ```
pub fn newton_refine<Sy: PlanarSystem + ?Sized>(system: &Sy, seed: [f64; 2]) -> Option<[f64; 2]> {
    let mut p = seed;
    for _ in 0..64 {
        let f = value(system, p[0], p[1]);
        if f[0].abs() < 1e-13 && f[1].abs() < 1e-13 {
            return Some(p);
        }
        let j = jacobian(system, p[0], p[1]);
        let det = determinant(j);
        if det.abs() < 1e-14 {
            return None;
        }
        // Solve J·δ = f by Cramer's rule, then step against it.
        let dx = (f[0] * j[1][1] - f[1] * j[0][1]) / det;
        let dy = (j[0][0] * f[1] - j[1][0] * f[0]) / det;
        // Cap the step so a near-singular Jacobian cannot fling the seed away.
        let len = (dx * dx + dy * dy).sqrt();
        let damp = if len > 1.0 { 1.0 / len } else { 1.0 };
        p = [p[0] - dx * damp, p[1] - dy * damp];
        if !p[0].is_finite() || !p[1].is_finite() {
            return None;
        }
    }
    let f = value(system, p[0], p[1]);
    (f[0].abs() < 1e-9 && f[1].abs() < 1e-9).then_some(p)
}

/// Finds every equilibrium in the window by Newton refinement from an
/// `n × n` grid of seeds, de-duplicated and classified.
///
/// Equilibria that Newton walks to from outside the window are discarded, so the
/// result is exactly what a portrait of that window should show.
///
/// ```
/// use manim_dynamics::equilibria::{find_equilibria, EquilibriumKind};
/// use manim_dynamics::Pendulum;
/// // One period of the undamped pendulum: a centre at 0 and saddles at ±π.
/// let eqs = find_equilibria(&Pendulum { damping: 0.0 }, (-4.0, 4.0), (-3.0, 3.0), 21);
/// assert_eq!(eqs.len(), 3);
/// assert_eq!(eqs.iter().filter(|e| e.kind == EquilibriumKind::Saddle).count(), 2);
/// ```
pub fn find_equilibria<Sy: PlanarSystem + ?Sized>(
    system: &Sy,
    x_range: (f64, f64),
    y_range: (f64, f64),
    seeds_per_axis: usize,
) -> Vec<Equilibrium> {
    let n = seeds_per_axis.max(2);
    let span = (x_range.1 - x_range.0)
        .abs()
        .max((y_range.1 - y_range.0).abs())
        .max(1.0);
    // Slack for "inside the window" (Newton may land a hair outside a boundary
    // equilibrium) and for calling two refined roots the same point.
    let tol = 1e-6 * span;
    let dedup = 1e-5 * span;
    let mut found: Vec<Equilibrium> = Vec::new();

    for i in 0..n {
        for j in 0..n {
            let x = x_range.0 + (x_range.1 - x_range.0) * i as f64 / (n - 1) as f64;
            let y = y_range.0 + (y_range.1 - y_range.0) * j as f64 / (n - 1) as f64;
            let Some(p) = newton_refine(system, [x, y]) else {
                continue;
            };
            if p[0] < x_range.0 - tol
                || p[0] > x_range.1 + tol
                || p[1] < y_range.0 - tol
                || p[1] > y_range.1 + tol
            {
                continue;
            }
            let dup = found
                .iter()
                .any(|e| (e.point[0] - p[0]).abs() < dedup && (e.point[1] - p[1]).abs() < dedup);
            if dup {
                continue;
            }
            let jac = jacobian(system, p[0], p[1]);
            found.push(Equilibrium {
                point: p,
                kind: classify(jac),
                jacobian: jac,
            });
        }
    }
    found.sort_by(|a, b| {
        a.point[0]
            .partial_cmp(&b.point[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.point[1]
                    .partial_cmp(&b.point[1])
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    found
}

/// Draws one marker per equilibrium on `axes`, shaped by class: a filled dot for
/// attractors, a hollow ring for repellers, a cross for saddles, and a ring with
/// a dot for centres.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_dynamics::equilibria::{add_markers, find_equilibria};
/// use manim_dynamics::Pendulum;
/// let axes = Axes::new([-4.0, 4.0, 1.0], [-3.0, 3.0, 1.0]);
/// let mut scene = SceneState::new();
/// let eqs = find_equilibria(&Pendulum { damping: 0.0 }, (-4.0, 4.0), (-3.0, 3.0), 21);
/// let g = add_markers(&mut scene, &axes, &eqs, 0.12);
/// assert!(scene.contains(g));
/// ```
pub fn add_markers(
    scene: &mut SceneState,
    axes: &Axes,
    equilibria: &[Equilibrium],
    size: f32,
) -> MobjectId<VGroup> {
    let mut members: Vec<AnyId> = Vec::new();
    for eq in equilibria {
        let c = axes.c2p(eq.point[0] as f32, eq.point[1] as f32);
        let color = eq.kind.color();
        match eq.kind {
            EquilibriumKind::StableNode | EquilibriumKind::StableSpiral => {
                members.push(
                    scene
                        .add(Dot::at(c).radius(size).with_fill(color, 1.0))
                        .erase(),
                );
            }
            EquilibriumKind::UnstableNode | EquilibriumKind::UnstableSpiral => {
                members.push(
                    scene
                        .add(
                            Circle::new()
                                .radius(size)
                                .with_move_to(c)
                                .with_stroke(color, 3.0, 1.0),
                        )
                        .erase(),
                );
            }
            EquilibriumKind::Saddle => {
                for (dx, dy) in [(1.0, 1.0), (1.0, -1.0)] {
                    let a = c + manim_math::Point::new(-size * dx, -size * dy, 0.0);
                    let b = c + manim_math::Point::new(size * dx, size * dy, 0.0);
                    members.push(
                        scene
                            .add(Line::new(a, b).with_stroke(color, 3.0, 1.0))
                            .erase(),
                    );
                }
            }
            EquilibriumKind::Center | EquilibriumKind::Degenerate => {
                members.push(
                    scene
                        .add(
                            Circle::new()
                                .radius(size)
                                .with_move_to(c)
                                .with_stroke(color, 3.0, 1.0),
                        )
                        .erase(),
                );
                members.push(
                    scene
                        .add(Dot::at(c).radius(size * 0.35).with_fill(color, 1.0))
                        .erase(),
                );
            }
        }
    }
    VGroup::of(scene, members)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HopfNormalForm, Linear, Pendulum, VanDerPol};
    use std::f64::consts::PI;

    #[test]
    fn pendulum_has_a_centre_at_the_origin_and_a_saddle_at_pi() {
        let eqs = find_equilibria(&Pendulum { damping: 0.0 }, (-4.0, 4.0), (-2.0, 2.0), 25);
        let origin = eqs
            .iter()
            .find(|e| e.point[0].abs() < 1e-9 && e.point[1].abs() < 1e-9)
            .expect("origin equilibrium");
        assert_eq!(origin.kind, EquilibriumKind::Center);

        let inverted = eqs
            .iter()
            .find(|e| (e.point[0] - PI).abs() < 1e-8 && e.point[1].abs() < 1e-9)
            .expect("inverted equilibrium at (π, 0)");
        assert_eq!(inverted.kind, EquilibriumKind::Saddle);
        // Eigenvalues ±1 there: τ = 0, Δ = −1.
        assert!(inverted.trace().abs() < 1e-12);
        assert!((inverted.determinant() + 1.0).abs() < 1e-12);
        let [lo, hi] = real_eigenvalues(inverted.jacobian).unwrap();
        assert!((lo + 1.0).abs() < 1e-12 && (hi - 1.0).abs() < 1e-12);

        // …and at −π too; exactly three equilibria in this window.
        assert_eq!(eqs.len(), 3);
        assert!(eqs
            .iter()
            .any(|e| (e.point[0] + PI).abs() < 1e-8 && e.kind == EquilibriumKind::Saddle));
    }

    #[test]
    fn damping_turns_the_pendulum_centre_into_a_stable_spiral() {
        let eqs = find_equilibria(&Pendulum { damping: 0.4 }, (-1.0, 1.0), (-1.0, 1.0), 11);
        assert_eq!(eqs.len(), 1);
        assert_eq!(eqs[0].kind, EquilibriumKind::StableSpiral);
        // τ = −b, Δ = 1 ⇒ τ² − 4Δ = 0.16 − 4 < 0, complex pair.
        assert!((eqs[0].trace() + 0.4).abs() < 1e-12);
        assert!((eqs[0].determinant() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn heavy_damping_makes_it_a_stable_node() {
        // b = 3 ⇒ τ² − 4Δ = 9 − 4 > 0: two real negative eigenvalues.
        let eqs = find_equilibria(&Pendulum { damping: 3.0 }, (-1.0, 1.0), (-1.0, 1.0), 11);
        assert_eq!(eqs[0].kind, EquilibriumKind::StableNode);
    }

    #[test]
    fn van_der_pol_origin_is_an_unstable_spiral_for_small_mu() {
        let eqs = find_equilibria(&VanDerPol { mu: 1.0 }, (-1.0, 1.0), (-1.0, 1.0), 9);
        assert_eq!(eqs.len(), 1);
        assert_eq!(eqs[0].kind, EquilibriumKind::UnstableSpiral);
        // For μ ≥ 2 the spiral becomes an unstable node (τ² ≥ 4Δ).
        let eqs = find_equilibria(&VanDerPol { mu: 3.0 }, (-1.0, 1.0), (-1.0, 1.0), 9);
        assert_eq!(eqs[0].kind, EquilibriumKind::UnstableNode);
    }

    #[test]
    fn hopf_origin_changes_stability_at_mu_zero() {
        for (mu, want) in [
            (-0.2, EquilibriumKind::StableSpiral),
            (0.2, EquilibriumKind::UnstableSpiral),
        ] {
            let h = HopfNormalForm { mu, omega: 1.0 };
            let eqs = find_equilibria(&h, (-0.05, 0.05), (-0.05, 0.05), 5);
            assert_eq!(eqs[0].kind, want, "μ = {mu}");
        }
        // Exactly at onset the linearisation is a centre.
        let h = HopfNormalForm {
            mu: 0.0,
            omega: 1.0,
        };
        assert_eq!(classify(jacobian(&h, 0.0, 0.0)), EquilibriumKind::Center);
    }

    #[test]
    fn classification_covers_the_trace_determinant_plane() {
        let cases = [
            (
                Linear {
                    a: -1.0,
                    b: 0.0,
                    c: 0.0,
                    d: -2.0,
                },
                EquilibriumKind::StableNode,
            ),
            (
                Linear {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 2.0,
                },
                EquilibriumKind::UnstableNode,
            ),
            (
                Linear {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: -2.0,
                },
                EquilibriumKind::Saddle,
            ),
            (
                Linear {
                    a: -0.5,
                    b: -2.0,
                    c: 2.0,
                    d: -0.5,
                },
                EquilibriumKind::StableSpiral,
            ),
            (
                Linear {
                    a: 0.5,
                    b: -2.0,
                    c: 2.0,
                    d: 0.5,
                },
                EquilibriumKind::UnstableSpiral,
            ),
            (
                Linear {
                    a: 0.0,
                    b: -2.0,
                    c: 2.0,
                    d: 0.0,
                },
                EquilibriumKind::Center,
            ),
        ];
        for (sys, want) in cases {
            assert_eq!(classify(jacobian(&sys, 0.0, 0.0)), want);
        }
    }

    #[test]
    fn saddle_eigenvectors_are_invariant_directions() {
        let eq = find_equilibria(&Pendulum { damping: 0.0 }, (2.0, 4.0), (-1.0, 1.0), 9)[0];
        let (stable, unstable) = eq.saddle_directions().unwrap();
        let j = eq.jacobian;
        for (v, lam) in [(stable, -1.0), (unstable, 1.0)] {
            // Jv = λv, componentwise.
            let jv = [
                j[0][0] * v[0] + j[0][1] * v[1],
                j[1][0] * v[0] + j[1][1] * v[1],
            ];
            assert!((jv[0] - lam * v[0]).abs() < 1e-9, "{jv:?} vs λ{v:?}");
            assert!((jv[1] - lam * v[1]).abs() < 1e-9);
        }
    }

    #[test]
    fn newton_converges_from_far_seeds_and_reports_failure_honestly() {
        let p = Pendulum { damping: 0.0 };
        let got = newton_refine(&p, [2.6, 1.5]).unwrap();
        assert!(value(&p, got[0], got[1])[0].abs() < 1e-12);
        // A system with no equilibrium at all: ẋ = 1, ẏ = 1.
        struct NoFixedPoint;
        impl PlanarSystem for NoFixedPoint {
            fn eval<S: manim_fields::ad::Scalar>(&self, _x: S, _y: S) -> [S; 2] {
                [S::constant(1.0), S::constant(1.0)]
            }
        }
        assert!(newton_refine(&NoFixedPoint, [0.0, 0.0]).is_none());
    }
}
