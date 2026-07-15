//! The FE-99 substring-isolation showcase: write the quadratic formula, recolor
//! the discriminant in place with `set_color_by_tex`, then `Indicate` exactly
//! those glyphs via `get_parts_by_tex`.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example math_isolation
//! ```
//!
//! Frames land in `out/math_isolation/frame_NNNNN.png`.
//!
//! API notes vs CE:
//! - `MathTex::{get_parts_by_tex, set_color_by_tex}` are associated functions
//!   taking `(scene_state, id, tex, ..)`, not `&self` methods.
//! - `set_color_by_tex` mutates the glyphs immediately (it is a state edit, not
//!   an animation); the following `wait` snapshots the recolored formula.
//! - `Indicate` lives in `manim::animations`, not the prelude. A `Vec` of
//!   animations plays them together, so we indicate all six discriminant glyphs
//!   at once.

use manim::animations::Indicate;
use manim::prelude::*;

/// Scene builder for this gallery example.
pub struct MathIsolation;

impl SceneBuilder for MathIsolation {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let formula = MathTex::new(r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}")?
            .font_size(60.0)
            .add_to(scene.state_mut());

        scene.play(Write::new(formula).run_time(2.0))?;
        scene.wait(0.3);

        // Recolor the discriminant `b^2 - 4ac` (6 glyphs, one occurrence).
        let recolored = MathTex::set_color_by_tex(scene.state_mut(), formula, "b^2 - 4ac", YELLOW)?;
        debug_assert_eq!(recolored, 6);
        scene.wait(0.6);

        // Indicate exactly those glyphs.
        let parts = MathTex::get_parts_by_tex(scene.state(), formula, "b^2 - 4ac");
        let flashes: Vec<Indicate> = parts
            .iter()
            .flatten()
            .map(|&glyph| Indicate::new(glyph).color(RED).run_time(1.2))
            .collect();
        scene.play(flashes)?;
        scene.wait(0.3);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&MathIsolation, Config::low())?;
    manim::render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/math_isolation")?;
    println!("Rendered frames to out/math_isolation");
    Ok(())
}
