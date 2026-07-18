//! Opinionated architecture diagrams built on [`ComputeGraph`].
//!
//! [`LayerBlockDiagram`] wraps a [`ComputeGraph`] and offers ready-made presets
//! for common architectures — [`LayerBlockDiagram::mlp`],
//! [`LayerBlockDiagram::lenet`], and [`LayerBlockDiagram::transformer_block`] —
//! each pre-populated with tensor-shape captions. The transformer preset wires
//! the two residual/skip connections so they arc around the attention and
//! feed-forward sublayers, exercising [`ComputeGraph`]'s skip-edge routing.

use manim_core::prelude::{MobjectId, SceneState, VGroup};

use crate::graph::{ComputeGraph, NodeKind};

/// An architecture diagram: a thin, preset-oriented wrapper over a
/// [`ComputeGraph`].
///
/// ```
/// use manim_nn::blockdiagram::LayerBlockDiagram;
/// let d = LayerBlockDiagram::mlp(&[784, 128, 64, 10]);
/// // input + (linear, relu) × 2 + output = 6 nodes.
/// assert_eq!(d.graph().node_count(), 6);
/// ```
pub struct LayerBlockDiagram {
    graph: ComputeGraph,
}

impl LayerBlockDiagram {
    /// The underlying compute graph.
    pub fn graph(&self) -> &ComputeGraph {
        &self.graph
    }

    /// Renders the diagram into `scene` (delegates to
    /// [`ComputeGraph::render`]).
    ///
    /// ```
    /// use manim_core::scene_state::SceneState;
    /// use manim_nn::blockdiagram::LayerBlockDiagram;
    /// let mut scene = SceneState::new();
    /// let d = LayerBlockDiagram::mlp(&[4, 3, 2]);
    /// let g = d.render(&mut scene);
    /// assert!(scene.family(g.erase()).len() > 1);
    /// ```
    pub fn render(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        self.graph.render(scene)
    }

    /// A multilayer perceptron: `input → (linear, activation)* → output`, one
    /// hidden `(linear, relu)` pair per interior layer size, with each block's
    /// shape caption set to its size.
    ///
    /// ```
    /// use manim_nn::blockdiagram::LayerBlockDiagram;
    /// let d = LayerBlockDiagram::mlp(&[8, 4, 2]);
    /// // input, (linear, relu), output.
    /// assert_eq!(d.graph().node_count(), 4);
    /// assert_eq!(d.graph().ranks(), vec![0, 1, 2, 3]);
    /// ```
    pub fn mlp(layer_sizes: &[usize]) -> Self {
        let mut g = ComputeGraph::new();
        if layer_sizes.is_empty() {
            return Self { graph: g };
        }
        let mut prev = g.add_node(NodeKind::Input, "input", Some(vec![layer_sizes[0]]));
        let last = layer_sizes.len() - 1;
        for (i, &sz) in layer_sizes.iter().enumerate().skip(1) {
            if i == last {
                let out = g.add_node(NodeKind::Output, "output", Some(vec![sz]));
                g.add_edge(prev, out);
            } else {
                let lin = g.add_node(NodeKind::Linear, format!("linear {sz}"), Some(vec![sz]));
                g.add_edge(prev, lin);
                let act = g.add_node(NodeKind::Activation, "relu", Some(vec![sz]));
                g.add_edge(lin, act);
                prev = act;
            }
        }
        Self { graph: g }
    }

    /// A LeNet-style convolutional network:
    /// `conv → pool → conv → pool → fc → fc → output`, with LeNet-5 tensor
    /// shapes on each block.
    ///
    /// ```
    /// use manim_nn::blockdiagram::LayerBlockDiagram;
    /// let d = LayerBlockDiagram::lenet();
    /// assert_eq!(d.graph().node_count(), 8);
    /// // A straight chain, so ranks are 0..8.
    /// assert_eq!(d.graph().ranks(), (0..8).collect::<Vec<_>>());
    /// ```
    pub fn lenet() -> Self {
        let layers: [(NodeKind, &str, &[usize]); 8] = [
            (NodeKind::Input, "input", &[1, 32, 32]),
            (NodeKind::Conv, "conv 5×5", &[6, 28, 28]),
            (NodeKind::Pool, "pool", &[6, 14, 14]),
            (NodeKind::Conv, "conv 5×5", &[16, 10, 10]),
            (NodeKind::Pool, "pool", &[16, 5, 5]),
            (NodeKind::Linear, "fc", &[120]),
            (NodeKind::Linear, "fc", &[84]),
            (NodeKind::Output, "output", &[10]),
        ];
        Self {
            graph: ComputeGraph::sequential(&layers),
        }
    }

    /// A single transformer encoder block:
    /// `input → LayerNorm → Multi-Head Attention → add → LayerNorm → FFN →
    /// add → output`, plus the **two residual/skip edges** — `input → add₁`
    /// (around attention) and `add₁ → add₂` (around the FFN). Both skips span
    /// more than one rank, so [`ComputeGraph`] routes them as bowed arcs.
    ///
    /// ```
    /// use manim_nn::blockdiagram::LayerBlockDiagram;
    /// let d = LayerBlockDiagram::transformer_block();
    /// let g = d.graph();
    /// assert_eq!(g.node_count(), 8);
    /// // Exactly two edges span more than one rank: the residual connections.
    /// let ranks = g.ranks();
    /// let skips = g
    ///     .edges()
    ///     .iter()
    ///     .filter(|(f, t)| ranks[t.0] - ranks[f.0] > 1)
    ///     .count();
    /// assert_eq!(skips, 2);
    /// ```
    pub fn transformer_block() -> Self {
        let mut g = ComputeGraph::new();
        let input = g.add_node(NodeKind::Input, "input", Some(vec![512]));
        let norm1 = g.add_node(NodeKind::Norm, "layernorm", None);
        let attn = g.add_node(NodeKind::Attention, "multi-head attn", Some(vec![512]));
        let add1 = g.add_node(NodeKind::Other, "add", Some(vec![512]));
        let norm2 = g.add_node(NodeKind::Norm, "layernorm", None);
        let ffn = g.add_node(NodeKind::Linear, "ffn", Some(vec![2048]));
        let add2 = g.add_node(NodeKind::Other, "add", Some(vec![512]));
        let output = g.add_node(NodeKind::Output, "output", Some(vec![512]));

        // Main path.
        g.add_edge(input, norm1);
        g.add_edge(norm1, attn);
        g.add_edge(attn, add1);
        g.add_edge(add1, norm2);
        g.add_edge(norm2, ffn);
        g.add_edge(ffn, add2);
        g.add_edge(add2, output);

        // Residual (skip) connections — these span multiple ranks.
        g.add_edge(input, add1);
        g.add_edge(add1, add2);

        Self { graph: g }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mlp_structure() {
        let d = LayerBlockDiagram::mlp(&[784, 256, 128, 10]);
        // input + (linear, relu) × 2 + output.
        assert_eq!(d.graph().node_count(), 6);
        assert_eq!(d.graph().edges().len(), 5);
    }

    #[test]
    fn lenet_is_a_chain() {
        let d = LayerBlockDiagram::lenet();
        assert_eq!(d.graph().node_count(), 8);
        assert_eq!(d.graph().edges().len(), 7);
        assert_eq!(d.graph().ranks(), (0..8).collect::<Vec<_>>());
    }

    #[test]
    fn transformer_block_has_two_skip_edges() {
        let d = LayerBlockDiagram::transformer_block();
        let g = d.graph();
        assert_eq!(g.node_count(), 8);
        let ranks = g.ranks();
        let skips: Vec<_> = g
            .edges()
            .iter()
            .filter(|(f, t)| ranks[t.0] - ranks[f.0] > 1)
            .collect();
        assert_eq!(skips.len(), 2, "the two residual connections span >1 rank");
    }

    #[test]
    fn empty_mlp_is_empty() {
        let d = LayerBlockDiagram::mlp(&[]);
        assert_eq!(d.graph().node_count(), 0);
    }
}
