# Materials & Domain Coloring

> **Crates:** `manim-render` (the GPU pipeline), `manim-core::display`
> (the `Material` type), `manim-sci::material_quad` (the scene-facing mobject).

Some mathematics is per-vertex, and some is per-pixel. A polygon is per-vertex:
subdividing it more finely does not change what it *is*. A phase portrait of a
rational function is per-pixel — every pixel is a distinct evaluation of `f(z)`,
and approximating it with a mesh means either a wastefully dense triangle soup
or a visibly faceted lie.

So the renderer grew a **material axis**: a `DrawItem` can carry a `Material`
instead of a vector fill, and the fragment shader evaluates the colouring from a
sampled field texture pinned to the item's quad in scene space.

## The three built-in materials

There are exactly three, and no user-supplied shaders. That is a deliberate
constraint: a closed set keeps the tessellation cache key hashable, keeps wasm
builds honest, and — more importantly — covers the mathematics that is actually
per-pixel.

```rust,ignore
MaterialKind::PhaseHue { modulus_contours: bool }
// Complex field → hue = arg f(z), brightness stepped by log|f|.
// The Needham phase portrait. Zeros show as full hue wheels wound
// counterclockwise; poles wind clockwise; the winding number is
// literally countable off the picture.

MaterialKind::Heatmap { colormap: Colormap }
// Real scalar → colormap LUT. Viridis, Magma, Coolwarm, Turbo.

MaterialKind::FieldTexture { colormap, contours: Option<ContourParams> }
// Scalar → colormap plus iso-contour overlay, drawn analytically in
// the shader rather than as extracted polylines.
```

`Coolwarm` is diverging (blue → white → red) and is the right choice for signed
data around zero — a potential, a curvature, a residual. `Viridis` and `Magma`
are perceptually uniform, which matters more than it sounds: a rainbow map
invents ridges in smooth data, and readers will believe them.

## MaterialQuad

`MaterialQuad` is the scene-level citizen: a world-space rectangle painted
per-pixel by one of those materials.

```rust,ignore
use manim_core::display::Colormap;
use manim_sci::material_quad::MaterialQuad;

// A phase portrait of f over [-2,2]², sampled at 256².
MaterialQuad::domain_coloring([-2.0, 2.0], [-2.0, 2.0], &f, 256)
    .add_to(scene.state_mut());

// A scalar heatmap.
MaterialQuad::heatmap([-2.0, 2.0], [-2.0, 2.0], &potential, 256, Colormap::Coolwarm)
    .add_to(scene.state_mut());

// Contoured field texture.
MaterialQuad::field_contours([-2.0, 2.0], [-2.0, 2.0], &potential, 256, contour_params)
    .add_to(scene.state_mut());
```

The field is sampled on the CPU into a texture grid, then uploaded. Material
equality is by texture `Arc` identity plus the small `Copy` parameters, so the
renderer caches the uploaded texture and an unchanged quad costs one bind.

## Resampling: the interaction hinge

```rust,ignore
MaterialQuad::resample(scene.state_mut(), quad_id, new_material);
```

This is the mechanism the [interactive VCA figure](./interactive.md) re-renders
through. When a reader drags a pole, the `ComplexField` is rebuilt from the new
root positions and the quad resampled in place — no mobject churn, one texture
upload.

CPU resampling is fast enough to do this live. Measured on release-native:

| Grid | Time per resample |
|---|---|
| 128² | ≈ 0.16 ms |
| 256² | ≈ 0.64 ms |
| 512² | ≈ 2.5 ms |

At 256² that is under a tenth of an 8 ms frame budget, which is why the
interactive figure can afford to resample at 128² *while* dragging and sharpen
to 256² on release without the reader noticing a seam. A GPU compute pass for
field evaluation is designed but not needed at these sizes.

## Meshes, too

The same colour-by-value idea applies to 3-D geometry through
`set_fill_by_value`: a per-face scalar mapped through a colormap. That is what
paints the torus by its Gaussian curvature in the
[surfaces chapter](./surfaces.md), and it is a mesh-side path rather than a
material — no texture, just per-face colour from the field.

## Reading a function by its colours

<figure class="manim-figure">
  <img src="assets/materials/domain_coloring_gallery.png" alt="three domain-coloring panels: a rational function, sin 2z, and log z">
  <figcaption>
    Three phase portraits. Left: <code>f(z) = (z²−1)/(z²+1)</code>, zeros at
    <code>±1</code> (green), poles at <code>±i</code> (red). Centre:
    <code>sin 2z</code>. Right: <code>log z</code>, with its principal branch cut
    marked.
  </figcaption>
</figure>

Domain coloring paints each point `z` with the hue of `arg f(z)`, so the full
colour wheel appears exactly where `f` winds. Around a simple **zero** the hue
cycles once counter-clockwise; around a simple **pole** it cycles once the other
way; the number of wheels is the order. The argument principle,
`(1/2πi)∮ f′/f = Z − P`, is being read straight off the picture — you count
wheels and handedness.

- **Left**, `f(z) = (z² − 1)/(z² + 1)`: zeros at `z = ±1`, poles at `z = ±i` —
  four hue wheels, two of each handedness.
- **Centre**, `f(z) = sin 2z`: zeros at `z = kπ/2`, a row of identical wheels
  marching along the real axis, with `|f|` blowing up off it.
- **Right**, `f(z) = log z`: a single zero at `z = 1` and no poles, yet the hue
  jumps discontinuously across the negative real axis. That is the principal
  **branch cut**, where `arg` wraps from `+π` to `−π` — a discontinuity in the
  *chart*, not in the function.

Each panel samples its own world rectangle, so the function is pre-shifted by the
panel centre and each picture is genuinely `f` about that panel's origin.

<div class="source-note">

Source: [`crates/manim-sci/examples/domain_coloring_gallery.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-sci/examples/domain_coloring_gallery.rs)

</div>

```rust
{{#include ../../crates/manim-sci/examples/domain_coloring_gallery.rs}}
```

```sh
cargo run --release -p manim-sci --example domain_coloring_gallery --features render-examples
```

## Level sets and the gradient

<figure class="manim-figure">
  <img src="assets/materials/heatmap_contours.png" alt="a contoured Coolwarm heatmap of a saddle with two Gaussian bumps, gradient arrows overlaid">
  <figcaption>
    A saddle with two Gaussian bumps, drawn as a Coolwarm heatmap with
    iso-contours every <code>0.3</code>, and the exact gradient <code>∇f</code>
    drawn as white arrows on top.
  </figcaption>
</figure>

The field is

```text
f(x, y) = 1.5·e^{−((x+1.6)² + y²)} − 1.2·e^{−((x−1.6)² + (y−0.6)²)} + 0.12·(x² − y²)
```

and the picture is built to make two facts unavoidable.

**Contours are level sets.** Walking along one changes nothing, so the
directional derivative along a contour vanishes: `∇f · t̂ = 0`. The arrows
therefore cross every contour at a **right angle** — never along one. This is the
kind of claim a reader nods at in prose and only believes once they can check it
against a picture at twenty places at once.

**Crowded contours mean a steep gradient.** Contour spacing is `Δf/|∇f|`, so on
the flanks of the bumps the lines pile up and the arrows are long, while near the
saddle and the bump summits — where `∇f = 0` — the lines spread out and the
arrows shrink to nothing.

The gradients are exact, not finite-differenced: the field is written once
generically over the AD `Scalar` type, and `ScalarField::grad` differentiates it
forward through dual numbers. An approximated gradient would break the
perpendicularity slightly and visibly, right where the field is steepest.

<div class="source-note">

Source: [`crates/manim-sci/examples/heatmap_contours.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-sci/examples/heatmap_contours.rs)

</div>

```rust
{{#include ../../crates/manim-sci/examples/heatmap_contours.rs}}
```

```sh
cargo run --release -p manim-sci --example heatmap_contours --features render-examples
```

> A third material example, `mobius_flow`, animates a phase portrait under a
> flow — it lives in the [deformations chapter](./deformations.md), where the
> map matters more than the shading.
