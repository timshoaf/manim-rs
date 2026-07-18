//! The trefoil knot swept as a solid tube, colored by torsion.
//!
//! A knot is an **embedding** `S¹ ↪ ℝ³` — a loop that cannot be untangled into a
//! circle without cutting. The trefoil is the simplest nontrivial one, here the
//! `(2, 3)` torus knot `γ(t) = ((2 + cos 3t)·cos 2t, (2 + cos 3t)·sin 2t, sin 3t)`
//! for `t ∈ [0, 2π]`.
//!
//! Sweeping a circular cross-section along `γ` needs a frame `(n, b)` normal to
//! the tangent at every `t`. The obvious choice, the **Frenet frame**, is defined
//! by `n = γ̈/|γ̈|` and flips discontinuously wherever the curvature `κ → 0`
//! (an inflection), spinning the tube through a spurious half-twist. This tube
//! instead uses a **rotation-minimizing frame**, propagated by the double
//! reflection method (Wang et al. 2008): each ring is the previous one carried
//! along the curve with **zero** rotation about the tangent, so the sweep has no
//! twist beyond what the curve's own geometry demands.
//!
//! The surface is colored by the **torsion** `τ = (γ̇ × γ̈)·γ⃛ / |γ̇ × γ̈|²`, the
//! rate at which the curve escapes its own osculating plane — the trefoil's τ
//! oscillates three times around the loop, once per lobe. A turntable makes the
//! over/under crossings read as genuinely three-dimensional.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-sci --example trefoil_tube --features render-examples
//! ```
//!
//! Frames land in `out/trefoil_tube/frame_NNNNN.png`.

use std::f64::consts::TAU;

use manim_core::display::Colormap;
use manim_core::mesh::Mesh;
use manim_core::prelude::*;
use manim_sci::curveviz::{trefoil, TubeMesh};
use manim_sci::diffgeo::torsion;

/// Rings along the knot and sides per ring.
const N_ALONG: usize = 360;
const N_AROUND: usize = 20;

/// Scene builder for the `trefoil_tube` gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(68_f32.to_radians(), -40_f32.to_radians());

        let curve = trefoil();
        // Rotation-minimizing sweep; `closed` welds the last ring to the first so
        // the loop has no seam.
        let mut mesh = TubeMesh::along_curve(&curve, (0.0, TAU), 0.32, N_ALONG, N_AROUND, true);

        // Torsion sampled once per ring, then auto-ranged into a colormap. The
        // vertex layout is N_ALONG rings of N_AROUND vertices, ring-major.
        let taus: Vec<f64> = (0..N_ALONG)
            .map(|i| torsion(&curve, TAU * i as f64 / (N_ALONG - 1) as f64))
            .collect();
        // τ has sharp spikes, so compress it through a tanh instead of a linear
        // min/max stretch — otherwise the spikes eat the whole colormap.
        let scale = taus.iter().map(|t| t.abs()).sum::<f64>() / taus.len() as f64;
        let colors: Vec<Color> = (0..mesh.positions.len())
            .map(|k| {
                let x = 0.5 + 0.5 * (taus[k / N_AROUND] / scale.max(1e-12)).tanh();
                Colormap::Turbo.sample(x as f32)
            })
            .collect();
        mesh.colors = Some(colors);

        scene.add(Mesh::new(mesh));

        // A full turntable revolution: the three crossings alternate over/under.
        for _ in 0..90 {
            scene.rotate_camera(TAU as f32 / 90.0);
            scene.wait(0.07);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/trefoil_tube")?;
    println!("Rendered frames to out/trefoil_tube");
    Ok(())
}
