//! [`InstancedMesh`]: one base [`TriMesh`] drawn at many transforms.

use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use manim_color::{Color, WHITE};

use super::frame::LocalFrame;
use super::{mesh_style, MeshMaterial, MeshMobject, MeshPayload, TriMesh};
use crate::impl_mobject;
use crate::mobject::MobjectData;

/// Latitude bands of the sphere [`InstancedMesh::spheres`] instances.
///
/// Deliberately coarse: an instanced cloud spends its budget on instance count,
/// not on per-atom tessellation.
pub const DEFAULT_ATOM_RINGS: usize = 12;
/// Divisions around the cylinder [`InstancedMesh::cylinders`] instances.
pub const DEFAULT_BOND_SEGMENTS: usize = 12;

/// One placement of an [`InstancedMesh`]'s base geometry.
///
/// ```
/// use manim_core::mesh::Instance;
/// use glam::{Mat4, Vec3};
/// use manim_color::RED;
/// let i = Instance::new(Mat4::from_translation(Vec3::X), RED);
/// assert_eq!(i.color, RED);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Instance {
    /// This instance's local → mobject-space matrix, applied before the
    /// mobject's own model transform.
    pub transform: Mat4,
    /// This instance's tint, multiplied into the material color.
    pub color: Color,
}

impl Instance {
    /// An instance at `transform`, tinted `color`.
    pub fn new(transform: Mat4, color: Color) -> Self {
        Self { transform, color }
    }
}

/// One base [`TriMesh`] drawn at many [`Instance`] transforms in a single draw
/// call — the path for 10k-atom molecules, particle clouds, and lattices.
///
/// Whole-cloud motion goes through the mobject transform (`shift`, `rotate`, …)
/// and never touches the instance buffer; per-instance edits go through
/// [`update_instances`](Self::update_instances), which bumps the generation so
/// the renderer re-uploads the instance buffer alone, leaving the base mesh's
/// buffers cached.
///
/// ```
/// use manim_core::mesh::InstancedMesh;
/// use glam::Vec3;
/// // A three-atom cloud: one draw call.
/// let atoms = InstancedMesh::spheres(&[Vec3::ZERO, Vec3::X, Vec3::Y], 0.3);
/// assert_eq!(atoms.instances().len(), 3);
/// ```
#[derive(Clone)]
pub struct InstancedMesh {
    data: MobjectData,
    mesh: Arc<TriMesh>,
    material: MeshMaterial,
    frame: LocalFrame,
    instances: Arc<[Instance]>,
}
impl_mobject!(InstancedMesh, mesh);

impl InstancedMesh {
    /// An instanced mesh from `geometry` and `instances`.
    pub fn new(geometry: impl Into<Arc<TriMesh>>, instances: Vec<Instance>) -> Self {
        let mesh = geometry.into();
        let instances: Arc<[Instance]> = instances.into();
        let frame = LocalFrame::of_bounds(cloud_bounds(&mesh, &instances));
        Self {
            data: MobjectData::new(frame.path_for(Mat4::IDENTITY), mesh_style()),
            mesh,
            material: MeshMaterial::default(),
            frame,
            instances,
        }
    }

    /// A cloud of spheres of `radius` at `centers`, all tinted white.
    ///
    /// ```
    /// use manim_core::mesh::InstancedMesh;
    /// use glam::Vec3;
    /// use manim_core::mobject::MobjectExt;
    /// let m = InstancedMesh::spheres(&[Vec3::ZERO, 4.0 * Vec3::X], 1.0);
    /// // The cloud's bounds span both atoms plus their radii: x ∈ [-1, 5].
    /// assert!((m.width() - 6.0).abs() < 0.1);
    /// ```
    pub fn spheres(centers: &[Vec3], radius: f32) -> Self {
        let instances = centers
            .iter()
            .map(|c| {
                Instance::new(
                    Mat4::from_scale_rotation_translation(Vec3::splat(radius), Quat::IDENTITY, *c),
                    WHITE,
                )
            })
            .collect();
        Self::new(
            TriMesh::uv_sphere(DEFAULT_ATOM_RINGS, DEFAULT_ATOM_RINGS * 2),
            instances,
        )
    }

    /// A set of cylinders of `radius`, each spanning one `(start, end)` pair.
    ///
    /// The unit cylinder runs along `+Z` from `z = -0.5` to `z = 0.5`, so each
    /// instance transform maps those two ends exactly onto its endpoints.
    /// Zero-length pairs are skipped — they have no orientation.
    ///
    /// ```
    /// use manim_core::mesh::InstancedMesh;
    /// use glam::Vec3;
    /// let bonds = InstancedMesh::cylinders(&[(Vec3::ZERO, 2.0 * Vec3::Y)], 0.1);
    /// let m = bonds.instances()[0].transform;
    /// // The cylinder's ends land on the endpoints.
    /// assert!((m.transform_point3(-0.5 * Vec3::Z) - Vec3::ZERO).length() < 1e-5);
    /// assert!((m.transform_point3(0.5 * Vec3::Z) - 2.0 * Vec3::Y).length() < 1e-5);
    /// ```
    pub fn cylinders(endpoints: &[(Vec3, Vec3)], radius: f32) -> Self {
        let instances = endpoints
            .iter()
            .filter_map(|(a, b)| {
                let axis = *b - *a;
                let length = axis.length();
                if length <= 1e-9 {
                    return None;
                }
                Some(Instance::new(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(radius, radius, length),
                        Quat::from_rotation_arc(Vec3::Z, axis / length),
                        (*a + *b) * 0.5,
                    ),
                    WHITE,
                ))
            })
            .collect();
        Self::new(TriMesh::cylinder(DEFAULT_BOND_SEGMENTS), instances)
    }

    /// The base geometry.
    pub fn mesh(&self) -> &TriMesh {
        &self.mesh
    }

    /// The instances.
    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    /// The material.
    pub fn material(&self) -> &MeshMaterial {
        &self.material
    }

    /// The local → world model matrix of the cloud as a whole.
    pub fn transform(&self) -> Mat4 {
        self.frame.transform_of(&self.data.path)
    }

    /// Replaces the model matrix outright.
    pub fn set_transform(&mut self, transform: Mat4) -> &mut Self {
        self.data.path = self.frame.path_for(transform);
        self.data.bump_generation();
        self
    }

    /// Replaces every instance and bumps the generation.
    pub fn set_instances(&mut self, instances: Vec<Instance>) -> &mut Self {
        let transform = self.transform();
        self.instances = instances.into();
        self.refit(transform);
        self
    }

    /// Mutates the instances in place, then bumps the generation.
    ///
    /// The renderer re-uploads only the instance buffer; the base mesh's vertex
    /// and index buffers stay cached.
    ///
    /// ```
    /// use manim_core::mesh::InstancedMesh;
    /// use manim_core::mobject::Mobject;
    /// use glam::Vec3;
    /// use manim_color::RED;
    /// let mut m = InstancedMesh::spheres(&[Vec3::ZERO], 1.0);
    /// let before = m.data().generation;
    /// m.update_instances(|xs| xs[0].color = RED);
    /// assert_eq!(m.instances()[0].color, RED);
    /// assert!(m.data().generation > before);
    /// ```
    pub fn update_instances(&mut self, f: impl FnOnce(&mut Vec<Instance>)) -> &mut Self {
        let mut instances = self.instances.to_vec();
        f(&mut instances);
        self.set_instances(instances)
    }

    /// Replaces the base geometry, keeping the instances and transform.
    pub fn set_mesh(&mut self, geometry: impl Into<Arc<TriMesh>>) -> &mut Self {
        let transform = self.transform();
        self.mesh = geometry.into();
        self.refit(transform);
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

    /// Re-derives the frame from the current cloud extent, re-encoding
    /// `transform`, and bumps the generation.
    fn refit(&mut self, transform: Mat4) {
        self.frame = LocalFrame::of_bounds(cloud_bounds(&self.mesh, &self.instances));
        self.data.path = self.frame.path_for(transform);
        self.data.bump_generation();
    }
}

impl MeshMobject for InstancedMesh {
    fn payload(&self) -> MeshPayload {
        MeshPayload {
            mesh: Arc::clone(&self.mesh),
            transform: self.transform(),
            material: self.material,
            instances: Some(Arc::clone(&self.instances)),
            height: None,
        }
    }
}

/// The union of the base mesh's bounds transformed by every instance, so the
/// mobject's bounding box covers the whole cloud rather than one atom.
fn cloud_bounds(mesh: &TriMesh, instances: &[Instance]) -> Option<(Vec3, Vec3)> {
    let (lo, hi) = mesh.bounds()?;
    let corners = [
        Vec3::new(lo.x, lo.y, lo.z),
        Vec3::new(hi.x, lo.y, lo.z),
        Vec3::new(lo.x, hi.y, lo.z),
        Vec3::new(hi.x, hi.y, lo.z),
        Vec3::new(lo.x, lo.y, hi.z),
        Vec3::new(hi.x, lo.y, hi.z),
        Vec3::new(lo.x, hi.y, hi.z),
        Vec3::new(hi.x, hi.y, hi.z),
    ];
    let mut out: Option<(Vec3, Vec3)> = None;
    for instance in instances {
        for c in corners {
            let p = instance.transform.transform_point3(c);
            out = Some(match out {
                Some((min, max)) => (min.min(p), max.max(p)),
                None => (p, p),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::{Mobject, MobjectExt};
    use crate::scene_state::SceneState;
    use manim_color::RED;
    use manim_math::RIGHT;

    #[test]
    fn spheres_places_one_instance_per_center() {
        let centers = [Vec3::ZERO, Vec3::X, 2.0 * Vec3::Y];
        let m = InstancedMesh::spheres(&centers, 0.5);
        assert_eq!(m.instances().len(), 3);
        for (i, c) in centers.iter().enumerate() {
            let t = m.instances()[i].transform;
            // The unit sphere's center maps to the atom center, scaled by radius.
            assert!((t.transform_point3(Vec3::ZERO) - *c).length() < 1e-6);
            assert!((t.transform_point3(Vec3::X) - (*c + 0.5 * Vec3::X)).length() < 1e-6);
        }
    }

    /// The contract the render side leans on: the unit cylinder's two ends map
    /// exactly onto each pair's endpoints.
    #[test]
    fn cylinder_endpoints_map_through_the_instance_transform() {
        let pairs = [
            (Vec3::ZERO, 2.0 * Vec3::Z),
            (Vec3::new(1.0, 2.0, 3.0), Vec3::new(-4.0, 0.5, 1.0)),
            (Vec3::Y, -Vec3::Y),
        ];
        let m = InstancedMesh::cylinders(&pairs, 0.25);
        assert_eq!(m.instances().len(), 3);
        for (i, (a, b)) in pairs.iter().enumerate() {
            let t = m.instances()[i].transform;
            assert!(
                (t.transform_point3(-0.5 * Vec3::Z) - *a).length() < 1e-4,
                "start {i}"
            );
            assert!(
                (t.transform_point3(0.5 * Vec3::Z) - *b).length() < 1e-4,
                "end {i}"
            );
            // The radius is honored perpendicular to the axis.
            let radial = t.transform_point3(Vec3::X) - t.transform_point3(Vec3::ZERO);
            assert!((radial.length() - 0.25).abs() < 1e-4, "radius {i}");
        }
    }

    #[test]
    fn zero_length_cylinders_are_skipped() {
        let m = InstancedMesh::cylinders(&[(Vec3::Y, Vec3::Y), (Vec3::ZERO, Vec3::X)], 0.1);
        assert_eq!(m.instances().len(), 1);
    }

    #[test]
    fn bounds_cover_the_whole_cloud() {
        let m = InstancedMesh::spheres(&[Vec3::ZERO, 10.0 * Vec3::X], 1.0);
        // x ∈ [-1, 11].
        assert!((m.width() - 12.0).abs() < 0.2);
        assert!((m.get_center().x - 5.0).abs() < 0.2);
    }

    #[test]
    fn whole_cloud_transform_leaves_instances_alone() {
        let mut m = InstancedMesh::spheres(&[Vec3::ZERO, Vec3::X], 1.0);
        let before = m.instances().to_vec();
        m.shift(3.0 * RIGHT);
        assert_eq!(m.instances(), before.as_slice());
        assert!((m.transform().w_axis.truncate() - 3.0 * Vec3::X).length() < 1e-5);
    }

    #[test]
    fn cow_instance_mutation_spares_the_original() {
        let original = InstancedMesh::spheres(&[Vec3::ZERO], 1.0);
        let mut clone = original.clone();
        clone.update_instances(|xs| xs[0].color = RED);

        assert_eq!(original.instances()[0].color, WHITE);
        assert_eq!(clone.instances()[0].color, RED);
        assert!(clone.data().generation > original.data().generation);
        // The base mesh stays shared: only the instance buffer changed.
        assert!(Arc::ptr_eq(
            &original.mesh_payload().unwrap().mesh,
            &clone.mesh_payload().unwrap().mesh
        ));
    }

    #[test]
    fn set_instances_refits_the_bounds() {
        let mut m = InstancedMesh::spheres(&[Vec3::ZERO], 1.0);
        assert!((m.width() - 2.0).abs() < 0.1);
        m.update_instances(|xs| {
            xs.push(Instance::new(Mat4::from_translation(8.0 * Vec3::X), WHITE))
        });
        assert!((m.width() - 10.0).abs() < 0.2);
    }

    #[test]
    fn display_list_carries_the_instances() {
        let mut scene = SceneState::new();
        scene.add(InstancedMesh::spheres(&[Vec3::ZERO, Vec3::X], 0.5));
        let dl = scene.display_list();
        assert_eq!(dl.len(), 0);
        assert_eq!(dl.meshes().len(), 1);
        assert_eq!(dl.meshes()[0].instances.as_ref().unwrap().len(), 2);
    }
}
