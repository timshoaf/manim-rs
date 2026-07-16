//! [`Surface3D`]: a parametric surface meshed into a depth-tested [`TriMesh`].

use std::sync::Arc;

use glam::{Mat4, Vec3};
use manim_color::Color;

use super::frame::LocalFrame;
use super::{mesh_style, MeshMaterial, MeshMobject, MeshPayload, TriMesh};
use crate::impl_mobject;
use crate::mobject::MobjectData;

/// The default `(nu, nv)` cell resolution of a [`Surface3D`].
pub const DEFAULT_SURFACE3D_RESOLUTION: (usize, usize) = (32, 32);

/// The parametric function of a [`Surface3D`]: `(u, v) → position`.
///
/// Shared behind an [`Arc`] so cloning a surface — which snapshots do every
/// frame — never clones the closure.
pub type ParametricFn = Arc<dyn Fn(f64, f64) -> Vec3 + Send + Sync>;

/// A parametric surface `f(u, v)` over `u_range × v_range`, meshed at a given
/// resolution and re-meshed whenever any of those change.
///
/// This is the mesh-pipeline counterpart of [`threed::Surface`](crate::threed::Surface):
/// same CE-parity checkerboard, but real depth-tested geometry with analytic
/// normals instead of a group of projected, depth-sorted quad faces.
///
/// Cells carry their own vertices, so the two-tone checkerboard is crisp (CE's
/// `Surface` colors *faces*); normals are still the analytic per-corner ones, so
/// shading stays smooth across the seams. [`set_checkerboard`](Self::set_checkerboard)
/// with `None` turns it off — which only clears the vertex colors, leaving the
/// topology alone, so any two same-resolution surfaces always interpolate.
///
/// ```
/// use manim_core::mesh::Surface3D;
/// use glam::Vec3;
/// use std::f64::consts::{PI, TAU};
///
/// // A unit sphere.
/// let sphere = Surface3D::new(
///     |phi, theta| Vec3::new(
///         (phi.sin() * theta.cos()) as f32,
///         (phi.sin() * theta.sin()) as f32,
///         phi.cos() as f32,
///     ),
///     (0.0, PI),
///     (0.0, TAU),
/// )
/// .with_resolution(16, 32);
/// assert_eq!(sphere.resolution(), (16, 32));
/// // Cell-split vertices: nu × nv × 4.
/// assert_eq!(sphere.mesh().len(), 16 * 32 * 4);
/// ```
#[derive(Clone)]
pub struct Surface3D {
    data: MobjectData,
    mesh: Arc<TriMesh>,
    material: MeshMaterial,
    frame: LocalFrame,
    f: ParametricFn,
    u_range: (f64, f64),
    v_range: (f64, f64),
    resolution: (usize, usize),
    checkerboard: Option<[Color; 2]>,
}
impl_mobject!(Surface3D, mesh);

impl Surface3D {
    /// A surface from `f` over `u_range × v_range`, at
    /// [`DEFAULT_SURFACE3D_RESOLUTION`] with the default checkerboard.
    pub fn new(
        f: impl Fn(f64, f64) -> Vec3 + Send + Sync + 'static,
        u_range: (f64, f64),
        v_range: (f64, f64),
    ) -> Self {
        Self::from_arc(Arc::new(f), u_range, v_range)
    }

    /// A surface from an already-shared parametric function.
    ///
    /// Useful to hand the same `f` to several surfaces, and how
    /// [`MorphSurface`](super::MorphSurface) rebuilds a blended surface each
    /// frame without re-boxing the originals.
    pub fn from_arc(f: ParametricFn, u_range: (f64, f64), v_range: (f64, f64)) -> Self {
        let mut s = Self {
            data: MobjectData::new(Default::default(), mesh_style()),
            mesh: Arc::new(TriMesh::default()),
            material: MeshMaterial::default(),
            frame: LocalFrame::of_bounds(None),
            f,
            u_range,
            v_range,
            resolution: DEFAULT_SURFACE3D_RESOLUTION,
            checkerboard: Some(default_checkerboard()),
        };
        s.regenerate(Mat4::IDENTITY);
        s
    }

    /// The meshed geometry.
    pub fn mesh(&self) -> &TriMesh {
        &self.mesh
    }

    /// The geometry handle, for cheap sharing.
    pub fn mesh_arc(&self) -> &Arc<TriMesh> {
        &self.mesh
    }

    /// The material.
    pub fn material(&self) -> &MeshMaterial {
        &self.material
    }

    /// The parametric function.
    pub fn parametric(&self) -> &ParametricFn {
        &self.f
    }

    /// The `u` range.
    pub fn u_range(&self) -> (f64, f64) {
        self.u_range
    }

    /// The `v` range.
    pub fn v_range(&self) -> (f64, f64) {
        self.v_range
    }

    /// The `(nu, nv)` cell resolution.
    pub fn resolution(&self) -> (usize, usize) {
        self.resolution
    }

    /// The checkerboard colors, or `None` when disabled.
    pub fn checkerboard(&self) -> Option<[Color; 2]> {
        self.checkerboard
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

    /// Samples the surface at `(u, v)`.
    ///
    /// ```
    /// use manim_core::mesh::Surface3D;
    /// use glam::Vec3;
    /// let s = Surface3D::new(|u, v| Vec3::new(u as f32, v as f32, 0.0), (0.0, 1.0), (0.0, 1.0));
    /// assert_eq!(s.sample(0.25, 0.5), Vec3::new(0.25, 0.5, 0.0));
    /// ```
    pub fn sample(&self, u: f64, v: f64) -> Vec3 {
        (self.f)(u, v)
    }

    /// Replaces the parametric function and re-meshes.
    ///
    /// ```
    /// use manim_core::mesh::Surface3D;
    /// use manim_core::mobject::Mobject;
    /// use glam::Vec3;
    /// let mut s = Surface3D::new(|u, v| Vec3::new(u as f32, v as f32, 0.0), (0.0, 1.0), (0.0, 1.0))
    ///     .with_resolution(2, 2);
    /// let before = s.data().generation;
    /// s.set_parametric(|u, v| Vec3::new(u as f32, v as f32, 1.0));
    /// // Re-meshed, and the renderer's cache key moved on.
    /// assert!(s.mesh().positions.iter().all(|p| (p.z - 1.0).abs() < 1e-6));
    /// assert!(s.data().generation > before);
    /// ```
    pub fn set_parametric(
        &mut self,
        f: impl Fn(f64, f64) -> Vec3 + Send + Sync + 'static,
    ) -> &mut Self {
        self.set_parametric_arc(Arc::new(f))
    }

    /// Replaces the parametric function with a shared one and re-meshes.
    pub fn set_parametric_arc(&mut self, f: ParametricFn) -> &mut Self {
        self.f = f;
        self.regenerate_in_place()
    }

    /// Sets the `u` and `v` ranges and re-meshes.
    pub fn set_ranges(&mut self, u_range: (f64, f64), v_range: (f64, f64)) -> &mut Self {
        self.u_range = u_range;
        self.v_range = v_range;
        self.regenerate_in_place()
    }

    /// Sets the `(nu, nv)` cell resolution and re-meshes. Both are clamped to at
    /// least 1.
    pub fn set_resolution(&mut self, nu: usize, nv: usize) -> &mut Self {
        self.resolution = (nu.max(1), nv.max(1));
        self.regenerate_in_place()
    }

    /// Sets the two checkerboard colors, or clears them with `None` so the
    /// material color paints the whole surface.
    ///
    /// Only the vertex colors change; the topology does not.
    ///
    /// ```
    /// use manim_core::mesh::Surface3D;
    /// use glam::Vec3;
    /// let mut s = Surface3D::new(|u, v| Vec3::new(u as f32, v as f32, 0.0), (0.0, 1.0), (0.0, 1.0));
    /// assert!(s.mesh().colors.is_some());
    /// s.set_checkerboard(None);
    /// assert!(s.mesh().colors.is_none());
    /// ```
    pub fn set_checkerboard(&mut self, colors: Option<[Color; 2]>) -> &mut Self {
        self.checkerboard = colors;
        self.recolor();
        self
    }

    /// Replaces the material (appearance only; no generation bump).
    pub fn set_material(&mut self, material: MeshMaterial) -> &mut Self {
        self.material = material;
        self
    }

    /// Consuming builder for [`set_resolution`](Self::set_resolution).
    pub fn with_resolution(mut self, nu: usize, nv: usize) -> Self {
        self.set_resolution(nu, nv);
        self
    }

    /// Consuming builder for [`set_checkerboard`](Self::set_checkerboard).
    pub fn with_checkerboard(mut self, colors: Option<[Color; 2]>) -> Self {
        self.set_checkerboard(colors);
        self
    }

    /// Consuming builder for [`set_material`](Self::set_material).
    pub fn with_material(mut self, material: MeshMaterial) -> Self {
        self.set_material(material);
        self
    }

    /// Re-meshes, keeping the current model transform.
    fn regenerate_in_place(&mut self) -> &mut Self {
        let transform = self.transform();
        self.regenerate(transform);
        self
    }

    /// Rebuilds the mesh from the current parameters and re-encodes `transform`
    /// against the new local frame.
    fn regenerate(&mut self, transform: Mat4) {
        let mut mesh = TriMesh::from_parametric_cells(
            |u, v| (self.f)(u, v),
            self.u_range,
            self.v_range,
            self.resolution,
        );
        apply_checkerboard(&mut mesh, self.checkerboard, self.resolution);
        self.mesh = Arc::new(mesh);
        self.frame = LocalFrame::of(&self.mesh);
        self.data.path = self.frame.path_for(transform);
        self.data.bump_generation();
    }

    /// Repaints the existing mesh's vertex colors through copy-on-write.
    fn recolor(&mut self) {
        apply_checkerboard(
            Arc::make_mut(&mut self.mesh),
            self.checkerboard,
            self.resolution,
        );
        self.data.bump_generation();
    }
}

impl MeshMobject for Surface3D {
    fn payload(&self) -> MeshPayload {
        MeshPayload::new(Arc::clone(&self.mesh), self.transform(), self.material)
    }
}

/// The default checkerboard, matching manim CE's `Surface`.
///
/// ```
/// use manim_core::mesh::default_checkerboard;
/// assert_eq!(default_checkerboard(), [manim_color::BLUE_D, manim_color::BLUE_E]);
/// ```
pub fn default_checkerboard() -> [Color; 2] {
    [manim_color::BLUE_D, manim_color::BLUE_E]
}

/// Paints a cell-split mesh's vertices with a `(i + j) % 2` checkerboard, or
/// clears them when `colors` is `None`.
///
/// Assumes the vertex layout of
/// [`TriMesh::from_parametric_cells`]: cell `(i, j)` owns vertices
/// `4 * (i * nv + j) ..`.
fn apply_checkerboard(mesh: &mut TriMesh, colors: Option<[Color; 2]>, res: (usize, usize)) {
    let Some(colors) = colors else {
        mesh.colors = None;
        return;
    };
    let (nu, nv) = (res.0.max(1), res.1.max(1));
    let mut out = Vec::with_capacity(mesh.positions.len());
    for i in 0..nu {
        for j in 0..nv {
            out.extend_from_slice(&[colors[(i + j) % 2]; 4]);
        }
    }
    // Defensive: a mesh that is not the expected cell-split grid keeps no colors
    // rather than a mismatched buffer the renderer would read out of bounds.
    mesh.colors = (out.len() == mesh.positions.len()).then_some(out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::{Mobject, MobjectExt};
    use crate::scene_state::SceneState;
    use manim_color::{BLUE_D, BLUE_E, GREEN, RED};
    use manim_math::RIGHT;
    use std::f64::consts::{PI, TAU};

    fn plane() -> Surface3D {
        Surface3D::new(
            |u, v| Vec3::new(u as f32, v as f32, 0.0),
            (0.0, 1.0),
            (0.0, 1.0),
        )
        .with_resolution(2, 2)
    }

    fn sphere() -> Surface3D {
        Surface3D::new(
            |phi, theta| {
                Vec3::new(
                    (phi.sin() * theta.cos()) as f32,
                    (phi.sin() * theta.sin()) as f32,
                    phi.cos() as f32,
                )
            },
            (0.0, PI),
            (0.0, TAU),
        )
        .with_resolution(8, 16)
    }

    #[test]
    fn meshes_at_the_requested_resolution() {
        let s = plane();
        assert_eq!(s.resolution(), (2, 2));
        assert_eq!(s.mesh().len(), 2 * 2 * 4);
        assert_eq!(s.mesh().n_triangles(), 2 * 2 * 2);
    }

    #[test]
    fn sphere_vertices_are_on_the_unit_sphere() {
        assert!(sphere()
            .mesh()
            .positions
            .iter()
            .all(|p| (p.length() - 1.0).abs() < 1e-5));
    }

    #[test]
    fn checkerboard_alternates_by_cell_parity() {
        let s = plane();
        let colors = s.mesh().colors.as_ref().unwrap();
        assert_eq!(colors.len(), s.mesh().len());
        // Cells (0,0) and (1,1) share a parity; (0,1) and (1,0) take the other.
        let cell = |i: usize, j: usize| colors[4 * (i * 2 + j)];
        assert_eq!(cell(0, 0), BLUE_D);
        assert_eq!(cell(0, 1), BLUE_E);
        assert_eq!(cell(1, 0), BLUE_E);
        assert_eq!(cell(1, 1), BLUE_D);
        // A cell is one flat color across all four of its vertices.
        assert!(colors[0..4].iter().all(|c| *c == BLUE_D));
    }

    #[test]
    fn custom_checkerboard_is_used() {
        let s = plane().with_checkerboard(Some([RED, GREEN]));
        let colors = s.mesh().colors.as_ref().unwrap();
        assert_eq!(colors[0], RED);
        assert_eq!(colors[4], GREEN);
    }

    #[test]
    fn checkerboard_can_be_disabled_without_changing_topology() {
        let mut s = plane();
        let before = s.mesh().positions.clone();
        s.set_checkerboard(None);
        assert!(s.mesh().colors.is_none());
        assert_eq!(s.mesh().positions, before);
    }

    #[test]
    fn setters_regenerate_the_mesh_and_bump_generation() {
        let mut s = plane();

        let gen0 = s.data().generation;
        s.set_resolution(4, 3);
        assert_eq!(s.mesh().len(), 4 * 3 * 4);
        assert!(s.data().generation > gen0);

        let gen1 = s.data().generation;
        s.set_ranges((0.0, 2.0), (0.0, 2.0));
        let (min, max) = s.mesh().bounds().unwrap();
        assert!((min - Vec3::ZERO).length() < 1e-6);
        assert!((max - Vec3::new(2.0, 2.0, 0.0)).length() < 1e-6);
        assert!(s.data().generation > gen1);

        let gen2 = s.data().generation;
        s.set_parametric(|u, v| Vec3::new(u as f32, v as f32, 3.0));
        assert!(s.mesh().positions.iter().all(|p| (p.z - 3.0).abs() < 1e-6));
        assert!(s.data().generation > gen2);
    }

    #[test]
    fn regeneration_preserves_the_transform() {
        let mut s = plane();
        s.shift(2.0 * RIGHT);
        let before = s.transform();
        s.set_resolution(5, 5);
        assert!((s.transform().w_axis - before.w_axis).length() < 1e-4);
    }

    #[test]
    fn regeneration_keeps_the_checkerboard() {
        let mut s = plane();
        s.set_resolution(3, 3);
        let colors = s.mesh().colors.as_ref().unwrap();
        assert_eq!(colors.len(), 3 * 3 * 4);
        assert_eq!(colors[0], BLUE_D);
    }

    #[test]
    fn surface_in_a_scene_emits_one_mesh_item() {
        let mut scene = SceneState::new();
        scene.add(plane());
        let dl = scene.display_list();
        assert_eq!(dl.len(), 0);
        assert_eq!(dl.meshes().len(), 1);
        assert_eq!(dl.meshes()[0].mesh.len(), 2 * 2 * 4);
    }
}
