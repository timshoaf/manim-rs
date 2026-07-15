//! Port of manim CE's `MovingAngle` gallery example.
//!
//! A ray rotates about the origin and the angle it makes with a fixed horizontal
//! ray is redrawn every frame, driven by a [`ValueTracker`]. Run with:
//!
//! ```sh
//! cargo run -p manim --example moving_angle
//! ```
//!
//! Frames land in `out/moving_angle/frame_NNNN.png`.
//!
//! API note vs CE: CE uses `always_redraw(lambda: Angle(line1, line2))`. We have
//! no `always_redraw`, so an updater rebuilds the moving ray and the angle arc
//! from the tracker each frame (rebuilding an `Angle` value and copying its path).

use manim::core::geometry::Angle;
use manim::math::path::Path;
use manim::prelude::*;

/// The moving ray's endpoint at angle `theta` (length 3).
fn ray_end(theta: f32) -> Point {
    Point::new(3.0 * theta.cos(), 3.0 * theta.sin(), 0.0)
}

/// Scene builder for this gallery example.
pub struct MovingAngle;

impl SceneBuilder for MovingAngle {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let start_theta = std::f32::consts::FRAC_PI_6;

        let fixed = scene.add(Line::new(ORIGIN, 3.0 * RIGHT).with_stroke(WHITE, 4.0, 1.0));
        let moving = scene.add(Line::new(ORIGIN, ray_end(start_theta)).with_stroke(BLUE, 4.0, 1.0));
        let arc = scene.add(
            Angle::new(
                &Line::new(ORIGIN, 3.0 * RIGHT),
                &Line::new(ORIGIN, ray_end(start_theta)),
            )
            .with_color(YELLOW),
        );
        scene.play((Create::new(fixed), Create::new(moving), Create::new(arc)))?;

        let theta = scene.add(ValueTracker::new(start_theta));
        // Rebuild the moving ray from the tracker.
        scene
            .state_mut()
            .add_updater(moving.erase(), move |s, id, _| {
                let th = s.get(theta).get_value();
                s.get_dyn_mut(id).data_mut().path =
                    Path::from_corners(&[ORIGIN, ray_end(th)], false);
                s.get_dyn_mut(id).data_mut().bump_generation();
            });
        // Rebuild the angle arc from the tracker.
        scene.state_mut().add_updater(arc.erase(), move |s, id, _| {
            let th = s.get(theta).get_value();
            let l1 = Line::new(ORIGIN, 3.0 * RIGHT);
            let l2 = Line::new(ORIGIN, ray_end(th));
            let path = Angle::new(&l1, &l2).data().path.clone();
            s.get_dyn_mut(id).data_mut().path = path;
            s.get_dyn_mut(id).data_mut().bump_generation();
        });

        scene.play(SetValue::new(theta, std::f32::consts::FRAC_PI_2 + 0.6).run_time(2.0))?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&MovingAngle, Config::low())?;
    let mut renderer = manim::render::OffscreenRenderer::new(scene.config())?;
    let dir = std::path::Path::new("out/moving_angle");
    std::fs::create_dir_all(dir)?;
    let frames: Vec<_> = scene.frames().collect();
    for (i, (_t, list)) in frames.iter().enumerate() {
        renderer.render_to_png(list, dir.join(format!("frame_{i:04}.png")))?;
    }
    println!("Rendered {} frames to {}", frames.len(), dir.display());
    Ok(())
}
