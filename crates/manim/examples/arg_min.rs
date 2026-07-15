//! Port of manim CE's `ArgMinExample` gallery example.
//!
//! A parabola is plotted and a dot slides to its minimum, driven by a
//! [`ValueTracker`] and an updater. Run with:
//!
//! ```sh
//! cargo run -p manim --example arg_min
//! ```
//!
//! Frames land in `out/arg_min/frame_NNNN.png`.
//!
//! API note vs CE: CE uses `always_redraw` + `t.animate.set_value(...)`. We have
//! no `always_redraw`; instead we register an updater that reads the tracker and
//! `move_to`s the dot each frame, then animate the tracker with [`SetValue`].

use manim::prelude::*;

/// The parabola being minimized: vertex at `x = 2`, `y = 1`.
fn parabola(x: f32) -> f32 {
    (x - 2.0).powi(2) + 1.0
}

/// Scene builder for this gallery example.
pub struct ArgMin;

impl SceneBuilder for ArgMin {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let axes = Axes::with_lengths([0.0, 5.0, 1.0], [0.0, 6.0, 1.0], 7.0, 4.5);
        let coords = axes.coords(); // Copy snapshot for the updater closure
        let graph = axes.plot(parabola, None).with_color(BLUE);
        let axes = scene.add(axes);
        let graph = scene.add(graph);
        scene.play((Create::new(axes), Create::new(graph)))?;

        // Dot starts over x = 4 and will slide to the minimum at x = 2.
        let start_x = 4.0;
        let dot = scene.add(
            Dot::new()
                .with_fill(YELLOW, 1.0)
                .with_move_to(coords.coords_to_point(start_x, parabola(start_x))),
        );
        let tracker = scene.add(ValueTracker::new(start_x));
        scene
            .state_mut()
            .add_updater(dot.erase(), move |s, id, _ctx| {
                let x = s.get(tracker).get_value();
                s.move_to(id, coords.coords_to_point(x, parabola(x)));
            });

        scene.play(Create::new(dot))?;
        scene.play(SetValue::new(tracker, 2.0).run_time(2.0))?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&ArgMin, Config::low())?;
    let mut renderer = manim::render::OffscreenRenderer::new(scene.config())?;
    let dir = std::path::Path::new("out/arg_min");
    std::fs::create_dir_all(dir)?;
    let frames: Vec<_> = scene.frames().collect();
    for (i, (_t, list)) in frames.iter().enumerate() {
        renderer.render_to_png(list, dir.join(format!("frame_{i:04}.png")))?;
    }
    println!("Rendered {} frames to {}", frames.len(), dir.display());
    Ok(())
}
