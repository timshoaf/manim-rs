//! Text and math typesetting demo: writes a headline, then Euler's identity.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example hello_math
//! ```
//!
//! Produces `out/hello_math.mp4` (requires `ffmpeg` on PATH).

use manim::prelude::*;
use manim::render::export::VideoExporter;

struct HelloMath;

impl SceneBuilder for HelloMath {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let title = Text::new("manim, in Rust")
            .font_size(56.0)
            .color(YELLOW)
            .add_to(scene.state_mut());
        scene.state_mut().shift(title.erase(), 2.0 * UP);
        scene.play(Write::new(title).run_time(1.5))?;

        let euler = MathTex::new(r"e^{i\pi} + 1 = 0")
            .expect("valid formula")
            .font_size(72.0)
            .add_to(scene.state_mut());
        scene.state_mut().shift(euler.erase(), 0.5 * DOWN);
        scene.play(Write::new(euler).run_time(2.0))?;
        scene.wait(0.5);

        scene.play((
            FadeOut::new(title).shift(UP),
            FadeOut::new(euler).shift(DOWN),
        ))?;
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let config = Config::medium();
    let mut scene = Scene::build(&HelloMath, config.clone())?;
    std::fs::create_dir_all("out")?;
    VideoExporter::render_to_mp4(&mut scene, "out/hello_math.mp4", &config)?;
    println!(
        "Rendered {:.1}s at {} fps to out/hello_math.mp4",
        scene.total_duration(),
        config.fps
    );
    Ok(())
}
