//! [`TriMesh`]: the indexed triangle mesh payload, its unit-primitive builders,
//! and same-topology interpolation.

use glam::{Vec2, Vec3};
use manim_color::Color;

use crate::error::{CoreError, Result};

/// Relative step used for the central differences that give
/// [`TriMesh::from_parametric`] its normals.
const DIFF_STEP: f64 = 1e-3;

/// An indexed triangle mesh. Positions and normals are in mobject-local space.
///
/// Front faces are wound **counter-clockwise** and [`normals`](Self::normals)
/// are unit length — every builder here upholds both. Index triples address
/// [`positions`](Self::positions); the optional per-vertex
/// [`colors`](Self::colors) tint the material color, and
/// [`uvs`](Self::uvs) are texture coordinates.
///
/// ```
/// use manim_core::mesh::TriMesh;
/// // The unit sphere's normals are its positions.
/// let sphere = TriMesh::uv_sphere(8, 16);
/// for (p, n) in sphere.positions.iter().zip(&sphere.normals) {
///     assert!((p.normalize() - *n).length() < 1e-5);
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriMesh {
    /// Vertex positions in mobject-local space.
    pub positions: Vec<Vec3>,
    /// Unit vertex normals, parallel to [`positions`](Self::positions).
    pub normals: Vec<Vec3>,
    /// Optional per-vertex tint; `None` means "use the material color".
    pub colors: Option<Vec<Color>>,
    /// Optional per-vertex texture coordinates.
    pub uvs: Option<Vec<Vec2>>,
    /// Triangle indices, three per face, counter-clockwise when seen from the
    /// front.
    pub indices: Vec<u32>,
}

impl TriMesh {
    /// The number of vertices.
    ///
    /// ```
    /// use manim_core::mesh::TriMesh;
    /// // A 2 × 3 grid of cells has 3 × 4 vertices.
    /// assert_eq!(TriMesh::grid(2, 3).len(), 12);
    /// ```
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Whether the mesh has no vertices.
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// The number of triangles.
    ///
    /// ```
    /// use manim_core::mesh::TriMesh;
    /// // Each grid cell is two triangles.
    /// assert_eq!(TriMesh::grid(2, 3).n_triangles(), 12);
    /// ```
    pub fn n_triangles(&self) -> usize {
        self.indices.len() / 3
    }

    /// The axis-aligned local bounds as `(min, max)`, or `None` when empty.
    ///
    /// ```
    /// use manim_core::mesh::TriMesh;
    /// let (min, max) = TriMesh::uv_sphere(8, 16).bounds().unwrap();
    /// assert!((max.x - 1.0).abs() < 1e-5 && (min.x + 1.0).abs() < 1e-5);
    /// ```
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut it = self.positions.iter();
        let first = *it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))))
    }

    /// Six times the signed volume of the mesh, positive for a closed mesh with
    /// outward-facing counter-clockwise winding.
    ///
    /// This is the winding-consistency check the builders are tested against; it
    /// is only meaningful for closed meshes.
    ///
    /// ```
    /// use manim_core::mesh::TriMesh;
    /// assert!(TriMesh::uv_sphere(12, 24).signed_volume() > 0.0);
    /// assert!(TriMesh::cylinder(24).signed_volume() > 0.0);
    /// ```
    pub fn signed_volume(&self) -> f32 {
        self.indices
            .chunks_exact(3)
            .map(|t| {
                let a = self.positions[t[0] as usize];
                let b = self.positions[t[1] as usize];
                let c = self.positions[t[2] as usize];
                a.dot(b.cross(c)) / 6.0
            })
            .sum()
    }

    /// Replaces every vertex color with `colors`, or clears them with `None`.
    ///
    /// A color list of the wrong length is rejected, keeping the mesh valid.
    pub fn set_colors(&mut self, colors: Option<Vec<Color>>) -> Result<()> {
        match colors {
            Some(c) if c.len() != self.positions.len() => Err(CoreError::MeshTopology(format!(
                "color list has {} entries but the mesh has {} vertices",
                c.len(),
                self.positions.len()
            ))),
            other => {
                self.colors = other;
                Ok(())
            }
        }
    }

    /// A unit square in the `z = 0` plane, spanning `[-0.5, 0.5]²`, divided into
    /// `nu × nv` cells (so `(nu + 1) × (nv + 1)` vertices). Normals face `+Z`.
    ///
    /// `nu`/`nv` are clamped to at least 1.
    ///
    /// ```
    /// use manim_core::mesh::TriMesh;
    /// let g = TriMesh::grid(4, 4);
    /// assert_eq!(g.len(), 25);
    /// assert_eq!(g.indices.len(), 4 * 4 * 6);
    /// ```
    pub fn grid(nu: usize, nv: usize) -> Self {
        Self::from_parametric(
            |u, v| Vec3::new(u as f32, v as f32, 0.0),
            (-0.5, 0.5),
            (-0.5, 0.5),
            (nu, nv),
        )
    }

    /// A unit-radius sphere centered at the origin, with `rings` latitude bands
    /// and `segments` longitude divisions. Normals equal positions.
    ///
    /// ```
    /// use manim_core::mesh::TriMesh;
    /// let s = TriMesh::uv_sphere(8, 16);
    /// // Every vertex sits on the unit sphere.
    /// assert!(s.positions.iter().all(|p| (p.length() - 1.0).abs() < 1e-5));
    /// ```
    pub fn uv_sphere(rings: usize, segments: usize) -> Self {
        let (rings, segments) = (rings.max(2), segments.max(3));
        let mut positions = Vec::with_capacity((rings + 1) * (segments + 1));
        let mut uvs = Vec::with_capacity((rings + 1) * (segments + 1));
        for i in 0..=rings {
            let fv = i as f32 / rings as f32;
            let phi = fv * std::f32::consts::PI;
            let (sp, cp) = phi.sin_cos();
            for j in 0..=segments {
                let fu = j as f32 / segments as f32;
                let theta = fu * std::f32::consts::TAU;
                let (st, ct) = theta.sin_cos();
                positions.push(Vec3::new(sp * ct, sp * st, cp));
                uvs.push(Vec2::new(fu, fv));
            }
        }
        // Normals of a unit sphere are its positions; the poles are exact.
        let normals = positions.clone();
        let mut indices = Vec::with_capacity(rings * segments * 6);
        let at = |i: usize, j: usize| (i * (segments + 1) + j) as u32;
        for i in 0..rings {
            for j in 0..segments {
                // `i` grows southward, `j` grows counter-clockwise seen from +Z;
                // this ordering makes the faces wind CCW seen from outside.
                let (a, b, c, d) = (at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j));
                indices.extend_from_slice(&[a, d, b, b, d, c]);
            }
        }
        Self {
            positions,
            normals,
            colors: None,
            uvs: Some(uvs),
            indices,
        }
    }

    /// A capped unit-radius cylinder of height 1 along `+Z`, centered at the
    /// origin (so its caps sit at `z = ±0.5`), with `segments` divisions around.
    ///
    /// The side wall has radial normals; the caps have `±Z` normals, which is why
    /// their rims do not share vertices with the wall.
    ///
    /// ```
    /// use manim_core::mesh::TriMesh;
    /// let c = TriMesh::cylinder(16);
    /// // Closed, and approaching the true volume π r² h = π from below as the
    /// // inscribed 16-gon does.
    /// assert!((c.signed_volume() - std::f32::consts::PI).abs() < 0.1);
    /// ```
    pub fn cylinder(segments: usize) -> Self {
        let segments = segments.max(3);
        let mut m = Self::default();
        let ring = |z: f32| {
            (0..=segments).map(move |j| {
                let theta = j as f32 / segments as f32 * std::f32::consts::TAU;
                let (s, c) = theta.sin_cos();
                Vec3::new(c, s, z)
            })
        };
        // --- Side wall: radial normals, one shared seam vertex column. ---
        for (z, v) in [(-0.5, 0.0), (0.5, 1.0)] {
            for (j, p) in ring(z).enumerate() {
                m.positions.push(p);
                m.normals.push(Vec3::new(p.x, p.y, 0.0));
                m.uvs
                    .get_or_insert_with(Vec::new)
                    .push(Vec2::new(j as f32 / segments as f32, v));
            }
        }
        let stride = segments + 1;
        for j in 0..segments {
            let (b0, b1) = (j as u32, (j + 1) as u32);
            let (t0, t1) = (b0 + stride as u32, b1 + stride as u32);
            m.indices.extend_from_slice(&[b0, b1, t1, b0, t1, t0]);
        }
        // --- Caps: a fan per end, with its own vertices for the flat normal. ---
        for (z, normal, flip) in [(0.5_f32, Vec3::Z, false), (-0.5, Vec3::NEG_Z, true)] {
            let center = m.positions.len() as u32;
            m.positions.push(Vec3::new(0.0, 0.0, z));
            m.normals.push(normal);
            m.uvs.get_or_insert_with(Vec::new).push(Vec2::splat(0.5));
            let rim = m.positions.len() as u32;
            for p in ring(z) {
                m.positions.push(p);
                m.normals.push(normal);
                m.uvs
                    .get_or_insert_with(Vec::new)
                    .push(Vec2::new(p.x * 0.5 + 0.5, p.y * 0.5 + 0.5));
            }
            for j in 0..segments as u32 {
                let (a, b) = (rim + j, rim + j + 1);
                // The bottom cap faces -Z, so its fan winds the other way.
                if flip {
                    m.indices.extend_from_slice(&[center, b, a]);
                } else {
                    m.indices.extend_from_slice(&[center, a, b]);
                }
            }
        }
        m
    }

    /// A parametric surface `f(u, v)` sampled on a shared `(nu + 1) × (nv + 1)`
    /// vertex grid over `u_range × v_range`, with normals from analytic central
    /// differences: `n = normalize(∂f/∂u × ∂f/∂v)`.
    ///
    /// Winding follows the same convention, so a face is front-facing from the
    /// side its `∂u × ∂v` normal points to. Vertices are shared between cells;
    /// [`from_parametric_cells`](Self::from_parametric_cells) splits them
    /// instead. The differences step slightly outside the ranges at the borders,
    /// which is exact for an analytically-defined `f`. `nu`/`nv` are clamped to
    /// at least 1.
    ///
    /// ```
    /// use manim_core::mesh::TriMesh;
    /// use glam::Vec3;
    /// // A saddle: normals stay unit-length everywhere.
    /// let m = TriMesh::from_parametric(
    ///     |u, v| Vec3::new(u as f32, v as f32, (u * u - v * v) as f32),
    ///     (-1.0, 1.0),
    ///     (-1.0, 1.0),
    ///     (8, 8),
    /// );
    /// assert_eq!(m.len(), 81);
    /// assert!(m.normals.iter().all(|n| (n.length() - 1.0).abs() < 1e-4));
    /// ```
    pub fn from_parametric<F>(
        f: F,
        u_range: (f64, f64),
        v_range: (f64, f64),
        res: (usize, usize),
    ) -> Self
    where
        F: Fn(f64, f64) -> Vec3,
    {
        let (nu, nv) = (res.0.max(1), res.1.max(1));
        let sample = ParamSampler::new(&f, u_range, v_range);
        let mut positions = Vec::with_capacity((nu + 1) * (nv + 1));
        let mut normals = Vec::with_capacity((nu + 1) * (nv + 1));
        let mut uvs = Vec::with_capacity((nu + 1) * (nv + 1));
        for i in 0..=nu {
            let (u, fu) = sample.u_at(i, nu);
            for j in 0..=nv {
                let (v, fv) = sample.v_at(j, nv);
                positions.push(f(u, v));
                normals.push(sample.normal(u, v));
                uvs.push(Vec2::new(fu, fv));
            }
        }
        let mut indices = Vec::with_capacity(nu * nv * 6);
        let at = |i: usize, j: usize| (i * (nv + 1) + j) as u32;
        for i in 0..nu {
            for j in 0..nv {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                indices.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }
        Self {
            positions,
            normals,
            colors: None,
            uvs: Some(uvs),
            indices,
        }
    }

    /// Like [`from_parametric`](Self::from_parametric), but every cell gets its
    /// own four vertices — `nu × nv × 4` in all, cell `(i, j)` occupying vertices
    /// `4 * (i * nv + j) ..`.
    ///
    /// Splitting the vertices is what lets a cell carry its own flat color (the
    /// checkerboard of [`Surface3D`](crate::mesh::Surface3D)) without bleeding
    /// into its neighbors; normals are still the analytic per-corner ones, so
    /// smooth shading is unaffected.
    ///
    /// ```
    /// use manim_core::mesh::TriMesh;
    /// use glam::Vec3;
    /// let m = TriMesh::from_parametric_cells(
    ///     |u, v| Vec3::new(u as f32, v as f32, 0.0),
    ///     (0.0, 1.0),
    ///     (0.0, 1.0),
    ///     (3, 2),
    /// );
    /// assert_eq!(m.len(), 3 * 2 * 4);
    /// assert_eq!(m.n_triangles(), 3 * 2 * 2);
    /// ```
    pub fn from_parametric_cells<F>(
        f: F,
        u_range: (f64, f64),
        v_range: (f64, f64),
        res: (usize, usize),
    ) -> Self
    where
        F: Fn(f64, f64) -> Vec3,
    {
        let (nu, nv) = (res.0.max(1), res.1.max(1));
        let sample = ParamSampler::new(&f, u_range, v_range);
        let mut m = Self {
            positions: Vec::with_capacity(nu * nv * 4),
            normals: Vec::with_capacity(nu * nv * 4),
            colors: None,
            uvs: Some(Vec::with_capacity(nu * nv * 4)),
            indices: Vec::with_capacity(nu * nv * 6),
        };
        for i in 0..nu {
            for j in 0..nv {
                let base = m.positions.len() as u32;
                // Corners in the same (a, b, c, d) order as the shared grid.
                for (di, dj) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
                    let (u, fu) = sample.u_at(i + di, nu);
                    let (v, fv) = sample.v_at(j + dj, nv);
                    m.positions.push(f(u, v));
                    m.normals.push(sample.normal(u, v));
                    if let Some(uvs) = &mut m.uvs {
                        uvs.push(Vec2::new(fu, fv));
                    }
                }
                m.indices
                    .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        m
    }

    /// Interpolates two same-topology meshes: positions and colors blend
    /// linearly, normals blend then re-normalize.
    ///
    /// The two meshes must agree on vertex count and index buffer; otherwise this
    /// returns [`CoreError::MeshTopology`] rather than producing a corrupt mesh.
    /// Per-vertex colors and uvs are taken from `a` when only one side has them.
    ///
    /// ```
    /// use manim_core::mesh::TriMesh;
    /// use glam::Vec3;
    /// let flat = TriMesh::grid(2, 2);
    /// let mut bumped = flat.clone();
    /// bumped.positions.iter_mut().for_each(|p| p.z += 1.0);
    /// // The endpoints are exact and the midpoint is halfway.
    /// assert_eq!(TriMesh::lerp(&flat, &bumped, 0.0).unwrap(), flat);
    /// assert_eq!(TriMesh::lerp(&flat, &bumped, 1.0).unwrap(), bumped);
    /// let mid = TriMesh::lerp(&flat, &bumped, 0.5).unwrap();
    /// assert!((mid.positions[0].z - 0.5).abs() < 1e-6);
    /// ```
    pub fn lerp(a: &TriMesh, b: &TriMesh, t: f32) -> Result<TriMesh> {
        if a.positions.len() != b.positions.len() {
            return Err(CoreError::MeshTopology(format!(
                "vertex counts differ: {} vs {}",
                a.positions.len(),
                b.positions.len()
            )));
        }
        if a.indices != b.indices {
            return Err(CoreError::MeshTopology(
                "index buffers differ; only same-topology meshes interpolate".into(),
            ));
        }
        // Exact endpoints: `t` of 0 or 1 must reproduce the input bit-for-bit.
        if t == 0.0 {
            return Ok(a.clone());
        }
        if t == 1.0 {
            return Ok(b.clone());
        }
        let positions = a
            .positions
            .iter()
            .zip(&b.positions)
            .map(|(p, q)| p.lerp(*q, t))
            .collect();
        let normals = a
            .normals
            .iter()
            .zip(&b.normals)
            .map(|(m, n)| {
                let blended = m.lerp(*n, t);
                // Opposed normals cancel exactly at the midpoint; keep `a`'s.
                blended.try_normalize().unwrap_or(*m)
            })
            .collect();
        let colors = match (&a.colors, &b.colors) {
            (Some(ca), Some(cb)) if ca.len() == cb.len() => Some(
                ca.iter()
                    .zip(cb)
                    .map(|(x, y)| x.interpolate(y, t))
                    .collect(),
            ),
            (Some(ca), _) => Some(ca.clone()),
            (None, other) => other.clone(),
        };
        Ok(TriMesh {
            positions,
            normals,
            colors,
            uvs: a.uvs.clone().or_else(|| b.uvs.clone()),
            indices: a.indices.clone(),
        })
    }

    /// Applies `m` to every position and normal, baking a transform into the
    /// geometry. Normals use the inverse-transpose, so non-uniform scaling stays
    /// correct, and are re-normalized.
    ///
    /// ```
    /// use manim_core::mesh::TriMesh;
    /// use glam::{Mat4, Vec3};
    /// let mut m = TriMesh::grid(1, 1);
    /// m.transform(Mat4::from_translation(Vec3::Z));
    /// assert!(m.positions.iter().all(|p| (p.z - 1.0).abs() < 1e-6));
    /// ```
    pub fn transform(&mut self, m: glam::Mat4) {
        let normal_matrix = glam::Mat3::from_mat4(m).inverse().transpose();
        for p in &mut self.positions {
            *p = m.transform_point3(*p);
        }
        for n in &mut self.normals {
            *n = (normal_matrix * *n).try_normalize().unwrap_or(*n);
        }
    }
}

/// Shared parameter-grid helper: maps grid indices to parameters and evaluates
/// central-difference normals.
struct ParamSampler<'a, F> {
    f: &'a F,
    u_range: (f64, f64),
    v_range: (f64, f64),
    du: f64,
    dv: f64,
}

impl<'a, F: Fn(f64, f64) -> Vec3> ParamSampler<'a, F> {
    fn new(f: &'a F, u_range: (f64, f64), v_range: (f64, f64)) -> Self {
        let step = |r: (f64, f64)| ((r.1 - r.0).abs() * DIFF_STEP).max(1e-6);
        Self {
            f,
            u_range,
            v_range,
            du: step(u_range),
            dv: step(v_range),
        }
    }

    /// The `(parameter, normalized [0, 1] coordinate)` at grid index `i` of `n`.
    fn u_at(&self, i: usize, n: usize) -> (f64, f32) {
        let t = i as f64 / n as f64;
        (
            self.u_range.0 + (self.u_range.1 - self.u_range.0) * t,
            t as f32,
        )
    }

    fn v_at(&self, j: usize, n: usize) -> (f64, f32) {
        let t = j as f64 / n as f64;
        (
            self.v_range.0 + (self.v_range.1 - self.v_range.0) * t,
            t as f32,
        )
    }

    /// The unit normal at `(u, v)` from analytic central differences, falling
    /// back to `+Z` where the surface is degenerate (a pole, a crease).
    fn normal(&self, u: f64, v: f64) -> Vec3 {
        let fu = ((self.f)(u + self.du, v) - (self.f)(u - self.du, v)) / (2.0 * self.du as f32);
        let fv = ((self.f)(u, v + self.dv) - (self.f)(u, v - self.dv)) / (2.0 * self.dv as f32);
        fu.cross(fv).try_normalize().unwrap_or(Vec3::Z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_color::{BLUE, RED};

    /// Every builder must produce unit normals, parallel to positions.
    #[test]
    fn builders_produce_unit_normals() {
        for m in [
            TriMesh::grid(4, 4),
            TriMesh::uv_sphere(8, 12),
            TriMesh::cylinder(12),
        ] {
            assert_eq!(m.normals.len(), m.positions.len());
            for n in &m.normals {
                assert!((n.length() - 1.0).abs() < 1e-5, "normal {n} not unit");
            }
        }
    }

    #[test]
    fn sphere_normals_are_normalized_positions() {
        let s = TriMesh::uv_sphere(10, 20);
        for (p, n) in s.positions.iter().zip(&s.normals) {
            assert!((p.normalize() - *n).length() < 1e-5);
        }
    }

    /// Closed meshes must wind CCW-outward, which makes the signed volume
    /// positive and equal to the analytic volume.
    #[test]
    fn closed_builders_have_positive_signed_volume() {
        let sphere = TriMesh::uv_sphere(48, 96);
        let expect = 4.0 / 3.0 * std::f32::consts::PI;
        assert!(sphere.signed_volume() > 0.0);
        assert!(
            (sphere.signed_volume() - expect).abs() < 0.01,
            "{}",
            sphere.signed_volume()
        );

        // Radius 1, height 1 ⇒ volume π r² h = π.
        let cyl = TriMesh::cylinder(96);
        let expect = std::f32::consts::PI;
        assert!(cyl.signed_volume() > 0.0);
        assert!(
            (cyl.signed_volume() - expect).abs() < 0.01,
            "{}",
            cyl.signed_volume()
        );
    }

    #[test]
    fn grid_dims_and_extent() {
        let g = TriMesh::grid(3, 5);
        assert_eq!(g.len(), 4 * 6);
        assert_eq!(g.indices.len(), 3 * 5 * 6);
        assert_eq!(g.n_triangles(), 30);
        let (min, max) = g.bounds().unwrap();
        assert!((min - Vec3::new(-0.5, -0.5, 0.0)).length() < 1e-6);
        assert!((max - Vec3::new(0.5, 0.5, 0.0)).length() < 1e-6);
        // A flat grid faces +Z.
        assert!(g.normals.iter().all(|n| (*n - Vec3::Z).length() < 1e-5));
    }

    #[test]
    fn grid_degenerate_resolution_is_clamped() {
        let g = TriMesh::grid(0, 0);
        assert_eq!(g.len(), 4);
        assert_eq!(g.n_triangles(), 2);
    }

    #[test]
    fn grid_winding_is_ccw_from_front() {
        let g = TriMesh::grid(2, 2);
        for t in g.indices.chunks_exact(3) {
            let (a, b, c) = (
                g.positions[t[0] as usize],
                g.positions[t[1] as usize],
                g.positions[t[2] as usize],
            );
            // CCW seen from +Z ⇒ the face normal agrees with the vertex normal.
            assert!((b - a).cross(c - a).dot(Vec3::Z) > 0.0);
        }
    }

    #[test]
    fn parametric_sphere_normals_are_radial() {
        let m = TriMesh::from_parametric(
            |phi, theta| {
                Vec3::new(
                    (phi.sin() * theta.cos()) as f32,
                    (phi.sin() * theta.sin()) as f32,
                    phi.cos() as f32,
                )
            },
            (0.1, std::f64::consts::PI - 0.1), // avoid the degenerate poles
            (0.0, std::f64::consts::TAU),
            (12, 24),
        );
        for (p, n) in m.positions.iter().zip(&m.normals) {
            // Radial up to orientation: the parameterization fixes the sign.
            assert!((p.normalize().dot(*n).abs() - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn parametric_matches_grid_for_the_plane() {
        let p = TriMesh::from_parametric(
            |u, v| Vec3::new(u as f32, v as f32, 0.0),
            (-0.5, 0.5),
            (-0.5, 0.5),
            (2, 2),
        );
        let g = TriMesh::grid(2, 2);
        assert_eq!(p.indices, g.indices);
        assert_eq!(p.positions, g.positions);
    }

    #[test]
    fn parametric_cells_split_vertices_per_cell() {
        let m = TriMesh::from_parametric_cells(
            |u, v| Vec3::new(u as f32, v as f32, 0.0),
            (0.0, 1.0),
            (0.0, 1.0),
            (2, 2),
        );
        assert_eq!(m.len(), 16);
        assert_eq!(m.n_triangles(), 8);
        // Cell (0, 0) occupies vertices 0..4 and is a unit-quarter square.
        assert!((m.positions[0] - Vec3::ZERO).length() < 1e-6);
        assert!((m.positions[2] - Vec3::new(0.5, 0.5, 0.0)).length() < 1e-6);
    }

    #[test]
    fn lerp_endpoints_are_exact() {
        let a = TriMesh::grid(2, 2);
        let mut b = a.clone();
        b.positions.iter_mut().for_each(|p| p.z += 2.0);
        assert_eq!(TriMesh::lerp(&a, &b, 0.0).unwrap(), a);
        assert_eq!(TriMesh::lerp(&a, &b, 1.0).unwrap(), b);
    }

    #[test]
    fn lerp_midpoint_blends_positions_normals_and_colors() {
        let mut a = TriMesh::grid(1, 1);
        a.set_colors(Some(vec![RED; 4])).unwrap();
        let mut b = a.clone();
        b.positions.iter_mut().for_each(|p| p.z += 4.0);
        b.normals.iter_mut().for_each(|n| *n = Vec3::X);
        b.set_colors(Some(vec![BLUE; 4])).unwrap();

        let mid = TriMesh::lerp(&a, &b, 0.5).unwrap();
        assert!((mid.positions[0].z - 2.0).abs() < 1e-6);
        // Normals blend then re-normalize: halfway between +Z and +X, unit long.
        assert!((mid.normals[0].length() - 1.0).abs() < 1e-6);
        assert!((mid.normals[0] - Vec3::new(1.0, 0.0, 1.0).normalize()).length() < 1e-6);
        assert_eq!(mid.colors.unwrap()[0], RED.interpolate(&BLUE, 0.5));
    }

    #[test]
    fn lerp_rejects_topology_mismatch() {
        let a = TriMesh::grid(1, 1);
        let b = TriMesh::grid(2, 2);
        assert!(TriMesh::lerp(&a, &b, 0.5).is_err());
        // Same vertex count, different indices.
        let mut c = a.clone();
        c.indices.reverse();
        assert!(TriMesh::lerp(&a, &c, 0.5).is_err());
    }

    #[test]
    fn set_colors_rejects_wrong_length() {
        let mut m = TriMesh::grid(1, 1);
        assert!(m.set_colors(Some(vec![RED; 3])).is_err());
        assert!(m.colors.is_none());
        assert!(m.set_colors(Some(vec![RED; 4])).is_ok());
        assert!(m.set_colors(None).is_ok());
    }

    #[test]
    fn transform_bakes_positions_and_normals() {
        let mut m = TriMesh::grid(1, 1);
        // A quarter turn about +X sends the +Z normal to -Y.
        m.transform(glam::Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2));
        assert!((m.normals[0] - Vec3::NEG_Y).length() < 1e-5);
    }
}
