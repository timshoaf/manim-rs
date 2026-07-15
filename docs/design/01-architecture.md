# Architecture

## Crate layout

```
manim_rust/
├── crates/
│   ├── manim-math      # vectors, bezier, paths, transforms, space ops, rate functions
│   ├── manim-color     # Color type, spaces, gradients, all manim CE named colors
│   ├── manim-core      # scene graph (arena), Mobject/VMobject, animations, timeline, updaters, config
│   ├── manim-render    # wgpu renderer: tessellation, pipelines, camera, offscreen + surface targets
│   ├── manim-text      # text & math typesetting → VMobject outlines (cosmic-text, typst)
│   ├── manim-dioxus    # Dioxus components/hooks wrapping a scene on a canvas
│   └── manim           # facade crate: re-exports + `prelude`
│       └── examples/   # runnable gallery mirroring manim CE examples
├── docs/               # migration guide + design/ documents
└── CHANGELOG.md
```

Dependency DAG (strictly acyclic):

```
manim-math ─┬─→ manim-core ─┬─→ manim-render ─┬─→ manim ─→ manim-dioxus
manim-color ┘               └─→ manim-text ───┘
```

- `manim-math` and `manim-color` are dependency-light leaves (glam only). They
  compile on any target including wasm with no GPU deps.
- `manim-core` is **renderer-agnostic**: it owns the scene graph and animation
  engine and produces per-frame display lists. It never touches wgpu. This keeps
  the core testable headlessly and fast.
- `manim-render` consumes display lists, tessellates (lyon), and draws (wgpu).
  It supports two targets: a surface (window/canvas) and an offscreen texture
  (image/video export, golden tests).
- `manim` is what users depend on: `use manim::prelude::*;`.

## Key dependencies

| Concern | Choice | Why |
|---|---|---|
| Linear algebra | `glam` (f32) | fast, SIMD, wasm-friendly, the ecosystem standard for graphics |
| Tessellation | `lyon` | mature path tessellation, handles fills/strokes/joins/caps |
| GPU | `wgpu` | the WebGPU implementation; native + wasm from one codebase |
| Windowing | `winit` | native preview windows |
| Text shaping | `cosmic-text` | full shaping/bidi/fallback, pure Rust |
| Glyph outlines | `ttf-parser` (via cosmic-text's fontdb) | glyph → bezier path extraction |
| Math typesetting | `typst` libraries | pure-Rust LaTeX alternative, wasm-able; replaces manim's LaTeX toolchain |
| SVG import | `usvg` | normalized SVG parsing → paths |
| Property testing | `proptest` | math invariants |
| Image compare | `image` + perceptual diff | golden-image render tests |

## The frame pipeline

```
user code (declarative)          manim-core                        manim-render
─────────────────────────  ───────────────────────────  ─────────────────────────────
scene.play(anims…)   ───→  Timeline schedules segments
                            each tick(dt):
                              run updaters
                              step animations (alpha)
                              mutate arena mobjects
                              extract DisplayList  ───→  diff/dirty-check → tessellate
                                                          changed paths → GPU buffers
                                                          encode render pass → present
```

A `DisplayList` is a flat, z-ordered `Vec<DrawItem>` where a `DrawItem` is a
resolved path (bezier subpaths) + fill/stroke style + transform. It is the *only*
contract between core and render, which makes both sides independently testable:
core tests assert on display lists; render golden-tests feed hand-built display lists.

## Threading & platforms

- Core tick and tessellation run on one thread (wasm has no threads by default).
  Tessellation results are cached per-mobject and invalidated by a dirty flag /
  generation counter, so static mobjects cost nothing per frame.
- Native: winit event loop; wgpu on any backend (Vulkan/Metal/DX12/GL).
- Wasm: `wasm-bindgen` + canvas; the same event loop abstraction drives
  `requestAnimationFrame`.

## Error handling

- Construction/builder APIs are infallible where manim is (bad input panics in
  debug with a clear message, saturates in release only where manim does).
- I/O boundaries (font loading, SVG parsing, file export, GPU init) return
  `Result<_, ManimError>` (`thiserror`-derived, one error enum per crate,
  aggregated in the facade).
