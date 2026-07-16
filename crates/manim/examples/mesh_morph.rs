//! A homeomorphism demo: a flat sheet rolls into a cylinder, closes into a
//! torus, and relaxes back — each stage a `MorphSurface` tween in parameter
//! space.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example mesh_morph
//! ```
//!
//! Frames land in `out/mesh_morph/frame_NNNNN.png`.
//!
//! What this demonstrates:
//! - `MorphSurface`: tweening a `Surface3D` between two parameterizations by
//!   interpolating `f₀(u, v) → f₁(u, v)` on the **shared (u, v) grid**. Because
//!   both sides are sampled at the same parameters there is no correspondence
//!   problem to solve — the surface stays correctly meshed and normaled at every
//!   intermediate frame, and normals are recomputed as it deforms.
//! - Why that matters: the sheet → cylinder → torus chain is the textbook
//!   homeomorphism sequence, and it only reads correctly with real occlusion —
//!   once the torus closes, its near half must hide its far half.
//! - `u`/`v` ranges tween too, and the source's resolution is preserved across
//!   the whole chain.
//!
//! API notes vs CE:
//! - CE's `Surface` has no morph animation; the closest is `Transform` between
//!   two surface mobjects, which matches *submobject faces* and can shear badly
//!   when the parameterizations differ. Tweening in parameter space is the mesh
//!   pipeline's answer (docs/design/12-mesh-pipeline.md §3).
//! - `MorphSurface::new(id, f, u_range, v_range)` takes the target
//!   parameterization directly; `::from_arc` reuses another `Surface3D`'s shared
//!   closure via `Surface3D::parametric`.
//! - See also `MorphMesh`, the same-topology vertex lerp between two `Mesh`es.

use manim::color::{PURPLE_B, TEAL_D};
use manim::prelude::*;

/// Scene builder for this gallery example.
pub struct MeshMorph;

/// Major radius of the torus (distance from its axis to the tube's center).
const R_MAJOR: f64 = 1.9;
/// Minor radius of the torus (the tube's own radius).
const R_MINOR: f64 = 0.75;

/// The flat sheet: `(u, v) ↦ (u, v, 0)`, scaled to roughly the torus's footprint.
fn sheet(u: f64, v: f64) -> Vec3 {
    Vec3::new(
        (u / PI as f64 * R_MAJOR * 1.6) as f32,
        (v / PI as f64 * R_MAJOR * 1.6) as f32,
        0.0,
    )
}

/// The open cylinder: `u` wraps around the axis, `v` runs along it. Homeomorphic
/// to the sheet with its two `u` edges glued.
fn cylinder(u: f64, v: f64) -> Vec3 {
    Vec3::new(
        (R_MAJOR * u.cos()) as f32,
        (R_MAJOR * u.sin()) as f32,
        (v / PI as f64 * R_MAJOR * 1.6) as f32,
    )
}

/// The torus: `u` around the axis, `v` around the tube — the cylinder with its
/// two `v` edges glued too.
fn torus(u: f64, v: f64) -> Vec3 {
    let r = R_MAJOR + R_MINOR * v.cos();
    Vec3::new(
        (r * u.cos()) as f32,
        (r * u.sin()) as f32,
        (R_MINOR * v.sin()) as f32,
    )
}

impl SceneBuilder for MeshMorph {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(68_f32.to_radians(), -50_f32.to_radians());

        // Both stages share the (u, v) ∈ [-π, π]² grid — that shared domain is
        // exactly what makes the morph correspondence-free.
        let range = (-PI as f64, PI as f64);
        let surface = scene.add(
            Surface3D::new(sheet, range, range)
                .with_resolution(64, 48)
                .with_checkerboard(Some([TEAL_D, PURPLE_B]))
                .with_material(
                    MeshMaterial::default()
                        .with_shading(Shading::Smooth)
                        .with_lighting(0.26, 0.74, 0.4)
                        .with_shininess(32.0),
                ),
        );

        scene.wait(0.4);
        // Sheet → cylinder: glue the u edges.
        scene.play(MorphSurface::new(surface, cylinder, range, range).run_time(2.0))?;
        scene.wait(0.3);
        // Cylinder → torus: glue the v edges. Past this point the surface
        // self-occludes, which only the depth-tested path gets right.
        scene.play(MorphSurface::new(surface, torus, range, range).run_time(2.0))?;

        // Orbit the finished torus so the occlusion reads.
        let steps = 40;
        for _ in 0..steps {
            scene.rotate_camera(TAU / steps as f32);
            scene.wait(0.06);
        }

        // …and relax all the way back to the sheet.
        scene.play(MorphSurface::new(surface, sheet, range, range).run_time(2.0))?;
        scene.wait(0.4);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&MeshMorph, Config::low())?;
    manim::render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/mesh_morph")?;
    println!("Rendered frames to out/mesh_morph");
    Ok(())
}
