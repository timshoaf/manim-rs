//! Forward-pass activation pulses: a wave of [`ShowPassingFlash`] sweeping the
//! edges of a [`ComputeGraph`], staggered so earlier layers fire first.
//!
//! For every edge a thin [`Line`] is laid down along the edge's source → target
//! anchors. The lines are ordered by their **source node's rank** and driven by
//! a [`LaggedStart`], so the flashes cascade rank-by-rank through the network —
//! the visual signature of a forward pass.
//!
//! ```
//! use manim_core::scene_state::SceneState;
//! use manim_nn::blockdiagram::LayerBlockDiagram;
//! use manim_nn::flow::forward_pass;
//! let mut scene = SceneState::new();
//! let diagram = LayerBlockDiagram::transformer_block();
//! let pulse = forward_pass(&mut scene, diagram.graph());
//! let _ = pulse; // hand to `scene.play(pulse)`
//! ```

use manim_core::animations::{LaggedStart, ShowPassingFlash};
use manim_core::geometry::Line;
use manim_core::mobject::MobjectId;
use manim_core::scene_state::SceneState;

use crate::graph::ComputeGraph;

/// The edge lines of a forward pass, in pulse order (by source rank), together
/// with each line's source rank. Build the animation with [`Self::animation`],
/// or use the edge-line ids directly (e.g. to recolour a path).
pub struct ForwardPass {
    /// The per-edge [`Line`] ids, ordered by source rank (earliest first).
    pub lines: Vec<MobjectId<Line>>,
    /// The source-node rank of each line, parallel to [`Self::lines`].
    pub src_ranks: Vec<usize>,
}

impl ForwardPass {
    /// A [`LaggedStart`] of one [`ShowPassingFlash`] per edge line, in pulse
    /// order — the flashes sweep the network from the lowest rank up.
    pub fn animation(&self) -> LaggedStart {
        let flashes: Vec<ShowPassingFlash> = self
            .lines
            .iter()
            .map(|&line| ShowPassingFlash::new(line))
            .collect();
        LaggedStart::new(flashes)
    }
}

/// Lays down the per-edge pulse lines for `graph`, ordered by source rank, and
/// returns them as a [`ForwardPass`] (without building the animation yet).
///
/// The lines are added to `scene` in rank order, so a [`LaggedStart`] over them
/// staggers earlier-rank edges first.
///
/// ```
/// use manim_core::scene_state::SceneState;
/// use manim_nn::blockdiagram::LayerBlockDiagram;
/// use manim_nn::flow::forward_pass_setup;
/// let mut scene = SceneState::new();
/// let diagram = LayerBlockDiagram::transformer_block();
/// let fp = forward_pass_setup(&mut scene, diagram.graph());
/// assert_eq!(fp.lines.len(), diagram.graph().edges().len());
/// ```
pub fn forward_pass_setup(scene: &mut SceneState, graph: &ComputeGraph) -> ForwardPass {
    let anchors = graph.edge_anchors();
    let edges = graph.edges();
    let ranks = graph.ranks();
    // The source rank of edge `i`, used both to order and to report the wave.
    // `edges[i].0` is the source `NodeId`; its `.0` is the node index into ranks.
    let src_rank = |i: usize| ranks[edges[i].0 .0];

    let mut order: Vec<usize> = (0..anchors.len()).collect();
    order.sort_by_key(|&i| src_rank(i));

    let mut lines = Vec::with_capacity(order.len());
    let mut src_ranks = Vec::with_capacity(order.len());
    for i in order {
        let (start, end) = anchors[i];
        lines.push(scene.add(Line::new(start, end)));
        src_ranks.push(src_rank(i));
    }
    ForwardPass { lines, src_ranks }
}

/// Builds a forward-pass pulse over `graph`: a thin line per edge plus a
/// [`LaggedStart`] of [`ShowPassingFlash`], staggered by source rank so the wave
/// sweeps the network front-to-back. Play it with `scene.play(...)`.
///
/// Use [`forward_pass_setup`] instead when you also need the edge-line ids.
///
/// ```
/// use manim_core::scene_state::SceneState;
/// use manim_nn::blockdiagram::LayerBlockDiagram;
/// use manim_nn::flow::forward_pass;
/// let mut scene = SceneState::new();
/// let diagram = LayerBlockDiagram::transformer_block();
/// let pulse = forward_pass(&mut scene, diagram.graph());
/// let _ = pulse;
/// ```
pub fn forward_pass(scene: &mut SceneState, graph: &ComputeGraph) -> LaggedStart {
    forward_pass_setup(scene, graph).animation()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockdiagram::LayerBlockDiagram;

    #[test]
    fn one_flash_per_edge_staggered_by_source_rank() {
        let mut scene = SceneState::new();
        let diagram = LayerBlockDiagram::transformer_block();
        let graph = diagram.graph();
        let fp = forward_pass_setup(&mut scene, graph);

        // One pulse line per edge.
        assert_eq!(fp.lines.len(), graph.edges().len());
        // The animation builds one child per edge.
        let _anim = fp.animation();

        // Lines are ordered by source rank: the wave never runs backwards.
        for w in fp.src_ranks.windows(2) {
            assert!(w[0] <= w[1], "pulse order is not monotone in source rank");
        }
    }
}
