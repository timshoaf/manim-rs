//! A solid cube tumbling about a horizontal axis, viewed from a tilted 3-D
//! camera so the depth-sorted faces read correctly. Port of the spirit of manim
//! CE's `ThreeDScene` cube demos.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example three_d_cube
//! ```
//!
//! Frames land in `out/three_d_cube/frame_NNNNN.png`.
//!
//! API notes vs CE:
//! - `Cube::new(side)` builds six solid (filled) faces centered at the origin.
//! - The built-in `Rotating` / `.animate().rotate()` only spin about the screen-Z
//!   (OUT) axis. For a genuine rotation about an arbitrary world axis we step
//!   `SceneState::rotate_about(id, angle, ORIGIN, RIGHT)` between short `wait`s —
//!   this also exercises the renderer's per-frame depth sort as faces swing from
//!   front to back.

use manim::prelude::*;

/// Scene builder for this gallery example.
pub struct ThreeDCube;

impl SceneBuilder for ThreeDCube {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(70_f32.to_radians(), -50_f32.to_radians());

        let axes = ThreeDAxes::with_ranges([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0])
            .add_to(scene.state_mut());
        let cube = Cube::new(2.0).add_to(scene.state_mut());

        scene.play(Create::new(axes))?;
        scene.play(FadeIn::new(cube))?;

        // One full tumble about the world X (RIGHT) axis over ~4s.
        let steps = 60;
        for _ in 0..steps {
            scene
                .state_mut()
                .rotate_about(cube, TAU / steps as f32, ORIGIN, RIGHT);
            scene.wait(0.06);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&ThreeDCube, Config::low())?;
    manim::render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/three_d_cube")?;
    println!("Rendered frames to out/three_d_cube");
    Ok(())
}
