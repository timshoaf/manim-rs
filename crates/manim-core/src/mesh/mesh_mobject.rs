//! [`Mesh`]: the plain triangle-mesh mobject.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use manim_color::Color;

use super::frame::LocalFrame;
use super::{mesh_style, MeshMaterial, MeshMobject, MeshPayload, TriMesh};
use crate::impl_mobject;
use crate::mobject::MobjectData;

/// Latitude bands of [`Mesh::sphere`].
const SPHERE_RINGS: usize = 24;
/// Longitude divisions of [`Mesh::sphere`].
const SPHERE_SEGMENTS: usize = 48;
/// Divisions around [`Mesh::cylinder`].
const CYLINDER_SEGMENTS: usize = 32;

/// A depth-tested triangle mesh: shared [`TriMesh`] geometry, a
/// [`MeshMaterial`], and a model transform.
///
/// Unlike the [`threed`](crate::threed) mobjects, this is not a `VMobject` — it
/// carries no bezier outline and emits no
/// [`DrawItem`](crate::display::DrawItem). It renders through the mesh pass,
/// where it occludes and is occluded by other meshes per pixel.
///
/// The ordinary transform API applies — `shift`, `rotate`, `scale`, family ops on
/// the scene, updaters, and `.animate()` all work; see the [module docs](super)
/// for how, and for the two edges of that encoding. Styling
/// ([`set_fill`](crate::mobject::MobjectExt::set_fill) and friends) does *not* —
/// a mesh's appearance is its [`MeshMaterial`], set with
/// [`set_material`](Self::set_material).
///
/// ```
/// use manim_core::mesh::{Mesh, MeshMaterial};
/// use manim_core::mobject::{Buildable, MobjectExt};
/// use manim_core::scene_state::SceneState;
/// use manim_color::BLUE;
/// use manim_math::RIGHT;
///
/// let mut scene = SceneState::new();
/// let ball = scene.add(
///     Mesh::sphere()
///         .with_material(MeshMaterial::new(BLUE))
///         .with_shift(2.0 * RIGHT),
/// );
/// // The transform rides on the mobject path, so family ops move it.
/// scene.shift(ball, RIGHT);
/// assert!((scene.get(ball).transform().w_axis.truncate().x - 3.0).abs() < 1e-5);
/// ```
#[derive(Clone)]
pub struct Mesh {
    data: MobjectData,
    mesh: Arc<TriMesh>,
    material: MeshMaterial,
    frame: LocalFrame,
}
impl_mobject!(Mesh, mesh);

impl Mesh {
    /// A mesh from `geometry`, at the identity transform with the default
    /// material.
    ///
    /// ```
    /// use manim_core::mesh::{Mesh, TriMesh};
    /// use glam::Mat4;
    /// let m = Mesh::new(TriMesh::grid(4, 4));
    /// assert_eq!(m.transform(), Mat4::IDENTITY);
    /// assert_eq!(m.mesh().len(), 25);
    /// ```
    pub fn new(geometry: impl Into<Arc<TriMesh>>) -> Self {
        let mesh = geometry.into();
        let frame = LocalFrame::of(&mesh);
        Self {
            // A mesh draws through the mesh pass; the path exists only to carry
            // the model transform, and the style is never consulted.
            data: MobjectData::new(frame.path_for(Mat4::IDENTITY), mesh_style()),
            mesh,
            material: MeshMaterial::default(),
            frame,
        }
    }

    /// A unit-radius sphere centered at the origin.
    ///
    /// ```
    /// use manim_core::mesh::Mesh;
    /// use manim_core::mobject::MobjectExt;
    /// assert!((Mesh::sphere().width() - 2.0).abs() < 1e-4);
    /// ```
    pub fn sphere() -> Self {
        Self::new(TriMesh::uv_sphere(SPHERE_RINGS, SPHERE_SEGMENTS))
    }

    /// A capped unit-radius cylinder of height 1 along `+Z`, centered at the
    /// origin.
    pub fn cylinder() -> Self {
        Self::new(TriMesh::cylinder(CYLINDER_SEGMENTS))
    }

    /// A unit square in the `z = 0` plane, divided into `nu × nv` cells.
    pub fn grid(nu: usize, nv: usize) -> Self {
        Self::new(TriMesh::grid(nu, nv))
    }

    /// The shared geometry.
    pub fn mesh(&self) -> &TriMesh {
        &self.mesh
    }

    /// The geometry handle, for cheap sharing with another mobject.
    pub fn mesh_arc(&self) -> &Arc<TriMesh> {
        &self.mesh
    }

    /// The material.
    pub fn material(&self) -> &MeshMaterial {
        &self.material
    }

    /// The local → world model matrix, decoded from the mobject path.
    pub fn transform(&self) -> Mat4 {
        self.frame.transform_of(&self.data.path)
    }

    /// Replaces the model matrix outright, discarding any accumulated transform.
    ///
    /// ```
    /// use manim_core::mesh::Mesh;
    /// use glam::{Mat4, Vec3};
    /// let mut m = Mesh::sphere();
    /// m.set_transform(Mat4::from_translation(Vec3::Y));
    /// assert!((m.transform().w_axis.truncate() - Vec3::Y).length() < 1e-5);
    /// ```
    pub fn set_transform(&mut self, transform: Mat4) -> &mut Self {
        self.data.path = self.frame.path_for(transform);
        self.data.bump_generation();
        self
    }

    /// Replaces the geometry, keeping the current model transform, and bumps the
    /// generation so the renderer re-uploads.
    ///
    /// ```
    /// use manim_core::mesh::{Mesh, TriMesh};
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::RIGHT;
    /// let mut m = Mesh::sphere();
    /// m.shift(2.0 * RIGHT);
    /// let before = m.transform();
    /// m.set_mesh(TriMesh::cylinder(8));
    /// // New geometry, same placement.
    /// assert_eq!(m.mesh().n_triangles(), TriMesh::cylinder(8).n_triangles());
    /// assert!((m.transform().w_axis - before.w_axis).length() < 1e-5);
    /// ```
    pub fn set_mesh(&mut self, geometry: impl Into<Arc<TriMesh>>) -> &mut Self {
        let transform = self.transform();
        self.mesh = geometry.into();
        self.refit(transform);
        self
    }

    /// Mutates the geometry in place through copy-on-write: clones the vertex
    /// data only if it is shared, then bumps the generation.
    ///
    /// This is the updater-friendly entry point — the geometry of a mesh that no
    /// snapshot shares is edited without any allocation.
    ///
    /// ```
    /// use manim_core::mesh::Mesh;
    /// use manim_core::mobject::Mobject;
    /// let mut m = Mesh::grid(2, 2);
    /// let before = m.data().generation;
    /// m.update_mesh(|g| g.positions.iter_mut().for_each(|p| p.z += 1.0));
    /// assert!(m.data().generation > before);
    /// assert!(m.mesh().positions.iter().all(|p| (p.z - 1.0).abs() < 1e-6));
    /// ```
    pub fn update_mesh(&mut self, f: impl FnOnce(&mut TriMesh)) -> &mut Self {
        let transform = self.transform();
        f(Arc::make_mut(&mut self.mesh));
        self.refit(transform);
        self
    }

    /// Replaces the material. Appearance is not geometry, so this leaves the
    /// generation alone — the renderer keeps its cached buffers.
    ///
    /// ```
    /// use manim_core::mesh::{Mesh, MeshMaterial};
    /// use manim_core::mobject::Mobject;
    /// use manim_color::RED;
    /// let mut m = Mesh::sphere();
    /// let before = m.data().generation;
    /// m.set_material(MeshMaterial::new(RED));
    /// assert_eq!(m.material().base_color, RED);
    /// assert_eq!(m.data().generation, before);
    /// ```
    pub fn set_material(&mut self, material: MeshMaterial) -> &mut Self {
        self.material = material;
        self
    }

    /// Sets the material's base color.
    pub fn set_base_color(&mut self, color: Color) -> &mut Self {
        self.material.base_color = color;
        self
    }

    /// Sets the material's opacity.
    pub fn set_mesh_opacity(&mut self, opacity: f32) -> &mut Self {
        self.material.opacity = opacity;
        self
    }

    /// Consuming builder for [`set_material`](Self::set_material).
    pub fn with_material(mut self, material: MeshMaterial) -> Self {
        self.set_material(material);
        self
    }

    /// Consuming builder for [`set_transform`](Self::set_transform).
    pub fn with_transform(mut self, transform: Mat4) -> Self {
        self.set_transform(transform);
        self
    }

    /// Re-derives the local frame from the current geometry, re-encoding
    /// `transform` against it, and bumps the generation.
    fn refit(&mut self, transform: Mat4) {
        self.frame = LocalFrame::of(&self.mesh);
        self.data.path = self.frame.path_for(transform);
        self.data.bump_generation();
    }

    /// The mesh's world-space vertex positions, i.e. the geometry the renderer
    /// draws. Allocates; meant for tests and headless inspection.
    pub fn world_positions(&self) -> Vec<Vec3> {
        let m = self.transform();
        self.mesh
            .positions
            .iter()
            .map(|p| m.transform_point3(*p))
            .collect()
    }
}

impl MeshMobject for Mesh {
    fn payload(&self) -> MeshPayload {
        MeshPayload::new(Arc::clone(&self.mesh), self.transform(), self.material)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::{Buildable, Mobject, MobjectExt};
    use crate::scene_state::SceneState;
    use manim_color::RED;
    use manim_math::{RIGHT, UP};

    #[test]
    fn new_mesh_is_identity_and_default_material() {
        let m = Mesh::new(TriMesh::uv_sphere(8, 12));
        assert_eq!(m.transform(), Mat4::IDENTITY);
        assert_eq!(m.material(), &MeshMaterial::default());
    }

    #[test]
    fn shift_moves_the_model_transform() {
        let mut m = Mesh::sphere();
        m.shift(2.0 * RIGHT + UP);
        let t = m.transform();
        assert!((t.w_axis.truncate() - Vec3::new(2.0, 1.0, 0.0)).length() < 1e-5);
        // World geometry follows.
        assert!((m.get_center() - Vec3::new(2.0, 1.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn scale_and_rotate_reach_the_model_transform() {
        let mut m = Mesh::sphere();
        m.scale(3.0);
        assert!((m.transform().x_axis.truncate().length() - 3.0).abs() < 1e-5);

        let mut r = Mesh::sphere();
        r.rotate_about(std::f32::consts::FRAC_PI_2, Vec3::ZERO, Vec3::Z);
        // A quarter turn about +Z sends x̂ to ŷ.
        assert!((r.transform().transform_point3(Vec3::X) - Vec3::Y).length() < 1e-5);
    }

    #[test]
    fn move_to_uses_the_mesh_center() {
        let mut m = Mesh::sphere();
        m.move_to(3.0 * UP);
        assert!((m.get_center() - 3.0 * UP).length() < 1e-5);
        assert!((m.transform().w_axis.truncate() - 3.0 * UP).length() < 1e-5);
    }

    #[test]
    fn bounding_box_tracks_the_transform() {
        let m = Mesh::sphere().with_scale(2.0);
        // A unit sphere scaled ×2: width 4.
        assert!((m.width() - 4.0).abs() < 1e-4);
    }

    /// Copy-on-write: mutating a clone must not disturb the original, and must
    /// give the renderer a fresh cache key.
    #[test]
    fn cow_mutation_bumps_generation_and_spares_the_original() {
        let original = Mesh::grid(2, 2);
        let mut clone = original.clone();
        assert!(Arc::ptr_eq(original.mesh_arc(), clone.mesh_arc()));

        clone.update_mesh(|g| g.positions.iter_mut().for_each(|p| p.z += 5.0));

        assert!(!Arc::ptr_eq(original.mesh_arc(), clone.mesh_arc()));
        assert!(clone.data().generation > original.data().generation);
        assert!(original.mesh().positions.iter().all(|p| p.z == 0.0));
        assert!(clone
            .mesh()
            .positions
            .iter()
            .all(|p| (p.z - 5.0).abs() < 1e-6));
    }

    #[test]
    fn update_mesh_without_sharing_does_not_reallocate() {
        let mut m = Mesh::grid(2, 2);
        let ptr = Arc::as_ptr(m.mesh_arc());
        m.update_mesh(|g| g.positions[0].z = 1.0);
        assert_eq!(ptr, Arc::as_ptr(m.mesh_arc()));
    }

    #[test]
    fn set_mesh_preserves_the_transform() {
        let mut m = Mesh::sphere();
        m.shift(2.0 * RIGHT).scale(2.0);
        let before = m.transform();
        m.set_mesh(TriMesh::grid(4, 4));
        assert!((m.transform().w_axis - before.w_axis).length() < 1e-4);
        assert!((m.transform().x_axis - before.x_axis).length() < 1e-4);
    }

    /// A grid is flat in z; refitting against it must not corrupt the transform.
    #[test]
    fn set_mesh_to_flat_geometry_preserves_the_transform() {
        let mut m = Mesh::sphere();
        m.shift(3.0 * UP);
        m.set_mesh(TriMesh::grid(2, 2));
        assert!((m.transform().w_axis.truncate() - 3.0 * UP).length() < 1e-4);
    }

    #[test]
    fn material_changes_do_not_bump_generation() {
        let mut m = Mesh::sphere();
        let before = m.data().generation;
        m.set_base_color(RED).set_mesh_opacity(0.5);
        assert_eq!(m.data().generation, before);
        assert!(m.material().is_translucent());
    }

    #[test]
    fn world_positions_apply_the_transform() {
        let m = Mesh::grid(1, 1).with_shift(2.0 * RIGHT);
        assert!(m
            .world_positions()
            .iter()
            .all(|p| (p.x - 2.0).abs() <= 0.5001));
    }

    #[test]
    fn mesh_in_a_scene_emits_no_draw_items() {
        let mut scene = SceneState::new();
        scene.add(Mesh::sphere());
        let dl = scene.display_list();
        assert_eq!(dl.len(), 0);
        assert_eq!(dl.meshes().len(), 1);
    }
}
