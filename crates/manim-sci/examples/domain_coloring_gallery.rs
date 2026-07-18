//! **Reading a complex function by its colours** — three domain-coloring panels
//! side by side.
//!
//! Domain coloring paints each point `z` of the plane with the *hue* of
//! `arg f(z)`, so the full colour wheel appears exactly where `f` winds. Around
//! a simple **zero** the hue cycles once counter-clockwise; around a simple
//! **pole** it cycles once the other way; the number of wheels is the order.
//! Equivalently, the argument principle `(1/2πi)∮ f′/f = Z − P` is being read
//! straight off the picture.
//!
//! - **Left**, `f(z) = (z² − 1)/(z² + 1)`: zeros at `z = ±1` (green dots), poles
//!   at `z = ±i` (red dots) — four hue wheels, two of each handedness.
//! - **Centre**, `f(z) = sin 2z`: zeros at `z = kπ/2`, a row of identical wheels
//!   marching along the real axis, with `|f|` blowing up off it.
//! - **Right**, `f(z) = log z`: a single zero at `z = 1` and no poles, but the
//!   hue jumps discontinuously across the negative real axis — the principal
//!   **branch cut** (dashed), where `arg` wraps from `+π` to `−π`.
//!
//! Each panel samples its own world rectangle, so the function is pre-shifted by
//! the panel centre and the picture is genuinely `f` about that panel's origin.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-sci --example domain_coloring_gallery --features render-examples
//! ```
//!
//! Frames land in `out/domain_coloring_gallery/frame_NNNNN.png`.

use manim_core::animations::Create;
use manim_core::prelude::*;
use manim_fields::complex::Complex;
use manim_fields::field::ComplexField;
use manim_sci::complex_viz::{branch_cut, zeros_poles_markers};
use manim_sci::material_quad::MaterialQuad;

/// Half-width of each square panel (three of these plus gaps fit in 14.2 units).
const R: f64 = 2.2;
/// Panel centres on the x-axis.
const CX: [f64; 3] = [-4.8, 0.0, 4.8];

/// `sin w`, built from `exp` (the [`Complex`] type has no trig of its own):
/// `sin w = (e^{iw} − e^{−iw}) / 2i`.
fn csin(w: Complex) -> Complex {
    let iw = Complex::new(-w.im, w.re);
    (iw.exp() - (-iw).exp()) / Complex::new(0.0, 2.0)
}

/// A domain-coloring panel of `f`, drawn in the square of half-width [`R`] about
/// `cx`. The sampled world coordinate is shifted back to the origin first, so
/// the panel shows `f` itself rather than a translate of it.
fn panel(scene: &mut Scene, cx: f64, f: impl Fn(Complex) -> Complex + Send + Sync + 'static) {
    let field = ComplexField::new(move |z| f(z - Complex::real(cx)));
    MaterialQuad::domain_coloring([cx - R, cx + R], [-R, R], (256, 256), &field)
        .add_to(scene.state_mut());
}

/// Scene builder for the `domain_coloring_gallery` gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        // (z² − 1)/(z² + 1): two zeros and two poles, all of order one.
        panel(scene, CX[0], |z| {
            (z * z - Complex::one()) / (z * z + Complex::one())
        });
        // sin 2z: zeros every π/2 along the real axis.
        panel(scene, CX[1], |z| csin(z.scale(2.0)));
        // log z: one zero at 1, and a branch cut down the negative real axis.
        panel(scene, CX[2], |z| z.ln());

        // Markers live in scene coordinates, so shift each root by its panel centre.
        let shift = |cx: f64, re: f64, im: f64| Complex::new(re + cx, im);
        let m0 = zeros_poles_markers(
            scene.state_mut(),
            &[shift(CX[0], 1.0, 0.0), shift(CX[0], -1.0, 0.0)],
            &[shift(CX[0], 0.0, 1.0), shift(CX[0], 0.0, -1.0)],
        );
        let m1 = zeros_poles_markers(
            scene.state_mut(),
            &[
                shift(CX[1], 0.0, 0.0),
                shift(CX[1], std::f64::consts::FRAC_PI_2, 0.0),
                shift(CX[1], -std::f64::consts::FRAC_PI_2, 0.0),
            ],
            &[],
        );
        let m2 = zeros_poles_markers(scene.state_mut(), &[shift(CX[2], 1.0, 0.0)], &[]);
        // The dashed principal cut, from the panel edge in to the log singularity.
        branch_cut(
            scene.state_mut(),
            shift(CX[2], -R, 0.0),
            shift(CX[2], 0.0, 0.0),
        );

        // A short reveal of the annotations over the (static) coloured panels.
        scene.wait(0.6);
        scene.play((
            Create::new(m0).run_time(1.6),
            Create::new(m1).run_time(1.6),
            Create::new(m2).run_time(1.6),
        ))?;
        scene.wait(1.2);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/domain_coloring_gallery",
    )?;
    println!("Rendered frames to out/domain_coloring_gallery");
    Ok(())
}
