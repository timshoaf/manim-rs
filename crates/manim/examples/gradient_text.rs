//! Port of manim CE's `GradientImageOrText` gallery example (text half).
//!
//! Renders a word whose letters step through a color gradient, plus a
//! gradient-filled bar. Run with:
//!
//! ```sh
//! cargo run -p manim --example gradient_text
//! ```
//!
//! Frames land in `out/gradient_text/frame_NNNN.png`.
//!
//! API note vs CE (a DX gap): CE's `text.set_color_by_gradient(...)` distributes
//! a gradient across a mobject's *submobjects*. Our `MobjectExt::set_color_by_
//! gradient` only sets a fill gradient on a *single* mobject's style, and `Text`
//! draws as per-glyph children — so it does not tint a whole word. Below we tint
//! the glyph children by hand (sampling the gradient per letter); the bar shows
//! the single-mobject gradient-fill path working as intended.

use manim::prelude::*;
use manim::render::OffscreenRenderer;

/// Scene builder for this gallery example.
pub struct GradientText;

impl SceneBuilder for GradientText {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let stops = [BLUE, GREEN, YELLOW, RED];

        // Gradient word, tinted glyph-by-glyph (see the module note).
        let word = Text::new("GRADIENT")
            .font_size(72.0)
            .add_to(scene.state_mut());
        scene.shift(word, 1.0 * UP);
        let glyphs = scene.state().get_dyn(word.erase()).data().children.clone();
        let n = glyphs.len().max(1);
        for (i, glyph) in glyphs.iter().enumerate() {
            let t = i as f32 / (n - 1).max(1) as f32;
            let color = sample_gradient(&stops, t);
            scene
                .state_mut()
                .get_dyn_mut(*glyph)
                .data_mut()
                .style
                .set_fill(color, 1.0);
        }
        scene.play(Write::new(word).run_time(1.5))?;

        // A single fillable mobject with a real gradient fill (the working path).
        let mut bar = Rectangle::new();
        bar.set_color_by_gradient(&stops);
        let bar = scene.add(bar.with_shift(1.5 * DOWN).with_scale(0.6));
        scene.play(FadeIn::new(bar))?;
        scene.wait(0.5);
        Ok(())
    }
}

/// Samples a multi-stop gradient at `t` in `[0, 1]`.
fn sample_gradient(stops: &[Color], t: f32) -> Color {
    if stops.len() == 1 {
        return stops[0];
    }
    let scaled = t.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
    let i = (scaled.floor() as usize).min(stops.len() - 2);
    stops[i].interpolate(&stops[i + 1], scaled - i as f32)
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&GradientText, Config::low())?;
    let mut renderer = OffscreenRenderer::new(scene.config())?;
    let dir = std::path::Path::new("out/gradient_text");
    std::fs::create_dir_all(dir)?;
    let frames: Vec<_> = scene.frames().collect();
    for (i, (_t, list)) in frames.iter().enumerate() {
        renderer.render_to_png(list, dir.join(format!("frame_{i:04}.png")))?;
    }
    println!("Rendered {} frames to {}", frames.len(), dir.display());
    Ok(())
}
