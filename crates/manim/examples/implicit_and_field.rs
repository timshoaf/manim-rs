//! An implicit lemniscate curve drawn on over a faded rotational
//! `ArrowVectorField` backdrop.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example implicit_and_field
//! ```
//!
//! Frames land in `out/implicit_and_field/frame_NNNNN.png`.
//!
//! API notes vs CE:
//! - `ArrowVectorField::new(|p| ..)` takes a `Point -> Point` field; a single
//!   muted `with_colors` entry makes it a recessive backdrop.
//! - `plot_implicit_curve(|x, y| f, resolution)` lives on `Axes`/`CoordSystem`
//!   and traces `f(x, y) = 0` via marching squares.

use manim::color::GREY_B;
use manim::prelude::*;

/// Scene builder for this gallery example.
pub struct ImplicitAndField;

impl SceneBuilder for ImplicitAndField {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        // A faded rotational field: F(x, y) = (-y, x).
        let field = ArrowVectorField::new(|p: Point| Point::new(-p.y, p.x, 0.0))
            .with_x_range([-4.0, 4.0, 0.8])
            .with_y_range([-4.0, 4.0, 0.8])
            .with_colors(vec![GREY_B])
            .add_to(scene.state_mut());

        // Lemniscate of Bernoulli: (x^2 + y^2)^2 = a^2 (x^2 - y^2), a = 2.5.
        let axes = Axes::new([-4.0, 4.0, 1.0], [-4.0, 4.0, 1.0]);
        let curve = axes
            .plot_implicit_curve(
                |x, y| {
                    let r2 = x * x + y * y;
                    r2 * r2 - 6.25 * (x * x - y * y)
                },
                Some(140),
            )
            .with_stroke(YELLOW, 4.0, 1.0);
        let curve = scene.add(curve);

        scene.play(FadeIn::new(field).run_time(1.0))?;
        scene.play(Create::new(curve).run_time(2.0))?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&ImplicitAndField, Config::low())?;
    manim::render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/implicit_and_field",
    )?;
    println!("Rendered frames to out/implicit_and_field");
    Ok(())
}
