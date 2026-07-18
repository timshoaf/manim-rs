//! Caffeine (C8H10N4O2, 24 atoms) as a ball-and-stick model under a turntable
//! camera, rendered to a PNG sequence.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-chem --example caffeine --features render-examples
//! ```
//!
//! Frames land in `out/caffeine/frame_NNNNN.png`.
//!
//! What this shows:
//! - Parsing an embedded MDL V2000 molblock with
//!   [`from_sdf`](manim_chem::parsers::from_sdf) — the well-known PubChem
//!   caffeine geometry (CID 2519), with explicit bonds so no perception is
//!   needed.
//! - [`render::ball_and_stick`] building the model as two GPU-instanced draws
//!   (one atom cloud, one bond cloud) grouped in a `VGroup`.
//! - An ambient (turntable) camera orbit via `set_camera_orientation` +
//!   repeated `rotate_camera`/`wait`, which `VideoExporter` replays frame by
//!   frame.

use manim_chem::parsers::from_sdf;
use manim_chem::render;
use manim_core::prelude::*;
use manim_render::export::VideoExporter;

/// PubChem CID 2519 caffeine, 3-D conformer, as an MDL V2000 molblock.
///
/// Header (3 lines) + counts line + 24 atom lines + 25 bond lines. The parser
/// is whitespace-tolerant, so exact column alignment is not required.
const CAFFEINE_SDF: &str = "\
2519
  -manim-chem- caffeine

 24 25  0     0  0  0  0  0  0999 V2000
    0.4700    2.5688    0.0006 O   0  0  0  0  0  0  0  0  0  0  0  0
   -3.1271   -0.4436   -0.0003 O   0  0  0  0  0  0  0  0  0  0  0  0
   -0.9686   -1.3125    0.0000 N   0  0  0  0  0  0  0  0  0  0  0  0
    2.2182    0.1412   -0.0003 N   0  0  0  0  0  0  0  0  0  0  0  0
   -1.3477    1.0797   -0.0001 N   0  0  0  0  0  0  0  0  0  0  0  0
    1.4119   -1.9372    0.0002 N   0  0  0  0  0  0  0  0  0  0  0  0
    0.8579    0.2592   -0.0008 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.3897   -1.0264   -0.0004 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.0307    1.4220   -0.0006 C   0  0  0  0  0  0  0  0  0  0  0  0
   -1.9061   -0.2495   -0.0004 C   0  0  0  0  0  0  0  0  0  0  0  0
    2.5032   -1.1998    0.0003 C   0  0  0  0  0  0  0  0  0  0  0  0
   -1.4276    2.4304    0.0008 C   0  0  0  0  0  0  0  0  0  0  0  0
    3.1926    1.2061    0.0003 C   0  0  0  0  0  0  0  0  0  0  0  0
   -2.2969   -2.1881    0.0007 C   0  0  0  0  0  0  0  0  0  0  0  0
    3.5163   -1.5787    0.0008 H   0  0  0  0  0  0  0  0  0  0  0  0
   -1.0447    2.9296   -0.8927 H   0  0  0  0  0  0  0  0  0  0  0  0
   -2.4989    2.7221    0.0864 H   0  0  0  0  0  0  0  0  0  0  0  0
   -0.9034    2.8330    0.8749 H   0  0  0  0  0  0  0  0  0  0  0  0
    4.1962    0.7801   -0.0002 H   0  0  0  0  0  0  0  0  0  0  0  0
    3.0662    1.8215    0.8955 H   0  0  0  0  0  0  0  0  0  0  0  0
    3.0671    1.8215   -0.8949 H   0  0  0  0  0  0  0  0  0  0  0  0
   -1.6871   -3.0872    0.0011 H   0  0  0  0  0  0  0  0  0  0  0  0
   -2.9169   -2.2498    0.8891 H   0  0  0  0  0  0  0  0  0  0  0  0
   -2.9174   -2.2492   -0.8874 H   0  0  0  0  0  0  0  0  0  0  0  0
  1  9  2  0
  2 10  2  0
  3  8  1  0
  3 10  1  0
  3 14  1  0
  4  7  1  0
  4 11  1  0
  4 13  1  0
  5  9  1  0
  5 10  1  0
  5 12  1  0
  6  8  1  0
  6 11  2  0
  7  8  1  0
  7  9  1  0
 11 15  1  0
 12 16  1  0
 12 17  1  0
 12 18  1  0
 13 19  1  0
 13 20  1  0
 13 21  1  0
 14 22  1  0
 14 23  1  0
 14 24  1  0
M  END
$$$$
";

/// Scene builder for the caffeine turntable.
struct Caffeine;

impl SceneBuilder for Caffeine {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        // Look slightly down at the molecule (it lies near the z = 0 plane).
        scene.set_camera_orientation(72_f32.to_radians(), -45_f32.to_radians());

        let molecule = from_sdf(CAFFEINE_SDF).expect("embedded caffeine fixture parses");
        let _model = render::ball_and_stick(scene.state_mut(), &molecule);

        // Ambient turntable: one full revolution over ~3 s.
        let steps = 60;
        for _ in 0..steps {
            scene.rotate_camera(TAU / steps as f32);
            scene.wait(0.05);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Caffeine, Config::low())?;
    VideoExporter::render_to_png_sequence(&mut scene, "out/caffeine")?;
    println!("Rendered frames to out/caffeine");
    Ok(())
}
