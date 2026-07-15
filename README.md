# manim_rust

A ground-up reimplementation of [Manim Community Edition](https://docs.manim.community)
in Rust, rendering with WebGPU (`wgpu`) — declarative, real-time, and embeddable
in web apps via Dioxus.

> **Status: early development, end-to-end vertical slice working.** M0 (math &
> color) complete; M1 (mobjects & geometry), M2 (animation engine), and the
> offscreen wgpu renderer are functional — `SquareToCircle` renders to PNG/mp4
> (`cargo run -p manim --example square_to_circle`). 600 tests passing.
> See [`docs/design/`](docs/design/00-vision.md) for the architecture book and
> the [parity map](docs/design/10-parity-map.md) for coverage of the manim CE API.

## Goals

- **Rust-native & declarative** — typed handles into an arena scene graph,
  builder-style mobjects, animations as data. No `Rc<RefCell<…>>` soup.
- **Real-time** — interactive playback on native (winit) and web (wasm/canvas);
  offline deterministic rendering to PNG/video shares the same pipeline.
- **Full manim CE parity** — the entire public API surface, tracked issue by issue.
- **Extremely well tested** — property tests for math, golden-image tests for
  rendering, and a runnable doc example on every public item
  (`missing_docs` is a hard error workspace-wide).
- **Dioxus components** — `ManimPlayer` renders scenes inside wasm apps.

## A taste (target API)

```rust
use manim::prelude::*;

struct SquareToCircle;

impl SceneBuilder for SquareToCircle {
    fn construct(&self, scene: &mut Scene) -> manim::Result<()> {
        let square = scene.add(Square::new().fill(BLUE, 0.7));
        scene.play(square.animate().rotate(PI / 4.0))?;
        scene.play(TransformInto::new(square, Circle::new().fill(RED, 0.7)))?;
        scene.wait(1.0);
        Ok(())
    }
}

fn main() -> manim::Result<()> {
    manim::preview(SquareToCircle, Config::default()) // realtime window
}
```

## Workspace

| Crate | Purpose |
|---|---|
| `manim-math` | points, bezier, paths, space ops, rate functions |
| `manim-color` | `Color` + the full manim CE color catalog |
| `manim-core` | scene graph, mobjects, animations, timeline (renderer-agnostic) |
| `manim-render` | wgpu renderer: tessellation, pipelines, offscreen & surface targets |
| `manim-text` | text & math typesetting (cosmic-text, typst) |
| `manim` | facade + prelude |
| `manim-dioxus` | Dioxus `ManimPlayer` component *(planned, M7)* |

## Development

```sh
cargo test --workspace         # unit + property + doc tests
cargo clippy --workspace --all-targets
cargo doc --no-deps --open
```

Project tracking lives in Linear (project “Manim Rust”, issues FE-77…).
