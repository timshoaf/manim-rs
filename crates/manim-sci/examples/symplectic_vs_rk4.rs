//! **Why symplectic integrators exist** — the same pendulum, integrated two ways,
//! drawn in phase space.
//!
//! The pendulum is the separable Hamiltonian
//!
//! `H(q, p) = p²/2 − cos q`,  `q̇ = ∂H/∂p = p`,  `ṗ = −∂H/∂q = −sin q`,
//!
//! so its exact trajectories are level sets `H = const` — closed lens-shaped
//! loops for `H < 1`. Both curves below start from the same near-separatrix
//! state `(q, p) = (3, 0)` and take the same step `h = 0.7`.
//!
//! - **Teal** is Yoshida's 4th-order symplectic composition. It does not conserve
//!   `H` exactly, but it conserves a nearby *shadow* Hamiltonian exactly, so its
//!   energy error stays bounded forever: the orbit retraces one closed loop.
//! - **Gold** is classic RK4 — same formal order, no symplectic structure. Its
//!   energy error accumulates secularly, so the orbit spirals inward, shedding
//!   ~17% of its amplitude in a dozen swings.
//!
//! The lesson: for long-time Hamiltonian dynamics, *structure* beats *order*.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-sci --example symplectic_vs_rk4 --features render-examples
//! ```
//!
//! Frames land in `out/symplectic_vs_rk4/frame_NNNNN.png`.

use manim_core::animations::Create;
use manim_core::prelude::*;
use manim_fields::integrate::{rk4_step, yoshida4};
use manim_math::path::Path;

// manim-color is not a direct dependency of manim-sci, so the two catalog
// shades used here (TEAL_C, GOLD_C) are spelled out in linear RGB.
/// Symplectic (Yoshida 4th order) trace colour.
const TEAL: Color = Color::from_rgb(0.107, 0.631, 0.451);
/// Runge–Kutta 4 trace colour.
const GOLD: Color = Color::from_rgb(0.871, 0.413, 0.114);

/// Integration step — large enough that RK4's energy drift reads on screen.
const H: f64 = 0.7;
/// Steps traced (≈ 11 swings of the pendulum).
const STEPS: usize = 200;
/// Initial angle: released from rest just inside the separatrix.
const Q0: f64 = 3.0;

/// Phase-space `(q, p)` → scene point; independent scales fill the 16:9 frame.
fn phase_point(q: f64, p: f64) -> Point {
    Point::new(q as f32 * 1.9, p as f32 * 1.7, 0.0)
}

/// Turns a sampled trajectory into an open, smooth, stroked curve.
fn trace(scene: &mut Scene, pts: &[Point], color: Color) -> MobjectId<VMobject> {
    scene.add(
        VMobject::from_path(Path::from_smooth_anchors(pts, false)).with_stroke(color, 3.0, 1.0),
    )
}

/// Scene builder for the `symplectic_vs_rk4` gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        // Force F(q) = −∂V/∂q = −sin q (unit mass, unit gravity) for the
        // symplectic side, which needs the separable (q, p) splitting.
        let force = |q: &[f64]| vec![-q[0].sin()];
        // The identical system flattened to first order, y = (q, p), for RK4.
        let f = |_t: f64, y: &[f64]| vec![y[1], -y[0].sin()];

        // yoshida4 returns only the final state, so step it one step at a time
        // to record the whole orbit.
        let (mut q, mut p) = (vec![Q0], vec![0.0]);
        let mut sympl = Vec::with_capacity(STEPS);
        for _ in 0..STEPS {
            let next = yoshida4(&force, &q, &p, 1.0, H, 1);
            q = next.0;
            p = next.1;
            sympl.push(phase_point(q[0], p[0]));
        }

        // RK4 from the identical initial condition and step size.
        let mut y = vec![Q0, 0.0];
        let mut rk = Vec::with_capacity(STEPS);
        for i in 0..STEPS {
            y = rk4_step(&f, i as f64 * H, &y, H);
            rk.push(phase_point(y[0], y[1]));
        }

        // Gold = RK4 (an inward spiral) underneath; teal = symplectic (a single
        // closed loop) drawn on top of it, so the drift shows as gold left behind.
        let b = trace(scene, &rk, GOLD);
        let a = trace(scene, &sympl, TEAL);
        // The shared starting state, at the right-hand turning point p = 0.
        scene.add(Dot::at(phase_point(Q0, 0.0)).with_fill(WHITE, 1.0));

        // Trace both at once so the divergence accumulates in front of the viewer.
        scene.play((Create::new(a).run_time(5.0), Create::new(b).run_time(5.0)))?;
        scene.wait(1.0);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/symplectic_vs_rk4",
    )?;
    println!("Rendered frames to out/symplectic_vs_rk4");
    Ok(())
}
