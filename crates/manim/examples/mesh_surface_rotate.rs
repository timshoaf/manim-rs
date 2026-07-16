//! A shaded, depth-tested saddle surface under an ambient (turntable) camera
//! rotation — the mesh-pipeline counterpart of the `three_d_surface` example.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example mesh_surface_rotate
//! ```
//!
//! Frames land in `out/mesh_surface_rotate/frame_NNNNN.png`.
//!
//! What this demonstrates:
//! - `Surface3D`: a parametric `(u, v) → Vec3` surface carrying real indexed
//!   geometry, drawn by the depth-tested mesh pass with per-pixel Blinn-Phong
//!   shading — so the saddle's near lobe genuinely *occludes* its far one.
//! - `MeshMaterial`: base color plus lighting coefficients; `Shading::Smooth`
//!   interpolates vertex normals across faces.
//! - Checkerboard two-tone fill, for CE `Surface` parity.
//!
//! API notes vs CE:
//! - CE's `Surface` (and our `threed::Surface`, see `three_d_surface.rs`)
//!   projects bezier faces to 2D and depth-*sorts* them per frame. `Surface3D`
//!   instead hands the renderer a `TriMesh` that is depth-*tested* per pixel.
//!   Both paths ship; this one is the choice when geometry interpenetrates or
//!   needs real shading. See `docs/migration-guide.md`.
//! - The parametric closure returns a `Vec3` (glam), re-exported through the
//!   prelude. `Point` is the same type.
//! - The ambient orbit is the same `rotate_camera` + short-`wait` interleave the
//!   project-and-sort 3D examples use: the camera is snapshotted per `wait`, so
//!   `frames_with_camera` replays it. The mesh pass reads the very same camera.

use manim::color::{BLUE_D, BLUE_E};
use manim::prelude::*;

/// Scene builder for this gallery example.
pub struct MeshSurfaceRotate;

impl SceneBuilder for MeshSurfaceRotate {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        // Look down at the scene from an elevated, rotated vantage point.
        scene.set_camera_orientation(65_f32.to_radians(), -60_f32.to_radians());

        // Start as a flat sheet, meshed at 48×48 quads. Unlike the
        // project-and-sort path, resolution here costs GPU triangles, not CPU
        // re-tessellation — the mesh uploads once and is reused every frame.
        let surface = scene.add(
            Surface3D::new(
                |u, v| Vec3::new(u as f32, v as f32, 0.0),
                (-2.5, 2.5),
                (-2.5, 2.5),
            )
            .with_resolution(48, 48)
            .with_checkerboard(Some([BLUE_D, BLUE_E]))
            .with_material(
                MeshMaterial::default()
                    .with_shading(Shading::Smooth)
                    .with_lighting(0.25, 0.75, 0.35)
                    .with_shininess(24.0),
            ),
        );

        // Reveal by growing the saddle out of a flat sheet: `MorphSurface` tweens
        // in *parameter space*, so the mesh stays correctly normaled throughout.
        // (Style-based reveals like `FadeIn` are no-ops on a mesh — its
        // appearance is its `MeshMaterial`, not a `Style`.)
        scene.play(
            MorphSurface::new(
                surface,
                |u, v| Vec3::new(u as f32, v as f32, 0.4 * (u * u - v * v) as f32),
                (-2.5, 2.5),
                (-2.5, 2.5),
            )
            .run_time(1.5),
        )?;

        // Ambient turntable: one full revolution over ~4.5s.
        let steps = 60;
        for _ in 0..steps {
            scene.rotate_camera(TAU / steps as f32);
            scene.wait(0.075);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&MeshSurfaceRotate, Config::low())?;
    manim::render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/mesh_surface_rotate",
    )?;
    println!("Rendered frames to out/mesh_surface_rotate");
    Ok(())
}
