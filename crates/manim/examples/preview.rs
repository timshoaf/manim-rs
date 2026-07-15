//! Realtime preview of the canonical `SquareToCircle` scene in a window.
//!
//! Requires the `preview` feature (winit). Run with:
//!
//! ```sh
//! cargo run -p manim --example preview --features preview
//! ```
//!
//! Controls: Space play/pause, ←/→ seek ∓1 s, R restart, Esc quit.

use manim::prelude::*;

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
    // Opens a vsync'd window and plays the scene until you press Esc.
    manim::preview(&SquareToCircle, Config::low())?;
    Ok(())
}
