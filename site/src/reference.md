# Reference & Demos

## API documentation

Full rustdoc for every crate in the workspace, built with `--no-deps` and
published alongside this book:

<div class="demo-callout">

### 📖 [API reference (rustdoc)](./api/index.html)

</div>

| Crate | What it is |
|---|---|
| [`manim`](./api/manim/index.html) | The facade — prelude, `render()`, `preview()` |
| [`manim-core`](./api/manim_core/index.html) | Mobject model, animations, scene runtime, `Material` |
| [`manim-render`](./api/manim_render/index.html) | wgpu pipelines, offscreen renderer, exporters |
| [`manim-math`](./api/manim_math/index.html) | Paths, Béziers, geometry |
| [`manim-color`](./api/manim_color/index.html) | Colors and palettes |
| [`manim-text`](./api/manim_text/index.html) | Text, TeX (typst), `Code` |
| [`manim-fields`](./api/manim_fields/index.html) | AD, fields, `SpaceMap`, integrators, PDE |
| [`manim-sci`](./api/manim_sci/index.html) | Fields → mobjects: deformation, materials, surfaces, volumetrics |
| [`manim-quantum`](./api/manim_quantum/index.html) | Wavefunctions, eigenstates, Bloch sphere |
| [`manim-chem`](./api/manim_chem/index.html) | Molecules, lattices, orbitals |
| [`manim-nn`](./api/manim_nn/index.html) | Compute graphs, heatmaps, loss landscapes |
| [`manim-dioxus`](./api/manim_dioxus/index.html) | `ManimPlayer`, `Figure`, interaction widgets |

Docs are a hard gate in CI: `RUSTDOCFLAGS=-D warnings` plus
`broken_intra_doc_links = "deny"` at the workspace level, so a stale link fails
the build rather than shipping.

## Live demos

<div class="demo-callout">

### ▶ [Interactive demos](./demos/index.html)

</div>

The Dioxus app, compiled to wasm: a scene gallery, a render-on-demand textbook
page, and the Visual Complex Analysis slice with draggable zeros and poles. See
the [Interactive Web chapter](./interactive.md) for what each route
demonstrates.

## Source

- **Repository:** <https://github.com/cryptex-ai/manim_rust>
- **Design documents:** [`docs/design/`](https://github.com/cryptex-ai/manim_rust/tree/main/docs/design)
  — the source of truth for architecture. The scientific extensions are
  [`12-scientific-extensions.md`](https://github.com/cryptex-ai/manim_rust/blob/main/docs/design/12-scientific-extensions.md).
- **Examples:** [`crates/*/examples/`](https://github.com/cryptex-ai/manim_rust/tree/main/crates)
  — every figure in this book is one of these files, included verbatim.
