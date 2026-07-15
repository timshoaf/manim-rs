# Vision & Principles

## What this is

A ground-up reimplementation of [Manim Community Edition](https://docs.manim.community) in Rust,
rendering with WebGPU (`wgpu`), targeting **full API parity** with manim CE while being:

1. **Rust-native** — no `Rc<RefCell<>>` object soup emulating Python. Ownership,
   typed handles, builders, and traits are the idiom. If a design choice reads as
   "Python transliterated to Rust", it's wrong.
2. **Declarative** — scenes describe *what* appears and *how it animates*; the
   engine figures out the frames. Animations are data (structs describing an
   interpolation), not imperative mutation scripts.
3. **Real-time** — the renderer runs at interactive frame rates, driven by
   wall-clock time, on native (winit) and web (wasm + canvas). Offline
   deterministic rendering (fixed dt → PNG/video frames) is the same pipeline
   with a different clock.
4. **Extremely well tested** — every crate carries unit tests, property tests
   for math, golden-image tests for rendering, and **every public item has a
   doc comment with a runnable example** (doctests enforced in CI).
5. **Embeddable** — the endgame is Dioxus components so scenes run inside wasm
   web apps as ordinary UI elements.

## Quality bar

At or above manim CE:

- Docs: every public type/function/method has a docstring with a minimal working
  example. `#![deny(missing_docs)]` on all published crates.
- Examples: an `examples/` gallery mirroring manim CE's example gallery.
- Parity: the entire manim CE public API surface has a Rust equivalent
  (tracked in [10-parity-map.md](10-parity-map.md)).

## Non-goals (v1)

- Cairo backend (wgpu only; offscreen wgpu covers image export).
- Python interop / bindings.
- 1:1 *naming* parity where Python conventions clash with Rust (e.g. manim's
  `mobject.animate.shift(...)` becomes `mobject.animate().shift(...)`; snake_case
  methods stay, but `CamelCase` class constants become `SCREAMING_SNAKE` consts).
- GPU-side bezier rendering (Loop-Blinn / compute rasterization) — designed for,
  but v1 ships CPU tessellation via lyon. See [05-rendering.md](05-rendering.md).

## Naming

Workspace: `manim` (working title). Crates are `manim-*`. Publishing names can be
revisited before a crates.io release (the `manim` name may be taken/trademark-adjacent).

## Reference

manim CE v0.19 is the parity target. Its module map:
`mobject` (geometry, text, svg, types, 3d, graphing, matrix, table, vector_field, graph, boolean_ops, value_tracker),
`animation` (creation, transform, fading, indication, movement, rotation, growing, numbers, speedmodifier, transform_matching_parts, updaters, composition, changing, specialized),
`scene` (scene, moving_camera_scene, zoomed_scene, three_d_scene, vector_space_scene, section),
`camera`, `renderer`, `utils` (bezier, color, rate_functions, paths, space_ops, ...), `config`.
