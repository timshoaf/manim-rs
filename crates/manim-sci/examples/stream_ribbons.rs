//! Stream ribbons in an ABC flow — seeing vorticity, not just direction.
//!
//! The Arnold–Beltrami–Childress flow
//!
//! ```text
//! v(x, y, z) = (A sin z + C cos y,  B sin x + A cos z,  C sin y + B cos x)
//! ```
//!
//! is a **Beltrami field**: `∇×v = v`, so the vorticity is everywhere parallel to
//! the velocity. It is the textbook example of a steady, incompressible flow
//! whose streamlines are chaotic — a maximally "spinning" flow.
//!
//! A stream *tube* shows only where fluid goes. A stream **ribbon** additionally
//! carries a flat cross-section whose orientation is transported along the
//! streamline with the accumulated twist `½ ∫ (∇×v)·t̂ ds` — exactly the rate at
//! which a fluid parcel rotates about its own path. So the ribbon's edge-on/
//! face-on flicker *is* the local vorticity, made directly visible: in this
//! Beltrami field `(∇×v)·t̂ = |v|`, so every ribbon twists continuously, fastest
//! where the flow is fastest. A plain tube would look identical everywhere.
//!
//! Eight seeds keep the picture legible; a turntable resolves the helical
//! braiding of the trajectories.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-sci --example stream_ribbons --features render-examples
//! ```
//!
//! Frames land in `out/stream_ribbons/frame_NNNNN.png`.

use std::f64::consts::TAU;

use glam::DVec3;

use manim_core::animations::Create;
use manim_core::prelude::*;
use manim_fields::ad::Scalar;
use manim_fields::field::{ScalarClosure, ScalarField, VectorField3};
use manim_sci::vector_field_3d::{stream_ribbons, StreamParams};

/// The classic symmetric ABC parameters `A = √3, B = √2, C = 1`.
const A: f64 = 1.732_050_807_568_877_2; // √3
const B: f64 = std::f64::consts::SQRT_2;
const C: f64 = 1.0;

/// One Cartesian component of the ABC velocity field.
struct AbcComponent {
    axis: usize,
}

impl ScalarClosure for AbcComponent {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        match self.axis {
            0 => p[2].sin().scale(A) + p[1].cos().scale(C), // A sin z + C cos y
            1 => p[0].sin().scale(B) + p[2].cos().scale(A), // B sin x + A cos z
            _ => p[1].sin().scale(C) + p[0].cos().scale(B), // C sin y + B cos x
        }
    }
}

/// Scene builder for the `stream_ribbons` gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(66_f32.to_radians(), -45_f32.to_radians());

        let field = VectorField3::from_components(
            ScalarField::from_closure(AbcComponent { axis: 0 }),
            ScalarField::from_closure(AbcComponent { axis: 1 }),
            ScalarField::from_closure(AbcComponent { axis: 2 }),
        );

        // Eight seeds on the corners of a cube about the origin: the flow's
        // symmetry then keeps the ribbon bundle roughly centred in frame.
        let mut seeds = Vec::with_capacity(8);
        for i in 0..8 {
            let s = |bit: usize| if i >> bit & 1 == 1 { 1.0 } else { -1.0 };
            seeds.push(DVec3::new(0.95 * s(0), 0.95 * s(1), 0.95 * s(2)));
        }

        let ribbons = stream_ribbons(
            scene.state_mut(),
            &field,
            &seeds,
            StreamParams {
                length: 2.6,
                step: 0.03,
                radius: 0.11, // ribbon half-width
                n_around: 4,  // ignored by ribbons
                flux_conserving: false,
            },
        );

        // Draw the ribbons out along the flow, then orbit.
        scene.play(Create::new(ribbons).run_time(3.0))?;
        for _ in 0..45 {
            scene.rotate_camera(TAU as f32 / 90.0);
            scene.wait(0.06);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/stream_ribbons")?;
    println!("Rendered frames to out/stream_ribbons");
    Ok(())
}
