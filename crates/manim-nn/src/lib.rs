//! `manim-nn`: neural-network visualizers for `manim_rust`.
//!
//! - [`graph`] — [`ComputeGraph`](graph::ComputeGraph): a layered DAG laid out by
//!   longest-path ranking + barycenter ordering, rendered as blocks + edge
//!   splines.
//! - [`blockdiagram`] — [`LayerBlockDiagram`](blockdiagram::LayerBlockDiagram):
//!   opinionated architecture diagrams (MLP / CNN / transformer presets).
//! - [`heatmap`] — weight and attention heatmaps (via `manim_sci`'s material
//!   quad).
//! - [`landscape`] — [`LossLandscape`](landscape::LossLandscape): a height-field
//!   surface with SGD / momentum / Adam descent trajectories.
//! - [`flow`] — forward-pass activation pulses along graph edges.

pub mod blockdiagram;
pub mod flow;
pub mod graph;
pub mod heatmap;
pub mod landscape;
