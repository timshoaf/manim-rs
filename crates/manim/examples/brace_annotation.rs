//! Port of manim CE's `BraceAnnotation` gallery example.
//!
//! A line segment annotated with braces and text labels along and beneath it.
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example brace_annotation
//! ```
//!
//! Frames land in `out/brace_annotation/frame_NNNN.png`.
//!
//! API note vs CE: CE's `Brace(mobject)` measures a mobject and `brace.get_text`
//! attaches a label. Ours is `Brace::new(start, end, direction)` over an explicit
//! extent, and label placement uses `brace_label_point(buff)` (text attachment is
//! a manual `Text` add — no `BraceLabel` convenience in the facade yet).

use manim::prelude::*;
use manim::render::OffscreenRenderer;

/// Scene builder for this gallery example.
pub struct BraceAnnotation;

impl SceneBuilder for BraceAnnotation {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let start = 3.0 * LEFT;
        let end = 3.0 * RIGHT;
        let line = scene.add(Line::new(start, end).with_stroke(WHITE, 4.0, 1.0));
        scene.play(Create::new(line))?;

        // Brace under the segment, labelled with its length.
        let under = Brace::new(start, end, DOWN);
        let under_label_at = under.brace_label_point(0.35);
        let under = scene.add(under);
        let under_label = Text::new("6 units")
            .font_size(28.0)
            .add_to(scene.state_mut());
        scene
            .state_mut()
            .shift(under_label.erase(), under_label_at + 0.2 * DOWN);
        scene.play((Create::new(under), FadeIn::new(under_label)))?;

        // Brace above the right half, labelled.
        let over = Brace::new(ORIGIN, end, UP);
        let over_label_at = over.brace_label_point(0.35);
        let over = scene.add(over);
        let over_label = Text::new("half").font_size(28.0).add_to(scene.state_mut());
        scene
            .state_mut()
            .shift(over_label.erase(), over_label_at + 0.2 * UP);
        scene.play((Create::new(over), FadeIn::new(over_label)))?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&BraceAnnotation, Config::low())?;
    let mut renderer = OffscreenRenderer::new(scene.config())?;
    let dir = std::path::Path::new("out/brace_annotation");
    std::fs::create_dir_all(dir)?;
    let frames: Vec<_> = scene.frames().collect();
    for (i, (_t, list)) in frames.iter().enumerate() {
        renderer.render_to_png(list, dir.join(format!("frame_{i:04}.png")))?;
    }
    println!("Rendered {} frames to {}", frames.len(), dir.display());
    Ok(())
}
