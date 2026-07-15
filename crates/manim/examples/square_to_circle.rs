//! The canonical first manim scene: a blue square rotates, morphs into a red
//! circle, then fades out — rendered offline to a PNG frame sequence.
//!
//! Port of the manim CE quickstart `SquareToCircle`. Run with:
//!
//! ```sh
//! cargo run -p manim --example square_to_circle
//! ```
//!
//! Frames land in `out/square_to_circle/frame_NNNN.png`.

use manim::prelude::*;
use manim::render::OffscreenRenderer;

struct SquareToCircle;

impl SceneBuilder for SquareToCircle {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let square = scene.add(
            Square::new()
                .with_fill(BLUE, 0.7)
                .with_stroke(WHITE, 4.0, 1.0),
        );
        scene.play(Create::new(square))?;
        scene.play(square.animate().rotate(PI / 4.0))?;
        scene.play(TransformInto::new(
            square,
            Circle::new()
                .with_fill(RED, 0.7)
                .with_stroke(WHITE, 4.0, 1.0),
        ))?;
        scene.wait(0.5);
        scene.play(FadeOut::new(square).shift(DOWN))?;
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let config = Config::low();
    let mut scene = Scene::build(&SquareToCircle, config)?;
    let mut renderer = OffscreenRenderer::new(scene.config())?;

    let dir = std::path::Path::new("out/square_to_circle");
    std::fs::create_dir_all(dir)?;

    let mut count = 0usize;
    let frames: Vec<_> = scene.frames().collect();
    for (i, (_t, list)) in frames.iter().enumerate() {
        renderer.render_to_png(list, dir.join(format!("frame_{i:04}.png")))?;
        count += 1;
    }

    println!(
        "Rendered {count} frames ({:.1}s at {} fps) to {}",
        scene.total_duration(),
        scene.config().fps,
        dir.display()
    );
    println!("Make a video with: ffmpeg -framerate {} -i {}/frame_%04d.png -pix_fmt yuv420p square_to_circle.mp4", scene.config().fps, dir.display());
    Ok(())
}
