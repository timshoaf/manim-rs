//! Port of manim CE's `TransformMatchingTex` gallery example.
//!
//! One formula morphs into another, shared glyphs gliding into place while the
//! rest fade. Run with:
//!
//! ```sh
//! cargo run -p manim --example transform_matching_tex
//! ```
//!
//! Frames land in `out/transform_matching_tex/frame_NNNN.png`.
//!
//! API note vs CE: `TransformMatchingTex` lives in `manim_text` and is **not** in
//! the facade prelude — imported by full path here. It matches glyphs by shape
//! signature (no substring isolation needed), unlike CE which matches on TeX
//! substrings.

use manim::prelude::*;
use manim::text::TransformMatchingTex;

/// Scene builder for this gallery example.
pub struct TransformMatchingTexDemo;

impl SceneBuilder for TransformMatchingTexDemo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let a = MathTex::new(r"e^{i\pi} + 1 = 0")
            .expect("valid formula")
            .font_size(72.0)
            .add_to(scene.state_mut());
        scene.play(Write::new(a).run_time(1.0))?;
        scene.wait(0.3);

        let b = MathTex::new(r"e^{i\pi} = -1")
            .expect("valid formula")
            .font_size(72.0)
            .add_to(scene.state_mut());
        scene.play(TransformMatchingTex::new(a, b).run_time(1.5))?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&TransformMatchingTexDemo, Config::low())?;
    let mut renderer = manim::render::OffscreenRenderer::new(scene.config())?;
    let dir = std::path::Path::new("out/transform_matching_tex");
    std::fs::create_dir_all(dir)?;
    let frames: Vec<_> = scene.frames().collect();
    for (i, (_t, list)) in frames.iter().enumerate() {
        renderer.render_to_png(list, dir.join(format!("frame_{i:04}.png")))?;
    }
    println!("Rendered {} frames to {}", frames.len(), dir.display());
    Ok(())
}
