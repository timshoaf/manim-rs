//! **Kepler's first two laws**, integrated symplectically from Newton's inverse
//! square law alone.
//!
//! With `G = M = m = 1` the force on the planet is
//!
//! `F(r) = −r̂ / |r|² = −r / |r|³`,  `H = |p|²/2 − 1/|r|`,
//!
//! a separable Hamiltonian, so Yoshida's 4th-order symplectic composition keeps
//! the energy bounded and the orbits closed over a full revolution.
//!
//! Three planets are released from the same point `r₀ = (1, 0)` with different
//! transverse speeds `v`. The eccentricity is `e = |v² − 1|` and the semi-major
//! axis `a = 1/(2 − v²)`, so `v = 1` gives a circle and larger `v` progressively
//! stretched ellipses — yet **every** orbit passes through the same white dot at
//! the origin, which sits at a *focus*, never at the centre. That is Kepler I.
//!
//! Over the outermost orbit three shaded wedges are swept over equal *time*
//! intervals: near perihelion the wedge is short and fat, near aphelion long and
//! thin, and the two areas match. That is Kepler II, `dA/dt = L/2 = const`.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-sci --example kepler_orbits --features render-examples
//! ```
//!
//! Frames land in `out/kepler_orbits/frame_NNNNN.png`.

use manim_core::animations::{Create, FadeIn};
use manim_core::prelude::*;
use manim_fields::integrate::yoshida4;
use manim_math::path::Path;

/// Samples per orbit (also the number of symplectic steps taken).
const N: usize = 240;
/// Rightward shift, in scene units, that centres the family of ellipses.
const SHIFT: f32 = 2.0;

/// Orbital-plane position → scene point (the sun sits at the origin, shifted).
fn orbit_point(x: f64, y: f64) -> Point {
    Point::new(x as f32 + SHIFT, y as f32, 0.0)
}

/// One closed orbit: `N` yoshida4 steps covering exactly one period
/// `T = 2π a^{3/2}` for the launch speed `v` at `r₀ = (1, 0)`.
fn orbit(v: f64) -> Vec<Point> {
    // Vis-viva at r = 1: v² = 2 − 1/a  ⟹  a = 1/(2 − v²).
    let a = 1.0 / (2.0 - v * v);
    let dt = std::f64::consts::TAU * a.powf(1.5) / N as f64;
    // Newtonian gravity F = −r/|r|³ (unit G·M).
    let force = |q: &[f64]| {
        let r3 = (q[0] * q[0] + q[1] * q[1]).sqrt().powi(3);
        vec![-q[0] / r3, -q[1] / r3]
    };
    let (mut q, mut p) = (vec![1.0, 0.0], vec![0.0, v]);
    (0..N)
        .map(|_| {
            let next = yoshida4(&force, &q, &p, 1.0, dt, 1);
            q = next.0;
            p = next.1;
            orbit_point(q[0], q[1])
        })
        .collect()
}

/// Scene builder for the `kepler_orbits` gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        // v = 1 is the circular orbit; 1.2 and 1.3 give e ≈ 0.44 and 0.69.
        let speeds = [(1.0, BLUE), (1.2, GREEN), (1.3, ORANGE)];
        let mut traces = Vec::new();
        let mut outer = Vec::new();
        for (v, color) in speeds {
            let pts = orbit(v);
            // Closed spline: after one full period the orbit returns to r₀.
            let id = scene.add(
                VMobject::from_path(Path::from_smooth_anchors(&pts, true))
                    .with_stroke(color, 4.0, 1.0),
            );
            traces.push(Create::new(id).run_time(4.0));
            outer = pts;
        }

        // Kepler II: wedges swept over equal step counts ⇒ equal times ⇒ equal
        // areas, however different their shapes. Steps 0, N/3 and 2N/3.
        let span = N / 12;
        let mut wedges = Vec::new();
        for start in [0, N / 3, 2 * N / 3] {
            let mut verts = vec![orbit_point(0.0, 0.0)]; // apex at the focus
            verts.extend_from_slice(&outer[start..start + span]);
            wedges.push(scene.add(Polygon::new(&verts).with_fill(YELLOW, 0.35)));
        }

        // The sun: one dot at the common focus of all three ellipses.
        scene.add(Dot::at(orbit_point(0.0, 0.0)).with_fill(WHITE, 1.0));

        scene.play(traces)?;
        scene.play(wedges.into_iter().map(FadeIn::new).collect::<Vec<_>>())?;
        scene.wait(1.0);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/kepler_orbits")?;
    println!("Rendered frames to out/kepler_orbits");
    Ok(())
}
