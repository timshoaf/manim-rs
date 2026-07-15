# web-vanilla — manim_rust in the browser

A minimal, framework-free wasm demo: the canonical `SquareToCircle` scene
rendered onto an HTML `<canvas>` by `manim-render`'s `CanvasSurface`, stepped in
a `requestAnimationFrame` loop.

This crate is **standalone** — it is excluded from the main Cargo workspace
(see the root `Cargo.toml` `exclude`), so native workspace builds stay lean and
window/ffmpeg-free. It reaches the real crates through path dependencies.

## What it shows

- `Scene::build` + `Scene::frames()` to precompute the animation's display lists.
- `CanvasSurface::new(canvas, &config).await` to bind a wgpu surface to the page
  canvas.
- A `wasm-bindgen` rAF loop calling `surface.render(&display_list)` per frame.

## Build

You need the wasm target and [`wasm-pack`](https://rustwasm.github.io/wasm-pack/):

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack   # once

# From this directory:
wasm-pack build --target web --out-dir pkg
```

That emits `pkg/manim_web_vanilla.js` + `.wasm`, which `index.html` imports.

## Run

Serve the directory over HTTP (ES modules + wasm can't load from `file://`):

```sh
python3 -m http.server 8080
# then open http://localhost:8080/
```

A WebGPU-capable browser (or one with the WebGL fallback wgpu selects) shows the
blue square rotate, morph into a red circle, and fade out on a loop.

## Verification status

`wasm-pack`/a browser were **not available in the authoring environment**, so
this was verified by compiling the crate to wasm directly:

```sh
cargo build --target wasm32-unknown-unknown --manifest-path examples/web-vanilla/Cargo.toml
```

The `.wasm` builds cleanly; the browser render loop itself has not been run
here. `manim-render`'s `CanvasSurface` and the `web` feature are exercised by
`cargo check --target wasm32-unknown-unknown -p manim-render --features web` in
CI.
