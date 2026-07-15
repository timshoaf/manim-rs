//! Port of manim CE's `VectorArrow` gallery example.
//!
//! A number plane with a dot at the origin, an arrow to `(2, 2)`, and two
//! labels. Run with:
//!
//! ```sh
//! cargo run -p manim --example vector_arrow
//! ```
//!
//! Frames land in `out/vector_arrow/frame_NNNN.png`.
//!
//! API note vs CE: CE's `NumberPlane` fills the frame by default; here we pass
//! explicit ranges. Positions come from `plane.coords_to_point(x, y)` (`c2p`).

use manim::prelude::*;
use manim::render::OffscreenRenderer;

/// Scene builder for this gallery example.
pub struct VectorArrow;

impl SceneBuilder for VectorArrow {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let plane = NumberPlane::new([-5.0, 5.0, 1.0], [-3.0, 3.0, 1.0]);
        let origin = plane.coords_to_point(0.0, 0.0);
        let tip = plane.coords_to_point(2.0, 2.0);
        let _plane = scene.add(plane);

        let dot = scene.add(Dot::new().with_fill(WHITE, 1.0).with_move_to(origin));
        let arrow = scene.add(Arrow::new(origin, tip).with_color(YELLOW));

        let origin_label = Text::new("(0, 0)")
            .font_size(20.0)
            .add_to(scene.state_mut());
        scene
            .state_mut()
            .shift(origin_label.erase(), origin + 0.4 * DOWN + 0.5 * LEFT);
        let tip_label = Text::new("(2, 2)")
            .font_size(20.0)
            .add_to(scene.state_mut());
        scene
            .state_mut()
            .shift(tip_label.erase(), tip + 0.3 * UP + 0.4 * RIGHT);

        scene.play(Create::new(dot))?;
        scene.play(Create::new(arrow))?;
        scene.play((FadeIn::new(origin_label), FadeIn::new(tip_label)))?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&VectorArrow, Config::low())?;
    let mut renderer = OffscreenRenderer::new(scene.config())?;
    let dir = std::path::Path::new("out/vector_arrow");
    std::fs::create_dir_all(dir)?;
    let frames: Vec<_> = scene.frames().collect();
    for (i, (_t, list)) in frames.iter().enumerate() {
        renderer.render_to_png(list, dir.join(format!("frame_{i:04}.png")))?;
    }
    println!("Rendered {} frames to {}", frames.len(), dir.display());
    Ok(())
}
