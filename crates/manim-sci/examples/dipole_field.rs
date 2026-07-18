//! An electric dipole field, in full volumetric 3-D.
//!
//! Two point charges — `+q` at `(0, 0, +d)` and `−q` at `(0, 0, −d)` — give the
//! field `E(r) = Σ qᵢ (r − pᵢ) / |r − pᵢ|³`. The scene layers three ways of
//! seeing it:
//!
//! - **stream tubes** integrated from a ring of seeds around the dipole,
//! - a **slice plane** heatmap of the potential through the `xz`-plane, and
//! - a small **probability cloud** of the field magnitude `|E|` near the charges.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-sci --example dipole_field --features render-examples
//! ```
//!
//! Frames land in `out/dipole_field/frame_NNNNN.png`.
//!
//! NOTE: the stream-tube layer calls [`manim_sci::vector_field_3d::stream_tubes`],
//! which is authored in the same S8 wave as this example; if that module is still
//! a placeholder in your checkout, this example compiles once it lands (the rest
//! of the scene — slice plane and cloud — uses already-shipped APIs).

use std::f64::consts::TAU;

use glam::DVec3;

use manim_core::animations::Create;
use manim_core::display::Colormap;
use manim_core::prelude::*;

use manim_fields::ad::Scalar;
use manim_fields::field::{ScalarClosure, ScalarField, VectorField3};

use manim_sci::vector_field_3d::{stream_tubes, StreamParams};
use manim_sci::volumetrics::{density_cloud, field_slice, CloudParams};

/// Half the charge separation: `+q` sits at `+d ẑ`, `−q` at `−d ẑ`.
const D: f64 = 0.5;

/// One Cartesian component of the dipole field `E(r)`.
///
/// `axis` selects `Eₓ`, `E_y`, or `E_z`. Generic over the AD [`Scalar`] so the
/// same closure serves value and derivative evaluation.
struct DipoleComponent {
    axis: usize,
}

impl ScalarClosure for DipoleComponent {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        let d = S::constant(D);
        // r − p₊ (charge +q at +d ẑ).
        let rp = [p[0], p[1], p[2] - d];
        let inv_rp3 = (rp[0] * rp[0] + rp[1] * rp[1] + rp[2] * rp[2])
            .sqrt()
            .powi(3)
            .recip();
        // r − p₋ (charge −q at −d ẑ).
        let rm = [p[0], p[1], p[2] + d];
        let inv_rm3 = (rm[0] * rm[0] + rm[1] * rm[1] + rm[2] * rm[2])
            .sqrt()
            .powi(3)
            .recip();
        rp[self.axis] * inv_rp3 - rm[self.axis] * inv_rm3
    }
}

/// The dipole potential `V(r) = 1/|r − p₊| − 1/|r − p₋|` (the slice-plane scalar).
struct DipolePotential;

impl ScalarClosure for DipolePotential {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        let d = S::constant(D);
        let rp = [p[0], p[1], p[2] - d];
        let inv_rp = (rp[0] * rp[0] + rp[1] * rp[1] + rp[2] * rp[2])
            .sqrt()
            .recip();
        let rm = [p[0], p[1], p[2] + d];
        let inv_rm = (rm[0] * rm[0] + rm[1] * rm[1] + rm[2] * rm[2])
            .sqrt()
            .recip();
        inv_rp - inv_rm
    }
}

/// The field magnitude `|E(r)|` (the density the probability cloud traces).
struct DipoleMagnitude;

impl ScalarClosure for DipoleMagnitude {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        let ex = DipoleComponent { axis: 0 }.eval(p);
        let ey = DipoleComponent { axis: 1 }.eval(p);
        let ez = DipoleComponent { axis: 2 }.eval(p);
        (ex * ex + ey * ey + ez * ez).sqrt()
    }
}

/// The dipole vector field, assembled from its three component scalar fields.
fn dipole_field() -> VectorField3 {
    VectorField3::from_components(
        ScalarField::from_closure(DipoleComponent { axis: 0 }),
        ScalarField::from_closure(DipoleComponent { axis: 1 }),
        ScalarField::from_closure(DipoleComponent { axis: 2 }),
    )
}

/// A ring of `n` seed points at radius `r` in the plane `z = z0`, encircling the
/// dipole axis — where the field lines loop from `+q` to `−q`.
fn seed_ring(n: usize, r: f64, z0: f64) -> Vec<DVec3> {
    (0..n)
        .map(|i| {
            let a = TAU * i as f64 / n as f64;
            DVec3::new(r * a.cos(), r * a.sin(), z0)
        })
        .collect()
}

/// Scene builder for the dipole-field gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(70_f32.to_radians(), -50_f32.to_radians());

        let field = dipole_field();

        // Slice plane: the potential through the xz-plane (normal = +y), the
        // classic dipole "+/−" heatmap lobes.
        let potential = ScalarField::from_closure(DipolePotential);
        field_slice(
            scene.state_mut(),
            &potential,
            DVec3::ZERO,
            DVec3::Y,
            2.5,
            160,
            Colormap::Coolwarm,
        );

        // Stream tubes seeded on a ring around the dipole, integrating the field
        // lines that arc from the positive to the negative charge.
        let seeds = seed_ring(16, 0.6, 0.0);
        let tubes = stream_tubes(
            scene.state_mut(),
            &field,
            &seeds,
            StreamParams {
                length: 8.0,
                step: 0.02,
                radius: 0.015,
                n_around: 12,
                flux_conserving: false,
            },
        );

        // A probability cloud of |E| clustered near the charges (the field is
        // strongest there); max_density caps the near-charge singularity.
        let magnitude = ScalarField::from_closure(DipoleMagnitude);
        density_cloud(
            scene.state_mut(),
            &magnitude,
            CloudParams {
                n_samples: 1500,
                seed: 0x00D1_901E,
                radius: 0.02,
                bounds_min: DVec3::new(-0.9, -0.9, -1.1),
                bounds_max: DVec3::new(0.9, 0.9, 1.1),
                max_density: 12.0,
            },
        );

        // Reveal the field lines, then an ambient turntable so the 3-D structure
        // of the lobes and tubes reads.
        scene.play(Create::new(tubes).run_time(3.0))?;
        for _ in 0..40 {
            scene.rotate_camera(TAU as f32 / 80.0);
            scene.wait(0.05);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/dipole_field")?;
    println!("Rendered frames to out/dipole_field");
    Ok(())
}
