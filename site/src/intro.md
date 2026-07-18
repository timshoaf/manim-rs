# manim-rs

**Scientific animation in Rust — a toolkit for constructivist textbooks.**

`manim_rust` is a reimplementation of [Manim](https://www.manim.community/)'s
mobject/animation model in Rust on top of `wgpu`, extended into a *scientific*
graphics library: visual complex analysis, differential geometry, topology,
quantum mechanics, chemistry, and neural networks.

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/deformations/conformal_square.mp4">
  </video>
  <figcaption>
    The conformal map <code>z ↦ z²</code>, drawn by an adaptively-subdivided
    deformation grid. Thirty lines of Rust —
    <a href="./deformations.html">see the chapter</a>.
  </figcaption>
</figure>

## The thesis

Most mathematical exposition is *declarative*: it states a result and then
justifies it. The reader receives. A **constructivist** text instead hands the
reader the object and lets them turn it over — drag the pole and watch the phase
portrait swirl, tug the geodesic and watch it refuse to leave the surface, slide
the barrier height and watch the tunneling amplitude collapse.

Tristan Needham's *Visual Complex Analysis* is the north-star aesthetic here;
direct manipulation is the north-star pedagogy. That combination sets the
engineering requirements, and they are unusual:

- **The mathematics must be a value, not a picture.** A `Field` and a `SpaceMap`
  live in a crate that has never heard of a mobject. They are testable to
  numerical tolerances — symplectic integrators are checked for energy drift,
  marching cubes for Euler characteristic, flux integrals against the divergence
  theorem. A figure that lies is worse than no figure.
- **Derivatives must be exact.** Every gradient, Jacobian, curvature, and surface
  normal comes from forward-mode automatic differentiation, never a finite
  difference. Normals on an isosurface are the field's own gradient.
- **Per-pixel mathematics needs per-pixel evaluation.** Domain coloring is a
  function of the pixel, not the vertex, so phase portraits and heatmaps are GPU
  materials rather than dense meshes.
- **Idle figures must cost nothing.** A textbook page carries a dozen figures.
  They share one GPU device and render on demand — an untouched page renders
  zero frames. Battery is pedagogy.

## What is here

Every figure on this site is produced by a **real example file in the
repository**, included verbatim into the page it illustrates. There is no
prose-only pseudocode: what you read is what was compiled and rendered to
produce the figure above it.

| Chapter | Crate | What it builds |
|---|---|---|
| [Fields & AD](./fields.md) | `manim-fields` | Dual numbers, fields, `SpaceMap`, integrators, PDE steppers |
| [Materials](./materials.md) | `manim-sci` | Domain coloring, heatmaps, field textures |
| [Deformations](./deformations.md) | `manim-sci` | `ApplyMap`, `DeformationGrid`, the complex-analysis kit |
| [Surfaces & Topology](./surfaces.md) | `manim-sci` | Marching cubes, curvature, geodesics, parallel transport |
| [Quantum](./quantum.md) | `manim-quantum` | Wavefunctions, eigenstates, Bloch sphere, tunneling |
| [Chemistry](./chemistry.md) | `manim-chem` | Molecules, lattices, orbitals, instanced rendering |
| [Neural Networks](./neural-nets.md) | `manim-nn` | Compute graphs, heatmaps, loss landscapes |
| [3-D Fields](./volumetrics.md) | `manim-sci` | Stream tubes/ribbons, tensor glyphs, clouds, flux |
| [Interactive Web](./interactive.md) | `manim-dioxus` | `Figure`, `use_parameter`, `DragHandle`, `OrbitControls` |

The [API reference](./reference.md) and the [live interactive
demos](./reference.md) are linked from the reference page.

## Layering

`manim-fields` sits at the bottom with no manim dependencies at all — it is a
standalone applied-math crate (`glam` and `rustfft` only). `manim-sci` bridges
fields to mobjects and meshes. The domain kits sit on top of both, and none of
them are needed to use plain `manim`.

```text
manim-quantum   manim-chem   manim-nn      ← domain kits
        └───────────┼───────────┘
              manim-sci                    ← fields → mobjects/meshes
             ┌──────┴──────┐
      manim-fields    manim-core           ← pure math │ mobject model
                           │
                     manim-render          ← wgpu pipelines, materials
```

Everything works in `f64` below the `manim-sci` boundary and `f32` above it;
`to_field` / `to_scene` are the only places precision is dropped.
