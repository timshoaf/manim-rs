# manim_rust

A ground-up reimplementation of [Manim Community Edition](https://docs.manim.community)
in Rust, rendering with WebGPU (`wgpu`) — declarative, real-time, and embeddable
in web apps via Dioxus.

> **Status: milestones M0–M5 and the M7 web player are done.** Math & color
> (M0), mobjects & geometry (M1), the animation engine (M2), the offscreen +
> realtime wgpu renderer (M3), text/TeX via cosmic-text & typst (M4), coordinate
> systems & plotting, vector fields, graphs, SVG/image import (M5), and the
> Dioxus `ManimPlayer` component (M7) are all functional. **950+ tests** pass
> (unit + property + doctests + golden-image), with goldens rendered headlessly
> on a software/GPU adapter and diffed against checked-in PNGs.
> See [`docs/design/`](docs/design/00-vision.md) for the architecture book and
> the [parity map](docs/design/10-parity-map.md) for manim CE API coverage.

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

## Quickstart

Everything a scene author needs is in one prelude:

```rust
use manim::prelude::*;

struct SquareToCircle;

impl SceneBuilder for SquareToCircle {
    fn construct(&self, scene: &mut Scene) -> manim::Result<()> {
        let square = scene.add(Square::new().with_fill(BLUE, 0.7));
        scene.play(square.animate().rotate(PI / 4.0))?;
        scene.play(TransformInto::new(square, Circle::new().with_fill(RED, 0.7)))?;
        scene.wait(1.0);
        Ok(())
    }
}

fn main() -> Result<(), manim::render::RenderError> {
    // Render to an MP4 (needs `ffmpeg` on PATH):
    manim::render(&SquareToCircle, Config::low(), "square_to_circle.mp4")
}
```

Run the checked-in versions:

```sh
cargo run -p manim --example square_to_circle   # blue square → red circle → PNG sequence
cargo run -p manim --example hello_math          # headline + Euler's identity (text/TeX)
```

## Examples

`crates/manim/examples/` ports the manim CE gallery (each keeps a CE-diff note):

| Example | What it shows |
|---|---|
| `square_to_circle` | The canonical scene: rotate a square, morph to a circle, fade out |
| `hello_math` | Text + math typesetting: a headline, then Euler's identity |
| `vector_arrow` | Labeled vector arrow with a `NumberPlane` background |
| `sin_cos_plot` | `Axes` with plotted sin/cos curves and axis labels |
| `moving_around` | Shift / scale / recolor a square over time |
| `moving_angle` | An `Angle` that updates as one line rotates |
| `point_moving_on_shapes` | A dot animated along a circle and a line |
| `arg_min` | A parabola with a dot tracing its minimum |
| `boolean_operations` | Union / difference / intersection / exclusion of shapes |
| `brace_annotation` | `Brace`s annotating a segment |
| `gradient_text` | Gradient-filled text |
| `transform_matching_tex` | Glyph-matching transform between two TeX expressions |
| `preview` | Realtime winit preview window (needs `--features preview`) |

## Embed in a Dioxus web app

`manim-dioxus` exposes a `<ManimPlayer>` component — a scene is a first-class prop:

```rust
use dioxus::prelude::*;
use manim_dioxus::ManimPlayer;
use manim::prelude::*;

#[derive(Clone, PartialEq)]
struct Demo;
impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> manim::Result<()> {
        let c = scene.add(Circle::new().with_fill(BLUE, 0.7));
        scene.play(Create::new(c))?;
        Ok(())
    }
}

fn app() -> Element {
    rsx! {
        ManimPlayer {
            scene: Demo,
            autoplay: true,
            controls: true,
            width: "640px",
            height: "360px",
        }
    }
}
```

It mounts a `<canvas>`, precomputes the scene's frames, and plays them by wall
clock through the wgpu canvas surface — see
[`examples/dioxus-app/`](examples/dioxus-app/README.md) for a runnable gallery.

## Workspace

| Crate | Purpose |
|---|---|
| `manim-math` | points, bezier, paths, space ops, rate functions |
| `manim-color` | `Color` + the full manim CE color catalog |
| `manim-core` | scene graph, mobjects, animations, timeline, graphing, SVG/image (renderer-agnostic) |
| `manim-render` | wgpu renderer: tessellation, pipelines, offscreen & surface targets, video export |
| `manim-text` | text & math typesetting (cosmic-text, typst) |
| `manim` | facade + prelude |
| `manim-dioxus` | Dioxus `ManimPlayer` component (wasm) |

## Development

```sh
cargo test --workspace         # unit + property + doc tests
cargo clippy --workspace --all-targets
cargo doc --no-deps --open
```

The `manim` facade and the `examples/dioxus-app/` gallery build to
`wasm32-unknown-unknown`; golden-image tests run headless in CI.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full command set, the golden-image
`BLESS` workflow, and the optional feature flags (`preview`, `web`, `code`).

Project tracking lives in Linear (project “Manim Rust”, issues FE-77…).

## Publishing & crate naming

Nothing is published to crates.io yet — the version is `0.1.0` across the
workspace and the packaging metadata (descriptions, keywords, categories,
license) is in place ahead of a first release. One decision remains before we
publish: the facade crate name. **`manim` on crates.io may be unavailable or
reserved** — if so, the facade will ship under a fallback name (e.g. `manim-rs`)
while the `manim-*` component crates keep theirs. `manim-dioxus` is marked
`publish = false` for now (a thin integration/demo crate over the whole stack;
revisit once the component crates are on crates.io). This is tracked in the
[CHANGELOG](CHANGELOG.md).

## License

Licensed under the [MIT License](LICENSE).
