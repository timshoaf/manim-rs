//! Four geodesics racing across an egg-crate surface — geodesic deviation made visible.
//!
//! The surface is the Monge patch `z = a·sin(b·u)·cos(b·v)`, an "egg crate" of
//! alternating domes and saddles. A **geodesic** is the curved-space analogue of
//! a straight line: it satisfies `üᵏ + Γᵏᵢⱼ u̇ⁱ u̇ʲ = 0`, i.e. it never turns
//! *within* the surface, only along with it.
//!
//! Four geodesics leave the **same point** with initial directions differing by
//! only a few degrees. On a flat plane they would stay a fixed angle apart
//! forever. Here the Jacobi equation `D²ξ/ds² = −K·ξ` governs their separation
//! `ξ`: where the Gaussian curvature `K > 0` (the domes) neighbouring geodesics
//! **focus** back toward one another, and where `K < 0` (the saddles between the
//! domes) they **spread apart**. The surface is colored by `K` with a diverging
//! colormap, so the correlation is direct: watch the bundle pinch over warm
//! patches and fan out over cool ones.
//!
//! NOTE: the traces are projected 2-D strokes drawn over the mesh, so they are
//! not depth-occluded where a geodesic passes behind a dome — they read as
//! always-on-top. That is a known limitation of mixing strokes with meshes.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-sci --example geodesic_race --features render-examples
//! ```
//!
//! Frames land in `out/geodesic_race/frame_NNNNN.png`.

use std::f64::consts::TAU;

use manim_core::animations::Create;
use manim_core::display::Colormap;
use manim_core::prelude::*;
use manim_fields::ad::Scalar;
use manim_math::path::Path;
use manim_sci::curveviz::{surface_colored_by_curvature, CurvatureKind};
use manim_sci::diffgeo::SurfaceSampler;
use manim_sci::geodesics::geodesic;

/// Bump amplitude `a` and spatial frequency `b` of the egg crate.
const A: f64 = 0.55;
const B: f64 = 1.5;

/// The Monge patch `(u, v) ↦ (u, v, a·sin(bu)·cos(bv))`.
struct EggCrate;
impl SurfaceSampler for EggCrate {
    fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
        [u, v, (u.scale(B).sin() * v.scale(B).cos()).scale(A)]
    }
}

/// Lifts a `(u, v)` parameter pair to its embedded scene point.
fn embed(u: f64, v: f64) -> Point {
    let [x, y, z] = EggCrate.eval::<f64>(u, v);
    Point::new(x as f32, y as f32, z as f32)
}

/// Scene builder for the `geodesic_race` gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(62_f32.to_radians(), -50_f32.to_radians());

        // The egg crate, colored by Gaussian curvature K: domes K > 0, saddles K < 0.
        surface_colored_by_curvature(
            scene.state_mut(),
            &EggCrate,
            CurvatureKind::Gaussian,
            Colormap::Coolwarm,
            (-2.6, 2.6),
            (-2.6, 2.6),
            (80, 80),
        );

        // Four geodesics from one seed point, fanned over a ±5° spread.
        let (u0, v0) = (-2.2, -1.9);
        let mut races = Vec::new();
        for (k, color) in [YELLOW, ORANGE, GREEN, WHITE].into_iter().enumerate() {
            // `geodesic` renormalizes the initial velocity to unit embedding
            // speed, so all four advance at the same arc-length rate.
            let angle = 0.74 + (k as f64 - 1.5) * 0.085;
            let path = geodesic(&EggCrate, u0, v0, angle.cos(), angle.sin(), 7.4, 360);
            // Stop each trace at the edge of the drawn patch, so no geodesic
            // continues into empty space past the mesh.
            let pts: Vec<Point> = path
                .iter()
                .take_while(|&&(u, v)| u.abs() <= 2.6 && v.abs() <= 2.6)
                .map(|&(u, v)| embed(u, v))
                .collect();
            let id = scene.add(
                VMobject::from_path(Path::from_corners(&pts, false)).with_stroke(color, 5.0, 1.0),
            );
            races.push(Create::new(id).run_time(3.2));
        }

        // Equal run_time ⇒ the four traces really do race side by side.
        scene.play(races)?;
        // Turntable so the domes, saddles, and the divergence read in 3-D.
        for _ in 0..36 {
            scene.rotate_camera(TAU as f32 / 72.0);
            scene.wait(0.055);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/geodesic_race")?;
    println!("Rendered frames to out/geodesic_race");
    Ok(())
}
