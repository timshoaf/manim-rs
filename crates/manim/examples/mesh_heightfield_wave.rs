//! A traveling wave crossing a standing wave on a `HeightField`, driven by a
//! per-frame updater — the one-texture-upload-per-frame path.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example mesh_heightfield_wave
//! ```
//!
//! Frames land in `out/mesh_heightfield_wave/frame_NNNNN.png`.
//!
//! What this demonstrates:
//! - `HeightField`: a static `nu × nv` grid mesh plus an `R32Float` height
//!   texture sampled in the **vertex shader**. Normals come from finite
//!   differences of neighboring texels, also in-shader.
//! - The cheap-animation path: a field that changes every single frame costs one
//!   `nu × nv × 4 B` texture upload — the grid geometry itself is uploaded once
//!   and never re-tessellated. At 128×128 that's a 64 KB write per frame instead
//!   of re-meshing 32k triangles on the CPU.
//! - `update_heights` re-evaluates a closure over the grid and bumps the
//!   mobject's generation, which is what tells the renderer to re-upload just
//!   that texture (same allocation, since the dims don't change).
//!
//! API notes vs CE:
//! - Neither CE nor ManimGL has an equivalent — this is a beyond-CE capability
//!   of the mesh pipeline (docs/design/12-mesh-pipeline.md §7). The nearest CE
//!   analogue is rebuilding a `Surface` every frame, which is CPU-bound.
//! - The updater is an ordinary `SceneState::add_updater`: mesh mobjects use the
//!   same updater machinery as path mobjects. Capturing the typed
//!   `MobjectId<HeightField>` in the closure keeps `get_mut` typed.

use manim::color::{BLUE_D, TEAL_B};
use manim::prelude::*;

/// Scene builder for this gallery example.
pub struct MeshHeightfieldWave;

/// Grid resolution of the field, in both u and v.
const N: usize = 128;
/// Half-extent of the field in scene units.
const EXTENT: f32 = 3.0;

/// The wave field at time `t`: a traveling wave along +x crossed with a standing
/// wave in y, damped radially so the sheet settles at its edges.
fn wave(x: f32, y: f32, t: f32) -> f32 {
    let traveling = (2.0 * x - 3.0 * t).sin();
    let standing = (2.5 * y).cos() * (2.0 * t).cos();
    let damp = (-0.12 * (x * x + y * y)).exp();
    0.45 * damp * (traveling + 0.7 * standing)
}

impl SceneBuilder for MeshHeightfieldWave {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(62_f32.to_radians(), -45_f32.to_radians());

        // The grid is built once. `from_fn` seeds t = 0 heights.
        let field = scene.add(
            HeightField::from_fn(N, N, (EXTENT, EXTENT), |x, y| wave(x, y, 0.0)).with_material(
                MeshMaterial::new(BLUE_D)
                    .with_lighting(0.28, 0.72, 0.45)
                    .with_shininess(48.0),
            ),
        );

        // One updater; one R32Float upload per frame. Nothing is re-tessellated.
        scene.state_mut().add_updater(field, move |s, _id, ctx| {
            let t = ctx.time;
            s.get_mut(field).update_heights(|x, y| wave(x, y, t));
        });

        // Let the wave run while the camera orbits — the field is re-evaluated
        // every frame the timeline emits.
        let steps = 90;
        for _ in 0..steps {
            scene.rotate_camera(TAU / (2.0 * steps as f32));
            scene.wait(0.06);
        }

        // Settle: drop the updater and damp the sheet to a still, shallower
        // ripple, so the last frames show the field at rest. Recoloring is a
        // material change — the geometry and the grid are untouched.
        scene.state_mut().remove_updaters(field);
        scene
            .state_mut()
            .get_mut(field)
            .update_heights(|x, y| 0.25 * wave(x, y, 0.0))
            .set_material(MeshMaterial::new(TEAL_B).with_lighting(0.3, 0.7, 0.4));
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&MeshHeightfieldWave, Config::low())?;
    manim::render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/mesh_heightfield_wave",
    )?;
    println!("Rendered frames to out/mesh_heightfield_wave");
    Ok(())
}
