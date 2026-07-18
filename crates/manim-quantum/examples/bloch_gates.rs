//! `H·Z·H = X`, watched as a sequence of rotations of the Bloch sphere.
//!
//! A pure qubit `|ψ⟩ = cos(θ/2)|0⟩ + e^{iφ} sin(θ/2)|1⟩` is a unit vector in ℝ³,
//! and every single-qubit gate is a *rotation* of that sphere: `X`, `Y`, `Z` are
//! π turns about `x̂`, `ŷ`, `ẑ`, and `H` is a π turn about the diagonal axis
//! `(x̂ + ẑ)/√2`. Global phase — the one thing that distinguishes `HZH` from `X`
//! as 2×2 matrices — is exactly what the Bloch picture quotients out, so on the
//! sphere the identity is *exact* in SO(3).
//!
//! The scene starts at `|0⟩` (the north pole `+ẑ`) and traces three π arcs:
//!
//! - `H` swings `+ẑ → +x̂`, i.e. `|0⟩ → |+⟩`, onto the equator;
//! - `Z` spins the equator by π, `+x̂ → −x̂`, i.e. `|+⟩ → |−⟩`;
//! - `H` swings `−x̂ → −ẑ`, landing on `|1⟩`.
//!
//! North pole to south pole — precisely what a single `X` would have done. The
//! reason is conjugation: because `H` is a π rotation about the bisector of `x̂`
//! and `ẑ`, it *exchanges* those two axes, so `H(Z)H` is the same rotation as
//! `Z` but performed about `x̂` instead — which is `X`. Blue marks `ẑ`, red `x̂`;
//! the traced arc is green for each `H` and orange for the `Z`.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-quantum --example bloch_gates --features render-examples
//! ```
//!
//! Frames land in `out/bloch_gates/frame_NNNNN.png`.

use std::f64::consts::{FRAC_1_SQRT_2, PI as PI64};

use glam::{DQuat, DVec3};

use manim_core::prelude::*;

use manim_quantum::bloch::{BlochSphere, Gate};

/// Sub-steps used to sweep one π gate, and the hold between sub-steps.
const STEPS: usize = 32;
/// Seconds per sub-step (one gate therefore runs `STEPS * DT` ≈ 1.6 s).
const DT: f32 = 0.05;

/// A Bloch vector as a scene-space point (the sphere has unit radius).
fn pt(v: DVec3) -> Point {
    Point::new(v.x as f32, v.y as f32, v.z as f32)
}

/// Sweeps the state arrow through `gate`'s π rotation about `axis`, laying down
/// the great-circle arc it traces.
///
/// The axis is passed explicitly because [`manim_quantum::bloch::gate_rotation`]
/// exposes the finished SO(3) matrix, not its axis–angle form — and manim-core's
/// `Rotate` animation only spins about `OUT`, so an arbitrary-axis turn is done
/// by incrementally `rotate_about`-ing the arrow between `wait`s.
fn sweep(
    scene: &mut Scene,
    arrow: MobjectId<VGroup>,
    q: &mut BlochSphere,
    gate: Gate,
    axis: DVec3,
    color: Color,
) {
    let start = q.state();
    let mut prev = pt(start);
    let d = PI64 / STEPS as f64;
    for i in 1..=STEPS {
        // Exact partial rotation of the *initial* vector — no drift accumulates.
        let v = DQuat::from_axis_angle(axis.normalize(), d * i as f64) * start;
        let p = pt(v);
        scene.add(Line::new(prev, p).with_stroke(color, 6.0, 1.0));
        prev = p;
        // glam's from_axis_angle and manim-math's rotation_matrix share the same
        // right-handed convention, so arrow and arc stay locked together.
        scene
            .state_mut()
            .rotate_about(arrow, d as f32, ORIGIN, pt(axis));
        scene.rotate_camera(TAU / 480.0); // gentle drift, ~1/5 turn overall
        scene.wait(DT);
    }
    q.apply_gate(gate); // re-sync the state exactly at the end of the gate
}

/// A qubit driven through `H`, `Z`, `H` — the long way round to `X`.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(70_f32.to_radians(), -60_f32.to_radians());

        // Wireframe sphere: three great circles rather than a solid mesh, so the
        // state arrow stays visible from every angle.
        for axis in [OUT, RIGHT, UP] {
            let c = scene.add(Circle::new().radius(1.0).with_stroke(WHITE, 2.0, 0.4));
            scene.state_mut().rotate_about(c, PI / 2.0, ORIGIN, axis);
        }
        // The two axes the Hadamard exchanges.
        scene.add(Line::new(-1.35 * OUT, 1.35 * OUT).with_stroke(BLUE, 3.0, 0.8));
        scene.add(Line::new(-1.35 * RIGHT, 1.35 * RIGHT).with_stroke(RED, 3.0, 0.8));

        let mut q = BlochSphere::new(); // |0⟩ at the north pole
        let arrow = Arrow3D::of(scene.state_mut(), ORIGIN, OUT);

        // H's rotation axis is the bisector of x̂ and ẑ; Z's is ẑ itself.
        let h_axis = DVec3::new(FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2);
        sweep(scene, arrow, &mut q, Gate::H, h_axis, GREEN);
        scene.wait(0.3);
        sweep(scene, arrow, &mut q, Gate::Z, DVec3::Z, ORANGE);
        scene.wait(0.3);
        sweep(scene, arrow, &mut q, Gate::H, h_axis, GREEN);
        scene.wait(0.6);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/bloch_gates")?;
    println!("Rendered frames to out/bloch_gates");
    Ok(())
}
