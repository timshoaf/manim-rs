//! **The Van der Pol oscillator** — a phase portrait assembled piece by piece.
//!
//! `ẍ − μ(1 − x²)ẋ + x = 0` with `μ = 1`, written as the planar system
//! `ẋ = y`, `ẏ = μ(1 − x²)y − x`. The damping coefficient `−μ(1 − x²)` is
//! *negative* for `|x| < 1` and positive for `|x| > 1`: small oscillations are
//! pumped up, large ones are damped down, and everything is squeezed onto one
//! closed orbit in between. That orbit is a **limit cycle** — not one of a
//! family of nested loops, as in a conservative system, but a single isolated
//! attractor.
//!
//! The scene builds the standard reading of a portrait, in order:
//!
//! 1. **The direction field** — normalized arrows, so direction is legible and
//!    speed is not confused with it.
//! 2. **The nullclines** — green `ẋ = 0` (the axis `y = 0`, where the flow runs
//!    vertically) and purple `ẏ = 0` (the cubic `y = x/(μ(1 − x²))`, where it
//!    runs horizontally). They cross exactly once, at the origin.
//! 3. **The equilibrium** — that crossing, marked by class. The Jacobian there
//!    is `[[0, 1], [−1, μ]]`: trace `μ = 1 > 0`, determinant 1, so
//!    `τ² − 4Δ = −3 < 0` and orbits spiral *out*.
//! 4. **The limit cycle** — located by a Poincaré return map on the positive
//!    x-axis, not drawn by hand. Measured amplitude `≈ 2.009`, period `≈ 6.663`.
//! 5. **Two orbits**, one seeded inside the cycle and one outside, winding onto
//!    it from opposite sides — which is what "attracting" means.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-dynamics --example van_der_pol --features render-examples
//! ```
//!
//! Frames land in `out/van_der_pol/frame_NNNNN.png`.

use manim_core::animations::{Create, FadeIn};
use manim_core::prelude::*;
use manim_dynamics::cycles::{add_cycle, find_limit_cycle, Section};
use manim_dynamics::equilibria::{add_markers, find_equilibria};
use manim_dynamics::nullclines::add_nullclines;
use manim_dynamics::phase::PhasePortrait;
use manim_dynamics::VanDerPol;

/// The x window, in data units.
const X: [f32; 3] = [-3.2, 3.2, 1.0];
/// The y window, in data units.
const Y: [f32; 3] = [-3.2, 3.2, 1.0];

/// Scene builder for the `van_der_pol` gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let system = VanDerPol { mu: 1.0 };
        let axes = Axes::with_lengths(X, Y, 6.4, 6.4);
        let bounds = ((X[0] as f64, X[1] as f64), (Y[0] as f64, Y[1] as f64));
        let axes_id = scene.add(axes.clone());

        // 1. The direction field.
        let portrait = PhasePortrait::new(bounds.0, bounds.1)
            .grid(17)
            .streams(0.01, 1400);
        let arrows = portrait.add_arrows(scene.state_mut(), &axes, &system);

        // 2. Nullclines: ẋ = 0 in green, ẏ = 0 in purple.
        let (nx, ny) = add_nullclines(scene.state_mut(), &axes, &system, 200);

        // 3. The one equilibrium, marked by class (a red ring: unstable spiral).
        let eqs = find_equilibria(&system, bounds.0, bounds.1, 15);
        let markers = add_markers(scene.state_mut(), &axes, &eqs, 0.11);

        // 4. The limit cycle, located by return map rather than drawn by eye.
        let cycle = find_limit_cycle(&system, &Section::positive_x_axis(), 1.0, 80, 1e-9, 1e-3)
            .expect("Van der Pol has an attracting limit cycle at μ = 1");
        let cycle_id = add_cycle(scene.state_mut(), &axes, &cycle, RED);

        // 5. Orbits from inside and outside, both winding onto it.
        let orbits = portrait.add_streamlines(
            scene.state_mut(),
            &axes,
            &system,
            &[[0.08, 0.0], [3.0, 2.4]],
        );

        scene.play(Create::new(axes_id).run_time(0.8))?;
        scene.play(FadeIn::new(arrows).run_time(1.2))?;
        scene.play(vec![
            Create::new(nx).run_time(1.2),
            Create::new(ny).run_time(1.2),
        ])?;
        scene.play(FadeIn::new(markers).run_time(0.6))?;
        scene.play(Create::new(orbits).run_time(3.0))?;
        scene.play(Create::new(cycle_id).run_time(2.0))?;
        scene.wait(1.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/van_der_pol")?;
    println!("Rendered frames to out/van_der_pol");
    Ok(())
}
