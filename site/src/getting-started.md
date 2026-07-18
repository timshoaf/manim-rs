# Getting Started

## Requirements

- **Rust 1.85+** (edition 2021, workspace MSRV).
- **A GPU adapter.** Rendering goes through `wgpu`; any Vulkan / Metal / DX12 /
  GL adapter works. On headless machines and in CI, install
  [Mesa lavapipe](https://docs.mesa3d.org/drivers/llvmpipe.html) for a software
  Vulkan adapter (`apt install mesa-vulkan-drivers`) — that is exactly what this
  site's own figures are rendered with.
- **`ffmpeg` on `PATH`**, if you want MP4 output. PNG sequences need nothing.

## Adding the dependency

For plain animation, the `manim` facade re-exports everything:

```toml
[dependencies]
manim = { git = "https://github.com/cryptex-ai/manim_rust" }
```

For the scientific kits, depend on the ones you need. They pull in
`manim-fields` and `manim-sci` transitively:

```toml
[dependencies]
manim-core   = { git = "https://github.com/cryptex-ai/manim_rust" }
manim-fields = { git = "https://github.com/cryptex-ai/manim_rust" }  # AD, fields, maps, integrators
manim-sci    = { git = "https://github.com/cryptex-ai/manim_rust" }  # fields → mobjects
manim-quantum = { git = "https://github.com/cryptex-ai/manim_rust" } # or -chem, -nn
```

> **Rendering is behind a feature gate in the kits.** `manim-sci`,
> `manim-quantum`, `manim-chem`, and `manim-nn` keep `manim-render` optional so
> that a wasm build (or a pure-computation dependent) never pulls in the GPU
> stack. Enable `render-examples` to build anything that rasterizes:
>
> ```toml
> manim-sci = { git = "...", features = ["render-examples"] }
> ```

## Your first scene

A scene is a `SceneBuilder` whose `construct` appends to a timeline. Nothing
renders while you build — you are describing an animation, not drawing one.

```rust,no_run
use manim::prelude::*;

struct SquareToCircle;

impl SceneBuilder for SquareToCircle {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let square = scene.add(Square::new().with_fill(BLUE, 0.7));
        scene.play(square.animate().rotate(PI / 4.0))?;
        scene.play(TransformInto::new(square, Circle::new().with_fill(RED, 0.7)))?;
        scene.wait(1.0);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Needs `ffmpeg` on PATH.
    manim::render(&SquareToCircle, Config::low(), "square_to_circle.mp4")?;
    Ok(())
}
```

`scene.add` returns a typed handle. Handles are how you refer to a mobject
later — `square.animate()` builds an animation targeting it, and the animation
catalog (`Create`, `FadeIn`, `TransformInto`, `ApplyMap`, …) takes them
directly.

## Rendering

Three output paths, all from the same built `Scene`:

```rust,no_run
# use manim::prelude::*;
# struct Demo;
# impl SceneBuilder for Demo { fn construct(&self, _: &mut Scene) -> Result<()> { Ok(()) } }
# fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
// 1. MP4 in one line (ffmpeg).
manim::render(&Demo, Config::low(), "demo.mp4")?;

// 2. PNG sequence — no ffmpeg needed. Frames land in out/demo/frame_NNNNN.png.
let mut scene = Scene::build(&Demo, Config::low())?;
manim_render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/demo")?;
# Ok(()) }
```

```rust,no_run
# use manim::prelude::*;
# struct Demo;
# impl SceneBuilder for Demo { fn construct(&self, _: &mut Scene) -> Result<()> { Ok(()) } }
# fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
// 3. Realtime preview window (requires the `preview` feature — winit).
//    Space play/pause, ←/→ seek, R restart, Esc quit.
manim::preview(&Demo, Config::medium())?;
# Ok(()) }
```

`Config::low()` / `medium()` / `high()` set resolution and frame rate. Start at
`low()` — a scientific scene that samples a field per frame is CPU-bound, and
you will iterate a lot faster at 480p.

## Running the examples in this book

Every chapter includes a real example file from the repository. Each one is a
standalone binary, gated behind `render-examples`:

```sh
git clone https://github.com/cryptex-ai/manim_rust
cd manim_rust

cargo run -p manim-sci     --example conformal_square    --features render-examples
cargo run -p manim-sci     --example torus_curvature     --features render-examples
cargo run -p manim-sci     --example dipole_field        --features render-examples
cargo run -p manim-quantum --example wavepacket_barrier  --features render-examples
cargo run -p manim-chem    --example caffeine            --features render-examples
cargo run -p manim-nn      --example transformer_block   --features render-examples
```

Each writes a PNG sequence to `out/<example>/frame_NNNNN.png`. To get the MP4s
you see on this site, encode with `ffmpeg`:

```sh
ffmpeg -framerate 30 -i out/conformal_square/frame_%05d.png \
       -c:v libx264 -pix_fmt yuv420p -movflags +faststart \
       conformal_square.mp4
```

> **Build them in release.** These scenes evaluate fields, integrate ODEs, and
> march cubes on the CPU. A debug build of the dipole-field example is roughly
> two orders of magnitude slower than `--release`.
