# Neural Networks

> **Crate:** `manim-nn` — modules `graph`, `blockdiagram`, `heatmap`,
> `landscape`, `flow`.

Architecture diagrams are drawn by hand far more often than they should be,
which is why so many of them are wrong: the skip connection lands on the wrong
side of the norm, the shapes don't multiply. If the diagram is generated from a
graph description, it cannot disagree with itself.

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/nn/transformer_block.mp4">
  </video>
  <figcaption>
    A transformer block laid out automatically as a compute graph, drawn in and
    then swept by a forward-pass activation pulse staggered rank by rank.
  </figcaption>
</figure>

## Layout is a solved problem, so solve it

`ComputeGraph` takes a DAG and lays it out with the standard two-phase
approach:

1. **Longest-path ranking** assigns each node a layer — a node sits one rank
   past its deepest predecessor. This guarantees every edge points forward and
   the diagram reads in one direction.
2. **Barycenter ordering** sweeps within each rank, repeatedly placing each node
   at the average position of its neighbours. This is the classic
   Sugiyama-style crossing-reduction heuristic, and it is what stops a residual
   stream from weaving through the attention block.

Edges route as splines, with skip connections deliberately bowed clear of the
intervening ranks — the residual path in the figure above is legible precisely
because it does not overlap the sub-blocks it jumps.

## Presets

`LayerBlockDiagram` carries opinionated presets — `transformer_block()`,
plus MLP and CNN variants — with tensor-shape labels and the residual paths
already wired. The example uses `transformer_block()` in a single line, but the
underlying `ComputeGraph` is public: build your own architecture and it lays out
the same way.

## Heatmaps

`heatmap` renders weight matrices and attention patterns through the
[`MaterialQuad`](./materials.md) path — a matrix becomes a texture shaded by a
colormap, per-pixel, rather than a grid of rectangles. An attention map over a
512-token context is a 512² texture upload and one draw, not 260,000 mobjects.

## Loss landscapes

`LossLandscape` is a height-field surface over a two-parameter loss, with
descent trajectories traced across it for **SGD, momentum, and Adam**.

The gradients driving those trajectories are the exact AD gradients of the loss
closure from [`manim-fields`](./fields.md) — not a finite difference and not a
hand-derived formula. This matters for the figure's honesty: the whole point of
overlaying momentum against plain SGD is to show momentum carrying through a
narrow ravine that SGD oscillates across, and that behaviour depends on the
gradient being right at every step. Descent on an approximated gradient
produces a trajectory that is *shaped* like the lesson without being the lesson.

## Activation flow

`flow::forward_pass` sends a pulse along the graph's edges, staggered rank by
rank — internally a `LaggedStart` of `ShowPassingFlash` over the edge set. It
takes the graph, so it works on any `ComputeGraph`, presets included.

## The example

<div class="source-note">

Source: [`crates/manim-nn/examples/transformer_block.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-nn/examples/transformer_block.rs)

</div>

```rust
{{#include ../../crates/manim-nn/examples/transformer_block.rs}}
```

```sh
cargo run --release -p manim-nn --example transformer_block --features render-examples
```

## SGD vs momentum vs Adam

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/nn/loss_landscape_descent.mp4">
  </video>
  <figcaption>
    Three optimizers released from the same point on the same anisotropic ravine.
    Red is SGD, yellow momentum, green Adam.
  </figcaption>
</figure>

The loss is a deliberately **anisotropic** quadratic ravine,
`L(x, y) = 0.02·x² + 1.5·y²`. Its Hessian eigenvalues are `0.04` and `3.0` — a
**condition number of 75**: a narrow trough along the `x`-axis with steep walls in
`y` and an almost flat floor in `x`.

That choice is the point. Curvature anisotropy, not non-convexity, is what
actually separates first-order optimizers in practice, and a quadratic ravine is
the cleanest possible form of it. A non-convex landscape with local minima makes a
more dramatic picture and a much worse explanation.

All three start at `(−1.8, 1.05)`, high on the wall, and follow the **exact**
gradient — the loss is a `ScalarField`, so `∇L` comes from forward-mode AD.

- **SGD** (red, `lr = 0.3`) is stability-limited by the *steep* direction:
  `lr < 2/3.0`. It drops into the trough almost instantly, then crawls along the
  flat floor at `lr·0.04` per step and is still at `x ≈ −0.33` when the others
  have arrived.
- **Momentum** (yellow, `lr = 0.05, β = 0.9`) accumulates velocity along the
  floor — an effective step `lr/(1−β) = 10×` larger — and overshoots in `y` on the
  way in before settling.
- **Adam** (green, `lr = 0.04`) divides each coordinate by its own gradient RMS,
  so both directions move at ≈ `lr` per step regardless of curvature. It heads for
  the minimum nearly in a straight line and gets there first.

The whole story is a statement about the *ratio* of the eigenvalues, which is why
the exact gradient matters: an approximated one perturbs the flat direction by
roughly the same absolute amount as the steep one, and the flat direction is where
all the interesting behaviour is.

<div class="source-note">

Source: [`crates/manim-nn/examples/loss_landscape_descent.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-nn/examples/loss_landscape_descent.rs)

</div>

```rust
{{#include ../../crates/manim-nn/examples/loss_landscape_descent.rs}}
```

```sh
cargo run --release -p manim-nn --example loss_landscape_descent --features render-examples
```
