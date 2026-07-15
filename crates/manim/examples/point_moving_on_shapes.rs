//! Port of manim CE's `PointMovingOnShapes` gallery example.
//!
//! A dot travels around a circle's outline via [`MoveAlongPath`], then the whole
//! scene spins with [`Rotating`]. Run with:
//!
//! ```sh
//! cargo run -p manim --example point_moving_on_shapes
//! ```
//!
//! Frames land in `out/point_moving_on_shapes/frame_NNNN.png`.
//!
//! API note vs CE: `MoveAlongPath::new(dot, path)` takes an owned `Path`, so we
//! clone the circle's path before adding the circle to the scene.

use manim::prelude::*;
use manim::render::OffscreenRenderer;

/// Scene builder for this gallery example.
pub struct PointMovingOnShapes;

impl SceneBuilder for PointMovingOnShapes {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let circle = scene.add(Circle::new().with_stroke(BLUE, 4.0, 1.0));
        scene.play(Create::new(circle))?;

        // A dot starting at the circle's rightmost point, then riding the outline.
        // `MoveAlongPath::along` clones the circle's path at begin() time — no
        // manual path extraction / ownership dance.
        let dot = scene.add(Dot::new().with_fill(YELLOW, 1.0).with_move_to(RIGHT));
        scene.play(Create::new(dot))?;
        scene.play(MoveAlongPath::along(dot, circle).run_time(2.0))?;

        // Spin the pair a quarter turn about the origin.
        scene.play((
            Rotating::new(circle).angle(PI / 2.0).run_time(1.5),
            Rotating::new(dot).angle(PI / 2.0).run_time(1.5),
        ))?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&PointMovingOnShapes, Config::low())?;
    let mut renderer = OffscreenRenderer::new(scene.config())?;
    let dir = std::path::Path::new("out/point_moving_on_shapes");
    std::fs::create_dir_all(dir)?;
    let frames: Vec<_> = scene.frames().collect();
    for (i, (_t, list)) in frames.iter().enumerate() {
        renderer.render_to_png(list, dir.join(format!("frame_{i:04}.png")))?;
    }
    println!("Rendered {} frames to {}", frames.len(), dir.display());
    Ok(())
}
