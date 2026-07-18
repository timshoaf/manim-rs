//! 3-D vector-field and tensor visualizers (S8).
//!
//! This module turns a [`VectorField3`] (or a symmetric-tensor closure) into
//! renderable mesh mobjects:
//!
//! - [`stream_tubes`] integrates streamlines from seed points and sweeps a solid
//!   tube along each, colored by local flow speed. With
//!   [`StreamParams::flux_conserving`] the tube radius shrinks where the flow is
//!   fast, so a bundle of tubes reads like a flux (mass-conserving) picture.
//! - [`stream_ribbons`] sweeps a flat ribbon whose frame *twists* by the
//!   integrated vorticity (`½ ∇×v · t̂`) along the streamline — a direct picture
//!   of how the flow spins fluid parcels.
//! - [`tensor_glyphs`] eigendecomposes a field of symmetric 3×3 tensors and
//!   draws one ellipsoid per grid point, stretched along the eigenvectors by the
//!   eigenvalues (a stress / diffusion glyph).
//! - [`flux_through_surface`] numerically integrates `∮ F·n̂ dA` over a
//!   parametric surface — the discrete divergence theorem in a single call.
//!
//! The tensor machinery rests on [`eigen_symmetric_3x3`], a closed-form
//! symmetric-eigensolver (Cardano eigenvalues + cross-product eigenvectors, with
//! Gram–Schmidt completion for degenerate spectra) that needs no external linear
//! algebra.
//!
//! Everything computes fields/streamlines in `f64` and converts to `f32` only at
//! the mesh boundary, matching the crate-wide convention.
//!
//! ```
//! use glam::DVec3;
//! use manim_core::scene_state::SceneState;
//! use manim_fields::field::{ScalarField, VectorField3};
//! use manim_sci::vector_field_3d::{stream_tubes, StreamParams};
//!
//! // Rigid rotation field v = (−y, x, 0): streamlines are circles.
//! let field = VectorField3::from_components(
//!     ScalarField::coordinate(1).scale(-1.0),
//!     ScalarField::coordinate(0),
//!     ScalarField::constant(0.0),
//! );
//! let mut scene = SceneState::new();
//! let group = stream_tubes(&mut scene, &field, &[DVec3::new(1.0, 0.0, 0.0)],
//!     StreamParams { length: 3.0, step: 0.3, ..StreamParams::default() });
//! assert!(scene.contains(group));
//! ```

use std::f64::consts::{PI, TAU};

use glam::{DVec3, Mat4, Vec3};

use manim_core::display::Colormap;
use manim_core::geometry::VGroup;
use manim_core::mesh::{Instance, InstancedMesh, Mesh, TriMesh};
use manim_core::mobject::{AnyId, MobjectId};
use manim_core::scene_state::SceneState;

use manim_fields::field::VectorField3;

// ---------------------------------------------------------------------------
// Streamline sweeping
// ---------------------------------------------------------------------------

/// Parameters controlling a streamline sweep ([`stream_tubes`] /
/// [`stream_ribbons`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamParams {
    /// Total integration parameter ("time") to trace each streamline forward.
    pub length: f64,
    /// Integration increment; the streamline is advanced in `length / step`
    /// increments (each an internally sub-stepped RK4 flow).
    pub step: f64,
    /// Tube radius / ribbon half-width (scene units). When
    /// [`flux_conserving`](Self::flux_conserving) is set this is the *reference*
    /// radius at the mean speed.
    pub radius: f32,
    /// Sides of the tube cross-section (ignored by [`stream_ribbons`]).
    pub n_around: usize,
    /// If set, the tube radius scales as `1 / speed` (clamped): the tube **thins
    /// where the flow is fast** so the swept cross-section tracks a
    /// mass-conserving flux picture. If clear, the radius is constant.
    pub flux_conserving: bool,
}

impl Default for StreamParams {
    /// A medium-length streamline with a thin, constant-radius, 12-sided tube.
    fn default() -> Self {
        Self {
            length: 6.0,
            step: 0.1,
            radius: 0.06,
            n_around: 12,
            flux_conserving: false,
        }
    }
}

/// Integrates one streamline forward from `seed`, returning the ordered center
/// points (including the seed). Advances by `params.step` increments, each an
/// internally sub-stepped RK4 flow, until the total `params.length` is reached
/// or the state diverges.
fn streamline(field: &VectorField3, seed: DVec3, params: &StreamParams) -> Vec<DVec3> {
    let step = params.step.max(1e-9);
    let n = (params.length / step).ceil().max(1.0) as usize;
    let mut centers = Vec::with_capacity(n + 1);
    centers.push(seed);
    let mut q = seed;
    for _ in 0..n {
        let next = field.flow(q, step, 4);
        if !next.is_finite() {
            break;
        }
        centers.push(next);
        q = next;
    }
    centers
}

/// The pointwise flow speeds `|v(cᵢ)|` and their `(min, max, mean)`.
fn speeds_along(field: &VectorField3, centers: &[DVec3]) -> (Vec<f64>, f64, f64, f64) {
    let speeds: Vec<f64> = centers.iter().map(|&c| field.at(c).length()).collect();
    let smin = speeds.iter().copied().fold(f64::INFINITY, f64::min);
    let smax = speeds.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mean = if speeds.is_empty() {
        0.0
    } else {
        speeds.iter().sum::<f64>() / speeds.len() as f64
    };
    (speeds, smin, smax, mean)
}

/// A unit-length vector perpendicular to `t` (RMF seed).
fn seed_normal(t: DVec3) -> DVec3 {
    let a = if t.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    (a - t * a.dot(t)).normalize()
}

/// One double-reflection rotation-minimizing-frame step (Wang et al. 2008):
/// transport `n` from the frame at `p0` (tangent `t0`) to `p1` (tangent `t1`).
fn rmf_step(p0: DVec3, p1: DVec3, t0: DVec3, t1: DVec3, n: DVec3) -> DVec3 {
    let v1 = p1 - p0;
    let c1 = v1.dot(v1);
    if c1 < 1e-18 {
        return n;
    }
    let r_l = n - v1 * (2.0 / c1 * v1.dot(n));
    let t_l = t0 - v1 * (2.0 / c1 * v1.dot(t0));
    let v2 = t1 - t_l;
    let c2 = v2.dot(v2);
    if c2 < 1e-18 {
        return r_l.normalize();
    }
    (r_l - v2 * (2.0 / c2 * v2.dot(r_l))).normalize()
}

/// Unit tangents of a polyline by forward differences (last reuses the previous).
fn polyline_tangents(centers: &[DVec3]) -> Vec<DVec3> {
    let n = centers.len();
    let mut tang: Vec<DVec3> = Vec::with_capacity(n);
    for i in 0..n {
        let d = if i + 1 < n {
            centers[i + 1] - centers[i]
        } else {
            centers[i] - centers[i - 1]
        };
        let t = if d.length() > 1e-12 {
            d.normalize()
        } else if i > 0 {
            tang[i - 1]
        } else {
            DVec3::X
        };
        tang.push(t);
    }
    tang
}

/// Sweeps a circular tube along a **discrete** polyline (the streamline case:
/// [`TubeMesh`](crate::curveviz::TubeMesh) needs a differentiable sampler, but a
/// streamline is only sampled).
///
/// Frames are carried by the same double-reflection rotation-minimizing method
/// as `TubeMesh`: seed a normal ⟂ the first tangent, then propagate. Each ring
/// vertex is `center + r·(cosθ·n + sinθ·b)`. `radius_at(i)` gives the radius at
/// ring `i`. Vertices are laid out ring-major (`ring i, side j → i·n_around + j`)
/// so a caller can attach per-ring colors in the same order.
fn tube_from_polyline(
    centers: &[Vec3],
    radius_at: impl Fn(usize) -> f32,
    n_around: usize,
) -> TriMesh {
    let n = centers.len();
    let n_around = n_around.max(3);
    if n < 2 {
        return TriMesh::default();
    }

    let c: Vec<DVec3> = centers.iter().map(|p| p.as_dvec3()).collect();
    let tang = polyline_tangents(&c);

    // Rotation-minimizing frames.
    let mut frames = Vec::with_capacity(n);
    let mut nrm = seed_normal(tang[0]);
    frames.push(nrm);
    for i in 1..n {
        nrm = rmf_step(c[i - 1], c[i], tang[i - 1], tang[i], nrm);
        frames.push(nrm);
    }

    let mut positions = Vec::with_capacity(n * n_around);
    let mut normals = Vec::with_capacity(n * n_around);
    for (i, &center) in c.iter().enumerate() {
        let nvec = frames[i];
        let b = tang[i].cross(nvec).normalize();
        let r = radius_at(i) as f64;
        for j in 0..n_around {
            let theta = TAU * j as f64 / n_around as f64;
            let dir = nvec * theta.cos() + b * theta.sin();
            positions.push((center + dir * r).as_vec3());
            normals.push(dir.as_vec3());
        }
    }

    let mut indices = Vec::with_capacity((n - 1) * n_around * 6);
    for i in 0..n - 1 {
        for j in 0..n_around {
            let j1 = (j + 1) % n_around;
            let a = (i * n_around + j) as u32;
            let b = ((i + 1) * n_around + j) as u32;
            let cc = ((i + 1) * n_around + j1) as u32;
            let d = (i * n_around + j1) as u32;
            indices.extend_from_slice(&[a, b, cc, a, cc, d]);
        }
    }

    TriMesh {
        positions,
        normals,
        colors: None,
        uvs: None,
        indices,
    }
}

/// Traces a streamline from each seed and sweeps a solid **tube** along it,
/// coloring every vertex by the local flow speed through a [`Colormap`].
///
/// Each tube's radius is `params.radius`, unless
/// [`params.flux_conserving`](StreamParams::flux_conserving) is set, in which
/// case the radius scales as `1 / speed` (clamped to a sane range): the tube
/// **thins where the flow accelerates**, mimicking a stream *tube* of constant
/// enclosed flux (its cross-section must shrink as the fluid speeds up). All
/// tubes are returned in one [`VGroup`].
///
/// ```
/// use glam::DVec3;
/// use manim_core::scene_state::SceneState;
/// use manim_fields::field::{ScalarField, VectorField3};
/// use manim_sci::vector_field_3d::{stream_tubes, StreamParams};
///
/// let field = VectorField3::from_components(
///     ScalarField::coordinate(1).scale(-1.0),
///     ScalarField::coordinate(0),
///     ScalarField::constant(0.0),
/// );
/// let mut scene = SceneState::new();
/// let seeds = [DVec3::new(1.0, 0.0, 0.0), DVec3::new(1.5, 0.0, 0.0)];
/// let g = stream_tubes(&mut scene, &field, &seeds,
///     StreamParams { length: 2.0, step: 0.25, n_around: 8, ..StreamParams::default() });
/// assert!(scene.contains(g));
/// ```
pub fn stream_tubes(
    scene: &mut SceneState,
    field: &VectorField3,
    seeds: &[DVec3],
    params: StreamParams,
) -> MobjectId<VGroup> {
    let mut ids: Vec<AnyId> = Vec::new();

    for &seed in seeds {
        let centers = streamline(field, seed, &params);
        if centers.len() < 2 {
            continue;
        }
        let (speeds, smin, smax, mean) = speeds_along(field, &centers);

        let base = params.radius;
        let radii: Vec<f32> = if params.flux_conserving {
            speeds
                .iter()
                .map(|&s| {
                    let ratio = (mean / s.max(1e-6)).clamp(0.25, 4.0);
                    base * ratio as f32
                })
                .collect()
        } else {
            vec![base; centers.len()]
        };

        let cvecs: Vec<Vec3> = centers.iter().map(|c| c.as_vec3()).collect();
        let mut tube = tube_from_polyline(&cvecs, |i| radii[i], params.n_around);
        if tube.is_empty() {
            continue;
        }

        let span = (smax - smin).max(1e-9);
        let n_around = params.n_around.max(3);
        let mut colors = Vec::with_capacity(tube.positions.len());
        for &s in &speeds {
            let color = Colormap::Turbo.sample(((s - smin) / span) as f32);
            for _ in 0..n_around {
                colors.push(color);
            }
        }
        tube.colors = Some(colors);

        ids.push(scene.add(Mesh::new(tube)).into());
    }

    VGroup::of(scene, ids)
}

/// Traces a streamline from each seed and sweeps a flat **ribbon** whose frame
/// twists by the integrated local rotation of the flow — a picture of
/// **vorticity**.
///
/// The twist angle at arc position `s` is the accumulated
/// `∫ ½ (∇×v)·t̂ ds` (the component of angular velocity about the streamline
/// tangent): where the flow curls strongly about its own direction the ribbon
/// visibly spins. The ribbon half-width is `params.radius`; vertices are colored
/// by speed. All ribbons are returned in one [`VGroup`].
///
/// ```
/// use glam::DVec3;
/// use manim_core::scene_state::SceneState;
/// use manim_fields::field::{ScalarField, VectorField3};
/// use manim_sci::vector_field_3d::{stream_ribbons, StreamParams};
///
/// // A helical flow (−y, x, 0.4) has curl (0, 0, 2): the ribbon twists.
/// let field = VectorField3::from_components(
///     ScalarField::coordinate(1).scale(-1.0),
///     ScalarField::coordinate(0),
///     ScalarField::constant(0.4),
/// );
/// let mut scene = SceneState::new();
/// let g = stream_ribbons(&mut scene, &field, &[DVec3::new(1.0, 0.0, 0.0)],
///     StreamParams { length: 3.0, step: 0.25, radius: 0.15, ..StreamParams::default() });
/// assert!(scene.contains(g));
/// ```
pub fn stream_ribbons(
    scene: &mut SceneState,
    field: &VectorField3,
    seeds: &[DVec3],
    params: StreamParams,
) -> MobjectId<VGroup> {
    let mut ids: Vec<AnyId> = Vec::new();

    for &seed in seeds {
        let centers = streamline(field, seed, &params);
        let n = centers.len();
        if n < 2 {
            continue;
        }
        let tang = polyline_tangents(&centers);

        // Rotation-minimizing base frame.
        let mut frames = Vec::with_capacity(n);
        let mut nrm = seed_normal(tang[0]);
        frames.push(nrm);
        for i in 1..n {
            nrm = rmf_step(centers[i - 1], centers[i], tang[i - 1], tang[i], nrm);
            frames.push(nrm);
        }

        // Integrated vorticity twist about the tangent.
        let mut twist = 0.0;
        let mut twists = Vec::with_capacity(n);
        twists.push(0.0);
        for i in 1..n {
            let ds = (centers[i] - centers[i - 1]).length();
            twist += 0.5 * field.curl(centers[i]).dot(tang[i]) * ds;
            twists.push(twist);
        }

        let half = params.radius as f64;
        let mut positions = Vec::with_capacity(2 * n);
        let mut normals = Vec::with_capacity(2 * n);
        for i in 0..n {
            let b = tang[i].cross(frames[i]).normalize();
            let phi = twists[i];
            let w = frames[i] * phi.cos() + b * phi.sin();
            let face = w.cross(tang[i]).normalize();
            positions.push((centers[i] - w * half).as_vec3());
            positions.push((centers[i] + w * half).as_vec3());
            normals.push(face.as_vec3());
            normals.push(face.as_vec3());
        }

        let mut indices = Vec::with_capacity((n - 1) * 6);
        for i in 0..n - 1 {
            let a = (2 * i) as u32;
            let b = (2 * i + 1) as u32;
            let c = (2 * i + 2) as u32;
            let d = (2 * i + 3) as u32;
            indices.extend_from_slice(&[a, b, c, b, d, c]);
        }

        let (speeds, smin, smax, _) = speeds_along(field, &centers);
        let span = (smax - smin).max(1e-9);
        let mut colors = Vec::with_capacity(2 * n);
        for &s in &speeds {
            let col = Colormap::Turbo.sample(((s - smin) / span) as f32);
            colors.push(col);
            colors.push(col);
        }

        let mesh = TriMesh {
            positions,
            normals,
            colors: Some(colors),
            uvs: None,
            indices,
        };
        ids.push(scene.add(Mesh::new(mesh)).into());
    }

    VGroup::of(scene, ids)
}

// ---------------------------------------------------------------------------
// Symmetric 3×3 eigendecomposition + tensor glyphs
// ---------------------------------------------------------------------------

/// Determinant of a symmetric matrix stored as `[xx, xy, xz, yy, yz, zz]`.
fn det_sym3(m: [f64; 6]) -> f64 {
    let [xx, xy, xz, yy, yz, zz] = m;
    xx * (yy * zz - yz * yz) - xy * (xy * zz - yz * xz) + xz * (xy * yz - yy * xz)
}

/// A unit null-space vector of `A − λI` (for a symmetric `A`), or `None` when
/// `A − λI` has rank < 2 (a repeated eigenvalue), found as the largest
/// cross-product of the matrix rows.
fn null_vector(t: [f64; 6], lambda: f64) -> Option<DVec3> {
    let [xx, xy, xz, yy, yz, zz] = t;
    let r0 = DVec3::new(xx - lambda, xy, xz);
    let r1 = DVec3::new(xy, yy - lambda, yz);
    let r2 = DVec3::new(xz, yz, zz - lambda);
    let candidates = [r0.cross(r1), r0.cross(r2), r1.cross(r2)];
    let mut best = DVec3::ZERO;
    let mut best_norm = 0.0;
    for v in candidates {
        let m = v.length();
        if m > best_norm {
            best_norm = m;
            best = v;
        }
    }
    if best_norm > 1e-9 {
        Some(best / best_norm)
    } else {
        None
    }
}

/// Gram–Schmidt `v` against an orthonormal `basis`; `None` if it collapses.
fn orthonormalize_against(v: DVec3, basis: &[DVec3]) -> Option<DVec3> {
    let mut u = v;
    for b in basis {
        u -= *b * b.dot(u);
    }
    let n = u.length();
    if n > 1e-9 {
        Some(u / n)
    } else {
        None
    }
}

/// A unit vector completing an orthonormal `basis` of 0, 1, or 2 vectors.
fn complement(basis: &[DVec3]) -> DVec3 {
    match basis.len() {
        0 => DVec3::X,
        1 => {
            let a = basis[0];
            let axis = if a.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
            (axis - a * a.dot(axis)).normalize()
        }
        _ => basis[0].cross(basis[1]).normalize(),
    }
}

/// Closed-form eigendecomposition of a **symmetric** 3×3 matrix given its six
/// independent entries `[xx, xy, xz, yy, yz, zz]`.
///
/// Returns the eigenvalues in **descending** order together with matching
/// orthonormal eigenvectors, so that `A = Σₖ λₖ vₖ vₖᵀ`. Eigenvalues come from
/// the analytic (Cardano) formula for a symmetric matrix; each eigenvector is
/// the null space of `A − λI` via the largest cross-product of its rows, with
/// Gram–Schmidt completion for degenerate (repeated) eigenvalues — so isotropic
/// and uniaxial tensors return a valid orthonormal frame rather than `NaN`.
///
/// ```
/// use manim_sci::vector_field_3d::eigen_symmetric_3x3;
/// // diag(1, 2, 3): eigenvalues sort to (3, 2, 1).
/// let (vals, _vecs) = eigen_symmetric_3x3([1.0, 0.0, 0.0, 2.0, 0.0, 3.0]);
/// assert!((vals[0] - 3.0).abs() < 1e-9);
/// assert!((vals[2] - 1.0).abs() < 1e-9);
/// ```
pub fn eigen_symmetric_3x3(t: [f64; 6]) -> ([f64; 3], [DVec3; 3]) {
    let [xx, xy, xz, yy, yz, zz] = t;

    // Eigenvalues: analytic symmetric-3×3 (Cardano) form.
    let q = (xx + yy + zz) / 3.0;
    let p1 = xy * xy + xz * xz + yz * yz;
    let p2 = (xx - q).powi(2) + (yy - q).powi(2) + (zz - q).powi(2) + 2.0 * p1;
    let vals = if p2 < 1e-18 {
        // Isotropic: A = qI.
        [q, q, q]
    } else {
        let p = (p2 / 6.0).sqrt();
        let b = [
            (xx - q) / p,
            xy / p,
            xz / p,
            (yy - q) / p,
            yz / p,
            (zz - q) / p,
        ];
        let r = (det_sym3(b) / 2.0).clamp(-1.0, 1.0);
        let phi = r.acos() / 3.0;
        let e0 = q + 2.0 * p * phi.cos();
        let e2 = q + 2.0 * p * (phi + 2.0 * PI / 3.0).cos();
        let e1 = 3.0 * q - e0 - e2; // trace − others; keeps e0 ≥ e1 ≥ e2
        [e0, e1, e2]
    };

    // Eigenvectors: cross-product null spaces, orthonormalized in eigenvalue
    // order; degenerate slots filled from the orthonormal complement.
    let mut basis: Vec<DVec3> = Vec::with_capacity(3);
    let mut vecs = [DVec3::ZERO; 3];
    let mut assigned = [false; 3];
    for (k, &lambda) in vals.iter().enumerate() {
        if let Some(v) = null_vector(t, lambda) {
            if let Some(u) = orthonormalize_against(v, &basis) {
                vecs[k] = u;
                basis.push(u);
                assigned[k] = true;
            }
        }
    }
    for (k, done) in assigned.iter().enumerate() {
        if !done {
            let u = complement(&basis);
            vecs[k] = u;
            basis.push(u);
        }
    }

    (vals, vecs)
}

/// Builds one ellipsoid [`Instance`] per grid point: the local tensor is
/// eigendecomposed, and a unit sphere is placed, oriented to the eigenvectors
/// and scaled by `|λ|·scale` along each. The instance color encodes the
/// anisotropy `(λ_max − λ_min)/(|λ_max|+|λ_min|)` through [`Colormap::Viridis`].
fn glyph_instances(
    tensor: &dyn Fn(DVec3) -> [f64; 6],
    grid: &[DVec3],
    scale: f32,
) -> Vec<Instance> {
    grid.iter()
        .map(|&p| {
            let (vals, vecs) = eigen_symmetric_3x3(tensor(p));
            // Orient a right-handed frame (a reflected sphere is still a sphere,
            // but a proper rotation keeps the instance normals consistent).
            let v0 = vecs[0].as_vec3();
            let v1 = vecs[1].as_vec3();
            let v2 = v0.cross(v1).normalize();
            let s0 = vals[0].abs() as f32 * scale;
            let s1 = vals[1].abs() as f32 * scale;
            let s2 = vals[2].abs() as f32 * scale;
            let m = Mat4::from_cols(
                (v0 * s0).extend(0.0),
                (v1 * s1).extend(0.0),
                (v2 * s2).extend(0.0),
                p.as_vec3().extend(1.0),
            );
            let aniso = ((vals[0] - vals[2]) / (vals[0].abs() + vals[2].abs() + 1e-12))
                .clamp(0.0, 1.0) as f32;
            Instance::new(m, Colormap::Viridis.sample(aniso))
        })
        .collect()
}

/// Draws a symmetric-tensor field as **ellipsoid glyphs**: at every point of
/// `grid` the tensor is eigendecomposed and a unit sphere is stretched along the
/// eigenvectors by the eigenvalues (times `scale`), colored by anisotropy.
///
/// An isotropic tensor (`λ₀ = λ₁ = λ₂`) yields a sphere; a uniaxial one
/// (`diag(a, b, b)`) yields a prolate/oblate spheroid; a general one a triaxial
/// ellipsoid. All glyphs share **one** [`InstancedMesh`] (one instance per grid
/// point), wrapped in a [`VGroup`].
///
/// ```
/// use glam::DVec3;
/// use manim_core::scene_state::SceneState;
/// use manim_sci::vector_field_3d::tensor_glyphs;
///
/// // A uniform uniaxial stress diag(2, 0.5, 0.5) on a tiny grid.
/// let stress = |_p: DVec3| [2.0, 0.0, 0.0, 0.5, 0.0, 0.5];
/// let grid = [DVec3::ZERO, DVec3::X];
/// let mut scene = SceneState::new();
/// let g = tensor_glyphs(&mut scene, &stress, &grid, 0.3);
/// assert!(scene.contains(g));
/// ```
pub fn tensor_glyphs(
    scene: &mut SceneState,
    tensor: &dyn Fn(DVec3) -> [f64; 6],
    grid: &[DVec3],
    scale: f32,
) -> MobjectId<VGroup> {
    let instances = glyph_instances(tensor, grid, scale);
    let mesh = InstancedMesh::new(TriMesh::uv_sphere(12, 24), instances);
    let id = scene.add(mesh);
    VGroup::of(scene, [AnyId::from(id)])
}

// ---------------------------------------------------------------------------
// Surface flux
// ---------------------------------------------------------------------------

/// Numerically integrates the outward flux `∮ F·n̂ dA` of a vector field through
/// a parametric surface `surface(u, v)`.
///
/// The `(u, v)` domain is diced into `resolution × resolution` cells; at each
/// cell midpoint the two parametric tangents `∂S/∂u`, `∂S/∂v` (central
/// differences) are crossed to form the **oriented** area element
/// `(∂S/∂u × ∂S/∂v) du dv` — whose direction is the surface normal and whose
/// magnitude is the area — and dotted with the field. Summing over cells is the
/// discrete divergence theorem: for `F(p) = p` over the unit sphere it returns
/// `∮ p·n̂ dA = ∫ ∇·p dV = 4π`.
///
/// ```
/// use glam::DVec3;
/// use manim_fields::field::{ScalarField, VectorField3};
/// use manim_sci::vector_field_3d::flux_through_surface;
/// use std::f64::consts::{PI, TAU};
///
/// // Radial field F(p) = p through the unit sphere → 4π.
/// let field = VectorField3::from_components(
///     ScalarField::coordinate(0),
///     ScalarField::coordinate(1),
///     ScalarField::coordinate(2),
/// );
/// let sphere = |theta: f64, phi: f64| {
///     DVec3::new(theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos())
/// };
/// let flux = flux_through_surface(&field, sphere, (0.0, PI), (0.0, TAU), 48);
/// assert!((flux - 4.0 * PI).abs() < 0.05 * 4.0 * PI);
/// ```
pub fn flux_through_surface(
    field: &VectorField3,
    surface: impl Fn(f64, f64) -> DVec3,
    u_range: (f64, f64),
    v_range: (f64, f64),
    resolution: usize,
) -> f64 {
    let n = resolution.max(1);
    let du = (u_range.1 - u_range.0) / n as f64;
    let dv = (v_range.1 - v_range.0) / n as f64;
    let h = 1e-6;
    let mut total = 0.0;

    for i in 0..n {
        let u = u_range.0 + (i as f64 + 0.5) * du;
        for j in 0..n {
            let v = v_range.0 + (j as f64 + 0.5) * dv;
            let p = surface(u, v);
            let su = (surface(u + h, v) - surface(u - h, v)) / (2.0 * h);
            let sv = (surface(u, v + h) - surface(u, v - h)) / (2.0 * h);
            let darea = su.cross(sv); // oriented area element per unit (du·dv)
            total += field.at(p).dot(darea) * du * dv;
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_fields::field::ScalarField;

    // ---- eigendecomposition -------------------------------------------------

    /// Reconstruct `A = Σ λ v vᵀ` back into the six-entry storage.
    fn reconstruct(vals: [f64; 3], vecs: [DVec3; 3]) -> [f64; 6] {
        let mut m = [0.0; 6];
        for (k, v) in vecs.iter().enumerate() {
            let l = vals[k];
            m[0] += l * v.x * v.x;
            m[1] += l * v.x * v.y;
            m[2] += l * v.x * v.z;
            m[3] += l * v.y * v.y;
            m[4] += l * v.y * v.z;
            m[5] += l * v.z * v.z;
        }
        m
    }

    fn assert_orthonormal(vecs: &[DVec3; 3]) {
        for a in 0..3 {
            assert!((vecs[a].length() - 1.0).abs() < 1e-9, "vec {a} not unit");
            for b in (a + 1)..3 {
                assert!(vecs[a].dot(vecs[b]).abs() < 1e-8, "vecs {a},{b} not ⟂");
            }
        }
    }

    fn assert_close6(a: [f64; 6], b: [f64; 6], tol: f64) {
        for k in 0..6 {
            assert!((a[k] - b[k]).abs() < tol, "entry {k}: {} vs {}", a[k], b[k]);
        }
    }

    #[test]
    fn eigen_diagonal_matrix() {
        let t = [1.0, 0.0, 0.0, 2.0, 0.0, 3.0]; // diag(1,2,3)
        let (vals, vecs) = eigen_symmetric_3x3(t);
        assert!((vals[0] - 3.0).abs() < 1e-6);
        assert!((vals[1] - 2.0).abs() < 1e-6);
        assert!((vals[2] - 1.0).abs() < 1e-6);
        assert_orthonormal(&vecs);
        assert_close6(reconstruct(vals, vecs), t, 1e-6);
    }

    #[test]
    fn eigen_nondiagonal_matrix() {
        // A = [[2,0,0],[0,3,4],[0,4,9]] → eigenvalues {11, 2, 1}.
        let t = [2.0, 0.0, 0.0, 3.0, 4.0, 9.0];
        let (vals, vecs) = eigen_symmetric_3x3(t);
        assert!((vals[0] - 11.0).abs() < 1e-6, "λ0 = {}", vals[0]);
        assert!((vals[1] - 2.0).abs() < 1e-6, "λ1 = {}", vals[1]);
        assert!((vals[2] - 1.0).abs() < 1e-6, "λ2 = {}", vals[2]);
        assert_orthonormal(&vecs);
        assert_close6(reconstruct(vals, vecs), t, 1e-6);
    }

    #[test]
    fn eigen_full_symmetric_reconstructs() {
        // A dense symmetric matrix (all off-diagonals nonzero).
        let t = [4.0, 1.0, 2.0, 5.0, 3.0, 6.0];
        let (vals, vecs) = eigen_symmetric_3x3(t);
        assert_orthonormal(&vecs);
        assert_close6(reconstruct(vals, vecs), t, 1e-6);
    }

    #[test]
    fn eigen_isotropic_and_uniaxial_are_well_formed() {
        // Isotropic 2·I: equal eigenvalues, still an orthonormal frame.
        let (vals, vecs) = eigen_symmetric_3x3([2.0, 0.0, 0.0, 2.0, 0.0, 2.0]);
        assert!(vals.iter().all(|&l| (l - 2.0).abs() < 1e-9));
        assert_orthonormal(&vecs);
        // Uniaxial diag(2, 0.5, 0.5): one eigenvalue 2, a repeated 0.5.
        let (vals, vecs) = eigen_symmetric_3x3([2.0, 0.0, 0.0, 0.5, 0.0, 0.5]);
        assert!((vals[0] - 2.0).abs() < 1e-9);
        assert!((vals[1] - 0.5).abs() < 1e-9 && (vals[2] - 0.5).abs() < 1e-9);
        assert_orthonormal(&vecs);
        assert_close6(
            reconstruct(vals, vecs),
            [2.0, 0.0, 0.0, 0.5, 0.0, 0.5],
            1e-9,
        );
    }

    // ---- tensor glyphs ------------------------------------------------------

    /// The three per-axis scale factors of an instance (its column lengths).
    fn instance_scales(inst: &Instance) -> [f32; 3] {
        [
            inst.transform.x_axis.truncate().length(),
            inst.transform.y_axis.truncate().length(),
            inst.transform.z_axis.truncate().length(),
        ]
    }

    #[test]
    fn glyph_isotropic_tensor_is_a_sphere() {
        let iso = |_p: DVec3| [1.5, 0.0, 0.0, 1.5, 0.0, 1.5];
        let inst = glyph_instances(&iso, &[DVec3::ZERO], 1.0);
        let s = instance_scales(&inst[0]);
        // All three axes scaled equally (≈ 1.5).
        assert!((s[0] - 1.5).abs() < 1e-5);
        assert!((s[1] - 1.5).abs() < 1e-5);
        assert!((s[2] - 1.5).abs() < 1e-5);
        // Sits at the grid point.
        assert!(inst[0].transform.w_axis.truncate().length() < 1e-6);
    }

    #[test]
    fn glyph_uniaxial_tensor_is_prolate() {
        // diag(2, 0.5, 0.5): one long axis, two equal short axes → 4:1 ratio.
        let uni = |_p: DVec3| [2.0, 0.0, 0.0, 0.5, 0.0, 0.5];
        let inst = glyph_instances(&uni, &[DVec3::new(1.0, 2.0, 3.0)], 0.5);
        let mut s = instance_scales(&inst[0]);
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // Sorted scales ≈ (0.25, 0.25, 1.0) → max/min = 4.
        assert!((s[2] / s[0] - 4.0).abs() < 1e-4, "ratio {}", s[2] / s[0]);
        assert!((s[1] / s[0] - 1.0).abs() < 1e-4, "short axes unequal");
        // Placed at its grid point.
        assert!((inst[0].transform.w_axis.truncate() - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
    }

    // ---- flux / divergence theorem -----------------------------------------

    fn unit_sphere(theta: f64, phi: f64) -> DVec3 {
        DVec3::new(
            theta.sin() * phi.cos(),
            theta.sin() * phi.sin(),
            theta.cos(),
        )
    }

    #[test]
    fn divergence_theorem_radial_field_over_sphere() {
        // F(p) = p ⇒ ∮ F·n dA = ∫ div(p) dV = 3·(4/3 π) = 4π.
        let field = VectorField3::from_components(
            ScalarField::coordinate(0),
            ScalarField::coordinate(1),
            ScalarField::coordinate(2),
        );
        let flux = flux_through_surface(&field, unit_sphere, (0.0, PI), (0.0, TAU), 64);
        let want = 4.0 * PI;
        println!("radial flux through unit sphere = {flux:.6} (want {want:.6})");
        assert!(
            (flux - want).abs() < 0.01 * want,
            "flux {flux} not within 1% of {want}"
        );
    }

    #[test]
    fn flux_of_x_field_over_sphere() {
        // F = (x, 0, 0) ⇒ ∮ F·n dA = ∫ div dV = 1·(4/3 π) = 4π/3.
        let field = VectorField3::from_components(
            ScalarField::coordinate(0),
            ScalarField::constant(0.0),
            ScalarField::constant(0.0),
        );
        let flux = flux_through_surface(&field, unit_sphere, (0.0, PI), (0.0, TAU), 64);
        let want = 4.0 * PI / 3.0;
        println!("(x,0,0) flux through unit sphere = {flux:.6} (want {want:.6})");
        assert!(
            (flux - want).abs() < 0.01 * want,
            "flux {flux} not within 1% of {want}"
        );
    }

    // ---- streamlines --------------------------------------------------------

    fn rotational_field() -> VectorField3 {
        VectorField3::from_components(
            ScalarField::coordinate(1).scale(-1.0),
            ScalarField::coordinate(0),
            ScalarField::constant(0.0),
        )
    }

    #[test]
    fn streamline_of_rotation_is_a_circle() {
        // Seeded at (1,0,0), the streamline of (−y, x, 0) is the unit circle.
        let field = rotational_field();
        let params = StreamParams {
            length: 6.0,
            step: 0.1,
            ..StreamParams::default()
        };
        let centers = streamline(&field, DVec3::new(1.0, 0.0, 0.0), &params);
        assert!(centers.len() > 10);
        for c in &centers {
            let radius = (c.x * c.x + c.y * c.y).sqrt();
            assert!((radius - 1.0).abs() < 1e-3, "off-circle radius {radius}");
            assert!(c.z.abs() < 1e-9, "left the plane: z = {}", c.z);
        }
    }

    #[test]
    fn stream_tube_builds_nonempty_mesh() {
        let field = rotational_field();
        let params = StreamParams {
            length: 3.0,
            step: 0.2,
            radius: 0.1,
            n_around: 8,
            flux_conserving: false,
        };
        let centers = streamline(&field, DVec3::new(1.0, 0.0, 0.0), &params);
        let cvecs: Vec<Vec3> = centers.iter().map(|c| c.as_vec3()).collect();
        let tube = tube_from_polyline(&cvecs, |_| params.radius, params.n_around);
        assert!(!tube.positions.is_empty());
        assert!(!tube.indices.is_empty());
        assert_eq!(tube.positions.len(), centers.len() * params.n_around);

        // And the public entry point registers a group in the scene.
        let mut scene = SceneState::new();
        let g = stream_tubes(&mut scene, &field, &[DVec3::new(1.0, 0.0, 0.0)], params);
        assert!(scene.contains(g));
    }

    #[test]
    fn stream_ribbon_registers_in_scene() {
        let field = VectorField3::from_components(
            ScalarField::coordinate(1).scale(-1.0),
            ScalarField::coordinate(0),
            ScalarField::constant(0.4),
        );
        let mut scene = SceneState::new();
        let params = StreamParams {
            length: 3.0,
            step: 0.25,
            radius: 0.15,
            ..StreamParams::default()
        };
        let g = stream_ribbons(&mut scene, &field, &[DVec3::new(1.0, 0.0, 0.0)], params);
        assert!(scene.contains(g));
    }
}
