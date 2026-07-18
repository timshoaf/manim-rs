//! A transformer block laid out as a compute graph, drawn in and then swept by
//! a forward-pass activation pulse.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-nn --example transformer_block --features render-examples
//! ```
//!
//! Frames land in `out/transformer_block/frame_NNNNN.png`.

use manim_core::animations::Create;
use manim_core::prelude::*;

use manim_nn::blockdiagram::LayerBlockDiagram;
use manim_nn::flow::forward_pass;

/// Scene builder for the transformer-block gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        // The opinionated transformer-block preset: attention + MLP sub-blocks
        // with residual paths, laid out by the compute-graph ranker.
        let diagram = LayerBlockDiagram::transformer_block();
        let group = diagram.render(scene.state_mut());

        // Draw the architecture in…
        scene.play(Create::new(group))?;
        scene.wait(0.3);

        // …then send a forward-pass pulse sweeping through the graph, staggered
        // rank-by-rank (a LaggedStart of ShowPassingFlash over the edges).
        let pulse = forward_pass(scene.state_mut(), diagram.graph());
        scene.play(pulse)?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/transformer_block",
    )?;
    println!("Rendered frames to out/transformer_block");
    Ok(())
}
