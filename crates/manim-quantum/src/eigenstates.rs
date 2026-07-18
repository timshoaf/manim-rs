//! Analytic quantum eigenstates and their visualizations.
//!
//! Everything here is in natural units with `ħ = m = 1` (and, for hydrogen,
//! atomic units with Bohr radius `a₀ = 1`). Three families are provided:
//!
//! - the **particle in a box** `[0, L]`
//!   ([`particle_in_box`], [`box_energy`]),
//! - the 1-D **harmonic oscillator** via Hermite polynomials
//!   ([`harmonic_oscillator`], [`harmonic_energy`]), and
//! - the **hydrogen atom**: real radial functions `R_{nl}` (associated
//!   Laguerre) times *real* spherical harmonics `Y_{lm}`
//!   ([`hydrogen_wavefunction`]), packaged as a [`ScalarField`] over `(x,y,z)`
//!   ([`hydrogen_orbital`]) and rendered as a two-lobe isosurface
//!   ([`orbital_isosurface`]).
//!
//! ```
//! use manim_quantum::eigenstates::{particle_in_box, harmonic_energy};
//! // Ground state of the box peaks at the middle.
//! let mid = particle_in_box(1, 1.0, 0.5);
//! assert!(mid > 0.0);
//! // Oscillator energies are n + 1/2.
//! assert!((harmonic_energy(2) - 2.5).abs() < 1e-12);
//! ```

use std::f64::consts::PI;

use manim_core::geometry::VGroup;
use manim_core::mesh::Mesh;
use manim_core::mobject::MobjectId;
use manim_core::prelude::{BLUE, RED};
use manim_core::scene_state::SceneState;
use manim_fields::ad::Scalar;
use manim_fields::field::{ScalarClosure, ScalarField};
use manim_sci::isosurface::Isosurface;

// ---------------------------------------------------------------------------
// Small numeric helpers.
// ---------------------------------------------------------------------------

/// `n!` as an `f64` (exact for the small `n` used by these eigenstates).
fn factorial(n: usize) -> f64 {
    (1..=n).fold(1.0, |acc, k| acc * k as f64)
}

// ---------------------------------------------------------------------------
// Particle in a box on [0, L].
// ---------------------------------------------------------------------------

/// The `n`-th particle-in-a-box eigenfunction on `[0, L]`.
///
/// `ψₙ(x) = √(2/L)·sin(nπx/L)` for `x ∈ [0, L]`, and `0` outside the well.
/// States are indexed from `n = 1` (the ground state); `n = 0` is the trivial
/// zero function.
///
/// ```
/// use manim_quantum::eigenstates::particle_in_box;
/// // Nodes at the walls.
/// assert!(particle_in_box(1, 1.0, 0.0).abs() < 1e-12);
/// assert!(particle_in_box(1, 1.0, 1.0).abs() < 1e-12);
/// ```
pub fn particle_in_box(n: usize, l: f64, x: f64) -> f64 {
    if n == 0 || x < 0.0 || x > l {
        return 0.0;
    }
    (2.0 / l).sqrt() * (n as f64 * PI * x / l).sin()
}

/// The energy `Eₙ = n²π²/(2L²)` of the `n`-th box eigenstate (`ħ = m = 1`).
///
/// ```
/// use manim_quantum::eigenstates::box_energy;
/// use std::f64::consts::PI;
/// assert!((box_energy(1, 1.0) - PI * PI / 2.0).abs() < 1e-12);
/// ```
pub fn box_energy(n: usize, l: f64) -> f64 {
    let n = n as f64;
    n * n * PI * PI / (2.0 * l * l)
}

// ---------------------------------------------------------------------------
// Harmonic oscillator via Hermite polynomials.
// ---------------------------------------------------------------------------

/// The physicists' Hermite polynomial `Hₙ(x)` by the stable upward recurrence
/// `H_{n+1} = 2x·Hₙ − 2n·H_{n-1}` (`H₀ = 1`, `H₁ = 2x`).
///
/// ```
/// use manim_quantum::eigenstates::hermite;
/// assert!((hermite(2, 3.0) - (4.0 * 9.0 - 2.0)).abs() < 1e-9); // H₂ = 4x²−2
/// ```
pub fn hermite(n: usize, x: f64) -> f64 {
    if n == 0 {
        return 1.0;
    }
    let mut hm1 = 1.0; // H₀
    let mut h = 2.0 * x; // H₁
    for k in 1..n {
        let next = 2.0 * x * h - 2.0 * k as f64 * hm1;
        hm1 = h;
        h = next;
    }
    h
}

/// The `n`-th normalized harmonic-oscillator eigenfunction
/// `ψₙ(x) = (2ⁿ n! √π)^{-1/2} Hₙ(x) e^{-x²/2}` (`ħ = m = ω = 1`).
///
/// ```
/// use manim_quantum::eigenstates::harmonic_oscillator;
/// use std::f64::consts::PI;
/// // Ground state at the origin is π^{-1/4}.
/// let g = harmonic_oscillator(0, 0.0);
/// assert!((g - PI.powf(-0.25)).abs() < 1e-12);
/// ```
pub fn harmonic_oscillator(n: usize, x: f64) -> f64 {
    let norm = (2.0_f64.powi(n as i32) * factorial(n) * PI.sqrt()).sqrt();
    hermite(n, x) * (-0.5 * x * x).exp() / norm
}

/// The oscillator energy `Eₙ = n + 1/2` (`ħ = ω = 1`).
///
/// ```
/// use manim_quantum::eigenstates::harmonic_energy;
/// assert!((harmonic_energy(0) - 0.5).abs() < 1e-12);
/// ```
pub fn harmonic_energy(n: usize) -> f64 {
    n as f64 + 0.5
}

// ---------------------------------------------------------------------------
// Hydrogen: radial (associated Laguerre) × real spherical harmonics.
// ---------------------------------------------------------------------------

/// The associated Laguerre polynomial `L^α_k(x)` by its three-term recurrence
/// `(k+1)L^α_{k+1} = (2k+1+α−x)L^α_k − (k+α)L^α_{k-1}`.
///
/// ```
/// use manim_quantum::eigenstates::assoc_laguerre;
/// // L¹₁(x) = 2 − x.
/// assert!((assoc_laguerre(1, 1, 0.5) - 1.5).abs() < 1e-12);
/// ```
pub fn assoc_laguerre(k: usize, alpha: usize, x: f64) -> f64 {
    let a = alpha as f64;
    let mut lkm1 = 1.0; // L₀ = 1
    if k == 0 {
        return lkm1;
    }
    let mut lk = 1.0 + a - x; // L₁ = 1 + α − x
    for kk in 1..k {
        let kf = kk as f64;
        let next = ((2.0 * kf + 1.0 + a - x) * lk - (kf + a) * lkm1) / (kf + 1.0);
        lkm1 = lk;
        lk = next;
    }
    lk
}

/// The associated Legendre function `Pₗᵐ(x)` for `m ≥ 0`, **without** the
/// Condon–Shortley phase (the `(1−x²)^{m/2}` factor is folded in).
///
/// ```
/// use manim_quantum::eigenstates::assoc_legendre;
/// // P₁⁰(x) = x.
/// assert!((assoc_legendre(1, 0, 0.3) - 0.3).abs() < 1e-12);
/// ```
pub fn assoc_legendre(l: usize, m: usize, x: f64) -> f64 {
    // Pₘᵐ = (2m−1)!!·(1−x²)^{m/2}  (positive; no Condon–Shortley phase).
    let mut pmm = 1.0;
    if m > 0 {
        let somx2 = ((1.0 - x) * (1.0 + x)).sqrt();
        let mut fact = 1.0;
        for _ in 0..m {
            pmm *= fact * somx2;
            fact += 2.0;
        }
    }
    if l == m {
        return pmm;
    }
    let mut pmmp1 = x * (2.0 * m as f64 + 1.0) * pmm; // P_{m+1}^m
    if l == m + 1 {
        return pmmp1;
    }
    let mut pll = 0.0;
    for ll in (m + 2)..=l {
        let lf = ll as f64;
        let mf = m as f64;
        pll = ((2.0 * lf - 1.0) * x * pmmp1 - (lf + mf - 1.0) * pmm) / (lf - mf);
        pmm = pmmp1;
        pmmp1 = pll;
    }
    pll
}

/// A **real** spherical harmonic `Yₗᵐ(θ, φ)` (orthonormal over the sphere),
/// with `m > 0` the `cos(mφ)` (cosine-like) partner and `m < 0` the
/// `sin(|m|φ)` partner. Valid for `l` up to `3` (s, p, d, f) and beyond.
///
/// ```
/// use manim_quantum::eigenstates::real_spherical_harmonic;
/// use std::f64::consts::PI;
/// // Y₀₀ = 1/(2√π) everywhere.
/// let y = real_spherical_harmonic(0, 0, 1.0, 0.5);
/// assert!((y - 1.0 / (2.0 * PI.sqrt())).abs() < 1e-12);
/// ```
pub fn real_spherical_harmonic(l: usize, m: i32, theta: f64, phi: f64) -> f64 {
    let mabs = m.unsigned_abs() as usize;
    let plm = assoc_legendre(l, mabs, theta.cos());
    let norm = ((2 * l + 1) as f64 / (4.0 * PI) * factorial(l - mabs) / factorial(l + mabs)).sqrt();
    match m.cmp(&0) {
        std::cmp::Ordering::Greater => {
            std::f64::consts::SQRT_2 * norm * plm * (mabs as f64 * phi).cos()
        }
        std::cmp::Ordering::Less => {
            std::f64::consts::SQRT_2 * norm * plm * (mabs as f64 * phi).sin()
        }
        std::cmp::Ordering::Equal => norm * plm,
    }
}

/// The normalized hydrogen radial function `R_{nl}(r)` (atomic units,
/// `a₀ = 1`): `R_{nl}(r) = N·e^{-r/n}·(2r/n)^l·L^{2l+1}_{n−l−1}(2r/n)` with
/// `N = √[(2/n)³·(n−l−1)! / (2n·(n+l)!)]`.
///
/// ```
/// use manim_quantum::eigenstates::hydrogen_radial;
/// // R₁₀(r) = 2 e^{-r}; at r=0 that is 2.
/// assert!((hydrogen_radial(1, 0, 0.0) - 2.0).abs() < 1e-12);
/// ```
pub fn hydrogen_radial(n: usize, l: usize, r: f64) -> f64 {
    let nf = n as f64;
    let rho = 2.0 * r / nf;
    let lag = assoc_laguerre(n - l - 1, 2 * l + 1, rho);
    let norm = ((2.0 / nf).powi(3) * factorial(n - l - 1) / (2.0 * nf * factorial(n + l))).sqrt();
    norm * (-r / nf).exp() * rho.powi(l as i32) * lag
}

/// The full **real** hydrogen wavefunction `ψ_{nlm}(r, θ, φ) = R_{nl}(r)·Yₗᵐ`.
///
/// ```
/// use manim_quantum::eigenstates::hydrogen_wavefunction;
/// use std::f64::consts::PI;
/// // 1s at the origin: R₁₀(0)·Y₀₀ = 2 · 1/(2√π) = 1/√π.
/// let psi = hydrogen_wavefunction(1, 0, 0, 0.0, 0.0, 0.0);
/// assert!((psi - 1.0 / PI.sqrt()).abs() < 1e-12);
/// ```
pub fn hydrogen_wavefunction(n: usize, l: usize, m: i32, r: f64, theta: f64, phi: f64) -> f64 {
    hydrogen_radial(n, l, r) * real_spherical_harmonic(l, m, theta, phi)
}

/// The Cartesian closure behind [`hydrogen_orbital`].
///
/// The associated-Laguerre / Legendre recurrences and the `acos` / `atan2` of
/// the Cartesian→spherical conversion are not all expressible through the
/// [`Scalar`] trait (there is no `acos`/`atan2` on it), so the value is
/// computed in plain `f64` and wrapped with [`Scalar::constant`]. This **loses
/// automatic differentiation**: the field's gradient is zero, so an isosurface
/// built from it falls back to a default normal. That is fine here — orbitals
/// feed [`orbital_isosurface`] for its level set, not a gradient.
struct OrbitalClosure {
    n: usize,
    l: usize,
    m: i32,
}

impl ScalarClosure for OrbitalClosure {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        let x = p[0].value();
        let y = p[1].value();
        let z = p[2].value();
        let r = (x * x + y * y + z * z).sqrt();
        let theta = if r > 0.0 { (z / r).acos() } else { 0.0 };
        let phi = y.atan2(x);
        S::constant(hydrogen_wavefunction(self.n, self.l, self.m, r, theta, phi))
    }
}

/// The real hydrogen orbital `ψ_{nlm}` as a [`ScalarField`] over `(x, y, z)`.
///
/// The field is built from an `f64`-fallback closure, so it carries no usable
/// gradient (fine: it feeds isosurface level-sets, not derivatives).
///
/// ```
/// use manim_quantum::eigenstates::hydrogen_orbital;
/// use manim_fields::Point;
/// use std::f64::consts::PI;
/// let psi = hydrogen_orbital(1, 0, 0);
/// // Agrees with the closed form at the origin (1s peak = 1/√π).
/// assert!((psi.at(Point::ZERO) - 1.0 / PI.sqrt()).abs() < 1e-9);
/// ```
pub fn hydrogen_orbital(n: usize, l: usize, m: i32) -> ScalarField {
    ScalarField::from_closure(OrbitalClosure { n, l, m })
}

/// Builds a two-lobe isosurface of the real orbital `ψ_{nlm}`: the positive
/// lobe (surface `ψ = +level`, blue) and the negative lobe (surface
/// `ψ = −level`, red), grouped together.
///
/// The sampling region auto-sizes to a few `n²` Bohr radii, which comfortably
/// contains the bulk of the orbital's probability.
///
/// ```
/// use manim_core::scene_state::SceneState;
/// use manim_core::mobject::MobjectExt;
/// use manim_quantum::eigenstates::orbital_isosurface;
/// let mut scene = SceneState::new();
/// // A 2p_z orbital has two lobes; the group holds both meshes.
/// let g = orbital_isosurface(&mut scene, 2, 1, 0, 0.05);
/// assert!(scene.family(g.erase()).len() >= 2);
/// ```
pub fn orbital_isosurface(
    scene: &mut SceneState,
    n: usize,
    l: usize,
    m: i32,
    level: f64,
) -> MobjectId<VGroup> {
    let ext = (3.0 * (n * n) as f64).max(6.0);
    let min = [-ext, -ext, -ext];
    let max = [ext, ext, ext];
    let res = 28;

    let build = |scene: &mut SceneState, iso: f64, color| {
        let tri = Isosurface::new(hydrogen_orbital(n, l, m), iso)
            .region(min, max)
            .resolution(res)
            .mesh();
        let mut mesh = Mesh::new(tri);
        mesh.set_base_color(color);
        scene.add(mesh).erase()
    };

    let pos = build(scene, level, BLUE);
    let neg = build(scene, -level, RED);
    VGroup::of(scene, [pos, neg])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trapezoidal 1-D quadrature of `f` on `[a, b]` with `n` intervals.
    fn integrate(a: f64, b: f64, n: usize, f: impl Fn(f64) -> f64) -> f64 {
        let h = (b - a) / n as f64;
        let mut sum = 0.5 * (f(a) + f(b));
        for i in 1..n {
            sum += f(a + i as f64 * h);
        }
        sum * h
    }

    #[test]
    fn box_orthonormal() {
        let l = 1.0;
        for m in 1..=4 {
            for n in 1..=4 {
                let overlap = integrate(0.0, l, 4000, |x| {
                    particle_in_box(m, l, x) * particle_in_box(n, l, x)
                });
                let expect = if m == n { 1.0 } else { 0.0 };
                assert!(
                    (overlap - expect).abs() < 1e-3,
                    "⟨{m}|{n}⟩ = {overlap}, expected {expect}"
                );
            }
        }
    }

    #[test]
    fn box_node_count() {
        let l = 1.0;
        for n in 1..=5 {
            // Sample the open interval and count interior sign changes.
            let samples: Vec<f64> = (1..1000)
                .map(|i| particle_in_box(n, l, i as f64 * l / 1000.0))
                .collect();
            let nodes = samples.windows(2).filter(|w| w[0] * w[1] < 0.0).count();
            assert_eq!(nodes, n - 1, "box state n={n} should have n−1 nodes");
        }
    }

    #[test]
    fn harmonic_normalized_and_energy() {
        for n in 0..=4 {
            let norm = integrate(-10.0, 10.0, 4000, |x| {
                let p = harmonic_oscillator(n, x);
                p * p
            });
            println!("harmonic ∫|ψ_{n}|² dx = {norm:.6}");
            assert!((norm - 1.0).abs() < 1e-3, "ψ_{n} not normalized: {norm}");
            assert!((harmonic_energy(n) - (n as f64 + 0.5)).abs() < 1e-12);
        }
    }

    #[test]
    fn hydrogen_radial_nodes() {
        // R_{nl} has n − l − 1 radial nodes.
        for &(n, l) in &[(1, 0), (2, 0), (2, 1), (3, 0), (3, 1), (3, 2), (4, 1)] {
            // Offset off the integer grid so a node never lands exactly on a
            // sample (which would read as no sign change).
            let samples: Vec<f64> = (0..4000)
                .map(|i| hydrogen_radial(n, l, 0.005 + i as f64 * 0.01))
                .collect();
            let nodes = samples.windows(2).filter(|w| w[0] * w[1] < 0.0).count();
            assert_eq!(nodes, n - l - 1, "R_{n}{l} node count");
        }
    }

    /// Riemann-sum quadrature of `f(x,y,z)` over `[-ext, ext]³`.
    fn integrate3(ext: f64, n: usize, f: impl Fn(f64, f64, f64) -> f64) -> f64 {
        let h = 2.0 * ext / n as f64;
        let coord = |i: usize| -ext + (i as f64 + 0.5) * h;
        let mut sum = 0.0;
        for i in 0..n {
            let x = coord(i);
            for j in 0..n {
                let y = coord(j);
                for k in 0..n {
                    sum += f(x, y, coord(k));
                }
            }
        }
        sum * h * h * h
    }

    /// ψ_{nlm} evaluated in Cartesian coordinates.
    fn psi_xyz(n: usize, l: usize, m: i32, x: f64, y: f64, z: f64) -> f64 {
        let r = (x * x + y * y + z * z).sqrt();
        let theta = if r > 0.0 { (z / r).acos() } else { 0.0 };
        let phi = y.atan2(x);
        hydrogen_wavefunction(n, l, m, r, theta, phi)
    }

    #[test]
    fn hydrogen_normalized() {
        // 2p_z is smooth (vanishes at the origin), so a grid converges nicely.
        let norm = integrate3(14.0, 120, |x, y, z| {
            let p = psi_xyz(2, 1, 0, x, y, z);
            p * p
        });
        println!("hydrogen ∫|ψ_2p_z|² d³r = {norm:.6}");
        assert!((norm - 1.0).abs() < 0.05, "2p_z normalization: {norm}");
    }

    #[test]
    fn hydrogen_orthogonal() {
        // 2p_z and 2p_x share a radial part but have orthogonal angular parts.
        let overlap = integrate3(14.0, 120, |x, y, z| {
            psi_xyz(2, 1, 0, x, y, z) * psi_xyz(2, 1, 1, x, y, z)
        });
        println!("hydrogen ⟨2p_z|2p_x⟩ = {overlap:.6}");
        assert!(overlap.abs() < 1e-3, "2p_z ⟂ 2p_x overlap: {overlap}");
    }
}
