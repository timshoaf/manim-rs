# manim-dioxus-gallery

A standalone [Dioxus](https://dioxuslabs.com/) web app showing `manim-dioxus`
rendering `manim_rust` scenes into live `<canvas>` elements through wgpu. A
top-level switch picks one of three routes:

- **Gallery** — a scene picker driving a single `<ManimPlayer>` (SquareToCircle,
  an axes plot, a vector field, a 3-D mesh, a live-orbit height field, a
  cursor-follow scene, a zoomed inset), each with the built-in play/pause +
  scrubber controls.
- **Textbook page** — a dozen render-on-demand `<Figure>`s under one shared GPU
  device (`ManimGpuProvider`), each drawn once when scrolled into view then idle
  at ~0 cost (FE-138).
- **Visual Complex Analysis** — the scientific-extensions v1 exit slice
  (FE-140): three interactive complex-analysis figures under one shared device.

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
serve with any static file server. (A plain `cargo build --target
wasm32-unknown-unknown` compiles the crate but doesn't emit the JS/HTML glue —
use `dx` for a runnable app.)

## What it shows

**Gallery** — `ManimPlayer { scene, autoplay, loop_playback, controls }`: a scene
as a first-class component prop (any `SceneBuilder + Clone + PartialEq`). The
picker swaps the mounted player; each mounts its own `<canvas>`, precomputes
frames on the client, and plays them via `CanvasSurface` in a
`requestAnimationFrame` loop outside the VDOM. The controls bar reads/writes the
shared `SceneController` (also reachable via `use_scene_controller()`).

**Textbook page** — a `ManimGpuProvider` wrapping twelve `<Figure>`s. One wgpu
device is requested for the whole page; each figure builds its surface with
`CanvasSurface::with_shared`. A figure renders on demand (a `RenderSchedule`
state machine): once on mount / when scrolled into view, then it parks until
woken. An idle page renders zero frames.

**Visual Complex Analysis** — three figures under one `ManimGpuProvider`:

1. **Domain coloring** of a rational function `f(z) = Π(z−zᵢ)/Π(z−pⱼ)`. Drag the
   two teal zeros and two red poles (`DragHandleLayer` over a pure `DragSet`
   hit-test/capture state machine); each drag rebuilds the `ComplexField` and
   calls `MaterialQuad::resample`. A **phase** slider (`use_parameter`) rotates
   the hue. Resamples at 128² while dragging, 256² on settle.
2. **Conformal map timeline** — a `DeformationGrid` carried through `z ↦ z²`
   then a Möbius map via two `ApplyMap` plays, on a `<ManimPlayer>` timeline
   (play/scrub).
3. **Riemann sphere** — the mesh sphere plus a stereographic grid (a plane grid
   wrapped onto the sphere), orbited with `OrbitControls` (drag to rotate, wheel
   to zoom, with settle-window inertia).

The interaction widgets (`use_parameter`, `DragHandle`/`DragSet`,
`OrbitControls`) live in `manim-dioxus`; their state machines are pure and
unit-tested headlessly.

## Verification status

`dioxus-cli` and a browser were **not available in the authoring environment**,
so everything here was verified by compiling to wasm and by native unit tests of
the interaction/scheduling state machines and the CPU sampling paths:

```sh
# wasm builds cleanly (crate + example):
cargo build --target wasm32-unknown-unknown

# native state-machine + scene-construct + resample-timing tests:
cargo test                          # in this directory
cargo test -p manim-dioxus          # RenderSchedule, DragSet, OrbitState, Parameters, …
```

Measured CPU resample cost (release, native): **128² ≈ 0.16 ms, 256² ≈ 0.64 ms,
512² ≈ 2.5 ms** per frame — 256² is well under an 8 ms frame budget.

### Live-browser checklist (first manual session)

The render loops, GPU device sharing, and pointer/wheel interaction only run in a
real browser. This is the punch list to walk through with `dx serve`:

- [ ] **Gallery** — each scene plays; controls (space / ←→ / R, speed, sections,
      scrubber) work; the 3-D and live-orbit scenes render with depth.
- [ ] **Textbook page** — figures render as they scroll into view (the
      `IntersectionObserver` lazy-mount), showing the "Loading figure…"
      placeholder until first draw; once settled, an on-screen idle page holds at
      **zero** rendered frames (confirm via a frame counter / profiler).
- [ ] **Shared device** — the Textbook and VCA pages create **one** GPU device
      for all their canvases, not one per figure (check `chrome://gpu` / the
      WebGPU device count, or console logging in `SharedGpu::new`).
- [ ] **Materials in-browser** — domain-coloring / heatmap / field-texture quads
      render correctly through `CanvasSurface` (the same `OpsRenderer` path as
      offscreen goldens), including the VCA plane.
- [ ] **VCA Fig 1** — dragging zeros/poles feels responsive; the 128²→256²
      sharpen-on-release is acceptable (or bump `VCA_DRAG_RES` to 256² if the
      GPU texture re-upload has headroom); the phase slider re-colors live and
      wakes the figure for exactly one redraw.
- [ ] **VCA Fig 3** — `OrbitControls` drag-orbit + wheel-zoom + release inertia
      feel right; the stereographic grid reads clearly over the sphere (consider
      a translucent sphere if the back-face grid lines are distracting).
- [ ] **VCA Fig 2** — the `z ↦ z²` → Möbius timeline plays and scrubs smoothly.

`manim-dioxus` is exercised on `wasm32` in CI; the in-browser render loops
themselves have not been run in this environment.
