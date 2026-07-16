//! [`HeightField`]: a flat grid displaced by height data in the vertex shader.

use std::sync::Arc;

use glam::{Mat4, Vec3};

use super::frame::LocalFrame;
use super::{mesh_style, MeshMaterial, MeshMobject, MeshPayload, TriMesh};
use crate::error::{CoreError, Result};
use crate::impl_mobject;
use crate::mobject::MobjectData;

/// The displacement data a [`HeightField`] hands the renderer.
///
/// The renderer uploads [`heights`](Self::heights) as an `nu × nv` `R32Float`
/// texture and displaces the grid's vertices along `+Z` in the **vertex
/// shader**, deriving normals from finite differences of neighboring texels. A
/// live field therefore costs one `nu × nv × 4 B` upload per frame and no CPU
/// re-meshing at all.
///
/// ```
/// use manim_core::mesh::HeightField;
/// let f = HeightField::from_fn(4, 4, (2.0, 2.0), |x, y| x + y);
/// let payload = f.height_payload();
/// assert_eq!((payload.nu, payload.nv), (4, 4));
/// assert_eq!(payload.heights.len(), 16);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct HeightPayload {
    /// Grid **vertices** along `x` (not cells), matching the height texture's
    /// width.
    pub nu: usize,
    /// Grid vertices along `y`, matching the height texture's height.
    pub nv: usize,
    /// Row-major `nu × nv` heights, indexed `j * nu + i`.
    pub heights: Arc<[f32]>,
}

/// An `nu × nv` grid over a scene-space extent, displaced by per-vertex heights.
///
/// The mobject supplies the **flat** grid geometry; displacement happens on the
/// GPU (see [`HeightPayload`]). That split is the whole point: a wave equation or
/// an ultrasound field can rewrite its heights every frame without any of the
/// CPU re-meshing a [`Surface3D`](super::Surface3D) would do.
///
/// `nu`/`nv` count **vertices**, so the grid has `(nu - 1) × (nv - 1)` cells and
/// [`heights`](Self::heights) has exactly `nu * nv` entries.
///
/// ```
/// use manim_core::mesh::HeightField;
/// use manim_core::mobject::MobjectExt;
/// // A 64 × 64 grid spanning 6 × 4 scene units.
/// let f = HeightField::from_fn(64, 64, (6.0, 4.0), |x, y| (x * x + y * y).sqrt().sin());
/// assert_eq!(f.mesh().len(), 64 * 64);
/// assert!((f.width() - 6.0).abs() < 1e-4);
/// // The mesh itself is flat; the renderer does the displacing.
/// assert!(f.mesh().positions.iter().all(|p| p.z == 0.0));
/// ```
#[derive(Clone)]
pub struct HeightField {
    data: MobjectData,
    mesh: Arc<TriMesh>,
    material: MeshMaterial,
    frame: LocalFrame,
    nu: usize,
    nv: usize,
    extent: (f32, f32),
    heights: Arc<[f32]>,
}
impl_mobject!(HeightField, mesh);

impl HeightField {
    /// A height field over an `nu × nv` vertex grid spanning `extent` =
    /// `(width, depth)` scene units, centered on the origin in the `z = 0` plane.
    ///
    /// `heights` must have exactly `nu * nv` entries, row-major (`j * nu + i`).
    /// `nu`/`nv` are clamped to at least 2 (a single-vertex grid has no cells).
    ///
    /// ```
    /// use manim_core::mesh::HeightField;
    /// assert!(HeightField::new(2, 2, (1.0, 1.0), vec![0.0; 4]).is_ok());
    /// assert!(HeightField::new(2, 2, (1.0, 1.0), vec![0.0; 3]).is_err());
    /// ```
    pub fn new(nu: usize, nv: usize, extent: (f32, f32), heights: Vec<f32>) -> Result<Self> {
        let (nu, nv) = (nu.max(2), nv.max(2));
        if heights.len() != nu * nv {
            return Err(CoreError::MeshTopology(format!(
                "height field is {nu} × {nv} = {} vertices but got {} heights",
                nu * nv,
                heights.len()
            )));
        }
        let mesh = Arc::new(flat_grid(nu, nv, extent));
        let frame = LocalFrame::of(&mesh);
        Ok(Self {
            data: MobjectData::new(frame.path_for(Mat4::IDENTITY), mesh_style()),
            mesh,
            material: MeshMaterial::default(),
            frame,
            nu,
            nv,
            extent,
            heights: heights.into(),
        })
    }

    /// A height field whose heights come from `f(x, y)` evaluated at each grid
    /// vertex's **scene-space** position.
    ///
    /// ```
    /// use manim_core::mesh::HeightField;
    /// // A plane tilted along x, sampled over x ∈ [-1, 1].
    /// let f = HeightField::from_fn(3, 3, (2.0, 2.0), |x, _y| x);
    /// assert_eq!(f.heights()[0], -1.0); // the (0, 0) corner
    /// assert_eq!(f.heights()[2], 1.0);
    /// ```
    pub fn from_fn(nu: usize, nv: usize, extent: (f32, f32), f: impl Fn(f32, f32) -> f32) -> Self {
        let (nu, nv) = (nu.max(2), nv.max(2));
        let heights = grid_coords(nu, nv, extent)
            .map(|(x, y, _, _)| f(x, y))
            .collect();
        // The dimensions agree by construction, so this cannot fail.
        Self::new(nu, nv, extent, heights).expect("from_fn builds a correctly-sized height grid")
    }

    /// The flat grid geometry.
    pub fn mesh(&self) -> &TriMesh {
        &self.mesh
    }

    /// The material.
    pub fn material(&self) -> &MeshMaterial {
        &self.material
    }

    /// The `(nu, nv)` **vertex** dimensions.
    pub fn dims(&self) -> (usize, usize) {
        (self.nu, self.nv)
    }

    /// The `(width, depth)` scene-space extent.
    pub fn extent(&self) -> (f32, f32) {
        self.extent
    }

    /// The row-major heights.
    pub fn heights(&self) -> &[f32] {
        &self.heights
    }

    /// The local → world model matrix.
    pub fn transform(&self) -> Mat4 {
        self.frame.transform_of(&self.data.path)
    }

    /// Replaces the model matrix outright.
    pub fn set_transform(&mut self, transform: Mat4) -> &mut Self {
        self.data.path = self.frame.path_for(transform);
        self.data.bump_generation();
        self
    }

    /// The displacement payload handed to the renderer.
    pub fn height_payload(&self) -> HeightPayload {
        HeightPayload {
            nu: self.nu,
            nv: self.nv,
            heights: Arc::clone(&self.heights),
        }
    }

    /// Replaces the heights, keeping the grid. Rejects a wrongly-sized buffer.
    pub fn set_heights(&mut self, heights: Vec<f32>) -> Result<&mut Self> {
        if heights.len() != self.nu * self.nv {
            return Err(CoreError::MeshTopology(format!(
                "height field is {} × {} = {} vertices but got {} heights",
                self.nu,
                self.nv,
                self.nu * self.nv,
                heights.len()
            )));
        }
        self.heights = heights.into();
        self.data.bump_generation();
        Ok(self)
    }

    /// Re-evaluates the heights from `f(x, y)` at each grid vertex — the
    /// per-frame entry point for an evolving field.
    ///
    /// ```
    /// use manim_core::mesh::HeightField;
    /// use manim_core::mobject::Mobject;
    /// let mut f = HeightField::from_fn(8, 8, (2.0, 2.0), |_, _| 0.0);
    /// let before = f.data().generation;
    /// f.update_heights(|x, y| x * y);
    /// // Only the heights moved on; the grid's own buffers stay cached.
    /// assert!(f.data().generation > before);
    /// ```
    pub fn update_heights(&mut self, f: impl Fn(f32, f32) -> f32) -> &mut Self {
        self.heights = grid_coords(self.nu, self.nv, self.extent)
            .map(|(x, y, _, _)| f(x, y))
            .collect();
        self.data.bump_generation();
        self
    }

    /// Replaces the material (appearance only; no generation bump).
    pub fn set_material(&mut self, material: MeshMaterial) -> &mut Self {
        self.material = material;
        self
    }

    /// Consuming builder for [`set_material`](Self::set_material).
    pub fn with_material(mut self, material: MeshMaterial) -> Self {
        self.set_material(material);
        self
    }
}

impl MeshMobject for HeightField {
    fn payload(&self) -> MeshPayload {
        MeshPayload {
            mesh: Arc::clone(&self.mesh),
            transform: self.transform(),
            material: self.material,
            instances: None,
            height: Some(self.height_payload()),
        }
    }
}

/// The `(x, y, u, v)` of each grid vertex in row-major order, where `(x, y)` is
/// scene-space and `(u, v)` are the normalized `[0, 1]` coordinates.
fn grid_coords(
    nu: usize,
    nv: usize,
    extent: (f32, f32),
) -> impl Iterator<Item = (f32, f32, f32, f32)> {
    (0..nv).flat_map(move |j| {
        (0..nu).map(move |i| {
            let u = i as f32 / (nu - 1) as f32;
            let v = j as f32 / (nv - 1) as f32;
            ((u - 0.5) * extent.0, (v - 0.5) * extent.1, u, v)
        })
    })
}

/// The flat `nu × nv`-vertex grid over `extent`, in row-major vertex order so it
/// matches the height buffer's indexing exactly.
fn flat_grid(nu: usize, nv: usize, extent: (f32, f32)) -> TriMesh {
    let mut mesh = TriMesh {
        positions: Vec::with_capacity(nu * nv),
        normals: vec![Vec3::Z; nu * nv],
        colors: None,
        uvs: Some(Vec::with_capacity(nu * nv)),
        indices: Vec::with_capacity((nu - 1) * (nv - 1) * 6),
    };
    for (x, y, u, v) in grid_coords(nu, nv, extent) {
        mesh.positions.push(Vec3::new(x, y, 0.0));
        if let Some(uvs) = &mut mesh.uvs {
            uvs.push(glam::Vec2::new(u, v));
        }
    }
    let at = |i: usize, j: usize| (j * nu + i) as u32;
    for j in 0..nv - 1 {
        for i in 0..nu - 1 {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            // CCW seen from +Z, matching TriMesh::grid.
            mesh.indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::{Mobject, MobjectExt};
    use crate::scene_state::SceneState;
    use manim_math::RIGHT;

    #[test]
    fn grid_dims_match_the_height_buffer() {
        let f = HeightField::from_fn(8, 5, (4.0, 2.0), |_, _| 0.0);
        assert_eq!(f.dims(), (8, 5));
        assert_eq!(f.mesh().len(), 40);
        assert_eq!(f.heights().len(), 40);
        assert_eq!(f.mesh().n_triangles(), 7 * 4 * 2);
    }

    #[test]
    fn the_supplied_mesh_is_flat_and_spans_the_extent() {
        let f = HeightField::from_fn(4, 4, (6.0, 2.0), |x, y| x + y);
        assert!(f.mesh().positions.iter().all(|p| p.z == 0.0));
        let (min, max) = f.mesh().bounds().unwrap();
        assert!((min - Vec3::new(-3.0, -1.0, 0.0)).length() < 1e-6);
        assert!((max - Vec3::new(3.0, 1.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn heights_are_row_major_and_sampled_in_scene_space() {
        let f = HeightField::from_fn(3, 3, (2.0, 2.0), |x, y| x * 10.0 + y);
        // Vertex (i, j) lives at index j * nu + i, at scene (x, y).
        assert_eq!(f.heights()[0], -11.0); // (-1, -1)
        assert_eq!(f.heights()[2], 9.0); // (1, -1)
        assert_eq!(f.heights()[6], -9.0); // (-1, 1)
        assert_eq!(f.heights()[8], 11.0); // (1, 1)
    }

    #[test]
    fn mesh_vertex_order_matches_the_height_index() {
        let f = HeightField::from_fn(4, 3, (2.0, 2.0), |_, _| 0.0);
        let (nu, _) = f.dims();
        for (idx, (x, y, _, _)) in grid_coords(4, 3, (2.0, 2.0)).enumerate() {
            let p = f.mesh().positions[idx];
            assert!((p.x - x).abs() < 1e-6 && (p.y - y).abs() < 1e-6);
            // Which is exactly `j * nu + i`.
            assert_eq!(idx, (idx / nu) * nu + idx % nu);
        }
    }

    #[test]
    fn winding_is_ccw_from_front() {
        let f = HeightField::from_fn(4, 4, (2.0, 2.0), |_, _| 0.0);
        for t in f.mesh().indices.chunks_exact(3) {
            let ps = f.mesh().positions.clone();
            let (a, b, c) = (ps[t[0] as usize], ps[t[1] as usize], ps[t[2] as usize]);
            assert!((b - a).cross(c - a).dot(Vec3::Z) > 0.0);
        }
    }

    #[test]
    fn new_rejects_a_mismatched_height_buffer() {
        assert!(HeightField::new(4, 4, (1.0, 1.0), vec![0.0; 15]).is_err());
        assert!(HeightField::new(4, 4, (1.0, 1.0), vec![0.0; 16]).is_ok());
    }

    #[test]
    fn set_heights_validates_and_bumps() {
        let mut f = HeightField::from_fn(3, 3, (2.0, 2.0), |_, _| 0.0);
        assert!(f.set_heights(vec![1.0; 8]).is_err());
        let before = f.data().generation;
        assert!(f.set_heights(vec![1.0; 9]).is_ok());
        assert!(f.data().generation > before);
        assert!(f.heights().iter().all(|h| *h == 1.0));
    }

    /// The per-frame path: heights change, the grid's geometry does not.
    #[test]
    fn update_heights_leaves_the_grid_shared() {
        let mut f = HeightField::from_fn(8, 8, (2.0, 2.0), |_, _| 0.0);
        let mesh = f.mesh_payload().unwrap().mesh;
        f.update_heights(|x, _| x);
        assert!(Arc::ptr_eq(&mesh, &f.mesh_payload().unwrap().mesh));
        assert!(f.heights().iter().any(|h| *h != 0.0));
    }

    #[test]
    fn transform_rides_on_the_mobject_path() {
        let mut f = HeightField::from_fn(4, 4, (2.0, 2.0), |_, _| 0.0);
        f.shift(3.0 * RIGHT);
        assert!((f.transform().w_axis.truncate() - 3.0 * Vec3::X).length() < 1e-5);
    }

    #[test]
    fn display_list_carries_the_height_payload() {
        let mut scene = SceneState::new();
        scene.add(HeightField::from_fn(4, 4, (2.0, 2.0), |x, y| x * y));
        let dl = scene.display_list();
        assert_eq!(dl.len(), 0);
        let item = &dl.meshes()[0];
        let height = item.height.as_ref().unwrap();
        assert_eq!((height.nu, height.nv), (4, 4));
        assert_eq!(height.heights.len(), 16);
        assert!(item.instances.is_none());
    }
}
