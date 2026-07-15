//! [`Graph`] and [`DiGraph`]: node–link diagrams with deterministic layouts.
//!
//! Port of manim CE's `Graph` / `DiGraph`. The module is named `network` to
//! avoid clashing with the [`graphing`](crate::graphing) module (function
//! plots).
//!
//! # Design: single mobject, index-addressable
//!
//! A graph is one mobject whose path holds a small filled circle per vertex and
//! a line per edge (plus an arrow tip per edge for a [`DiGraph`]). Vertices are
//! addressed by index via [`Graph::vertex_point`], and
//! [`Graph::change_layout`] recomputes the whole path — edges therefore follow
//! their vertices with no updaters. This is the simpler, fully deterministic
//! choice (per the FE-105 brief) over arena-child vertices/edges, which would
//! require scene insertion; graphs are monochrome, so per-vertex styling is not
//! needed.

use manim_color::gradient::ColorRng;
use manim_color::WHITE;
use manim_math::path::{Path, SubPath};
use manim_math::space_ops::normalize_or_zero;
use manim_math::{Point, ORIGIN, TAU};

use crate::impl_mobject;
use crate::mobject::MobjectData;
use crate::style::Style;

/// Default vertex dot radius (scene units).
pub const VERTEX_RADIUS: f32 = 0.12;

/// A vertex layout algorithm. All are deterministic; [`Spring`](Self::Spring)
/// takes an explicit `seed` (no global RNG).
#[derive(Debug, Clone, PartialEq)]
pub enum GraphLayout {
    /// Evenly spaced on a circle of the given radius.
    Circular {
        /// Circle radius in scene units.
        radius: f32,
    },
    /// Fruchterman–Reingold force-directed layout, seeded for reproducibility.
    Spring {
        /// RNG seed for the initial placement.
        seed: u64,
        /// Number of relaxation iterations.
        iterations: usize,
    },
    /// Concentric rings: `rings[k]` is placed on a circle of radius `k + 1`.
    Shell {
        /// Vertex indices per ring, innermost first.
        rings: Vec<Vec<usize>>,
    },
    /// Rooted tree: BFS layers from `root`, spread horizontally.
    Tree {
        /// The root vertex index.
        root: usize,
    },
}

impl GraphLayout {
    /// A circular layout of radius `2.0` (the common default).
    pub fn circular() -> Self {
        GraphLayout::Circular { radius: 2.0 }
    }

    /// A spring layout with `seed` and 100 iterations.
    pub fn spring(seed: u64) -> Self {
        GraphLayout::Spring {
            seed,
            iterations: 100,
        }
    }
}

/// A node–link diagram: vertices (dots) joined by edges (lines), with a
/// deterministic [`GraphLayout`].
///
/// ```
/// use manim_core::network::{Graph, GraphLayout};
/// // A triangle laid out on a circle.
/// let g = Graph::new(3, &[(0, 1), (1, 2), (2, 0)], GraphLayout::circular());
/// assert_eq!(g.vertex_count(), 3);
/// // Vertex 0 sits on the layout circle (radius 2).
/// assert!((g.vertex_point(0).length() - 2.0).abs() < 1e-5);
/// ```
#[derive(Clone)]
pub struct Graph {
    data: MobjectData,
    edges: Vec<(usize, usize)>,
    positions: Vec<Point>,
    directed: bool,
    vertex_radius: f32,
    tip_length: f32,
}
impl_mobject!(Graph);

impl Graph {
    /// An undirected graph on `n_vertices` with `edges`, laid out by `layout`.
    pub fn new(n_vertices: usize, edges: &[(usize, usize)], layout: GraphLayout) -> Self {
        Self::build(n_vertices, edges, layout, false)
    }

    /// A directed graph (edges drawn with arrow tips).
    pub fn new_directed(n_vertices: usize, edges: &[(usize, usize)], layout: GraphLayout) -> Self {
        Self::build(n_vertices, edges, layout, true)
    }

    fn build(
        n_vertices: usize,
        edges: &[(usize, usize)],
        layout: GraphLayout,
        directed: bool,
    ) -> Self {
        let positions = compute_layout(n_vertices, edges, &layout);
        let mut g = Self {
            data: MobjectData::new(Path::default(), graph_style()),
            edges: edges.to_vec(),
            positions,
            directed,
            vertex_radius: VERTEX_RADIUS,
            tip_length: 0.2,
        };
        g.rebuild();
        g
    }

    /// The number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// The number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// The scene point of vertex `i`.
    pub fn vertex_point(&self, i: usize) -> Point {
        self.positions[i]
    }

    /// Whether the graph is directed.
    pub fn is_directed(&self) -> bool {
        self.directed
    }

    /// Recomputes vertex positions with `layout` and rebuilds the geometry
    /// (edges follow their vertices).
    ///
    /// ```
    /// use manim_core::network::{Graph, GraphLayout};
    /// let mut g = Graph::new(4, &[(0, 1), (1, 2)], GraphLayout::circular());
    /// g.change_layout(GraphLayout::Tree { root: 0 });
    /// // The root is at the top layer.
    /// assert!(g.vertex_point(0).y >= g.vertex_point(1).y);
    /// ```
    pub fn change_layout(&mut self, layout: GraphLayout) {
        self.positions = compute_layout(self.positions.len(), &self.edges, &layout);
        self.rebuild();
    }

    /// Rebuilds the path: one filled circle per vertex, one line per edge (with
    /// a tip for directed graphs).
    fn rebuild(&mut self) {
        let mut subpaths = Vec::new();

        // Edges first, so vertex dots draw on top of edge ends.
        for &(u, v) in &self.edges {
            let (a, b) = (self.positions[u], self.positions[v]);
            if self.directed {
                // Stop the shaft at the target vertex's rim and add a tip.
                let dir = normalize_or_zero(b - a);
                let tip_base = b - dir * (self.vertex_radius + self.tip_length);
                subpaths.push(SubPath::from_corners(&[a, tip_base]));
                let perp = Point::new(-dir.y, dir.x, 0.0);
                let apex = b - dir * self.vertex_radius;
                let hw = self.tip_length * 0.5;
                let mut tip =
                    SubPath::from_corners(&[apex, tip_base + perp * hw, tip_base - perp * hw]);
                tip.closed = true;
                subpaths.push(tip);
            } else {
                subpaths.push(SubPath::from_corners(&[a, b]));
            }
        }

        for &c in &self.positions {
            subpaths.push(circle_subpath(c, self.vertex_radius));
        }

        self.data.path = Path { subpaths };
    }
}

/// A directed graph — a thin alias for [`Graph::new_directed`].
///
/// ```
/// use manim_core::network::{DiGraph, GraphLayout};
/// let g = DiGraph::new(3, &[(0, 1), (1, 2)], GraphLayout::circular());
/// assert!(g.is_directed());
/// ```
pub struct DiGraph;

impl DiGraph {
    /// Builds a directed [`Graph`] (a factory, so `new` returns a `Graph`).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(n_vertices: usize, edges: &[(usize, usize)], layout: GraphLayout) -> Graph {
        Graph::new_directed(n_vertices, edges, layout)
    }
}

/// The default graph style: filled and stroked white (dots + edges).
fn graph_style() -> Style {
    let mut s = Style::stroked(WHITE);
    s.fill_color = Some(WHITE);
    s.fill_opacity = 1.0;
    s.stroke_width = 2.5;
    s
}

/// A closed regular-polygon approximation of a circle at `center`.
fn circle_subpath(center: Point, radius: f32) -> SubPath {
    const SEGMENTS: usize = 24;
    let pts: Vec<Point> = (0..SEGMENTS)
        .map(|i| {
            let a = i as f32 / SEGMENTS as f32 * TAU;
            center + Point::new(radius * a.cos(), radius * a.sin(), 0.0)
        })
        .collect();
    let mut sp = SubPath::from_corners(&pts);
    sp.closed = true;
    sp
}

/// Dispatches to the requested layout algorithm.
fn compute_layout(n: usize, edges: &[(usize, usize)], layout: &GraphLayout) -> Vec<Point> {
    match layout {
        GraphLayout::Circular { radius } => circular_layout(n, *radius),
        GraphLayout::Spring { seed, iterations } => spring_layout(n, edges, *seed, *iterations),
        GraphLayout::Shell { rings } => shell_layout(n, rings),
        GraphLayout::Tree { root } => tree_layout(n, edges, *root),
    }
}

/// Evenly spaced on a circle of `radius`.
fn circular_layout(n: usize, radius: f32) -> Vec<Point> {
    (0..n)
        .map(|i| {
            let a = i as f32 / n.max(1) as f32 * TAU;
            Point::new(radius * a.cos(), radius * a.sin(), 0.0)
        })
        .collect()
}

/// Concentric rings; ring `k` gets radius `k + 1`. Vertices not in any ring
/// stay at the origin.
fn shell_layout(n: usize, rings: &[Vec<usize>]) -> Vec<Point> {
    let mut pos = vec![ORIGIN; n];
    for (k, ring) in rings.iter().enumerate() {
        let radius = (k + 1) as f32;
        let m = ring.len().max(1);
        for (i, &v) in ring.iter().enumerate() {
            if v < n {
                let a = i as f32 / m as f32 * TAU;
                pos[v] = Point::new(radius * a.cos(), radius * a.sin(), 0.0);
            }
        }
    }
    pos
}

/// Rooted BFS layers: each layer is a row (top → bottom), spread horizontally.
fn tree_layout(n: usize, edges: &[(usize, usize)], root: usize) -> Vec<Point> {
    let mut adj = vec![Vec::new(); n];
    for &(u, v) in edges {
        if u < n && v < n {
            adj[u].push(v);
            adj[v].push(u);
        }
    }
    let mut depth = vec![usize::MAX; n];
    let mut order = Vec::new();
    if root < n {
        depth[root] = 0;
        let mut queue = std::collections::VecDeque::from([root]);
        while let Some(u) = queue.pop_front() {
            order.push(u);
            for &w in &adj[u] {
                if depth[w] == usize::MAX {
                    depth[w] = depth[u] + 1;
                    queue.push_back(w);
                }
            }
        }
    }
    // Any unreached vertices (disconnected) go on a bottom layer.
    let max_depth = depth
        .iter()
        .filter(|&&d| d != usize::MAX)
        .copied()
        .max()
        .unwrap_or(0);
    for d in depth.iter_mut() {
        if *d == usize::MAX {
            *d = max_depth + 1;
        }
    }
    // Group by layer to spread each row.
    let layers = depth.iter().copied().max().unwrap_or(0) + 1;
    let mut per_layer: Vec<Vec<usize>> = vec![Vec::new(); layers];
    for v in 0..n {
        per_layer[depth[v]].push(v);
    }
    let mut pos = vec![ORIGIN; n];
    let dy = 1.5;
    for (d, row) in per_layer.iter().enumerate() {
        let m = row.len();
        for (i, &v) in row.iter().enumerate() {
            let x = if m == 1 {
                0.0
            } else {
                (i as f32 / (m - 1) as f32 - 0.5) * (m as f32).min(6.0)
            };
            let y = (layers as f32 - 1.0) * dy * 0.5 - d as f32 * dy;
            pos[v] = Point::new(x, y, 0.0);
        }
    }
    pos
}

/// Fruchterman–Reingold force-directed layout, seeded for determinism.
fn spring_layout(n: usize, edges: &[(usize, usize)], seed: u64, iterations: usize) -> Vec<Point> {
    if n == 0 {
        return Vec::new();
    }
    let (w, h) = (6.0_f32, 6.0_f32);
    let area = w * h;
    let k = (area / n as f32).sqrt();
    let mut rng = ColorRng::new(seed);
    let mut pos: Vec<Point> = (0..n)
        .map(|_| Point::new((rng.next_f32() - 0.5) * w, (rng.next_f32() - 0.5) * h, 0.0))
        .collect();

    let mut temp = w / 10.0;
    for _ in 0..iterations {
        let mut disp = vec![Point::ZERO; n];
        // Repulsion between every pair.
        for i in 0..n {
            for j in 0..n {
                if i != j {
                    let d = pos[i] - pos[j];
                    let dist = d.length().max(0.01);
                    disp[i] += d / dist * (k * k / dist);
                }
            }
        }
        // Attraction along edges.
        for &(u, v) in edges {
            if u < n && v < n {
                let d = pos[u] - pos[v];
                let dist = d.length().max(0.01);
                let delta = d / dist * (dist * dist / k);
                disp[u] -= delta;
                disp[v] += delta;
            }
        }
        // Displace, capped by temperature, then bound to the box.
        for i in 0..n {
            let dlen = disp[i].length().max(0.01);
            pos[i] += disp[i] / dlen * dlen.min(temp);
            pos[i].x = pos[i].x.clamp(-w / 2.0, w / 2.0);
            pos[i].y = pos[i].y.clamp(-h / 2.0, h / 2.0);
        }
        temp *= 0.95;
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::Mobject;

    #[test]
    fn circular_layout_on_the_circle() {
        let g = Graph::new(6, &[], GraphLayout::Circular { radius: 1.0 });
        for i in 0..6 {
            assert!((g.vertex_point(i).length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn spring_layout_is_seed_deterministic() {
        let edges = [(0, 1), (1, 2), (2, 3), (3, 0), (0, 2)];
        let a = Graph::new(
            4,
            &edges,
            GraphLayout::Spring {
                seed: 42,
                iterations: 60,
            },
        );
        let b = Graph::new(
            4,
            &edges,
            GraphLayout::Spring {
                seed: 42,
                iterations: 60,
            },
        );
        let c = Graph::new(
            4,
            &edges,
            GraphLayout::Spring {
                seed: 7,
                iterations: 60,
            },
        );
        for i in 0..4 {
            assert!((a.vertex_point(i) - b.vertex_point(i)).length() < 1e-6);
        }
        // A different seed gives a different layout.
        let differs = (0..4).any(|i| (a.vertex_point(i) - c.vertex_point(i)).length() > 1e-3);
        assert!(differs);
    }

    #[test]
    fn spring_layout_is_finite_and_bounded() {
        let edges = [(0, 1), (1, 2), (2, 3), (3, 4), (4, 0)];
        let g = Graph::new(
            5,
            &edges,
            GraphLayout::Spring {
                seed: 1,
                iterations: 200,
            },
        );
        for i in 0..5 {
            let p = g.vertex_point(i);
            assert!(p.is_finite());
            assert!(p.x.abs() <= 3.0 + 1e-3 && p.y.abs() <= 3.0 + 1e-3);
        }
    }

    #[test]
    fn tree_layers_by_depth() {
        // 0 -> {1, 2}; 1 -> 3. Depths: 0, 1, 1, 2.
        let g = Graph::new(4, &[(0, 1), (0, 2), (1, 3)], GraphLayout::Tree { root: 0 });
        let y0 = g.vertex_point(0).y;
        let y1 = g.vertex_point(1).y;
        let y3 = g.vertex_point(3).y;
        assert!(y0 > y1 + 1e-3, "root above its children");
        assert!(y1 > y3 + 1e-3, "each layer below the previous");
    }

    #[test]
    fn edge_endpoints_touch_vertex_centers() {
        let g = Graph::new(3, &[(0, 1)], GraphLayout::circular());
        // The first subpath is the single edge; its ends are vertex centers.
        let edge = &g.data().path.subpaths[0];
        let start = edge.curves.first().unwrap().p0;
        let end = edge.curves.last().unwrap().p3;
        assert!((start - g.vertex_point(0)).length() < 1e-5);
        assert!((end - g.vertex_point(1)).length() < 1e-5);
    }

    #[test]
    fn directed_graph_adds_tips() {
        let undirected = Graph::new(2, &[(0, 1)], GraphLayout::circular());
        let directed = Graph::new_directed(2, &[(0, 1)], GraphLayout::circular());
        // Directed edges add a (closed) tip subpath, so more subpaths overall.
        assert!(directed.data().path.subpaths.len() > undirected.data().path.subpaths.len());
    }

    #[test]
    fn shell_layout_places_rings() {
        let rings = vec![vec![0], vec![1, 2, 3]];
        let g = Graph::new(4, &[], GraphLayout::Shell { rings });
        // Inner ring at radius 1, outer ring at radius 2.
        assert!((g.vertex_point(0).length() - 1.0).abs() < 1e-5);
        for i in 1..4 {
            assert!((g.vertex_point(i).length() - 2.0).abs() < 1e-5);
        }
    }
}
