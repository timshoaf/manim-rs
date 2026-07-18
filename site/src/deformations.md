# Deformations & Complex Analysis

> **Crate:** `manim-sci` — modules `deform` and `complex_viz`.

A complex function is a map of the plane to itself, and the honest way to show
one is to *deform the plane* and watch what happens to the grid. This is
Needham's move, and it is the reason `SpaceMap` exists as a first-class value:
`z ↦ z²` is not a picture, it is a transformation you can apply to anything.

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/deformations/conformal_square.mp4">
  </video>
  <figcaption>
    <code>z ↦ z²</code> applied to an adaptively-subdivided grid, over a faded
    ghost copy of the undeformed plane. Angles between grid lines are preserved
    everywhere except the origin — the map is conformal, and you can see it.
  </figcaption>
</figure>

## ApplyMap re-evaluates; ApplyFunction interpolates

This distinction is the whole point of the module, and getting it wrong
produces animations that are subtly, persuasively false.

Core's `ApplyFunction` computes the endpoint and lerps: a point travels in a
straight line from `p` to `f(p)`. `ApplyMap` instead evaluates the homotopy
`H(x, α)` at every frame against the **original** points, snapshotted in
`begin()`. For a nonlinear map the two disagree everywhere in between.

Under `z ↦ z²` with linear interpolation, a grid intersection cuts a chord
across the plane and the intermediate frames show a shape that is not the image
of anything. Under `ApplyMap` it sweeps the actual path, and every intermediate
frame is a genuine conformal image of the plane at a partial power. One of
these teaches complex analysis; the other teaches tweening.

`FlowMap` is the sibling for vector fields: it advances points along the
generating field's integral curves, so the intermediate frames are genuine
time-`t` flow maps.

## The adaptive grid

`DeformationGrid` carries a `SpaceMap` and subdivides its lines according to the
**local Jacobian distortion** — exactly the `SpaceMap::jacobian` AD query from
the [fields chapter](./fields.md). Under `z ↦ z²`, `|J| = 2|z|`, so the grid
densifies away from the origin where the map stretches hardest and stays cheap
near it. A uniform grid would either alias at the edges or waste ten thousand
segments in the middle.

The `.faded(0.25)` ghost grid in the example is a reference frame: without an
undeformed copy to compare against, a deformation animation is just motion.

## The complex-analysis kit

`complex_viz` supplies the surrounding furniture: conformal grid images,
zero/pole markers, branch-cut indicators, and `RiemannSphere` — the
stereographic projection that ties the plane to the sphere and makes the point
at infinity an ordinary place. Combined with the
[phase-hue material](./materials.md), that is the full Needham toolkit.

## The example

<div class="source-note">

Source: [`crates/manim-sci/examples/conformal_square.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-sci/examples/conformal_square.rs)

</div>

```rust
{{#include ../../crates/manim-sci/examples/conformal_square.rs}}
```

```sh
cargo run --release -p manim-sci --example conformal_square --features render-examples
```

## A Möbius map as a flow

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/deformations/mobius_flow.mp4">
  </video>
  <figcaption>
    <code>z ↦ (az + b)/(cz + d)</code> happening <em>continuously</em>. The grid
    stretches and bends into arcs, yet every crossing stays a right angle; the
    three white circles stay circles.
  </figcaption>
</figure>

Every one-parameter subgroup of Möbius maps is the flow of a *quadratic*
holomorphic vector field `ż = a + bz + cz²`. The one used here,

```text
ż = v(z) = (i/2)(1 + z²)
```

integrates in closed form to
`φ_t(z) = (cos s · z + i sin s)/(−i sin s · z + cos s)` with `s = t/2` — a
genuine Möbius map at every `t`. Seen on the Riemann sphere it is simply a
rotation about the axis through the fixed points `z = ±i`, which is the sense in
which "a Möbius transformation is a rotation of the sphere" is literally true
rather than a slogan.

Two things to watch:

**Conformality.** `v` is holomorphic, so the flow's Jacobian is `s·R` — a scaling
times a rotation, with no shear. The grid distorts wildly and every crossing
stays a right angle. The faded ghost grid underneath is the undeformed reference
to check against.

**Circles to circles.** The three white circles ride the same flow. A Möbius map
sends circles to circles (or lines), so they stay round while moving and changing
size — they never become ellipses, which is exactly what a non-Möbius deformation
of comparable violence would do to them.

<div class="source-note">

Source: [`crates/manim-sci/examples/mobius_flow.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-sci/examples/mobius_flow.rs)

</div>

```rust
{{#include ../../crates/manim-sci/examples/mobius_flow.rs}}
```

```sh
cargo run --release -p manim-sci --example mobius_flow --features render-examples
```

## See it live

The [interactive demo](./reference.md) carries this further: a domain-coloring
figure whose zeros and poles you can **drag**, with the phase portrait
resampling under your cursor. That is the constructivist version — see the
[Interactive Web chapter](./interactive.md).
