//! **Fourier epicycles** — a chain of rotating circles drawing a closed curve.
//!
//! Any closed plane curve is a periodic complex function `f(t)`, and any such
//! function is a sum of pure rotations,
//!
//! `f(t) = Σₖ cₖ e^{2πikt}`.
//!
//! Each term is literally a circle: radius `|cₖ|`, starting phase `arg cₖ`,
//! spinning `k` times per period. Stack them tip to tail, largest first, and the
//! pen at the end of the last arm retraces the original curve.
//!
//! The curve here is a heart, `x = 16sin³t`, `y = 13cos t − 5cos 2t − 2cos 3t −
//! cos 4t` (scaled down), sampled 512 times and FFT'd; the chain draws the
//! **20 largest** of those 512 coefficients — the reconstruction error at 20
//! terms is already well under a tenth of a scene unit, which is why the traced
//! curve and the faint reference curve underneath sit on top of each other.
//!
//! The epicycle count is what makes the point: fewer circles blur the cusp at
//! the top and the point at the bottom (those are the high harmonics), more
//! circles sharpen them. Truncating a Fourier series *is* low-pass filtering the
//! shape.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-signal --example epicycles --features render-examples
//! ```
//!
//! Frames land in `out/epicycles/frame_NNNNN.png`.

use std::f64::consts::TAU;

use manim_core::animations::{Create, FadeIn};
use manim_core::prelude::*;
use manim_fields::Complex;
use manim_signal::fourier::{EpicycleChain, FourierSeries};

/// Samples of one period of the target curve.
const N_SAMPLES: usize = 512;
/// Circles in the chain.
const N_TERMS: usize = 20;
/// Seconds for one full trip around the curve.
const PERIOD: f64 = 6.0;

/// The heart curve, scaled into the frame and centred.
fn heart(t: f64) -> Complex {
    let a = TAU * t;
    let x = 16.0 * a.sin().powi(3);
    let y = 13.0 * a.cos() - 5.0 * (2.0 * a).cos() - 2.0 * (3.0 * a).cos() - (4.0 * a).cos();
    Complex::new(x * 0.14, y * 0.14)
}

/// Scene builder for the `epicycles` gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let series = FourierSeries::from_closure(heart, N_SAMPLES);

        // The reference curve the chain is about to reproduce, drawn faint.
        let target: Vec<Point> = (0..=N_SAMPLES)
            .map(|i| {
                let z = heart(i as f64 / N_SAMPLES as f64);
                Point::new(z.re as f32, z.im as f32, 0.0)
            })
            .collect();
        let reference = scene.add(
            VMobject::from_path(manim_math::path::Path::from_corners(&target, true))
                .with_stroke(BLUE, 2.0, 0.35),
        );

        let chain = EpicycleChain::new(series)
            .terms(N_TERMS)
            .period(PERIOD)
            .colors(Color::from_rgb(0.45, 0.55, 0.75), WHITE, YELLOW);
        let ids = chain.add_to(scene.state_mut());
        // A full period of trace at 60 fps, with headroom for the second lap.
        chain.animate(scene.state_mut(), &ids, 8 * 60);

        scene.play(Create::new(reference).run_time(1.5))?;
        scene.play(FadeIn::new(ids.group).run_time(1.0))?;
        // Two laps: the first draws the curve, the second shows it closing on
        // itself exactly.
        scene.wait(2.0 * PERIOD as f32);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/epicycles")?;
    println!("Rendered frames to out/epicycles");
    Ok(())
}
