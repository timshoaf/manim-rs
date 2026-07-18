//! Curvature visualization and swept tubes.
//!
//! - [`surface_colored_by_curvature`] bakes a surface's Gaussian or mean
//!   curvature into per-vertex colors through a [`Colormap`].
//! - [`TubeMesh::along_curve`] sweeps a circular cross-section along a space
//!   curve using a **rotation-minimizing frame** (not the raw Frenet frame),
//!   which stays well-defined through inflection points where the Frenet normal
//!   flips.
//! - [`trefoil`] / [`figure_eight`] are ready-made knot curves.

use glam::{DVec3, Vec3};

use manim_core::display::Colormap;
use manim_core::mesh::{Mesh, TriMesh};
use manim_core::mobject::MobjectId;
use manim_core::scene_state::SceneState;

use manim_fields::ad::Scalar;

use crate::diffgeo::{
    frenet_frame, gaussian_curvature, mean_curvature, normal, CurveSampler, SurfaceSampler,
};

/// Which curvature scalar to visualize.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurvatureKind {
    /// Gaussian curvature `K = κ₁κ₂`.
    Gaussian,
    /// Mean curvature `H = (κ₁+κ₂)/2`.
    Mean,
}

fn to_vec3(p: DVec3) -> Vec3 {
    p.as_vec3()
}

fn sample_position<Sf: SurfaceSampler>(s: &Sf, u: f64, v: f64) -> DVec3 {
    let [x, y, z] = s.eval::<f64>(u, v);
    DVec3::new(x, y, z)
}

/// Builds a triangulated surface mesh colored by curvature: samples an
/// `nu × nv` grid over `u_range × v_range`, evaluates the chosen curvature at
/// each vertex, and maps it through `colormap` (auto-ranged to the sampled
/// min/max).
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::display::Colormap;
/// use manim_fields::ad::Scalar;
/// use manim_sci::curveviz::{surface_colored_by_curvature, CurvatureKind};
/// use manim_sci::diffgeo::SurfaceSampler;
/// // A torus surface.
/// struct Torus;
/// impl SurfaceSampler for Torus {
///     fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
///         let r = S::constant(1.0) + u.cos().scale(0.4);
///         [r * v.cos(), r * v.sin(), u.sin().scale(0.4)]
///     }
/// }
/// let mut scene = Scene::new(Config::default());
/// let m = surface_colored_by_curvature(
///     scene.state_mut(), &Torus, CurvatureKind::Gaussian, Colormap::Coolwarm,
///     (0.0, std::f64::consts::TAU), (0.0, std::f64::consts::TAU), (24, 24));
/// assert!(scene.state().contains(m));
/// ```
pub fn surface_colored_by_curvature<Sf: SurfaceSampler>(
    scene: &mut SceneState,
    sampler: &Sf,
    kind: CurvatureKind,
    colormap: Colormap,
    u_range: (f64, f64),
    v_range: (f64, f64),
    resolution: (usize, usize),
) -> MobjectId<Mesh> {
    let (nu, nv) = (resolution.0.max(1), resolution.1.max(1));
    let (mut positions, mut normals, mut values) = (Vec::new(), Vec::new(), Vec::new());

    for i in 0..=nu {
        let u = u_range.0 + (u_range.1 - u_range.0) * i as f64 / nu as f64;
        for j in 0..=nv {
            let v = v_range.0 + (v_range.1 - v_range.0) * j as f64 / nv as f64;
            positions.push(to_vec3(sample_position(sampler, u, v)));
            normals.push(to_vec3(normal(sampler, u, v).normalize()));
            values.push(match kind {
                CurvatureKind::Gaussian => gaussian_curvature(sampler, u, v),
                CurvatureKind::Mean => mean_curvature(sampler, u, v),
            });
        }
    }

    // Auto-range the colormap to the sampled curvature extent.
    let vmin = values.iter().copied().fold(f64::INFINITY, f64::min);
    let vmax = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (vmax - vmin).max(1e-12);
    let colors = values
        .iter()
        .map(|&k| colormap.sample(((k - vmin) / span) as f32))
        .collect();

    let mut indices = Vec::with_capacity(nu * nv * 6);
    let idx = |i: usize, j: usize| (i * (nv + 1) + j) as u32;
    for i in 0..nu {
        for j in 0..nv {
            let (a, b, c, d) = (idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
            indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }

    let mesh = TriMesh {
        positions,
        normals,
        colors: Some(colors),
        uvs: None,
        indices,
    };
    scene.add(Mesh::new(mesh))
}

/// A tube swept along a space curve.
pub struct TubeMesh;

impl TubeMesh {
    /// Sweeps a circle of `radius` along `curve` over `t_range`, using a
    /// rotation-minimizing frame propagated by the double-reflection method
    /// (Wang et al. 2008) — robust through inflection points, unlike the Frenet
    /// frame. `n_along` rings × `n_around` sides. `closed` welds the last ring to
    /// the first (for knots / closed loops).
    ///
    /// ```
    /// use manim_fields::ad::Scalar;
    /// use manim_sci::curveviz::TubeMesh;
    /// use manim_sci::diffgeo::CurveSampler;
    /// // A circle.
    /// struct Circle;
    /// impl CurveSampler for Circle {
    ///     fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
    ///         [t.cos(), t.sin(), S::constant(0.0)]
    ///     }
    /// }
    /// let tube = TubeMesh::along_curve(&Circle, (0.0, std::f64::consts::TAU), 0.1, 40, 12, true);
    /// assert!(!tube.positions.is_empty() && !tube.indices.is_empty());
    /// ```
    pub fn along_curve<C: CurveSampler>(
        curve: &C,
        t_range: (f64, f64),
        radius: f64,
        n_along: usize,
        n_around: usize,
        closed: bool,
    ) -> TriMesh {
        let n_along = n_along.max(2);
        let n_around = n_around.max(3);

        // Sample centre points and unit tangents.
        let mut centers = Vec::with_capacity(n_along);
        let mut tangents = Vec::with_capacity(n_along);
        for i in 0..n_along {
            let t = t_range.0 + (t_range.1 - t_range.0) * i as f64 / (n_along - 1) as f64;
            let [x, y, z] = curve.eval::<f64>(t);
            centers.push(DVec3::new(x, y, z));
            tangents.push(frenet_frame(curve, t).t);
        }

        // Rotation-minimizing frame: seed a normal ⟂ the first tangent, then
        // propagate by double reflection.
        let mut nrm = seed_normal(tangents[0]);
        let mut frames = Vec::with_capacity(n_along);
        frames.push(nrm);
        for i in 1..n_along {
            nrm = rmf_step(
                centers[i - 1],
                centers[i],
                tangents[i - 1],
                tangents[i],
                nrm,
            );
            frames.push(nrm);
        }

        // Ring vertices + outward normals.
        let mut positions = Vec::with_capacity(n_along * n_around);
        let mut vnormals = Vec::with_capacity(n_along * n_around);
        for i in 0..n_along {
            let n = frames[i];
            let b = tangents[i].cross(n).normalize();
            for j in 0..n_around {
                let theta = std::f64::consts::TAU * j as f64 / n_around as f64;
                let dir = n * theta.cos() + b * theta.sin();
                positions.push(to_vec3(centers[i] + dir * radius));
                vnormals.push(to_vec3(dir));
            }
        }

        // Triangulate quads between consecutive rings.
        let ring_count = if closed { n_along } else { n_along - 1 };
        let mut indices = Vec::with_capacity(ring_count * n_around * 6);
        for i in 0..ring_count {
            let i1 = (i + 1) % n_along;
            for j in 0..n_around {
                let j1 = (j + 1) % n_around;
                let a = (i * n_around + j) as u32;
                let b = (i1 * n_around + j) as u32;
                let c = (i1 * n_around + j1) as u32;
                let d = (i * n_around + j1) as u32;
                indices.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }

        TriMesh {
            positions,
            normals: vnormals,
            colors: None,
            uvs: None,
            indices,
        }
    }
}

/// A unit-length vector perpendicular to `t`.
fn seed_normal(t: DVec3) -> DVec3 {
    let a = if t.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    (a - t * a.dot(t)).normalize()
}

/// One double-reflection RMF step: transport `n` from the frame at `p0` (tangent
/// `t0`) to `p1` (tangent `t1`).
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

/// The trefoil knot — the `(2, 3)` torus knot `((2+cos 3t)cos 2t,
/// (2+cos 3t)sin 2t, sin 3t)`, `t ∈ [0, 2π]`.
pub fn trefoil() -> impl CurveSampler {
    struct Trefoil;
    impl CurveSampler for Trefoil {
        fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
            let r = t.scale(3.0).cos() + S::constant(2.0);
            [
                r * t.scale(2.0).cos(),
                r * t.scale(2.0).sin(),
                t.scale(3.0).sin(),
            ]
        }
    }
    Trefoil
}

/// The figure-eight knot `((2+cos 2t)cos 3t, (2+cos 2t)sin 3t, sin 4t)`,
/// `t ∈ [0, 2π]`.
pub fn figure_eight() -> impl CurveSampler {
    struct FigureEight;
    impl CurveSampler for FigureEight {
        fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
            let r = t.scale(2.0).cos() + S::constant(2.0);
            [
                r * t.scale(3.0).cos(),
                r * t.scale(3.0).sin(),
                t.scale(4.0).sin(),
            ]
        }
    }
    FigureEight
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    struct Circle;
    impl CurveSampler for Circle {
        fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
            [t.cos(), t.sin(), S::constant(0.0)]
        }
    }

    #[test]
    fn tube_vertices_sit_at_radius_from_the_axis() {
        // Every tube vertex is `radius` from its ring centre on the unit circle.
        let r = 0.15;
        let tube = TubeMesh::along_curve(&Circle, (0.0, TAU), r, 60, 10, true);
        assert_eq!(tube.positions.len(), 60 * 10);
        for p in &tube.positions {
            let planar = (p.x * p.x + p.y * p.y).sqrt();
            let d = ((planar - 1.0).powi(2) + p.z * p.z).sqrt();
            assert!((d - r as f32).abs() < 1e-4, "off-tube distance {d}");
        }
    }

    #[test]
    fn rmf_frame_stays_orthonormal() {
        let n0 = seed_normal(DVec3::X);
        assert!((n0.length() - 1.0).abs() < 1e-12);
        assert!(n0.dot(DVec3::X).abs() < 1e-12);
        let n1 = rmf_step(
            DVec3::ZERO,
            DVec3::new(0.0, 1.0, 0.0),
            DVec3::Y,
            DVec3::Y,
            n0,
        );
        assert!((n1.length() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn knots_are_closed_loops() {
        let close = |g: [f64; 3], h: [f64; 3]| {
            (g[0] - h[0]).abs() + (g[1] - h[1]).abs() + (g[2] - h[2]).abs() < 1e-9
        };
        assert!(close(
            trefoil().eval::<f64>(0.0),
            trefoil().eval::<f64>(TAU)
        ));
        assert!(close(
            figure_eight().eval::<f64>(0.0),
            figure_eight().eval::<f64>(TAU)
        ));
    }
}
