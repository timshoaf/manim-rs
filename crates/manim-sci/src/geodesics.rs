//! Geodesics and parallel transport on parametric surfaces.
//!
//! Everything here is driven by the intrinsic metric `g = [[E, F], [F, G]]` of a
//! [`SurfaceSampler`] and its first derivatives, which are read off the surface's
//! second parameter derivatives (via the bivariate jet in [`diffgeo`]). From the
//! metric we form the Christoffel symbols and integrate:
//!
//! - [`geodesic`] — the geodesic equation `u''ᵏ = −Γᵏᵢⱼ u'ⁱ u'ʲ`, so the curve
//!   is *straight* in the surface's own geometry (e.g. great circles on a
//!   sphere).
//! - [`parallel_transport`] — the transport equation `w'ᵏ = −Γᵏᵢⱼ wⁱ (dγ/dt)ʲ`,
//!   which slides a tangent vector along a curve without intrinsic turning. Its
//!   failure to return unchanged around a loop (the *holonomy*) equals the
//!   enclosed Gaussian curvature — the Gauss–Bonnet theorem.
//!
//! Tangent vectors and velocities are expressed in the `(u, v)` coordinate
//! basis. Integration uses the adaptive [`manim_fields::integrate::rk45`]
//! solver.
//!
//! [`diffgeo`]: crate::diffgeo

use crate::diffgeo::{surface_derivs, SurfaceSampler};
use manim_fields::integrate::rk45;

/// Absolute / relative tolerances used for the internal ODE integrations.
const ATOL: f64 = 1e-10;
const RTOL: f64 = 1e-10;

/// The Christoffel symbols `Γᵏᵢⱼ` of the surface metric at `(u, v)`.
///
/// Indexed `[k][i][j]` with `0 = u`, `1 = v`. Built from the metric
/// `g = [[E, F], [F, G]]` and its first derivatives (obtained from the surface's
/// second parameter derivatives) via
/// `Γᵏᵢⱼ = ½ gᵏˡ (∂ᵢ gⱼˡ + ∂ⱼ gᵢˡ − ∂ˡ gᵢⱼ)`.
///
/// ```
/// use manim_sci::{diffgeo::SurfaceSampler, geodesics::christoffel};
/// use manim_fields::ad::Scalar;
/// struct UnitSphere;
/// impl SurfaceSampler for UnitSphere {
///     fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
///         [u.sin() * v.cos(), u.sin() * v.sin(), u.cos()]
///     }
/// }
/// let g = christoffel(&UnitSphere, 1.0, 0.5);
/// // Sphere: Γᵘ_vv = −sin u cos u, Γᵛ_uv = cot u.
/// let (s, c) = (1.0_f64.sin(), 1.0_f64.cos());
/// assert!((g[0][1][1] + s * c).abs() < 1e-9);
/// assert!((g[1][0][1] - c / s).abs() < 1e-9);
/// ```
#[allow(clippy::needless_range_loop)]
pub fn christoffel<Sm: SurfaceSampler + ?Sized>(s: &Sm, u: f64, v: f64) -> [[[f64; 2]; 2]; 2] {
    let d = surface_derivs(s, u, v);
    let (fu, fv, fuu, fuv, fvv) = (d.fu, d.fv, d.fuu, d.fuv, d.fvv);

    // Metric coefficients.
    let e = fu.dot(fu);
    let f = fu.dot(fv);
    let g = fv.dot(fv);

    // Metric first derivatives: ∂E/∂u = 2 f_u·f_uu, etc.
    let e_u = 2.0 * fu.dot(fuu);
    let e_v = 2.0 * fu.dot(fuv);
    let f_u = fuu.dot(fv) + fu.dot(fuv);
    let f_v = fuv.dot(fv) + fu.dot(fvv);
    let g_u = 2.0 * fv.dot(fuv);
    let g_v = 2.0 * fv.dot(fvv);

    // dg[l][a][b] = ∂ₗ g_{ab}, with l = 0 (∂u), 1 (∂v).
    let dg = [[[e_u, f_u], [f_u, g_u]], [[e_v, f_v], [f_v, g_v]]];

    // Inverse metric.
    let det = e * g - f * f;
    let ginv = [[g / det, -f / det], [-f / det, e / det]];

    let mut gamma = [[[0.0_f64; 2]; 2]; 2];
    for k in 0..2 {
        for i in 0..2 {
            for j in 0..2 {
                let mut acc = 0.0;
                for l in 0..2 {
                    acc += ginv[k][l] * (dg[i][j][l] + dg[j][i][l] - dg[l][i][j]);
                }
                gamma[k][i][j] = 0.5 * acc;
            }
        }
    }
    gamma
}

/// Second-derivative accelerations of the geodesic ODE at state `(u, v, u', v')`.
///
/// Returns `(u''`, `v'')` where `u''ᵏ = −Γᵏᵢⱼ u'ⁱ u'ʲ`.
fn geodesic_accel<Sm: SurfaceSampler + ?Sized>(
    s: &Sm,
    u: f64,
    v: f64,
    up: f64,
    vp: f64,
) -> (f64, f64) {
    let g = christoffel(s, u, v);
    let vel = [up, vp];
    let mut acc = [0.0_f64; 2];
    for (k, ak) in acc.iter_mut().enumerate() {
        let mut sum = 0.0;
        for i in 0..2 {
            for j in 0..2 {
                sum += g[k][i][j] * vel[i] * vel[j];
            }
        }
        *ak = -sum;
    }
    (acc[0], acc[1])
}

/// Integrates a geodesic on the surface from `(u0, v0)` in direction
/// `(du0, dv0)`, returning the sampled `(u, v)` path.
///
/// The initial direction is normalized to unit *embedding* speed, so `length` is
/// arc length along the surface. The path is returned as `samples + 1` points
/// (including the start), evenly spaced in arc length.
///
/// ```
/// use manim_sci::{diffgeo::SurfaceSampler, geodesics::geodesic};
/// use manim_fields::ad::Scalar;
/// struct UnitSphere;
/// impl SurfaceSampler for UnitSphere {
///     fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
///         [u.sin() * v.cos(), u.sin() * v.sin(), u.cos()]
///     }
/// }
/// // A meridian (constant v) is a great circle → stays on the unit sphere.
/// let path = geodesic(&UnitSphere, 0.5, 0.3, 1.0, 0.0, 1.0, 20);
/// assert_eq!(path.len(), 21);
/// for &(u, v) in &path {
///     let p = [u.sin() * v.cos(), u.sin() * v.sin(), u.cos()];
///     let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
///     assert!((r - 1.0).abs() < 1e-6);
/// }
/// ```
pub fn geodesic<Sm: SurfaceSampler + ?Sized>(
    s: &Sm,
    u0: f64,
    v0: f64,
    du0: f64,
    dv0: f64,
    length: f64,
    samples: usize,
) -> Vec<(f64, f64)> {
    // Normalize the initial velocity to unit embedding speed.
    let (e, f, g) = {
        let d = surface_derivs(s, u0, v0);
        (d.fu.dot(d.fu), d.fu.dot(d.fv), d.fv.dot(d.fv))
    };
    let speed = (e * du0 * du0 + 2.0 * f * du0 * dv0 + g * dv0 * dv0).sqrt();
    let (du0, dv0) = if speed > 0.0 {
        (du0 / speed, dv0 / speed)
    } else {
        (du0, dv0)
    };

    let rhs = |_t: f64, y: &[f64]| {
        let (au, av) = geodesic_accel(s, y[0], y[1], y[2], y[3]);
        vec![y[2], y[3], au, av]
    };

    let samples = samples.max(1);
    let mut out = Vec::with_capacity(samples + 1);
    out.push((u0, v0));
    let mut y = vec![u0, v0, du0, dv0];
    let dt = length / samples as f64;
    for i in 0..samples {
        let t0 = i as f64 * dt;
        let t1 = t0 + dt;
        y = rk45(&rhs, t0, &y, t1, ATOL, RTOL);
        out.push((y[0], y[1]));
    }
    out
}

/// Parallel-transports a tangent vector `w0` (in the `(u, v)` basis) along a
/// discrete path in parameter space, returning the vector's history.
///
/// The path is treated as piecewise-linear; over each segment the transport
/// equation `w'ᵏ = −Γᵏᵢⱼ wⁱ (dγ/dt)ʲ` is integrated with `rk45`. The returned
/// vector history has the same length as `path` (the first entry is `w0`).
///
/// Parallel transport preserves the metric length of the vector; around a closed
/// loop the returned vector is rotated by the enclosed Gaussian curvature
/// (holonomy / Gauss–Bonnet).
///
/// ```
/// use manim_sci::{diffgeo::SurfaceSampler, geodesics::{geodesic, parallel_transport}};
/// use manim_fields::ad::Scalar;
/// struct UnitSphere;
/// impl SurfaceSampler for UnitSphere {
///     fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
///         [u.sin() * v.cos(), u.sin() * v.sin(), u.cos()]
///     }
/// }
/// let path = geodesic(&UnitSphere, 0.5, 0.3, 1.0, 0.0, 1.0, 10);
/// let hist = parallel_transport(&UnitSphere, &path, (1.0, 0.0));
/// assert_eq!(hist.len(), path.len());
/// ```
pub fn parallel_transport<Sm: SurfaceSampler + ?Sized>(
    s: &Sm,
    path: &[(f64, f64)],
    w0: (f64, f64),
) -> Vec<(f64, f64)> {
    let mut hist = Vec::with_capacity(path.len());
    hist.push(w0);
    if path.len() < 2 {
        return hist;
    }
    let mut w = vec![w0.0, w0.1];
    for seg in path.windows(2) {
        let (u0, v0) = seg[0];
        let (u1, v1) = seg[1];
        let (vel_u, vel_v) = (u1 - u0, v1 - v0); // dγ/dt over t ∈ [0, 1]

        let rhs = |t: f64, w: &[f64]| {
            let u = u0 + (u1 - u0) * t;
            let v = v0 + (v1 - v0) * t;
            let g = christoffel(s, u, v);
            let vel = [vel_u, vel_v];
            let mut dw = [0.0_f64; 2];
            for (k, dwk) in dw.iter_mut().enumerate() {
                let mut acc = 0.0;
                for i in 0..2 {
                    for j in 0..2 {
                        acc += g[k][i][j] * w[i] * vel[j];
                    }
                }
                *dwk = -acc;
            }
            vec![dw[0], dw[1]]
        };

        w = rk45(&rhs, 0.0, &w, 1.0, ATOL, RTOL);
        hist.push((w[0], w[1]));
    }
    hist
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diffgeo::first_fundamental_form;
    use manim_fields::ad::Scalar;
    use std::f64::consts::{PI, TAU};

    struct UnitSphere;
    impl SurfaceSampler for UnitSphere {
        fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
            [u.sin() * v.cos(), u.sin() * v.sin(), u.cos()]
        }
    }

    /// Embed a `(u, v)` sphere point into ℝ³.
    fn embed(u: f64, v: f64) -> [f64; 3] {
        [u.sin() * v.cos(), u.sin() * v.sin(), u.cos()]
    }

    #[test]
    fn sphere_christoffel_matches_analytic() {
        let (u, v) = (0.9, 1.4);
        let g = christoffel(&UnitSphere, u, v);
        let (s, c) = (u.sin(), u.cos());
        // Nonzero symbols: Γᵘ_vv = −sin u cos u, Γᵛ_uv = Γᵛ_vu = cot u.
        assert!((g[0][1][1] + s * c).abs() < 1e-9);
        assert!((g[1][0][1] - c / s).abs() < 1e-9);
        assert!((g[1][1][0] - c / s).abs() < 1e-9);
        // The rest vanish.
        assert!(g[0][0][0].abs() < 1e-9);
        assert!(g[0][0][1].abs() < 1e-9);
        assert!(g[1][1][1].abs() < 1e-9);
        assert!(g[1][0][0].abs() < 1e-9);
    }

    #[test]
    fn great_circle_meridian_is_geodesic() {
        // Start at (u0, v0), heading in +u: traces the meridian at v = v0, which
        // is a great circle lying in the plane normal to (sin v0, −cos v0, 0).
        let (u0, v0) = (0.5, 0.3);
        let n = [v0.sin(), -v0.cos(), 0.0];
        let path = geodesic(&UnitSphere, u0, v0, 1.0, 0.0, 2.0, 60);
        for &(u, v) in &path {
            let p = embed(u, v);
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((r - 1.0).abs() < 1e-4, "off sphere: r = {r}");
            let plane = p[0] * n[0] + p[1] * n[1] + p[2] * n[2];
            assert!(plane.abs() < 1e-4, "left great-circle plane: {plane}");
        }
    }

    #[test]
    fn great_circle_equator_is_geodesic() {
        // Equator u = π/2, heading in +v; stays on the equator (z ≈ 0, r ≈ 1).
        let path = geodesic(&UnitSphere, PI / 2.0, 0.0, 0.0, 1.0, 3.0, 60);
        for &(u, v) in &path {
            let p = embed(u, v);
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((r - 1.0).abs() < 1e-4);
            assert!(p[2].abs() < 1e-4, "left equator: z = {}", p[2]);
        }
    }

    #[test]
    fn holonomy_around_latitude_loop_equals_enclosed_area() {
        // Transport a unit tangent around the latitude circle u = u0 (v: 0 → 2π).
        // The metric angle between start and end equals the spherical-cap area
        // 2π(1 − cos u0) (Gauss–Bonnet holonomy, unit sphere K = 1).
        let u0 = 0.4;
        let n = 400;
        let path: Vec<(f64, f64)> = (0..=n).map(|i| (u0, TAU * i as f64 / n as f64)).collect();

        let w0 = (1.0, 0.0);
        let hist = parallel_transport(&UnitSphere, &path, w0);
        let wf = *hist.last().unwrap();

        // Metric at u0 (E = 1, F = 0, G = sin²u0).
        let (e, f, g) = first_fundamental_form(&UnitSphere, u0, 0.0);
        let inner = |a: (f64, f64), b: (f64, f64)| {
            e * a.0 * b.0 + f * (a.0 * b.1 + a.1 * b.0) + g * a.1 * b.1
        };

        // Metric length is preserved (vector stays unit → stays tangent).
        let len_f = inner(wf, wf).sqrt();
        assert!((len_f - 1.0).abs() < 1e-3, "length drifted: {len_f}");

        let cos_ang = (inner(w0, wf) / (inner(w0, w0).sqrt() * len_f)).clamp(-1.0, 1.0);
        let angle = cos_ang.acos();
        let area = TAU * (1.0 - u0.cos());
        let rel = (angle - area).abs() / area;
        assert!(rel < 0.03, "holonomy {angle} vs area {area} (rel {rel})");
    }

    #[test]
    fn transported_vector_stays_tangent_along_the_way() {
        // Every intermediate transported vector, embedded into ℝ³, is perpendicular
        // to the surface normal (it is a combination of f_u, f_v, hence tangent) and
        // keeps constant metric length.
        let u0 = 0.7;
        let n = 200;
        let path: Vec<(f64, f64)> = (0..=n).map(|i| (u0, TAU * i as f64 / n as f64)).collect();
        let hist = parallel_transport(&UnitSphere, &path, (1.0, 0.0));

        for (idx, &(wu, wv)) in hist.iter().enumerate() {
            let (_u, v) = path[idx];
            let d = surface_derivs(&UnitSphere, u0, v);
            let w_embed = d.fu * wu + d.fv * wv;
            let normal = d.fu.cross(d.fv).normalize();
            assert!(w_embed.dot(normal).abs() < 1e-9, "not tangent at {idx}");
            let (e, f, g) = (d.fu.dot(d.fu), d.fu.dot(d.fv), d.fv.dot(d.fv));
            let len = (e * wu * wu + 2.0 * f * wu * wv + g * wv * wv).sqrt();
            assert!((len - 1.0).abs() < 1e-3, "length {len} at {idx}");
        }
    }
}
