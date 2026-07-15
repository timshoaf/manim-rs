//! Port of manim CE's `MovingAround` gallery example.
//!
//! A square demonstrates the `.animate()` builder: it shifts, scales, recolors,
//! and restores. Run with:
//!
//! ```sh
//! cargo run -p manim --example moving_around
//! ```
//!
//! Frames land in `out/moving_around/frame_NNNN.png`.
//!
//! API note vs CE: CE writes `square.animate.shift(RIGHT)`; here the builder is a
//! method call `square.animate().shift(RIGHT)` and chains transforms fluently.

use manim::prelude::*;
use manim::render::OffscreenRenderer;

/// Scene builder for this gallery example.
pub struct MovingAround;

impl SceneBuilder for MovingAround {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let square = scene.add(Square::new().with_fill(BLUE, 1.0));

        scene.play(square.animate().shift(2.0 * RIGHT))?;
        scene.play(square.animate().set_fill(ORANGE, 0.5))?;
        scene.play(square.animate().scale(0.3))?;
        // A single chained builder: move back to origin while growing and recoloring.
        scene.play(
            square
                .animate()
                .move_to(ORIGIN)
                .scale(3.0)
                .set_fill(GREEN, 1.0),
        )?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&MovingAround, Config::low())?;
    let mut renderer = OffscreenRenderer::new(scene.config())?;
    let dir = std::path::Path::new("out/moving_around");
    std::fs::create_dir_all(dir)?;
    let frames: Vec<_> = scene.frames().collect();
    for (i, (_t, list)) in frames.iter().enumerate() {
        renderer.render_to_png(list, dir.join(format!("frame_{i:04}.png")))?;
    }
    println!("Rendered {} frames to {}", frames.len(), dir.display());
    Ok(())
}
