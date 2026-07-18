//! A layered compute-graph visualizer.
//!
//! [`ComputeGraph`] is a directed acyclic graph of neural-network operations
//! that lays itself out as a left-to-right block diagram. Layout is the
//! classic *layered* (Sugiyama-style) recipe:
//!
//! 1. **Rank assignment** by **longest path** from a source: a node's rank is
//!    one more than the maximum rank of its predecessors, so every source sits
//!    at rank `0`. Ranks map to the x-axis (`x = rank · dx`), giving a strict
//!    left-to-right flow.
//! 2. **Within-rank ordering** by the **barycenter heuristic**: each node is
//!    positioned at the average order-index of its neighbours in the adjacent
//!    rank, then the rank is re-sorted by that barycenter. We run **two
//!    down-then-up sweeps** (a down sweep orders each rank by its predecessors,
//!    an up sweep by its successors; two full iterations of the pair). Vertical
//!    placement (`y`) follows the resulting order, centered on the origin.
//!
//! Each node renders as a [`RoundedRectangle`] block (colored by
//! [`NodeKind`]) with a [`Text`] label and an optional tensor-shape caption;
//! each edge renders as a smooth cubic spline with horizontal tangents.
//! **Skip connections** (edges spanning more than one rank) are bowed off-axis
//! so they arc around the intervening ranks.

use manim_core::prelude::*;
use manim_math::bezier::CubicBezier;
use manim_math::path::{Path, SubPath};
use manim_text::Text;

/// Rank spacing along the flow axis (x), in scene units.
const DX: f32 = 2.8;
/// Node spacing within a rank (y), in scene units.
const DY: f32 = 1.4;

/// An index into a [`ComputeGraph`]'s node list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(
    /// The position of the node in the graph's insertion order.
    pub usize,
);

/// The role of a node, which determines its block color and default size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// A network input (e.g. the source tensor).
    Input,
    /// A dense / fully-connected (linear) layer.
    Linear,
    /// A convolutional layer.
    Conv,
    /// An attention block (e.g. multi-head self-attention).
    Attention,
    /// A normalization layer (e.g. LayerNorm / BatchNorm).
    Norm,
    /// A pointwise activation (e.g. ReLU / GELU).
    Activation,
    /// A pooling / downsampling layer.
    Pool,
    /// A network output (e.g. logits).
    Output,
    /// Any other op (e.g. a residual add or a reshape).
    Other,
}

/// A single node: its [`NodeKind`], a display label, and optional tensor-shape
/// metadata used both for the block caption and (potentially) for sizing.
#[derive(Clone, Debug)]
pub struct Node {
    /// The node's role.
    pub kind: NodeKind,
    /// The human-readable label drawn on the block.
    pub label: String,
    /// The output tensor shape, if known (drawn as a small caption).
    pub shape: Option<Vec<usize>>,
}

/// A layered directed acyclic graph of ops, laid out left-to-right.
///
/// ```
/// use manim_nn::graph::{ComputeGraph, NodeKind};
/// let g = ComputeGraph::sequential(&[
///     (NodeKind::Input, "x", &[8]),
///     (NodeKind::Linear, "fc", &[4]),
///     (NodeKind::Output, "y", &[2]),
/// ]);
/// assert_eq!(g.node_count(), 3);
/// assert_eq!(g.edges().len(), 2);
/// assert_eq!(g.ranks(), vec![0, 1, 2]);
/// ```
#[derive(Clone, Debug, Default)]
pub struct ComputeGraph {
    nodes: Vec<Node>,
    edges: Vec<(NodeId, NodeId)>,
}

impl ComputeGraph {
    /// An empty graph.
    ///
    /// ```
    /// use manim_nn::graph::ComputeGraph;
    /// assert_eq!(ComputeGraph::new().node_count(), 0);
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node and returns its [`NodeId`].
    pub fn add_node(
        &mut self,
        kind: NodeKind,
        label: impl Into<String>,
        shape: Option<Vec<usize>>,
    ) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node {
            kind,
            label: label.into(),
            shape,
        });
        id
    }

    /// Adds a directed edge `from → to`.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) {
        self.edges.push((from, to));
    }

    /// Builds a straight chain `input → … → output` from `layers`, one node per
    /// tuple `(kind, label, shape)` (an empty shape slice means "no shape"),
    /// with an edge between each consecutive pair.
    ///
    /// ```
    /// use manim_nn::graph::{ComputeGraph, NodeKind};
    /// let g = ComputeGraph::sequential(&[
    ///     (NodeKind::Input, "in", &[16]),
    ///     (NodeKind::Linear, "h", &[8]),
    ///     (NodeKind::Output, "out", &[1]),
    /// ]);
    /// assert_eq!(g.ranks(), vec![0, 1, 2]);
    /// ```
    pub fn sequential(layers: &[(NodeKind, &str, &[usize])]) -> Self {
        let mut g = Self::new();
        let mut prev: Option<NodeId> = None;
        for (kind, label, shape) in layers {
            let shape = if shape.is_empty() {
                None
            } else {
                Some(shape.to_vec())
            };
            let id = g.add_node(*kind, *label, shape);
            if let Some(p) = prev {
                g.add_edge(p, id);
            }
            prev = Some(id);
        }
        g
    }

    /// The number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The edges, in insertion order.
    pub fn edges(&self) -> &[(NodeId, NodeId)] {
        &self.edges
    }

    /// The rank (layer index) of each node, by longest path from any source.
    ///
    /// `rank(v) = 0` if `v` has no predecessors, else
    /// `max(rank(p) + 1)` over predecessors `p`.
    ///
    /// ```
    /// use manim_nn::graph::{ComputeGraph, NodeKind};
    /// // Diamond DAG: A→B, A→C, B→D, C→D.
    /// let mut g = ComputeGraph::new();
    /// let a = g.add_node(NodeKind::Input, "A", None);
    /// let b = g.add_node(NodeKind::Linear, "B", None);
    /// let c = g.add_node(NodeKind::Linear, "C", None);
    /// let d = g.add_node(NodeKind::Output, "D", None);
    /// g.add_edge(a, b);
    /// g.add_edge(a, c);
    /// g.add_edge(b, d);
    /// g.add_edge(c, d);
    /// assert_eq!(g.ranks(), vec![0, 1, 1, 2]);
    /// ```
    pub fn ranks(&self) -> Vec<usize> {
        let n = self.nodes.len();
        let mut rank = vec![0usize; n];
        // Relax edges until the longest-path ranks stabilize. A DAG converges
        // in at most `n` passes.
        for _ in 0..n {
            let mut changed = false;
            for &(from, to) in &self.edges {
                let cand = rank[from.0] + 1;
                if rank[to.0] < cand {
                    rank[to.0] = cand;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        rank
    }

    /// The block size `(width, height)` for a node, chosen by its
    /// [`NodeKind`] (bigger roles get bigger blocks).
    pub fn block_size(&self, node: NodeId) -> (f32, f32) {
        kind_dims(self.nodes[node.0].kind)
    }

    /// The laid-out center point of each node's block, in scene coordinates,
    /// indexed by node.
    ///
    /// ```
    /// use manim_nn::graph::{ComputeGraph, NodeKind};
    /// let g = ComputeGraph::sequential(&[
    ///     (NodeKind::Input, "a", &[]),
    ///     (NodeKind::Output, "b", &[]),
    /// ]);
    /// let p = g.node_positions();
    /// assert_eq!(p.len(), g.node_count());
    /// // Rank increases left-to-right, so b is to the right of a.
    /// assert!(p[1].x > p[0].x);
    /// ```
    pub fn node_positions(&self) -> Vec<Point> {
        let ranks = self.ranks();
        let order = self.barycenter_order(&ranks);
        let pos = pos_within_rank(&order, self.nodes.len());
        let max_rank = ranks.iter().copied().max().unwrap_or(0);
        let mut out = Vec::with_capacity(self.nodes.len());
        for i in 0..self.nodes.len() {
            let r = ranks[i];
            let k = order[r].len();
            let o = pos[i];
            let x = (r as f32 - max_rank as f32 / 2.0) * DX;
            let y = ((k as f32 - 1.0) / 2.0 - o as f32) * DY;
            out.push(Point::new(x, y, 0.0));
        }
        out
    }

    /// The `(source-exit, target-entry)` anchor pair for each edge, in edge
    /// order. The exit sits on the right border of the source block and the
    /// entry on the left border of the target block.
    ///
    /// ```
    /// use manim_nn::graph::{ComputeGraph, NodeKind};
    /// let g = ComputeGraph::sequential(&[
    ///     (NodeKind::Input, "a", &[]),
    ///     (NodeKind::Output, "b", &[]),
    /// ]);
    /// let a = g.edge_anchors();
    /// assert_eq!(a.len(), 1);
    /// // Exit (right of source) is left of entry (left of target).
    /// assert!(a[0].0.x < a[0].1.x);
    /// ```
    pub fn edge_anchors(&self) -> Vec<(Point, Point)> {
        let pos = self.node_positions();
        self.edges
            .iter()
            .map(|&(from, to)| {
                let (aw, _) = self.block_size(from);
                let (bw, _) = self.block_size(to);
                let exit = pos[from.0] + Point::new(aw / 2.0, 0.0, 0.0);
                let entry = pos[to.0] - Point::new(bw / 2.0, 0.0, 0.0);
                (exit, entry)
            })
            .collect()
    }

    /// The number of edge crossings between adjacent ranks, given the current
    /// barycenter ordering. Lower is better; used to test the ordering.
    ///
    /// ```
    /// use manim_nn::graph::{ComputeGraph, NodeKind};
    /// // A→D, B→C laid out on two ranks: barycenter ordering uncrosses them.
    /// let mut g = ComputeGraph::new();
    /// let a = g.add_node(NodeKind::Input, "A", None);
    /// let b = g.add_node(NodeKind::Input, "B", None);
    /// let c = g.add_node(NodeKind::Output, "C", None);
    /// let d = g.add_node(NodeKind::Output, "D", None);
    /// g.add_edge(a, d);
    /// g.add_edge(b, c);
    /// assert_eq!(g.crossing_count(), 0);
    /// ```
    pub fn crossing_count(&self) -> usize {
        let ranks = self.ranks();
        let order = self.barycenter_order(&ranks);
        self.crossings_of(&ranks, &order)
    }

    /// Renders the graph into `scene`: a [`RoundedRectangle`] block plus label
    /// (and shape caption) per node, and a smooth cubic spline per edge, all
    /// collected under one returned [`VGroup`].
    ///
    /// ```
    /// use manim_core::scene_state::SceneState;
    /// use manim_nn::graph::{ComputeGraph, NodeKind};
    /// let mut scene = SceneState::new();
    /// let g = ComputeGraph::sequential(&[
    ///     (NodeKind::Input, "x", &[4]),
    ///     (NodeKind::Output, "y", &[2]),
    /// ]);
    /// let group = g.render(&mut scene);
    /// // The group has descendants (blocks, labels, edges).
    /// assert!(scene.family(group.erase()).len() > 1);
    /// ```
    pub fn render(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        let group = scene.add(VGroup::new());
        let positions = self.node_positions();
        let ranks = self.ranks();

        for (i, node) in self.nodes.iter().enumerate() {
            let (w, h) = kind_dims(node.kind);
            let color = kind_color(node.kind);
            let radius = (0.18_f32).min(h / 2.0);
            let rect = RoundedRectangle::with_params(w, h, radius)
                .with_fill(color, 0.22)
                .with_stroke(color, 3.0, 1.0)
                .with_move_to(positions[i]);
            let rid = scene.add(rect);
            scene.add_child(group, rid);

            let has_shape = node.shape.is_some();
            let label = Text::new(node.label.clone()).font_size(22.0).add_to(scene);
            let label_y = if has_shape { h * 0.16 } else { 0.0 };
            scene.move_to(label, positions[i] + Point::new(0.0, label_y, 0.0));
            scene.add_child(group, label);

            if let Some(shape) = &node.shape {
                let caption = shape
                    .iter()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
                    .join("×");
                let cap = Text::new(caption).font_size(15.0).add_to(scene);
                scene.move_to(cap, positions[i] - Point::new(0.0, h * 0.24, 0.0));
                scene.add_child(group, cap);
            }
        }

        let anchors = self.edge_anchors();
        for (idx, &(from, to)) in self.edges.iter().enumerate() {
            let (a, b) = anchors[idx];
            let span = ranks[to.0].saturating_sub(ranks[from.0]);
            let curve = edge_curve(a, b, span);
            let path = Path {
                subpaths: vec![SubPath {
                    curves: vec![curve],
                    closed: false,
                }],
            };
            let edge = VMobject::from_path(path).with_stroke(Color::GRAY, 2.5, 0.9);
            let eid = scene.add(edge);
            scene.add_child(group, eid);
        }

        group
    }

    // --- layout internals (also exercised by unit tests) ---

    /// The insertion-order layering: `rank -> node indices` in the order they
    /// were added. This is the *before-optimization* ordering.
    fn insertion_order(&self, ranks: &[usize]) -> Vec<Vec<usize>> {
        let max_rank = ranks.iter().copied().max().unwrap_or(0);
        let mut order = vec![Vec::new(); max_rank + 1];
        for (i, &r) in ranks.iter().enumerate() {
            order[r].push(i);
        }
        order
    }

    /// The layering after **two down-then-up barycenter sweeps**.
    fn barycenter_order(&self, ranks: &[usize]) -> Vec<Vec<usize>> {
        let n = self.nodes.len();
        let max_rank = ranks.iter().copied().max().unwrap_or(0);
        let mut order = self.insertion_order(ranks);

        // Predecessor / successor adjacency.
        let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut succs: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &(from, to) in &self.edges {
            succs[from.0].push(to.0);
            preds[to.0].push(from.0);
        }

        for _ in 0..2 {
            // Down sweep: order each rank by the barycenter of its predecessors.
            for r in 1..=max_rank {
                let pos = pos_within_rank(&order, n);
                sort_rank_by_barycenter(&mut order[r], &preds, &pos, ranks, r - 1);
            }
            // Up sweep: order each rank by the barycenter of its successors.
            for r in (0..max_rank).rev() {
                let pos = pos_within_rank(&order, n);
                sort_rank_by_barycenter(&mut order[r], &succs, &pos, ranks, r + 1);
            }
        }
        order
    }

    /// Counts crossings between every pair of adjacent ranks for `order`.
    fn crossings_of(&self, ranks: &[usize], order: &[Vec<usize>]) -> usize {
        let n = self.nodes.len();
        let pos = pos_within_rank(order, n);
        let max_rank = ranks.iter().copied().max().unwrap_or(0);
        let mut count = 0;
        for r in 0..max_rank {
            // Segments running from rank r to rank r+1, as (pos_from, pos_to).
            let segs: Vec<(usize, usize)> = self
                .edges
                .iter()
                .filter(|&&(f, t)| ranks[f.0] == r && ranks[t.0] == r + 1)
                .map(|&(f, t)| (pos[f.0], pos[t.0]))
                .collect();
            for i in 0..segs.len() {
                for j in (i + 1)..segs.len() {
                    let (a1, b1) = segs[i];
                    let (a2, b2) = segs[j];
                    if (a1 < a2 && b1 > b2) || (a1 > a2 && b1 < b2) {
                        count += 1;
                    }
                }
            }
        }
        count
    }
}

/// Node index -> its position within its own rank, for the given `order`.
fn pos_within_rank(order: &[Vec<usize>], n: usize) -> Vec<usize> {
    let mut pos = vec![0usize; n];
    for rank_nodes in order {
        for (i, &v) in rank_nodes.iter().enumerate() {
            pos[v] = i;
        }
    }
    pos
}

/// Stable-sorts the nodes in one rank by the average position of their
/// neighbours (in `adj`) that lie in `neighbor_rank`. Nodes with no such
/// neighbour keep their current position (barycenter = their own index).
fn sort_rank_by_barycenter(
    rank_nodes: &mut [usize],
    adj: &[Vec<usize>],
    pos: &[usize],
    ranks: &[usize],
    neighbor_rank: usize,
) {
    let mut keyed: Vec<(f32, usize)> = rank_nodes
        .iter()
        .map(|&v| {
            let mut sum = 0usize;
            let mut count = 0usize;
            for &nb in &adj[v] {
                if ranks[nb] == neighbor_rank {
                    sum += pos[nb];
                    count += 1;
                }
            }
            let bary = if count == 0 {
                pos[v] as f32
            } else {
                sum as f32 / count as f32
            };
            (bary, v)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (slot, (_, v)) in rank_nodes.iter_mut().zip(keyed) {
        *slot = v;
    }
}

/// The fill/stroke color for a node role.
fn kind_color(kind: NodeKind) -> Color {
    match kind {
        NodeKind::Input => GREEN,
        NodeKind::Linear => BLUE,
        NodeKind::Conv => Color::TEAL,
        NodeKind::Attention => PURPLE,
        NodeKind::Norm => Color::GOLD,
        NodeKind::Activation => YELLOW,
        NodeKind::Pool => Color::MAROON,
        NodeKind::Output => RED,
        NodeKind::Other => Color::GRAY,
    }
}

/// The block `(width, height)` for a node role.
fn kind_dims(kind: NodeKind) -> (f32, f32) {
    match kind {
        NodeKind::Input | NodeKind::Output => (1.4, 0.8),
        NodeKind::Norm => (1.7, 0.6),
        NodeKind::Activation => (1.2, 0.7),
        NodeKind::Pool => (1.4, 0.8),
        NodeKind::Attention => (2.1, 1.0),
        NodeKind::Conv => (1.9, 1.0),
        NodeKind::Linear => (1.6, 0.9),
        NodeKind::Other => (1.3, 0.7),
    }
}

/// The cubic spline for an edge from `a` (source exit) to `b` (target entry).
///
/// The handles are horizontal at both endpoints so the curve leaves and enters
/// the blocks cleanly. When `span > 1` the edge is a **skip connection**: its
/// handles are pushed off-axis (and lengthened) so it bows around the ranks it
/// jumps over.
fn edge_curve(a: Point, b: Point, span: usize) -> CubicBezier {
    let dx = (b.x - a.x).abs().max(0.1);
    if span > 1 {
        // Bow the skip edge upward, growing with the number of ranks jumped.
        let bow = 0.5 + 0.55 * span as f32;
        let reach = dx * 0.35;
        let h1 = a + Point::new(reach, bow, 0.0);
        let h2 = b + Point::new(-reach, bow, 0.0);
        CubicBezier::new(a, h1, h2, b)
    } else {
        let reach = dx * 0.45;
        let h1 = a + Point::new(reach, 0.0, 0.0);
        let h2 = b + Point::new(-reach, 0.0, 0.0);
        CubicBezier::new(a, h1, h2, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diamond_ranks() {
        let mut g = ComputeGraph::new();
        let a = g.add_node(NodeKind::Input, "A", None);
        let b = g.add_node(NodeKind::Linear, "B", None);
        let c = g.add_node(NodeKind::Linear, "C", None);
        let d = g.add_node(NodeKind::Output, "D", None);
        g.add_edge(a, b);
        g.add_edge(a, c);
        g.add_edge(b, d);
        g.add_edge(c, d);
        assert_eq!(g.ranks(), vec![0, 1, 1, 2]);
    }

    #[test]
    fn barycenter_reduces_crossings() {
        // A→D, B→C on two ranks; insertion order crosses, barycenter uncrosses.
        let mut g = ComputeGraph::new();
        let a = g.add_node(NodeKind::Input, "A", None);
        let b = g.add_node(NodeKind::Input, "B", None);
        let c = g.add_node(NodeKind::Output, "C", None);
        let d = g.add_node(NodeKind::Output, "D", None);
        g.add_edge(a, d);
        g.add_edge(b, c);

        let ranks = g.ranks();
        let before = g.crossings_of(&ranks, &g.insertion_order(&ranks));
        let after = g.crossing_count();
        assert!(after <= before, "crossings should not increase");
        assert!(
            after < before,
            "constructed crossing should be removed (before={before}, after={after})"
        );
        assert_eq!(before, 1);
        assert_eq!(after, 0);
    }

    #[test]
    fn edge_anchors_lie_on_block_borders() {
        let mut g = ComputeGraph::new();
        let a = g.add_node(NodeKind::Input, "A", None);
        let b = g.add_node(NodeKind::Linear, "B", None);
        let c = g.add_node(NodeKind::Output, "C", None);
        g.add_edge(a, b);
        g.add_edge(b, c);

        let pos = g.node_positions();
        let anchors = g.edge_anchors();
        for (idx, &(from, to)) in g.edges().iter().enumerate() {
            let (exit, entry) = anchors[idx];
            let (aw, ah) = g.block_size(from);
            let (bw, bh) = g.block_size(to);
            let ac = pos[from.0];
            let bc = pos[to.0];
            // Exit on the right border of the source, within its height.
            assert!((exit.x - (ac.x + aw / 2.0)).abs() < 1e-4);
            assert!((exit.y - ac.y).abs() <= ah / 2.0 + 1e-4);
            // Entry on the left border of the target, within its height.
            assert!((entry.x - (bc.x - bw / 2.0)).abs() < 1e-4);
            assert!((entry.y - bc.y).abs() <= bh / 2.0 + 1e-4);
        }
    }

    #[test]
    fn sequential_shape() {
        let g = ComputeGraph::sequential(&[
            (NodeKind::Input, "a", &[8]),
            (NodeKind::Linear, "b", &[6]),
            (NodeKind::Linear, "c", &[4]),
            (NodeKind::Output, "d", &[2]),
        ]);
        assert_eq!(g.node_count(), 4);
        assert_eq!(g.edges().len(), 3);
        assert_eq!(g.ranks(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn positions_match_node_count() {
        let g = ComputeGraph::sequential(&[
            (NodeKind::Input, "a", &[]),
            (NodeKind::Linear, "b", &[]),
            (NodeKind::Output, "c", &[]),
        ]);
        assert_eq!(g.node_positions().len(), g.node_count());
    }
}
