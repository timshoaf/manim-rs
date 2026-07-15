//! Port of manim CE's `BooleanOperations` gallery example.
//!
//! Two overlapping ellipses are combined with the four set operations
//! ([`Union`], [`Intersection`], [`Difference`], [`Exclusion`]) and laid out in a
//! row, each with a caption. Run with:
//!
//! ```sh
//! cargo run -p manim --example boolean_operations
//! ```
//!
//! Frames land in `out/boolean_operations/frame_NNNN.png`.
//!
//! API note vs CE: CE calls `Union(e1, e2)`; here the ops are `Union::new(&a, &b)`
//! and return a `VMobject` whose fill inherits the first operand's style. They
//! live in `manim_core::boolean` but are re-exported through the prelude.

use manim::prelude::*;
use manim::render::OffscreenRenderer;

/// Scene builder for this gallery example.
pub struct BooleanOperations;

impl SceneBuilder for BooleanOperations {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        // Two overlapping source circles (CE uses ellipses; Circle keeps it simple).
        let make_pair = || {
            let a = Circle::new().with_fill(BLUE, 0.5).with_shift(0.5 * LEFT);
            let b = Circle::new().with_fill(GREEN, 0.5).with_shift(0.5 * RIGHT);
            (a, b)
        };

        let slots = [3.6 * LEFT, 1.2 * LEFT, 1.2 * RIGHT, 3.6 * RIGHT];
        let labels = ["Union", "Intersection", "Difference", "Exclusion"];

        for (i, slot) in slots.iter().enumerate() {
            let (a, b) = make_pair();
            let result = match i {
                0 => Union::new(&a, &b),
                1 => Intersection::new(&a, &b),
                2 => Difference::new(&a, &b),
                _ => Exclusion::new(&a, &b),
            };
            let id = scene.add(result);
            scene.scale(id, 0.7);
            scene.shift(id, *slot + 0.6 * UP);
            scene.play(Create::new(id).run_time(0.4))?;

            let caption = Text::new(labels[i])
                .font_size(22.0)
                .add_to(scene.state_mut());
            scene.shift(caption, *slot + 1.6 * DOWN);
            scene.play(FadeIn::new(caption).run_time(0.2))?;
        }
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    render_png_frames(&BooleanOperations, "boolean_operations")
}

/// Renders a builder to a PNG frame sequence under `out/<name>/` (GPU, no ffmpeg).
fn render_png_frames(
    builder: &dyn SceneBuilder,
    name: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(builder, Config::low())?;
    let mut renderer = OffscreenRenderer::new(scene.config())?;
    let dir = std::path::Path::new("out").join(name);
    std::fs::create_dir_all(&dir)?;
    let frames: Vec<_> = scene.frames().collect();
    for (i, (_t, list)) in frames.iter().enumerate() {
        renderer.render_to_png(list, dir.join(format!("frame_{i:04}.png")))?;
    }
    println!("Rendered {} frames to {}", frames.len(), dir.display());
    Ok(())
}
