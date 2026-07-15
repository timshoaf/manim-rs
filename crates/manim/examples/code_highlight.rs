//! A syntax-highlighted `Code` block (the FE-119 feature) revealed with
//! `FadeIn`. Requires the `code` cargo feature:
//!
//! ```sh
//! cargo run -p manim --example code_highlight --features code
//! ```
//!
//! Frames land in `out/code_highlight/frame_NNNNN.png`.
//!
//! API notes vs CE:
//! - `Code` is gated behind the `code` feature and is reached as
//!   `manim::text::Code` (it is not in the prelude). `add_to` returns the
//!   `VGroup` of the background rect + colored monospace glyphs (+ optional line
//!   numbers).
//! - Highlighting uses syntect's bundled themes/syntaxes; `base16-ocean.dark` is
//!   the default.

use manim::prelude::*;
use manim::text::Code;

const SNIPPET: &str = r#"// gcd via Euclid's algorithm
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}"#;

/// Scene builder for this gallery example.
pub struct CodeHighlight;

impl SceneBuilder for CodeHighlight {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let code = Code::new(SNIPPET, Some("rust"))
            .with_line_numbers()
            .add_to(scene.state_mut());

        scene.play(FadeIn::new(code).run_time(1.5))?;
        scene.wait(1.0);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&CodeHighlight, Config::low())?;
    manim::render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/code_highlight")?;
    println!("Rendered frames to out/code_highlight");
    Ok(())
}
