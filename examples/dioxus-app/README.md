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
  (FE-140), now also the FE-144/145/147 surface: four interactive figures under
  one shared device, with pinch-zoom, constrained handles + readouts, a
  shareable URL, and an exercise block.

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
4. **Constrained handles + readouts** (FE-145) — one handle per `DragConstraint`
   kind: `Grid` (½-unit lattice), `Curve` (rides the unit circle, its angle read
   off a live `AngleMarker`), `Axis` (a horizontal rail), `Region` (clamped
   inside the drawn box). Each carries a `CoordinateReadout` / `DecimalReadout`
   that follows it and re-typesets only when its digits change.

Fig 1 additionally carries:

- **Pinch-zoom and two-finger pan** (FE-144). A `GestureRouter` tracks up to two
  contacts and arbitrates: one finger drags handles exactly as before, a second
  promotes the gesture to a pinch (cancelling the drag), and lifting one finger
  of a pinch does *not* fall back to dragging with the survivor. Desktop
  equivalents: ctrl/⌘+scroll zooms about the cursor, middle- or shift-drag pans.
  `PanZoom` applies the result to the 2-D camera, zooming about the pinch
  centroid.
- **A shareable URL** (FE-147). Handle positions and the phase slider are written
  to the fragment when a drag settles (`#vca=phase:0.5,z0:-1,0.6,…`, a compact
  grammar with pure encode/decode) and restored on mount. Written with
  `history.replaceState`, so it neither spams history nor scrolls the page.
- **An exercise block** (FE-147): "place both zeros on the unit circle", judged by
  a pure `ExerciseMachine` (live *achieved* flag + sticky *solved* credit, with a
  two-frame hold so a drag sweeping past the answer doesn't count), plus a reset
  button that restores the starting layout.

The interaction widgets (`use_parameter`, `DragHandle`/`DragSet`,
`OrbitControls`, `GestureRouter`/`PanZoom`, `DragConstraint`, the readout kit,
`UrlState`, `ExerciseMachine`) live in `manim-dioxus`; their state machines are
pure and unit-tested headlessly — 140 tests, no browser required.

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
- [ ] **Pinch-zoom (phone/tablet)** — two fingers on Fig 1 zoom about the point
      between them and pan with them; the handle under the first finger is
      *dropped* the instant the second lands, not flung; lifting one finger does
      not start dragging with the other; lifting both leaves the figure settled
      (no drift, no latched pointer).
- [ ] **Desktop pan/zoom** — ctrl/⌘+scroll zooms about the cursor (and does not
      scroll the page); middle-drag and shift-drag pan; a plain drag still moves
      handles.
- [ ] **Constrained handles (Fig 4)** — each handle obeys its constraint under a
      fast, wild drag (grid snaps, circle handle never leaves the circle, rail
      handle never leaves its line, box handle stops at the wall); the readouts
      track without lagging a frame behind, and text stays legible at phone
      width.
- [ ] **Readout cost** — dragging the circle handle continuously re-typesets only
      when a digit changes; watch for a frame-rate cliff if it re-typesets every
      frame.
- [ ] **Shareable URL** — drag Fig 1's handles, then copy the address bar into a
      new tab: the same layout and phase come back. The fragment updates on
      release (not per frame), the back button is not flooded, and the page does
      not jump-scroll when it is written.
- [ ] **Exercise** — placing both zeros on the unit circle flips the badge to
      "Solved"; dragging away dims it to "Solved earlier"; Reset restores the
      starting layout, clears the badge, and rewrites the URL.

`manim-dioxus` is exercised on `wasm32` in CI; the in-browser render loops
themselves have not been run in this environment.
