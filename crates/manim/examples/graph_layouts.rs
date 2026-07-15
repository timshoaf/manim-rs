//! A network `Graph` morphing between three layouts — circular, spring
//! (force-directed), and tree — via `TransformInto`.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example graph_layouts
//! ```
//!
//! Frames land in `out/graph_layouts/frame_NNNNN.png`.
//!
//! API notes vs CE:
//! - The network graph is `manim::prelude::Graph` (module `network`), distinct
//!   from function graphs. Layout is the `GraphLayout` enum
//!   (`Circular { radius }`, `Spring { seed, iterations }`, `Tree { root }`).
//! - `change_layout` mutates in place; to *animate* a re-layout we morph into a
//!   graph built with the new layout (same vertices/edges) via `TransformInto`.

use manim::prelude::*;

/// Scene builder for this gallery example.
pub struct GraphLayouts;

impl SceneBuilder for GraphLayouts {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let edges = [(0, 1), (0, 2), (1, 3), (1, 4), (2, 5), (2, 6)];
        let mk = |layout| Graph::new(7, &edges, layout);

        let title = Text::new("Graph Layouts").add_to(scene.state_mut());
        scene.state_mut().move_to(title, 3.2 * UP);

        let graph = scene.add(mk(GraphLayout::Circular { radius: 2.4 }));

        scene.play(FadeIn::new(title))?;
        scene.play(Create::new(graph).run_time(1.5))?;
        scene.wait(0.4);
        scene.play(TransformInto::new(graph, mk(GraphLayout::spring(7))).run_time(1.5))?;
        scene.wait(0.4);
        scene.play(TransformInto::new(graph, mk(GraphLayout::Tree { root: 0 })).run_time(1.5))?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&GraphLayouts, Config::low())?;
    manim::render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/graph_layouts")?;
    println!("Rendered frames to out/graph_layouts");
    Ok(())
}
