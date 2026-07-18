//! Probability clouds & slice planes — the volumetric-visualization kit.
//!
//! Two complementary ways to picture a scalar field that fills a volume:
//!
//! - [`density_cloud`] scatters a Monte-Carlo point cloud whose *number density*
//!   traces a [`ScalarField`] (a probability density like a hydrogen orbital
//!   `|ψ|²`), realized as one instanced draw of tiny spheres. The sampler is
//!   **rejection sampling** (see [`sample_points`]).
//! - [`field_slice`] cuts an arbitrary plane through the volume and paints it
//!   with the heatmap [`Material`] — the classic slice plane.
//!
//! # Why rejection sampling (and not Metropolis)?
//!
//! Rejection sampling draws candidate points uniformly in the bounding box and
//! keeps each with probability `density(p) / max_density`. The kept points are
//! **exactly** i.i.d. draws from the (normalized) density — no burn-in, no
//! autocorrelation, and trivially deterministic given a seed. A Metropolis /
//! MCMC walk would accept more candidates when the density spans many orders of
//! magnitude, but its samples are correlated and depend on a proposal width and
//! warm-up; for the compact, bounded densities these clouds visualize (orbitals,
//! Gaussians, field magnitudes) plain rejection is both simpler and unbiased, so
//! that is what we use. The only cost is wasted candidates where the density is
//! low, which we bound with an attempt cap.
//!
//! # Determinism
//!
//! No `rand`, no `Math::random` — a tiny seeded [`Rng`] (a SplitMix64-seeded
//! xorshift64\*) drives every draw, so a given `seed` always yields the same
//! cloud (the "ColorRng pattern"). This is what makes the histogram test below
//! reproducible.

use std::sync::Arc;

use glam::{DVec3, Mat4, Quat, Vec3};

use manim_core::display::{Colormap, FieldChannels, Material, MaterialKind, TextureData};
use manim_core::mesh::{Instance, InstancedMesh, TriMesh};
use manim_core::mobject::{Mobject, MobjectId};
use manim_core::scene_state::SceneState;
use manim_math::path::Path;

use manim_fields::field::ScalarField;

use crate::material_quad::MaterialQuad;
use crate::to_scene;

/// A deterministic seedable PRNG: SplitMix64 seeding into xorshift64\*.
///
/// Fully reproducible per seed and free of any platform entropy — the same
/// `seed` always produces the same stream (the "ColorRng pattern").
///
/// ```
/// use manim_sci::volumetrics::Rng;
/// let mut a = Rng::new(7);
/// let mut b = Rng::new(7);
/// assert_eq!(a.next_f64(), b.next_f64()); // identical streams per seed
/// ```
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// Seeds the generator, spreading `seed` through SplitMix64 so even `0` and
    /// nearby seeds give well-separated, non-degenerate streams.
    pub fn new(seed: u64) -> Self {
        // SplitMix64 finalizer — decorrelates low-entropy seeds.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // Guard the xorshift state against the all-zero fixed point.
        Rng(z | 1)
    }

    /// The next raw 64-bit word (xorshift64\*).
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// The next `f64` uniformly in `[0, 1)` (53-bit mantissa).
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Parameters for a Monte-Carlo [`density_cloud`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudParams {
    /// Number of accepted points to draw (the cloud's population).
    pub n_samples: usize,
    /// PRNG seed — the same seed reproduces the same cloud exactly.
    pub seed: u64,
    /// World-space radius of each point's sphere instance.
    pub radius: f32,
    /// Lower corner of the sampling box.
    pub bounds_min: DVec3,
    /// Upper corner of the sampling box.
    pub bounds_max: DVec3,
    /// An upper bound on the density over the box; a candidate at `p` is accepted
    /// with probability `density(p) / max_density` (clamped to `[0, 1]`). Set it
    /// to (an estimate of) the peak density — too small biases the cloud.
    pub max_density: f64,
}

impl Default for CloudParams {
    fn default() -> Self {
        Self {
            n_samples: 2000,
            seed: 0xC10D_5EED,
            radius: 0.02,
            bounds_min: DVec3::splat(-1.0),
            bounds_max: DVec3::splat(1.0),
            max_density: 1.0,
        }
    }
}

/// Draws the raw accepted points of a [`density_cloud`] by **rejection
/// sampling**: uniform candidates in `[bounds_min, bounds_max]` kept with
/// probability `density(p) / max_density`. The returned points are i.i.d. draws
/// from the (normalized) density, so binning them by radius recovers the density
/// profile — see the crate's histogram test.
///
/// Deterministic given `params.seed`. Bounded by an attempt cap
/// (`1000 · n_samples`, min `10_000`) so a mismatched `max_density` or an empty
/// region cannot loop forever; the returned `Vec` may then be short.
///
/// ```
/// use manim_fields::field::ScalarField;
/// use manim_sci::volumetrics::{sample_points, CloudParams};
/// // A uniform density with max_density = 1 accepts every candidate.
/// let density = ScalarField::constant(1.0);
/// let params = CloudParams { n_samples: 128, seed: 1, max_density: 1.0, ..Default::default() };
/// let pts = sample_points(&density, &params);
/// assert_eq!(pts.len(), 128);
/// ```
pub fn sample_points(density: &ScalarField, params: &CloudParams) -> Vec<DVec3> {
    let mut rng = Rng::new(params.seed);
    let mut pts = Vec::with_capacity(params.n_samples);
    let span = params.bounds_max - params.bounds_min;
    let max_attempts = params.n_samples.saturating_mul(1000).max(10_000);
    let mut attempts = 0usize;

    while pts.len() < params.n_samples && attempts < max_attempts {
        attempts += 1;
        let candidate = params.bounds_min
            + DVec3::new(
                span.x * rng.next_f64(),
                span.y * rng.next_f64(),
                span.z * rng.next_f64(),
            );
        let d = density.at(candidate).max(0.0);
        let accept_prob = (d / params.max_density).clamp(0.0, 1.0);
        if rng.next_f64() < accept_prob {
            pts.push(candidate);
        }
    }
    pts
}

/// A Monte-Carlo probability cloud: scatters `params.n_samples` tiny spheres so
/// their **number density** traces `density`, as a single instanced draw.
///
/// Each accepted point becomes one unit-sphere [`Instance`] scaled to
/// `params.radius`, tinted by the local density on the [`Magma`](Colormap::Magma)
/// colormap with opacity rising with density (denser → brighter and more
/// opaque). Deterministic given `params.seed`. Uses [`sample_points`] (rejection
/// sampling) under the hood.
///
/// ```
/// use manim_core::scene_state::SceneState;
/// use manim_fields::field::ScalarField;
/// use manim_sci::volumetrics::{density_cloud, CloudParams};
/// let mut scene = SceneState::new();
/// let density = ScalarField::constant(1.0);
/// let params = CloudParams { n_samples: 64, seed: 3, max_density: 1.0, ..Default::default() };
/// let id = density_cloud(&mut scene, &density, params);
/// assert_eq!(scene.get(id).instances().len(), 64);
/// ```
pub fn density_cloud(
    scene: &mut SceneState,
    density: &ScalarField,
    params: CloudParams,
) -> MobjectId<InstancedMesh> {
    let pts = sample_points(density, &params);

    // A low-poly base sphere: the cloud can hold thousands of these, so keep the
    // per-instance triangle count small.
    let base = TriMesh::uv_sphere(6, 10);

    let mut instances = Vec::with_capacity(pts.len());
    for p in &pts {
        let d = density.at(*p).max(0.0);
        let t = (d / params.max_density).clamp(0.0, 1.0);
        // Opacity encodes density too, so overlapping cloud regions read denser.
        let color = Colormap::Magma
            .sample(t as f32)
            .with_opacity((0.15 + 0.85 * t) as f32);
        let transform = Mat4::from_scale_rotation_translation(
            Vec3::splat(params.radius),
            Quat::IDENTITY,
            p.as_vec3(),
        );
        instances.push(Instance::new(transform, color));
    }

    scene.add(InstancedMesh::new(base, instances))
}

/// An orthonormal in-plane basis `(u, v)` for the plane with unit normal `n`
/// (`u ⟂ v ⟂ n`), chosen from a stable reference axis to avoid degeneracy.
fn plane_basis(normal: DVec3) -> (DVec3, DVec3) {
    let n = normal.normalize();
    // Pick a reference not (near-)parallel to n so the cross product is stable.
    let reference = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = reference.cross(n).normalize();
    let v = n.cross(u); // already unit (n ⟂ u, both unit)
    (u, v)
}

/// A heatmap slice plane: samples `field` on the plane through `origin`
/// perpendicular to `normal`, spanning `±half_extent` in the two in-plane
/// directions, and paints it with the heatmap [`Material`].
///
/// # Plane approach
///
/// The material pipeline shades a quad from a texture indexed by the quad's four
/// corner UVs — so the slice is oriented purely through geometry. We build an
/// orthonormal in-plane basis `(u, v) ⟂ normal`, sample `field.at(origin + s·u +
/// t·v)` over an `R32F` grid, and give the quad the four 3-D corners
/// `origin ± half_extent·u ± half_extent·v` in the UV order `(0,0) (1,0) (1,1)
/// (0,1)` that matches the grid's row-major layout. No local flat quad is left
/// behind and no post-hoc scene transform is needed: every corner satisfies
/// `(corner − origin)·normal = 0`, so the quad lies exactly in the requested
/// plane. (We reuse [`MaterialQuad::from_material`] for the material plumbing,
/// then overwrite its `path` with the oriented corners via the public
/// [`Mobject::data_mut`] accessor.)
///
/// ```
/// use glam::DVec3;
/// use manim_core::display::Colormap;
/// use manim_core::scene_state::SceneState;
/// use manim_fields::field::ScalarField;
/// use manim_sci::volumetrics::field_slice;
/// let mut scene = SceneState::new();
/// let field = ScalarField::coordinate(2); // f = z
/// let id = field_slice(
///     &mut scene,
///     &field,
///     DVec3::ZERO,
///     DVec3::X, // slice the yz-plane
///     2.0,
///     32,
///     Colormap::Viridis,
/// );
/// assert!(scene.get_dyn(id).data().material.is_some());
/// ```
pub fn field_slice(
    scene: &mut SceneState,
    field: &ScalarField,
    origin: DVec3,
    normal: DVec3,
    half_extent: f32,
    resolution: usize,
    colormap: Colormap,
) -> MobjectId<MaterialQuad> {
    let (u, v) = plane_basis(normal);
    let h = half_extent as f64;
    let res = resolution.max(2);

    // Sample the field on the plane into a row-major R32F texture: i (columns)
    // runs along +u ≡ UV-u, j (rows) along +v ≡ UV-v.
    let mut data = Vec::with_capacity(res * res);
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for j in 0..res {
        let t = -h + 2.0 * h * j as f64 / (res - 1) as f64;
        for i in 0..res {
            let s = -h + 2.0 * h * i as f64 / (res - 1) as f64;
            let value = field.at(origin + u * s + v * t);
            lo = lo.min(value);
            hi = hi.max(value);
            data.push(value as f32);
        }
    }

    let texture = TextureData {
        width: res as u32,
        height: res as u32,
        channels: FieldChannels::R,
        data,
        center: origin.as_vec3(),
        size: [(2.0 * h) as f32, (2.0 * h) as f32],
    };
    let material = Material {
        kind: MaterialKind::Heatmap { colormap },
        texture: Arc::new(texture),
        value_range: [lo as f32, hi as f32],
        opacity: 1.0,
    };

    // Build the material quad, then replace its (flat, local) rect path with the
    // plane-oriented corners in UV order (0,0),(1,0),(1,1),(0,1).
    let mut quad = MaterialQuad::from_material([-h, h], [-h, h], material);
    let corners = [
        to_scene(origin - u * h - v * h), // UV (0, 0)
        to_scene(origin + u * h - v * h), // UV (1, 0)
        to_scene(origin + u * h + v * h), // UV (1, 1)
        to_scene(origin - u * h + v * h), // UV (0, 1)
    ];
    quad.data_mut().path = Path::from_corners(&corners, true);
    quad.data_mut().bump_generation();
    quad.add_to(scene)
}

#[cfg(test)]
mod tests {
    use super::*;

    use manim_fields::ad::Scalar;
    use manim_fields::field::ScalarClosure;

    /// The isotropic Gaussian density `ρ(r) = exp(-r²)`, peaked (= 1) at the
    /// origin — an analytic radial profile the histogram test checks against.
    struct Gaussian;
    impl ScalarClosure for Gaussian {
        fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
            (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).scale(-1.0).exp()
        }
    }

    #[test]
    fn sample_points_is_deterministic_per_seed() {
        let density = ScalarField::from_closure(Gaussian);
        let params = CloudParams {
            n_samples: 500,
            seed: 42,
            max_density: 1.0,
            bounds_min: DVec3::splat(-3.0),
            bounds_max: DVec3::splat(3.0),
            ..Default::default()
        };

        let a = sample_points(&density, &params);
        let b = sample_points(&density, &params);
        assert_eq!(a, b, "same seed must reproduce the same points exactly");

        let mut other = params;
        other.seed = 43;
        let c = sample_points(&density, &other);
        assert_ne!(a, c, "a different seed must give different points");
    }

    /// The key test: the *radial* histogram of the accepted points, normalized by
    /// shell volume, must trace the analytic density `exp(-r²)`.
    #[test]
    fn radial_histogram_matches_analytic_density() {
        let density = ScalarField::from_closure(Gaussian);
        let params = CloudParams {
            n_samples: 40_000,
            seed: 2024,
            max_density: 1.0,
            bounds_min: DVec3::splat(-3.0),
            bounds_max: DVec3::splat(3.0),
            ..Default::default()
        };
        let pts = sample_points(&density, &params);
        assert!(
            pts.len() > 30_000,
            "acceptance too low ({} pts); check max_density",
            pts.len()
        );

        // Bins over r ∈ [0, R_MAX]; keep R_MAX well inside the box (half-width 3)
        // so no shell is truncated by the box faces.
        const N_BINS: usize = 10;
        const R_MAX: f64 = 2.0;
        let dr = R_MAX / N_BINS as f64;
        let mut counts = [0u64; N_BINS];
        for p in &pts {
            let r = p.length();
            if r < R_MAX {
                counts[(r / dr) as usize] += 1;
            }
        }

        // measured density ∝ count / shell_volume; analytic = exp(-r_mid²).
        // Compare each ratio (measured/analytic) against a common constant.
        let mut measured = [0.0f64; N_BINS];
        let mut analytic = [0.0f64; N_BINS];
        for k in 0..N_BINS {
            let r_lo = k as f64 * dr;
            let r_hi = r_lo + dr;
            let shell_vol = 4.0 / 3.0 * std::f64::consts::PI * (r_hi.powi(3) - r_lo.powi(3));
            let r_mid = 0.5 * (r_lo + r_hi);
            measured[k] = counts[k] as f64 / shell_vol;
            analytic[k] = (-r_mid * r_mid).exp();
        }

        // Fit the single normalization constant that best relates them (the total
        // sample count), then report per-shell relative error.
        let sum_ma: f64 = (0..N_BINS).map(|k| measured[k] * analytic[k]).sum();
        let sum_aa: f64 = (0..N_BINS).map(|k| analytic[k] * analytic[k]).sum();
        let scale = sum_ma / sum_aa; // least-squares fit of measured ≈ scale·analytic

        println!(
            "\nradial histogram vs analytic exp(-r^2)  ({} points)\n{:>6} {:>12} {:>12} {:>10}",
            pts.len(),
            "r_mid",
            "measured",
            "predicted",
            "rel.err"
        );
        let mut max_rel_err = 0.0f64;
        let mut sum_sq_meas = 0.0f64;
        let mut sum_sq_pred = 0.0f64;
        let mut sum_cross = 0.0f64;
        for k in 0..N_BINS {
            let r_mid = (k as f64 + 0.5) * dr;
            let predicted = scale * analytic[k];
            let rel_err = (measured[k] - predicted).abs() / predicted.max(1e-12);
            max_rel_err = max_rel_err.max(rel_err);
            sum_sq_meas += measured[k] * measured[k];
            sum_sq_pred += predicted * predicted;
            sum_cross += measured[k] * predicted;
            println!(
                "{r_mid:>6.2} {:>12.4} {:>12.4} {rel_err:>10.3}",
                measured[k], predicted
            );
        }
        let correlation = sum_cross / (sum_sq_meas.sqrt() * sum_sq_pred.sqrt());
        println!("max rel.err = {max_rel_err:.3},  correlation = {correlation:.5}\n");

        // The shape must track exp(-r²) tightly; correlation is robust to the
        // (few) sparse outer shells, and no well-populated shell should stray far.
        assert!(
            correlation > 0.999,
            "radial profile must correlate with exp(-r^2): got {correlation}"
        );
        assert!(
            max_rel_err < 0.15,
            "per-shell relative error too large: {max_rel_err}"
        );
    }

    #[test]
    fn field_slice_builds_material_in_the_plane() {
        let mut scene = SceneState::new();
        let field = ScalarField::from_closure(Gaussian);
        let origin = DVec3::new(0.3, -0.2, 0.5);
        let normal = DVec3::new(1.0, 2.0, -0.5);
        let id = field_slice(
            &mut scene,
            &field,
            origin,
            normal,
            1.5,
            24,
            Colormap::Viridis,
        );

        // A material was attached...
        assert!(
            scene.get_dyn(id).data().material.is_some(),
            "slice must carry a heatmap material"
        );

        // ...and every path anchor lies in the requested plane.
        let n = normal.normalize();
        let path = &scene.get_dyn(id).data().path;
        let mut n_corners = 0;
        for sub in &path.subpaths {
            for curve in &sub.curves {
                for pt in [curve.p0, curve.p3] {
                    let offset = pt.as_dvec3() - origin;
                    assert!(
                        offset.dot(n).abs() < 1e-5,
                        "corner off the plane: (p - origin)·n = {}",
                        offset.dot(n)
                    );
                    n_corners += 1;
                }
            }
        }
        assert!(n_corners >= 4, "expected a 4-corner quad path");
    }

    #[test]
    fn density_cloud_populates_instances() {
        let mut scene = SceneState::new();
        let density = ScalarField::from_closure(Gaussian);
        let params = CloudParams {
            n_samples: 300,
            seed: 9,
            max_density: 1.0,
            bounds_min: DVec3::splat(-3.0),
            bounds_max: DVec3::splat(3.0),
            radius: 0.03,
        };
        let id = density_cloud(&mut scene, &density, params);
        assert_eq!(scene.get(id).instances().len(), 300);
    }
}
