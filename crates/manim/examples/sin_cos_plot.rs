//! Port of manim CE's `SinAndCosFunctionPlot` gallery example.
//!
//! Axes with `sin` and `cos` plotted, axis labels, and a vertical marker line.
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example sin_cos_plot
//! ```
//!
//! Frames land in `out/sin_cos_plot/frame_NNNN.png`.
//!
//! API notes vs CE:
//! - `axes.plot(f, None)` returns a `FunctionGraph` mobject to `add`; `None`
//!   plots over the axes' own x-range.
//! - Axis labels come from the `AxesLabels` extension trait (`manim_text`),
//!   now available through the facade prelude.

use manim::prelude::*;

/// Scene builder for this gallery example.
pub struct SinCosPlot;

impl SceneBuilder for SinCosPlot {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let axes = Axes::with_lengths([-6.0, 6.0, 1.0], [-1.5, 1.5, 0.5], 11.0, 3.5);

        let sin_graph = axes.plot(|x| x.sin(), None).with_color(BLUE);
        let cos_graph = axes.plot(|x| x.cos(), None).with_color(RED);
        // A vertical line marking x = pi/2, where sin peaks.
        let marker = axes
            .get_vertical_line(std::f32::consts::FRAC_PI_2, 1.0)
            .with_stroke(YELLOW, 3.0, 1.0);

        // Axis labels (extension trait) — added before the axes value is consumed.
        // get_axis_labels now returns CoreError, so it composes with `?`.
        let labels = axes.get_axis_labels(scene.state_mut(), "x", "y")?;

        let axes = scene.add(axes);
        let sin_id = scene.add(sin_graph);
        let cos_id = scene.add(cos_graph);
        let marker = scene.add(marker);

        scene.play(Create::new(axes))?;
        scene.play(FadeIn::new(labels))?;
        scene.play((Create::new(sin_id), Create::new(cos_id)))?;
        scene.play(Create::new(marker))?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&SinCosPlot, Config::low())?;
    let mut renderer = manim::render::OffscreenRenderer::new(scene.config())?;
    let dir = std::path::Path::new("out/sin_cos_plot");
    std::fs::create_dir_all(dir)?;
    let frames: Vec<_> = scene.frames().collect();
    for (i, (_t, list)) in frames.iter().enumerate() {
        renderer.render_to_png(list, dir.join(format!("frame_{i:04}.png")))?;
    }
    println!("Rendered {} frames to {}", frames.len(), dir.display());
    Ok(())
}
