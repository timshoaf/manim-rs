//! Depth-tested triangle meshes: the second, parallel render path that layers
//! *under* the 2D vector pipeline. See `docs/design/12-mesh-pipeline.md`.
//!
//! Where the [`threed`](crate::threed) module *projects* 3D bezier paths to 2D
//! and depth-**sorts** whole faces per frame, the mobjects here carry real
//! indexed geometry ([`TriMesh`]) that a renderer depth-**tests** and shades per
//! pixel. Both paths are supported; neither replaces the other.
//!
//! | mobject | for |
//! | --- | --- |
//! | [`Mesh`] | one static or updater-driven [`TriMesh`] |
//! | [`Surface3D`] | a parametric `(u, v) → Vec3` surface, re-meshed on change |
//! | [`InstancedMesh`] | one base mesh drawn at many transforms (atoms, bonds) |
//! | [`HeightField`] | a grid displaced by height data in the vertex shader |
//!
//! # How it reaches the renderer
//!
//! A mesh mobject reports a [`MeshPayload`] from
//! [`Mobject::mesh_payload`](crate::mobject::Mobject::mesh_payload); the scene
//! turns that into a [`MeshItem`](crate::display::MeshItem) on the display list's
//! separate [`meshes`](crate::display::DisplayList::meshes) channel. Such a
//! mobject never emits a [`DrawItem`](crate::display::DrawItem), so a scene
//! without meshes is byte-identical to before.
//!
//! ```
//! use manim_core::mesh::Mesh;
//! use manim_core::scene_state::SceneState;
//! let mut scene = SceneState::new();
//! scene.add(Mesh::sphere());
//! let dl = scene.display_list();
//! assert_eq!(dl.meshes().len(), 1);
//! assert_eq!(dl.len(), 0); // no 2D draw items
//! ```
//!
//! # Sharing, mutation, and caching
//!
//! Geometry lives behind an [`Arc`], so timeline snapshots clone a pointer, not
//! a vertex buffer. Mutation is copy-on-write and bumps the mobject's generation
//! — the same `(source, generation)` key the tessellation cache uses, reused by
//! the renderer as its GPU buffer cache key.
//!
//! ```
//! use manim_core::mesh::{Mesh, TriMesh};
//! use manim_core::mobject::Mobject;
//! let mesh = Mesh::sphere();
//! let mut clone = mesh.clone();
//! clone.set_mesh(TriMesh::cylinder(8)); // copy-on-write
//! assert!(clone.data().generation > mesh.data().generation);
//! assert_eq!(mesh.mesh().n_triangles(), Mesh::sphere().mesh().n_triangles());
//! ```
//!
//! # Transforms
//!
//! Mesh mobjects reuse the ordinary transform API —
//! [`shift`](crate::mobject::MobjectExt::shift),
//! [`rotate`](crate::mobject::MobjectExt::rotate),
//! [`scale`](crate::mobject::MobjectExt::scale), the
//! [`SceneState`](crate::scene_state::SceneState) family ops, updaters,
//! `save_state`/[`Restore`](crate::animations::Restore), and `.animate()` — none
//! of which know meshes exist.
//!
//! That works because a mesh mobject's model matrix is *encoded in its
//! [`MobjectData::path`](crate::mobject::MobjectData::path)*: six anchors, the
//! ends of a cross through the mesh's local bounding box, carried in world space.
//! Every one of those features is a [`Path::apply`](manim_math::path::Path::apply)
//! mutation, and because the encoding is affine, an affine mutation of the
//! anchors is exactly the same affine change of the model matrix. Read the
//! matrix back with e.g. [`Mesh::transform`].
//!
//! Two consequences worth knowing:
//!
//! - [`apply_function`](crate::mobject::MobjectExt::apply_function) with a
//!   *non-affine* function reaches a mesh only through the affine part it induces
//!   on those six anchors. Deform the geometry itself with
//!   [`Mesh::update_mesh`] instead.
//! - A mesh's bounding box is the box of that cross — the octahedron inscribed in
//!   the local box. It is exact under an axis-aligned transform and can
//!   under-report a rotated one, but its *center* is always exactly the
//!   transformed mesh center, so [`move_to`](crate::mobject::MobjectExt::move_to),
//!   [`next_to`](crate::mobject::MobjectExt::next_to) and friends place meshes
//!   correctly.
//!
//! Styling (`set_fill`, `set_stroke`, …) does **not** apply: a mesh's appearance
//! is its [`MeshMaterial`].

mod anim;
mod frame;
mod height_field;
mod instanced;
mod mesh_mobject;
mod surface3d;
mod trimesh;

pub use anim::{MorphMesh, MorphSurface};
pub use height_field::{HeightField, HeightPayload};
pub use instanced::{Instance, InstancedMesh, DEFAULT_ATOM_RINGS, DEFAULT_BOND_SEGMENTS};
pub use mesh_mobject::Mesh;
pub use surface3d::{default_checkerboard, ParametricFn, Surface3D, DEFAULT_SURFACE3D_RESOLUTION};
pub use trimesh::TriMesh;

use std::sync::Arc;

use glam::Mat4;
use manim_color::{Color, WHITE};

/// How a renderer shades a mesh's faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shading {
    /// One normal per face: faceted, each triangle flat.
    Flat,
    /// Interpolated vertex normals: smooth across faces. The default.
    #[default]
    Smooth,
}

/// The surface appearance of a mesh mobject: a Blinn-Phong material.
///
/// The renderer shades in linear space as
/// `(ambient + diffuse·N·L)·albedo + specular·(N·H)^shininess`, where `albedo` is
/// the per-vertex color (when the [`TriMesh`] has one) times
/// [`base_color`](Self::base_color).
///
/// ```
/// use manim_core::mesh::{MeshMaterial, Shading};
/// use manim_color::BLUE;
/// let m = MeshMaterial::new(BLUE).with_opacity(0.5).with_shading(Shading::Flat);
/// assert_eq!(m.base_color, BLUE);
/// assert!(m.is_translucent());
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshMaterial {
    /// The base surface color, multiplied by any per-vertex color.
    pub base_color: Color,
    /// Overall opacity in `[0, 1]`; below 1 puts the mesh in the translucent
    /// queue.
    pub opacity: f32,
    /// Ambient (unlit) fraction of the albedo.
    pub ambient: f32,
    /// Lambertian diffuse coefficient.
    pub diffuse: f32,
    /// Specular highlight strength.
    pub specular: f32,
    /// Specular exponent; higher is a tighter highlight.
    pub shininess: f32,
    /// Faceted or smooth normals.
    pub shading: Shading,
}

impl Default for MeshMaterial {
    /// A white, opaque, smooth surface with a modest highlight.
    ///
    /// ```
    /// use manim_core::mesh::{MeshMaterial, Shading};
    /// let m = MeshMaterial::default();
    /// assert_eq!(m.opacity, 1.0);
    /// assert_eq!(m.shading, Shading::Smooth);
    /// assert!(!m.is_translucent());
    /// ```
    fn default() -> Self {
        Self {
            base_color: WHITE,
            opacity: 1.0,
            ambient: 0.3,
            diffuse: 0.7,
            specular: 0.3,
            shininess: 32.0,
            shading: Shading::Smooth,
        }
    }
}

impl MeshMaterial {
    /// The default material in `color`.
    pub fn new(color: Color) -> Self {
        Self {
            base_color: color,
            ..Self::default()
        }
    }

    /// Sets the opacity (builder).
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// Sets the shading model (builder).
    pub fn with_shading(mut self, shading: Shading) -> Self {
        self.shading = shading;
        self
    }

    /// Sets the Blinn-Phong lighting coefficients (builder).
    pub fn with_lighting(mut self, ambient: f32, diffuse: f32, specular: f32) -> Self {
        self.ambient = ambient;
        self.diffuse = diffuse;
        self.specular = specular;
        self
    }

    /// Sets the specular exponent (builder).
    pub fn with_shininess(mut self, shininess: f32) -> Self {
        self.shininess = shininess;
        self
    }

    /// Whether this material belongs in the renderer's translucent queue — i.e.
    /// its opacity, or its base color's alpha, is below 1.
    ///
    /// Per-vertex alpha can make an otherwise-opaque material translucent too;
    /// the renderer checks the mesh for that (see
    /// [`MeshItem::is_translucent`](crate::display::MeshItem::is_translucent)).
    pub fn is_translucent(&self) -> bool {
        self.opacity < 1.0 || self.base_color.opacity() < 1.0
    }
}

/// What a mesh mobject hands the scene when a display list is built.
///
/// This is the mobject-side half of [`MeshItem`](crate::display::MeshItem):
/// [`SceneState::display_list`](crate::scene_state::SceneState::display_list)
/// stamps on the `source` and `generation` to complete it. Implement
/// [`Mobject::mesh_payload`](crate::mobject::Mobject::mesh_payload) to return one.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshPayload {
    /// The shared geometry, in mobject-local space.
    pub mesh: Arc<TriMesh>,
    /// The local → world model matrix.
    pub transform: Mat4,
    /// The resolved surface appearance.
    pub material: MeshMaterial,
    /// Per-instance transforms/colors for an [`InstancedMesh`], else `None`.
    pub instances: Option<Arc<[Instance]>>,
    /// Vertex-shader displacement data for a [`HeightField`], else `None`.
    pub height: Option<HeightPayload>,
}

impl MeshPayload {
    /// A payload for a plain mesh: no instancing, no displacement.
    pub fn new(mesh: Arc<TriMesh>, transform: Mat4, material: MeshMaterial) -> Self {
        Self {
            mesh,
            transform,
            material,
            instances: None,
            height: None,
        }
    }
}

/// A mobject that renders through the mesh pass.
///
/// Implemented by [`Mesh`], [`Surface3D`], [`InstancedMesh`], and
/// [`HeightField`]. Pairing it with `impl_mobject!($t, mesh)` is what wires
/// [`payload`](Self::payload) up to
/// [`Mobject::mesh_payload`](crate::mobject::Mobject::mesh_payload), and so onto
/// the display list's mesh channel.
pub trait MeshMobject: crate::mobject::Mobject {
    /// This mobject's current geometry, transform, and appearance.
    fn payload(&self) -> MeshPayload;
}

/// The style a mesh mobject carries: fully invisible.
///
/// Mesh mobjects hold a [`MobjectData::path`](crate::mobject::MobjectData::path)
/// only to encode their model transform (see [`frame`]), and are skipped by the
/// 2D draw-item pass regardless. Giving them an invisible style keeps them
/// harmless if that path is ever inspected by generic code.
pub(crate) fn mesh_style() -> crate::style::Style {
    crate::style::Style {
        fill_color: None,
        stroke_color: None,
        ..crate::style::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_color::{BLUE, RED};

    #[test]
    fn default_material_is_opaque_white_and_smooth() {
        let m = MeshMaterial::default();
        assert_eq!(m.base_color, WHITE);
        assert_eq!(m.shading, Shading::Smooth);
        assert!(!m.is_translucent());
    }

    #[test]
    fn translucency_comes_from_opacity_or_base_alpha() {
        assert!(MeshMaterial::new(RED).with_opacity(0.5).is_translucent());
        assert!(MeshMaterial::new(BLUE.with_opacity(0.25)).is_translucent());
        assert!(!MeshMaterial::new(RED).is_translucent());
    }

    /// `SceneState`'s family ops are mesh-aware for free, because a mesh's model
    /// matrix *is* its path (see [`frame`]). These lock that in: the ops are the
    /// ones most likely to be "optimized" into a path-only fast path someday.
    mod scene_family_ops_reach_meshes {
        use crate::mesh::Mesh;
        use crate::mobject::MobjectExt;
        use crate::scene_state::SceneState;
        use glam::{Mat4, Vec3};

        fn assert_close(a: Mat4, b: Mat4) {
            for i in 0..4 {
                assert!(
                    (a.col(i) - b.col(i)).length() < 1e-4,
                    "column {i}: {:?} vs {:?}",
                    a.col(i),
                    b.col(i)
                );
            }
        }

        #[test]
        fn shift_translates_the_model_matrix() {
            let mut scene = SceneState::new();
            let m = scene.add(Mesh::sphere());
            scene.shift(m, Vec3::new(2.0, -1.0, 0.5));
            assert_close(
                scene[m].transform(),
                Mat4::from_translation(Vec3::new(2.0, -1.0, 0.5)),
            );
        }

        /// A shifted mesh's bounding box must follow it — meshes contribute to
        /// the family box through the same six anchors.
        #[test]
        fn shift_moves_the_bounding_box() {
            let mut scene = SceneState::new();
            let m = scene.add(Mesh::sphere());
            scene.shift(m, Vec3::X * 3.0);
            let center = scene.family_bounding_box(m).center();
            assert!((center - Vec3::X * 3.0).length() < 1e-4, "{center:?}");
        }

        /// `scale`/`rotate` are about the family center, so an off-origin mesh
        /// must come back conjugated by that point, not applied at the origin.
        #[test]
        fn scale_and_rotate_conjugate_by_the_center() {
            let mut scene = SceneState::new();
            let m = scene.add(Mesh::sphere());
            scene.shift(m, Vec3::X * 2.0);
            scene.scale(m, 3.0);
            // Scaling about its own center leaves the center put, scales the basis.
            assert_close(
                scene[m].transform(),
                Mat4::from_translation(Vec3::X * 2.0) * Mat4::from_scale(Vec3::splat(3.0)),
            );

            let angle = std::f32::consts::FRAC_PI_2;
            scene.rotate(m, angle);
            assert_close(
                scene[m].transform(),
                Mat4::from_translation(Vec3::X * 2.0)
                    * Mat4::from_rotation_z(angle)
                    * Mat4::from_scale(Vec3::splat(3.0)),
            );
        }

        #[test]
        fn move_to_places_the_mesh_center() {
            let mut scene = SceneState::new();
            let m = scene.add(Mesh::sphere());
            scene.move_to(m, Vec3::new(-1.0, 4.0, 0.0));
            let center = scene.family_bounding_box(m).center();
            assert!(
                (center - Vec3::new(-1.0, 4.0, 0.0)).length() < 1e-4,
                "{center:?}"
            );
        }

        /// The mixed case: a path mobject and a mesh under one group must move
        /// together, by the same delta, from one family op.
        #[test]
        fn mixed_family_moves_path_and_mesh_consistently() {
            use crate::geometry::Circle;
            use crate::prelude::VGroup;

            let mut scene = SceneState::new();
            let g = scene.add(VGroup::new());
            let circle = scene.add(Circle::new());
            let mesh = scene.add(Mesh::sphere());
            scene.add_child(g.erase(), circle.erase());
            scene.add_child(g.erase(), mesh.erase());

            let delta = Vec3::new(2.0, 3.0, 0.0);
            scene.shift(g.erase(), delta);

            assert!((scene.get(circle).get_center() - delta).length() < 1e-4);
            assert_close(scene[mesh].transform(), Mat4::from_translation(delta));
        }
    }

    #[test]
    fn material_builders_chain() {
        let m = MeshMaterial::new(RED)
            .with_lighting(0.1, 0.9, 0.5)
            .with_shininess(8.0)
            .with_shading(Shading::Flat);
        assert_eq!(
            (m.ambient, m.diffuse, m.specular, m.shininess),
            (0.1, 0.9, 0.5, 8.0)
        );
        assert_eq!(m.shading, Shading::Flat);
    }
}
