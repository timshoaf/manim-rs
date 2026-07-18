//! A 1-D Gaussian wavepacket tunneling through a rectangular barrier, drawn as a
//! phase-hue `|ψ|²` curve that evolves in real time.
//!
//! The packet (mean momentum `k₀ > 0`) drifts in from the left, partly reflects
//! off and partly tunnels through the barrier (the translucent band), and
//! separates into a transmitted and a reflected lobe. The density curve is drawn
//! as short segments colored by the local phase, so the fast carrier
//! oscillations sweep through the hue wheel as the packet moves.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-quantum --example wavepacket_barrier --features render-examples
//! ```
//!
//! Frames land in `out/wavepacket_barrier/frame_NNNNN.png`.

use manim_core::animation::Animation;
use manim_core::prelude::*;
use manim_math::path::Path;

use manim_quantum::wavefunction::{phase_color, PlotTransform};
use manim_quantum::wells::{TunnelingParams, TunnelingScene};

/// Display decimation: draw every `STRIDE`-th grid point (the full grid is finer
/// than the screen needs).
const STRIDE: usize = 4;
/// Total simulated time to animate over.
const SIM_TIME: f64 = 20.0;
/// Wall-clock run time of the animation, in seconds.
const RUN_TIME: f32 = 8.0;

/// Data-space plot placement shared by the geometry and the animation.
fn transform() -> PlotTransform {
    PlotTransform::new(Point::new(0.0, -2.5, 0.0), 0.12, 20.0)
}

/// Indices of the grid points we actually draw.
fn display_indices(n: usize) -> Vec<usize> {
    (0..n).step_by(STRIDE).collect()
}

/// Animation that evolves the tunneling packet and repaints its phase-hue curve.
struct WavepacketEvolve {
    scene: Option<TunnelingScene>,
    segments: Vec<MobjectId<Line>>,
    indices: Vec<usize>,
    tf: PlotTransform,
}

impl WavepacketEvolve {
    /// Rewrites each segment's endpoints and phase color from the current state.
    fn repaint(&self, state: &mut SceneState) {
        let Some(ts) = self.scene.as_ref() else {
            return;
        };
        let wf = ts.wavefunction_snapshot();
        let dens = wf.probability_density();
        for (seg, w) in self.segments.iter().zip(self.indices.windows(2)) {
            let (i, j) = (w[0], w[1]);
            let a = self.tf.map(wf.xs[i], dens[i]);
            let b = self.tf.map(wf.xs[j], dens[j]);
            let color = phase_color((wf.psi[i] + wf.psi[j]).arg());
            let m = state.get_mut(*seg);
            m.data_mut().path = Path::from_corners(&[a, b], false);
            m.data_mut().style.stroke_color = Some(color);
            m.data_mut().bump_generation();
        }
    }
}

impl Animation for WavepacketEvolve {
    fn begin(&mut self, _state: &mut SceneState) {
        // Re-seek safe: rebuild the simulation from scratch.
        self.scene = Some(TunnelingScene::new(TunnelingParams::default()));
    }

    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let target = alpha as f64 * SIM_TIME;
        if let Some(ts) = self.scene.as_mut() {
            ts.evolve_to(target);
        }
        self.repaint(state);
    }

    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }

    fn duration(&self) -> f32 {
        RUN_TIME
    }
}

/// The scene: baseline, barrier band, and the evolving phase-hue density curve.
/// Scene builder for the wavepacket-barrier gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let tf = transform();
        let ts = TunnelingScene::new(TunnelingParams::default());
        let indices = display_indices(ts.xs().len());

        // Baseline axis at |ψ|² = 0.
        let xs = ts.xs();
        let (x_lo, x_hi) = (xs[0], xs[xs.len() - 1]);
        scene.add(Line::new(tf.map(x_lo, 0.0), tf.map(x_hi, 0.0)).with_stroke(WHITE, 2.0, 0.6));

        // Translucent band marking the barrier region.
        let (bl, br) = (ts.barrier_left(), ts.barrier_right());
        let band_top = 0.3; // data-value units (well above the packet peak).
        let band = [
            tf.map(bl, 0.0),
            tf.map(br, 0.0),
            tf.map(br, band_top),
            tf.map(bl, band_top),
        ];
        scene.add(
            VMobject::from_path(Path::from_corners(&band, true))
                .with_fill(ORANGE, 0.35)
                .with_stroke(ORANGE, 2.0, 0.8),
        );

        // One Line per drawn interval, initialized from the t = 0 state.
        let wf = ts.wavefunction_snapshot();
        let dens = wf.probability_density();
        let mut segments = Vec::new();
        for w in indices.windows(2) {
            let (i, j) = (w[0], w[1]);
            let a = tf.map(wf.xs[i], dens[i]);
            let b = tf.map(wf.xs[j], dens[j]);
            let color = phase_color((wf.psi[i] + wf.psi[j]).arg());
            segments.push(scene.add(Line::new(a, b).with_stroke(color, 4.0, 1.0)));
        }

        scene.play(WavepacketEvolve {
            scene: None,
            segments,
            indices,
            tf,
        })?;
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/wavepacket_barrier",
    )?;
    println!("Rendered frames to out/wavepacket_barrier");
    Ok(())
}
