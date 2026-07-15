# manim-dioxus-gallery

A standalone [Dioxus](https://dioxuslabs.com/) web app showing `manim-dioxus`'s
`<ManimPlayer>` component: a scene picker driving three text-free scenes
(SquareToCircle, an axes plot, a rotational vector field), each with the built-in
play/pause + scrubber controls.

Excluded from the main Cargo workspace (see the root `Cargo.toml` `exclude`) so
the heavy dioxus tree doesn't burden native workspace builds.

## Build & run

Needs the wasm target and the Dioxus CLI:

```sh
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli   # provides `dx`

# From this directory — serves at http://localhost:8080 with hot reload:
dx serve --platform web
```

`dx build --platform web` produces a static bundle under `target/dx/` you can
serve with any static file server.

## What it shows

- `ManimPlayer { scene: SquareToCircle, autoplay: true, loop_playback: true, controls: true }`
  — a scene as a first-class component prop (any `SceneBuilder + Clone + PartialEq`).
- The scene picker swaps the mounted player; each mounts its own `<canvas>`,
  precomputes frames on the client, and plays them via `CanvasSurface` in a
  `requestAnimationFrame` loop outside the VDOM.
- The controls bar reads/writes the shared `SceneController` (also reachable in
  custom UI via `use_scene_controller()`).

## Verification status

`dioxus-cli` and a browser were **not available in the authoring environment**,
so this was verified by compiling to wasm directly:

```sh
cargo build --target wasm32-unknown-unknown \
  --manifest-path examples/dioxus-app/Cargo.toml
```

The `.wasm` builds cleanly; the in-browser render loop itself has not been run
here. `manim-dioxus` is exercised on `wasm32` (with and without related features)
in CI.
