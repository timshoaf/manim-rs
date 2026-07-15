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
//! API note vs CE: mirrors CE's `always_redraw(lambda: Angle(line1, line2))` via
//! [`Scene::always_redraw`] — the moving ray and angle arc are rebuilt from the
//! tracker each frame by a closure, exactly like CE.

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
        let theta = scene.add(ValueTracker::new(start_theta));

        // always_redraw: the moving ray and angle arc are rebuilt from the
        // tracker each frame — the terse CE idiom, no manual updaters.
        let _moving = scene.always_redraw(move |s| {
            let th = s.get(theta).get_value();
            Line::new(ORIGIN, ray_end(th)).with_stroke(BLUE, 4.0, 1.0)
        });
        let _arc = scene.always_redraw(move |s| {
            let th = s.get(theta).get_value();
            Angle::new(&Line::new(ORIGIN, 3.0 * RIGHT), &Line::new(ORIGIN, ray_end(th)))
                .with_color(YELLOW)
        });

        scene.play(Create::new(fixed))?;
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
