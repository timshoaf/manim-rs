//! SGD vs. momentum vs. Adam, racing down the same ravine from the same start.
//!
//! The loss is a deliberately **anisotropic** quadratic ravine,
//! `L(x, y) = 0.02·x² + 1.5·y²`. Its Hessian eigenvalues are `0.04` and `3.0`, a
//! condition number of `75`: the surface is a narrow trough running along the
//! `x`-axis with steep walls in `y` and an almost flat floor in `x`. Curvature
//! anisotropy — not non-convexity — is what actually separates first-order
//! optimizers, and this is the cleanest form of it.
//!
//! All three start at `(−1.8, 1.05)`, high on the wall, and follow the **exact**
//! gradient: the loss is a [`manim_fields::field::ScalarField`], so `∇L` comes
//! from forward-mode automatic differentiation, not finite differences.
//!
//! - **SGD** (red, `lr = 0.3`) is stability-limited by the *steep* direction:
//!   `lr < 2/3.0`. It drops into the trough almost instantly, then crawls along
//!   the flat floor at rate `lr·0.04` per step and is still at `x ≈ −0.33`.
//! - **Momentum** (yellow, `lr = 0.05, β = 0.9`) accumulates velocity along the
//!   floor — an effective step `lr/(1−β) = 10×` larger — and overshoots the
//!   trough in `y` on the way in before settling.
//! - **Adam** (green, `lr = 0.04`) divides each coordinate by its own gradient
//!   RMS, so both directions move at ≈ `lr` per step regardless of curvature; it
//!   heads for the minimum nearly in a straight line and gets there first.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-nn --example loss_landscape_descent --features render-examples
//! ```
//!
//! Frames land in `out/loss_landscape_descent/frame_NNNNN.png`.

use glam::DVec2;

use manim_core::animations::Create;
use manim_core::mesh::MeshMaterial;
use manim_core::prelude::*;

use manim_fields::ad::Scalar;
use manim_fields::field::{ScalarClosure, ScalarField};

use manim_nn::landscape::{LossLandscape, Optimizer};

/// Every optimizer starts here and takes this many steps.
const START: DVec2 = DVec2::new(-1.8, 1.05);
/// Iterations per optimizer (one polyline segment each).
const STEPS: usize = 140;

/// `L(x, y) = 0.02·x² + 1.5·y²` — flat along `x`, steep across `y`.
///
/// Generic over the AD [`Scalar`], so the same code yields both the value and
/// the exact gradient the optimizers consume.
struct Ravine;

impl ScalarClosure for Ravine {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        (p[0] * p[0]).scale(0.02) + (p[1] * p[1]).scale(1.5)
    }
}

/// Traces one optimizer's descent on the surface in its own color, lifted just
/// clear of the mesh so the polyline is not z-fought by the surface it lies on.
fn race(scene: &mut Scene, land: &LossLandscape, opt: Optimizer, color: Color) -> Create {
    let path = land.descend_on_surface(scene.state_mut(), START, opt, STEPS);
    scene.set_stroke(path, color, 5.0, 1.0);
    scene.state_mut().shift(path, 0.03 * OUT);
    Create::new(path).run_time(5.5)
}

/// The three trajectories drawn simultaneously on the ravine surface.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(64_f32.to_radians(), -62_f32.to_radians());

        let land = LossLandscape::new(ScalarField::from_closure(Ravine), [-2.2, 2.2], [-1.1, 1.1]);
        let surface = land.add_surface(scene.state_mut(), (72, 48));
        // Muted and slightly translucent: the trajectories are the subject.
        scene
            .state_mut()
            .get_mut(surface)
            .set_material(MeshMaterial::new(BLUE).with_opacity(0.85));

        let sgd = race(scene, &land, Optimizer::Sgd { lr: 0.3 }, RED);
        let momentum = race(
            scene,
            &land,
            Optimizer::Momentum {
                lr: 0.05,
                beta: 0.9,
            },
            YELLOW,
        );
        let adam = race(
            scene,
            &land,
            Optimizer::Adam {
                lr: 0.04,
                beta1: 0.9,
                beta2: 0.95,
                eps: 1e-8,
            },
            GREEN,
        );

        // Drawn together, so the pace of each optimizer reads as a race.
        scene.play((sgd, momentum, adam))?;
        for _ in 0..24 {
            scene.rotate_camera(TAU / 96.0);
            scene.wait(0.08);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/loss_landscape_descent",
    )?;
    println!("Rendered frames to out/loss_landscape_descent");
    Ok(())
}
