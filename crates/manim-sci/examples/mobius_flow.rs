//! **A Möbius transformation as a flow** — watching `z ↦ (az + b)/(cz + d)` happen
//! continuously instead of all at once.
//!
//! Every one-parameter subgroup of Möbius maps is the flow of a *quadratic*
//! holomorphic vector field `ż = a + bz + cz²`. The one used here,
//!
//! `ż = v(z) = (i/2)(1 + z²)`,
//!
//! integrates in closed form to `φ_t(z) = (cos s · z + i sin s)/(−i sin s · z + cos s)`
//! with `s = t/2` — a genuine Möbius map for every `t`, and, seen on the Riemann
//! sphere, simply a rotation about the axis through the fixed points `z = ±i`.
//!
//! Two things to watch:
//!
//! - **Conformality.** `v` is holomorphic, so the flow's Jacobian is `s·R` — a
//!   scaling times a rotation, with no shear. The grid stretches wildly and bends
//!   into arcs, yet every crossing stays a *right angle*. The faded ghost grid
//!   underneath is the undeformed reference to compare against.
//! - **Circles to circles.** The three white circles ride the same flow; a Möbius
//!   map sends circles to circles (or lines), so they stay round while moving and
//!   changing size.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-sci --example mobius_flow --features render-examples
//! ```
//!
//! Frames land in `out/mobius_flow/frame_NNNNN.png`.

use manim_core::prelude::*;
use manim_fields::ad::Scalar;
use manim_fields::field::{ScalarClosure, ScalarField, VectorField3};
use manim_sci::deform::{DeformationGrid, FlowMap};

/// Grid extent; kept clear of the moving pole of `φ_t` at `z = −i·cot(t/2)`.
const XR: [f64; 2] = [-3.4, 3.4];
const YR: [f64; 2] = [-1.7, 1.7];
/// Total flow time. `t = 0.6` puts the pole at `−3.2i`, safely outside the grid.
const T: f64 = 0.6;

/// `Re v = −xy`, from `(i/2)(1 + z²)` with `z = x + iy`.
struct VelX;
impl ScalarClosure for VelX {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        -(p[0] * p[1])
    }
}

/// `Im v = (1 + x² − y²)/2`, the other half of `(i/2)(1 + z²)`.
struct VelY;
impl ScalarClosure for VelY {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        (S::constant(1.0) + p[0] * p[0] - p[1] * p[1]).scale(0.5)
    }
}

/// Scene builder for the `mobius_flow` gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        // The planar holomorphic field ż = (i/2)(1 + z²), z-component zero.
        let field = VectorField3::from_components(
            ScalarField::from_closure(VelX),
            ScalarField::from_closure(VelY),
            ScalarField::constant(0.0),
        );

        // A faded, undeformed ghost grid: the reference the deformation is read
        // against.
        DeformationGrid::new(XR, YR, 0.5)
            .faded(0.15)
            .add_to(scene.state_mut());

        // The live grid, laid down undeformed and then advected by the flow.
        let grid = DeformationGrid::new(XR, YR, 0.5).add_to(scene.state_mut());

        // Test circles: a Möbius map carries circles to circles, so these stay
        // round however far they travel.
        let circles: Vec<AnyId> = [
            (-1.8_f32, 0.0_f32, 0.55_f32),
            (0.0, 0.0, 0.8),
            (1.8, 0.0, 0.55),
        ]
        .into_iter()
        .map(|(x, y, r)| {
            scene
                .add(
                    Circle::new()
                        .radius(r)
                        .with_move_to(Point::new(x, y, 0.0))
                        .with_stroke(WHITE, 3.0, 1.0),
                )
                .erase()
        })
        .collect();
        let ring = VGroup::of(scene.state_mut(), circles);

        scene.wait(0.4);
        // Both layers ride the same integral curves, so they stay consistent.
        scene.play((
            FlowMap::new(grid, field.clone(), T).run_time(4.0),
            FlowMap::new(ring, field, T).run_time(4.0),
        ))?;
        scene.wait(1.0);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/mobius_flow")?;
    println!("Rendered frames to out/mobius_flow");
    Ok(())
}
