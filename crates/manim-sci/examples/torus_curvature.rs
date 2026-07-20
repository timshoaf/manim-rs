//! A torus colored by its Gaussian curvature (positive/red on the outer rim,
//! negative/blue on the inner rim) with a geodesic traced on its surface, viewed
//! under an ambient camera rotation.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-sci --example torus_curvature --features render-examples
//! ```
//!
//! Frames land in `out/torus_curvature/frame_NNNNN.png`.

use std::f64::consts::TAU;

use manim_core::display::Colormap;
use manim_core::prelude::*;
use manim_fields::ad::Scalar;
use manim_sci::curveviz::{surface_colored_by_curvature, CurvatureKind, SpaceCurve};
use manim_sci::diffgeo::SurfaceSampler;
use manim_sci::geodesics::{embed_path, geodesic};

/// A torus of major radius 1, minor radius 0.4.
struct Torus;
impl SurfaceSampler for Torus {
    fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
        let r = S::constant(1.0) + u.cos().scale(0.4);
        [r * v.cos(), r * v.sin(), u.sin().scale(0.4)]
    }
}

/// Scene builder for the torus-curvature gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(65_f32.to_radians(), -55_f32.to_radians());

        // Torus surface colored by Gaussian curvature.
        surface_colored_by_curvature(
            scene.state_mut(),
            &Torus,
            CurvatureKind::Gaussian,
            Colormap::Coolwarm,
            (0.0, TAU),
            (0.0, TAU),
            (60, 60),
        );

        // A geodesic on the torus, swept into a depth-tested tube so the torus
        // occludes it where it runs around the far side. A flat stroke would
        // draw over the mesh and read as floating in front of the surface.
        let path = geodesic(&Torus, 0.4, 0.0, 0.35, 1.0, 14.0, 400);
        let _geo = SpaceCurve::new(embed_path(&Torus, &path))
            .with_radius(0.025)
            .with_color(YELLOW)
            .add_to(scene.state_mut());

        // No `Create` trace-on here: that animation interpolates a 2-D path's
        // points, and a mesh mobject's path encodes only its model transform, so
        // it would be a silent no-op. The tube is shown complete.
        scene.wait(0.5);
        // Ambient turntable so the curvature coloring reads in 3-D.
        for _ in 0..30 {
            scene.rotate_camera(TAU as f32 / 60.0);
            scene.wait(0.06);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/torus_curvature")?;
    println!("Rendered frames to out/torus_curvature");
    Ok(())
}
