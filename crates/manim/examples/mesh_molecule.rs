//! A toy molecule: a few hundred instanced atoms and bonds under a rotating
//! camera, with a translucent "molecular surface" bubble over one lobe.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example mesh_molecule
//! ```
//!
//! Frames land in `out/mesh_molecule/frame_NNNNN.png`.
//!
//! What this demonstrates:
//! - `InstancedMesh::spheres` / `::cylinders`: one base `TriMesh` drawn at many
//!   transforms via GPU instancing. The whole ball-and-stick model below is
//!   **two draw calls**, whatever the atom count — per-instance transform+color
//!   ride a second vertex buffer, uploaded only when they change. (Measured
//!   headroom: 10k instanced spheres ≈ 0.8 ms/frame.)
//! - Whole-cloud animation: `Rotating` on the `InstancedMesh` mobject spins the
//!   model transform; the instance buffer is never re-uploaded.
//! - Transparency: the bubble's `MeshMaterial::with_opacity` puts it in the
//!   renderer's translucent queue — drawn after the opaque pass, depth-tested
//!   read-only and sorted back-to-front, so atoms show *through* it correctly.
//!
//! API notes vs CE:
//! - CE has no instancing: a few hundred `Sphere`s would be a few hundred
//!   mobjects, each CPU-tessellated and depth-sorted per frame. This is a
//!   beyond-CE capability of the mesh pipeline (docs/design/12-mesh-pipeline.md).
//! - `Instance { transform: Mat4, color: Color }` is per-instance state; the
//!   `spheres`/`cylinders` helpers build the transforms for you, and
//!   `update_instances` lets you recolor without touching geometry.

use manim::color::{BLUE_B, GREY_B, RED_C, TEAL_C, WHITE};
use manim::prelude::*;

/// Scene builder for this gallery example.
pub struct MeshMolecule;

/// Builds a toy branched molecule: a hexagonal ring with substituent chains
/// radiating from it, returning `(atom centers, bond endpoint pairs)`.
///
/// Purely geometric — no chemistry is implied beyond the shape reading as a
/// ball-and-stick model.
fn build_molecule() -> (Vec<Vec3>, Vec<(Vec3, Vec3)>) {
    let mut atoms: Vec<Vec3> = Vec::new();
    let mut bonds: Vec<(Vec3, Vec3)> = Vec::new();

    // A central hexagonal ring in the z = 0 plane.
    let ring_r = 1.2_f32;
    let ring: Vec<Vec3> = (0..6)
        .map(|i| {
            let a = i as f32 / 6.0 * TAU;
            Vec3::new(ring_r * a.cos(), ring_r * a.sin(), 0.0)
        })
        .collect();
    for (i, &p) in ring.iter().enumerate() {
        atoms.push(p);
        bonds.push((p, ring[(i + 1) % 6]));
    }

    // From each ring atom, six strands spiralling out of the plane. Each link
    // adds one atom and one bond: 6 ring atoms × 8 links × 6 strands + the ring
    // itself = 294 atoms and 294 bonds — still just two draw calls.
    for (i, &start) in ring.iter().enumerate() {
        let out = start.normalize();
        let up = if i % 2 == 0 { 1.0 } else { -1.0 };
        let mut prev = start;
        for k in 1..=8 {
            let t = k as f32;
            // Branch into six strands per ring atom.
            for s in 0..6 {
                let twist = s as f32 / 6.0 * TAU + t * 0.5;
                let next = start
                    + out * (0.55 * t)
                    + Vec3::new(0.0, 0.0, up * 0.22 * t)
                    + Vec3::new(twist.cos(), twist.sin(), 0.0) * 0.28;
                atoms.push(next);
                bonds.push((prev, next));
                prev = next;
            }
        }
    }
    (atoms, bonds)
}

impl SceneBuilder for MeshMolecule {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(70_f32.to_radians(), -55_f32.to_radians());

        let (atoms, bonds) = build_molecule();

        // Atoms: one uv-sphere base mesh, one instance per atom. Recolor by
        // height so the structure reads under rotation — per-instance color is
        // free (it rides the instance buffer, not the geometry).
        let mut spheres = InstancedMesh::spheres(&atoms, 0.16);
        spheres.update_instances(|is| {
            for inst in is.iter_mut() {
                let z = inst.transform.w_axis.z;
                inst.color = if z > 0.35 {
                    RED_C
                } else if z < -0.35 {
                    BLUE_B
                } else {
                    TEAL_C
                };
            }
        });
        let atoms_id = scene
            .add(spheres.with_material(MeshMaterial::new(WHITE).with_lighting(0.25, 0.75, 0.5)));

        // Bonds: one cylinder base mesh, one instance per bond — draw call #2.
        let bonds_id = scene.add(
            InstancedMesh::cylinders(&bonds, 0.05)
                .with_material(MeshMaterial::new(GREY_B).with_lighting(0.3, 0.7, 0.2)),
        );

        // A translucent molecular-surface bubble over the upper lobe. Opacity
        // below 1 routes it to the sorted translucent queue.
        let bubble = scene.add(
            Mesh::sphere()
                .with_transform(Mat4::from_scale_rotation_translation(
                    Vec3::splat(2.2),
                    manim::glam::Quat::IDENTITY,
                    Vec3::new(1.4, 1.4, 0.8),
                ))
                .with_material(
                    MeshMaterial::new(TEAL_C)
                        .with_opacity(0.28)
                        .with_lighting(0.4, 0.6, 0.6)
                        .with_shininess(64.0),
                ),
        );

        // Whole-cloud rotation: the two instanced mobjects and the bubble spin
        // together about the screen axis. Only their model transforms change.
        scene.play((
            Rotating::new(atoms_id).angle(TAU).run_time(6.0),
            Rotating::new(bonds_id).angle(TAU).run_time(6.0),
            Rotating::new(bubble).angle(TAU).run_time(6.0),
        ))?;
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&MeshMolecule, Config::low())?;
    manim::render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/mesh_molecule")?;
    println!("Rendered frames to out/mesh_molecule");
    Ok(())
}
