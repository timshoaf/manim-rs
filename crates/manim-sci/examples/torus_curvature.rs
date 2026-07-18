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

use manim_core::animations::Create;
use manim_core::display::Colormap;
use manim_core::prelude::*;
use manim_fields::ad::Scalar;
use manim_math::path::Path;
use manim_sci::curveviz::{surface_colored_by_curvature, CurvatureKind};
use manim_sci::diffgeo::SurfaceSampler;
use manim_sci::geodesics::geodesic;

/// A torus of major radius 1, minor radius 0.4.
struct Torus;
impl SurfaceSampler for Torus {
    fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
        let r = S::constant(1.0) + u.cos().scale(0.4);
        [r * v.cos(), r * v.sin(), u.sin().scale(0.4)]
    }
}

fn embed(u: f64, v: f64) -> Point {
    let [x, y, z] = Torus.eval::<f64>(u, v);
    Point::new(x as f32, y as f32, z as f32)
}

struct TorusCurvature;

impl SceneBuilder for TorusCurvature {
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

        // A geodesic on the torus, embedded as a stroked 3-D polyline.
        let path = geodesic(&Torus, 0.4, 0.0, 0.35, 1.0, 14.0, 400);
        let pts: Vec<Point> = path.iter().map(|&(u, v)| embed(u, v)).collect();
        let geo = scene.add(
            VMobject::from_path(Path::from_corners(&pts, false)).with_stroke(YELLOW, 5.0, 1.0),
        );

        scene.play(Create::new(geo).run_time(3.0))?;
        // Ambient turntable so the curvature coloring reads in 3-D.
        for _ in 0..30 {
            scene.rotate_camera(TAU as f32 / 60.0);
            scene.wait(0.06);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&TorusCurvature, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/torus_curvature")?;
    println!("Rendered frames to out/torus_curvature");
    Ok(())
}
