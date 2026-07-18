//! **Level sets and the gradient** — a contoured heatmap of a scalar field with
//! its exact gradient drawn on top.
//!
//! The field is a saddle with two Gaussian bumps sitting in it,
//!
//! `f(x, y) = 1.5·e^{−((x+1.6)² + y²)} − 1.2·e^{−((x−1.6)² + (y−0.6)²)} + 0.12·(x² − y²)`,
//!
//! rendered as a Coolwarm heatmap with iso-contours `f = const` overlaid every
//! `0.3`. Two facts the picture is meant to make obvious:
//!
//! 1. **Contours are level sets.** Walking along one changes nothing, so the
//!    directional derivative along a contour vanishes: `∇f · t̂ = 0`. The white
//!    arrows, drawn as `∇f` at their basepoints, therefore cross every contour at
//!    a right angle — never along one.
//! 2. **Crowded contours mean a steep gradient.** Contour spacing is `Δf/|∇f|`,
//!    so on the flanks of the bumps the lines pile up and the arrows are long,
//!    while near the saddle and the bump summits (`∇f = 0`) the lines spread out
//!    and the arrows shrink to nothing.
//!
//! The gradients are exact, not finite-differenced: the field is written once
//! generically over the AD [`Scalar`] type and `ScalarField::grad` differentiates
//! it forwards through dual numbers.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-sci --example heatmap_contours --features render-examples
//! ```
//!
//! Frames land in `out/heatmap_contours/frame_NNNNN.png`.

use manim_core::animations::{Create, FadeIn};
use manim_core::display::Colormap;
use manim_core::prelude::*;
use manim_fields::ad::Scalar;
use manim_fields::field::{ScalarClosure, ScalarField};
use manim_sci::material_quad::MaterialQuad;

/// The visible world rectangle (also the sampled domain of the quad).
const X: [f64; 2] = [-4.4, 4.4];
const Y: [f64; 2] = [-2.5, 2.5];

/// A saddle carrying one positive and one negative Gaussian bump.
struct Terrain;

impl ScalarClosure for Terrain {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        let (x, y) = (p[0], p[1]);
        // Bump centres at (∓1.6, ·); a Gaussian is e^{−r²} about each.
        let bx = x + S::constant(1.6);
        let by = y;
        let cx = x - S::constant(1.6);
        let cy = y - S::constant(0.6);
        let hot = (-(bx * bx + by * by)).exp().scale(1.5);
        let cold = (-(cx * cx + cy * cy)).exp().scale(1.2);
        // The saddle x² − y² supplies the global structure the bumps sit in.
        hot - cold + (x * x - y * y).scale(0.12)
    }
}

/// Scene builder for the `heatmap_contours` gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let field = ScalarField::from_closure(Terrain);

        // Colormap + iso-contour overlay; 0.3 between successive level sets.
        let quad = MaterialQuad::field_contours(X, Y, (320, 200), &field, Colormap::Coolwarm, 0.3)
            .add_to(scene.state_mut());

        // Gradient arrows on a coarse lattice, skipping the near-flat spots
        // (where an arrowhead would be bigger than the arrow).
        let mut arrows = Vec::new();
        for i in 0..7 {
            for j in 0..4 {
                let x = -3.6 + 1.2 * i as f64;
                let y = -1.8 + 1.2 * j as f64;
                // Exact ∇f via forward-mode AD, not a finite difference.
                let g = field.grad(manim_fields::Point::new(x, y, 0.0));
                let start = Point::new(x as f32, y as f32, 0.0);
                // Scale down so long arrows stay inside the frame; 0.6 units per
                // unit of |∇f|.
                let end = start + Point::new(g.x as f32, g.y as f32, 0.0) * 0.6;
                if (end - start).length() > 0.22 {
                    arrows.push(scene.add(Arrow::new(start, end).with_stroke(WHITE, 3.0, 1.0)));
                }
            }
        }

        scene.play(FadeIn::new(quad).run_time(1.0))?;
        // Grow the gradient field in so the perpendicularity is watched, not found.
        scene.play(
            arrows
                .into_iter()
                .map(|a| Create::new(a).run_time(2.0))
                .collect::<Vec<_>>(),
        )?;
        scene.wait(1.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/heatmap_contours",
    )?;
    println!("Rendered frames to out/heatmap_contours");
    Ok(())
}
