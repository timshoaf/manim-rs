# Interactive Web

> **Crate:** `manim-dioxus`. Live app: [**/demos/**](./demos/index.html)

Everything up to this chapter produces video. Video is a fine medium for
exposition and a hopeless one for *construction* — you cannot drag a pole in an
MP4. This is where the constructivist thesis from the
[introduction](./intro.md) has to actually pay off.

<div class="demo-callout">

### ▶ [Open the live interactive demos](./demos/index.html)

Three routes: a scene gallery, a render-on-demand textbook page, and the
**Visual Complex Analysis** page — the exit criterion for the whole scientific
extension arc.

</div>

## Two embeds, not one

**`ManimPlayer`** is a timeline: a scene as a component prop, with play/pause,
scrubbing, speed, and section jumps. It precomputes frames on the client and
plays them through `CanvasSurface` in a `requestAnimationFrame` loop *outside*
the VDOM — Dioxus never re-renders per frame.

**`Figure`** is the lighter embed, and it is the one a textbook page is built
from. A figure renders **once** on mount or when scrolled into view, then parks.
It wakes only when something happens: a parameter changes, a pointer arrives, an
animation is running. A `RenderSchedule` state machine tracks this, and an idle
page renders **zero** frames.

That is not an optimization detail. A chapter carries a dozen figures; at 60fps
each, a reader's laptop fan spins up and the battery drains while they read
static prose. Idle figures must cost nothing, or the medium does not work.

## One device per page

`ManimGpuProvider` requests a single wgpu device for the whole page; each figure
builds its surface with `CanvasSurface::with_shared`. Twelve figures, one
device. Requesting an adapter per canvas would exhaust the browser's device
budget well before twelve.

## The interaction widgets

```rust,ignore
use_parameter(name, range, default)   // slider ⟷ ValueTracker, two-way bound
DragHandle / DragSet                  // pointer hit-test + capture over scene coords
OrbitControls                         // drag-orbit, wheel-zoom, release inertia
```

Their state machines are **pure and unit-tested headlessly** — `DragSet`'s
hit-test and capture logic, `OrbitState`'s inertia decay, `RenderSchedule`'s
wake/park transitions, and the `Parameters` store all have native tests that
never touch a browser. Browser-only code is the thin shell around them.

## The Visual Complex Analysis page

This page is the design document's stated exit criterion: *if it feels like the
textbook we wished we had, the architecture is right.* Three figures, one
shared device, all render-on-demand.

**1. Domain coloring with draggable zeros and poles.** A rational function
`f(z) = Π(z − zᵢ) / Π(z − pⱼ)` with two teal zeros and two red poles you can
drag. Each drag rebuilds the `ComplexField` and calls
[`MaterialQuad::resample`](./materials.md); a phase slider rotates the hue. It
resamples at 128² while dragging and sharpens to 256² on release — from the
timings in the materials chapter, that is 0.16 ms during motion and 0.64 ms once
on settle.

Dragging a zero into a pole and watching them annihilate is a thing you cannot
be told. The winding numbers cancel in front of you.

**2. Conformal map timeline.** The [`DeformationGrid`](./deformations.md)
carried through `z ↦ z²` and then a Möbius map by two `ApplyMap` plays, on a
scrubbable `ManimPlayer` timeline.

**3. Riemann sphere.** The mesh sphere with a stereographic grid wrapped onto
it, under `OrbitControls`. The point at infinity becomes an ordinary place you
can rotate to look at.

## Building it

```sh
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli          # provides `dx`

cd examples/dioxus-app
dx serve --platform web           # http://localhost:8080, hot reload
dx build --platform web --release # static bundle under target/dx/
```

A plain `cargo build --target wasm32-unknown-unknown` compiles the crate but
emits no JS/HTML glue — use `dx` for something runnable.

## Honest status

`manim-dioxus` is compiled for `wasm32` in CI and its state machines are
unit-tested natively. The **in-browser render loops themselves** — GPU device
sharing, the `IntersectionObserver` lazy-mount, pointer and wheel interaction —
have not been walked through in a live browser session yet. The
[app's README](https://github.com/cryptex-ai/manim_rust/blob/main/examples/dioxus-app/README.md)
carries the punch list for that first manual pass.
