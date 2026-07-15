//! A 3-D saddle surface with a checkerboard fill on `ThreeDAxes`, revealed and
//! then viewed under an ambient (turntable) camera rotation. Inspired by manim
//! CE's `ThreeDSurfacePlot`.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example three_d_surface
//! ```
//!
//! Frames land in `out/three_d_surface/frame_NNNNN.png`.
//!
//! API notes vs CE:
//! - There is no `ThreeDScene`; the ordinary `Scene` gains
//!   `set_camera_orientation(phi, theta)` and `rotate_camera(d_theta)`.
//! - The ambient rotation is driven by interleaving `rotate_camera` with short
//!   `wait`s; each `wait` snapshots the camera, so `frames_with_camera` (used by
//!   `VideoExporter`) replays the orbit.
//! - `Surface::new(|u, v| .., u_range, v_range)` takes 2-element ranges (no
//!   step); `.with_checkerboard(&[..])` colors faces in a checkerboard.

use manim::color::{BLUE_D, BLUE_E};
use manim::prelude::*;

/// Scene builder for this gallery example.
pub struct ThreeDSurface;

impl SceneBuilder for ThreeDSurface {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        // Look down at the scene from an elevated, rotated vantage point.
        scene.set_camera_orientation(65_f32.to_radians(), -60_f32.to_radians());

        let axes = ThreeDAxes::with_ranges([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0], [-2.0, 2.0, 1.0])
            .add_to(scene.state_mut());

        // z = 0.4 (x^2 - y^2): a saddle.
        let surface = Surface::new(
            |u, v| Point::new(u, v, 0.4 * (u * u - v * v)),
            [-2.5, 2.5],
            [-2.5, 2.5],
        )
        .with_resolution(20, 20)
        .with_checkerboard(&[BLUE_D, BLUE_E])
        .with_fill_opacity(0.9)
        .add_to(scene.state_mut());

        scene.play(Create::new(axes))?;
        scene.play(FadeIn::new(surface).run_time(1.5))?;

        // Ambient turntable: ~180° over ~3.5s.
        let steps = 40;
        for _ in 0..steps {
            scene.rotate_camera(PI / steps as f32);
            scene.wait(0.09);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&ThreeDSurface, Config::low())?;
    manim::render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/three_d_surface",
    )?;
    println!("Rendered frames to out/three_d_surface");
    Ok(())
}
