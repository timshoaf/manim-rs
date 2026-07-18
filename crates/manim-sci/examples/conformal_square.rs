//! The conformal map `z ↦ z²`: an adaptively-subdivided [`DeformationGrid`]
//! morphs from the identity to the squared plane, over a faded undeformed ghost
//! grid for reference.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-sci --example conformal_square
//! ```
//!
//! Frames land in `out/conformal_square/frame_NNNNN.png`.

use manim_core::prelude::*;
use manim_fields::map::SpaceMap;
use manim_sci::deform::{ApplyMap, DeformationGrid};

/// Scene builder for the conformal-square gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let map = SpaceMap::complex_power(2);
        let region = ([-2.0, 2.0], [-2.0, 2.0], 0.5);

        // A faded, undeformed ghost grid as a reference frame.
        DeformationGrid::new(region.0, region.1, region.2)
            .with_map(&map)
            .faded(0.25)
            .add_to(scene.state_mut());

        // The live grid: adaptively subdivided (denser where z² stretches most),
        // then deformed z ↦ z² by ApplyMap.
        let grid = DeformationGrid::new(region.0, region.1, region.2)
            .with_map(&map)
            .add_to(scene.state_mut());

        scene.wait(0.4);
        scene.play(ApplyMap::new(grid, &map).run_time(3.0))?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/conformal_square",
    )?;
    println!("Rendered frames to out/conformal_square");
    Ok(())
}
