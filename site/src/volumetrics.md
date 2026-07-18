# 3-D Fields & Volumetrics

> **Crate:** `manim-sci` — modules `vector_field_3d` and `volumetrics`.

A vector field in the plane is easy: draw arrows. In three dimensions arrows
occlude each other into a hedgehog and nothing is legible. The honest answers
are all *integrated* rather than sampled — follow the field and draw where it
goes.

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/volumetrics/dipole_field.mp4">
  </video>
  <figcaption>
    An electric dipole — <code>+q</code> above, <code>−q</code> below — seen
    three ways at once: stream tubes integrated from a seed ring, a potential
    heatmap sliced through the <code>xz</code>-plane, and a Monte-Carlo cloud of
    field magnitude near the charges.
  </figcaption>
</figure>

## Stream tubes and ribbons

`stream_tubes` integrates streamlines from seed points with the adaptive RK45
solver, then sweeps a tube along each one. `stream_ribbons` sweeps a flat
cross-section instead, which lets the ribbon's twist show the field's local
**rotation** — a ribbon that corkscrews is telling you the curl is nonzero along
that line, and a ribbon that stays flat is telling you it is irrotational.

Both sweep along a **rotation-minimizing frame**, as the
[knot tubes](./surfaces.md) do. With the raw Frenet frame, a streamline that
straightens out anywhere would produce a visible twist artifact exactly where
the physics is least interesting.

Seeding matters more than resolution. The example seeds a ring around the dipole
axis rather than a uniform box, because a box grid wastes most of its
streamlines in the far field where they all look the same and starves the region
between the charges where the structure is.

## Slice planes

`field_slice` puts a [`MaterialQuad`](./materials.md) on an arbitrary plane in
3-D — not just the axis-aligned ones — and shades it per-pixel by a scalar
field. In the dipole scene that is the potential through the `xz`-plane, and it
renders *in the 3-D scene*, depth-tested against the tubes, so the tubes pass
correctly in front of and behind it.

## Tensor glyphs

`TensorField2` values are symmetric 2-tensors — stress, strain, a metric, a
diffusion tensor. `tensor_glyphs` draws each as an ellipsoid whose axes are the
eigenvectors scaled by the eigenvalues, via GPU instancing.

The eigendecomposition uses **Cardano's closed-form solution** for the symmetric
3×3 case rather than an iterative solver. For a field sampled at thousands of
glyph sites per frame this is the difference between interactive and not, and
for symmetric 3×3 the closed form is both exact and unconditionally
well-conditioned.

## Density clouds

`density_cloud` Monte-Carlo samples a scalar density into instanced billboards —
the right representation for a probability cloud, an electron density, or (as
here) field magnitude, all of which are genuinely diffuse rather than surfaces.

Sampling is by **seeded rejection**, so the cloud is deterministic: the same
scene renders the same points every time. A cloud that reshuffles every frame
scintillates distractingly, and worse, makes goldens impossible.

## Flux, and checking the picture

`flux` integrates a vector field's flux through a surface. It is verified
against the divergence theorem — the flux through a closed surface compared with
the volume integral of the divergence inside it, agreeing to within **0.01%**.

This is the pattern worth stealing from this crate generally: every visualizer
whose output is a number has a test that pins that number to an independent
analytic route. Marching cubes is checked by Euler characteristic, split-step
evolution by `T + R = 1`, symplectic integration by energy drift, and flux by
Gauss's theorem.

## The example

<div class="source-note">

Source: [`crates/manim-sci/examples/dipole_field.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-sci/examples/dipole_field.rs)

</div>

The dipole field `E(r) = Σ qᵢ (r − pᵢ) / |r − pᵢ|³` is defined once as a
`Scalar`-generic closure per component, so the same definition serves value
evaluation for the streamlines and gradient evaluation for the divergence check.

```rust
{{#include ../../crates/manim-sci/examples/dipole_field.rs}}
```

```sh
cargo run --release -p manim-sci --example dipole_field --features render-examples
```

## Seeing vorticity: stream ribbons in an ABC flow

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/volumetrics/stream_ribbons.mp4">
  </video>
  <figcaption>
    Stream ribbons in an Arnold–Beltrami–Childress flow. The ribbons' twist is
    the field's local rotation — direction alone would not show it.
  </figcaption>
</figure>

The ABC flow is the standard test case here because it is **maximally helical**:
its curl is everywhere parallel to the field itself, so vorticity is present at
every point and nowhere visible from the streamline geometry alone. Two ribbons
can follow near-identical paths while twisting at completely different rates.
Draw arrows and you learn nothing; draw ribbons and the structure is immediate.

<div class="source-note">

Source: [`crates/manim-sci/examples/stream_ribbons.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-sci/examples/stream_ribbons.rs)

</div>

```rust
{{#include ../../crates/manim-sci/examples/stream_ribbons.rs}}
```

```sh
cargo run --release -p manim-sci --example stream_ribbons --features render-examples
```

## Tensor glyphs

<figure class="manim-figure">
  <img src="assets/volumetrics/tensor_glyph_field.png" alt="tensor glyph field">
  <figcaption>
    A diffusion-tensor field drawn as ellipsoid glyphs. The glyph <em>is</em> the
    tensor: its axes are the eigenvectors, its extents the eigenvalues.
  </figcaption>
</figure>

An ellipsoid glyph is not a decorative stand-in for a tensor — it is a faithful
picture of one. A prolate (cigar) glyph means one dominant eigenvalue: diffusion
is channelled along a fibre. An oblate (pancake) glyph means two comparable
eigenvalues: diffusion is confined to a sheet. A sphere means isotropy. This is
how diffusion-tensor MRI is read, and the shape vocabulary carries over to
stress and strain unchanged.

<div class="source-note">

Source: [`crates/manim-sci/examples/tensor_glyph_field.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-sci/examples/tensor_glyph_field.rs)

</div>

```rust
{{#include ../../crates/manim-sci/examples/tensor_glyph_field.rs}}
```

```sh
cargo run --release -p manim-sci --example tensor_glyph_field --features render-examples
```
