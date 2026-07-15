//! A "bar chart race": a `BarChart` morphing between three value sets with
//! `TransformInto`, over a fixed y-range so the bars scale comparably.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example bar_chart_race
//! ```
//!
//! Frames land in `out/bar_chart_race/frame_NNNNN.png`.
//!
//! API notes vs CE:
//! - `BarChart::new(&values)` plus builders (`with_y_range`, `with_bar_colors`).
//! - `change_bar_values` mutates a chart in place, but to *animate* a change we
//!   morph into a freshly-built chart with `TransformInto` (the target need not
//!   be added to the scene first).

use manim::color::TEAL;
use manim::prelude::*;

/// Scene builder for this gallery example.
pub struct BarChartRace;

impl SceneBuilder for BarChartRace {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let colors = [BLUE, TEAL, GREEN, YELLOW, ORANGE, RED];
        let mk = |values: &[f32]| {
            BarChart::new(values)
                .with_y_range([0.0, 8.0, 2.0])
                .with_bar_colors(&colors)
        };

        let title = Text::new("Bar Chart Race").add_to(scene.state_mut());
        scene.state_mut().move_to(title, 3.0 * UP);

        let chart = scene.add(mk(&[3.0, 5.0, 2.0, 6.0, 4.0, 1.0]));

        scene.play(FadeIn::new(title))?;
        scene.play(Create::new(chart).run_time(1.5))?;
        scene.wait(0.3);
        scene.play(TransformInto::new(chart, mk(&[6.0, 2.0, 7.0, 3.0, 1.0, 5.0])).run_time(1.5))?;
        scene.wait(0.3);
        scene.play(TransformInto::new(chart, mk(&[1.0, 7.0, 3.0, 5.0, 6.0, 2.0])).run_time(1.5))?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&BarChartRace, Config::low())?;
    manim::render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/bar_chart_race")?;
    println!("Rendered frames to out/bar_chart_race");
    Ok(())
}
