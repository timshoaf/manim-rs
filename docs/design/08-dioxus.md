# Dioxus Integration (`manim-dioxus`)

Goal: manim scenes as first-class Dioxus components in wasm apps.

```rust
use dioxus::prelude::*;
use manim_dioxus::prelude::*;

#[component]
fn App() -> Element {
    rsx! {
        ManimPlayer {
            scene: SquareToCircle,          // impl SceneBuilder + Clone
            autoplay: true,
            controls: true,                  // play/pause/scrub bar (optional)
            width: "640px", height: "360px",
        }
    }
}
```

## Architecture

- `ManimPlayer` renders a `<canvas>`; `use_effect` on mount creates the wgpu
  surface from the canvas (`wgpu::SurfaceTarget::Canvas`), builds the scene
  timeline, and starts a `requestAnimationFrame` loop via
  `wasm_bindgen_futures` + `web_sys`.
- The rAF loop lives outside the Dioxus VDOM (a `RealtimePlayer` in a
  `Rc<RefCell<…>>` owned by the component) — Dioxus only touches it through
  control signals. Canvas pixels never round-trip through the VDOM.
- Props → signals: `playing: Signal<bool>`, `progress: Signal<f32>` (two-way:
  scrubbing writes, playback publishes at ~10Hz to avoid re-render storms),
  `playback_rate`.
- `use_scene_controller()` hook returns a `SceneController` handle
  (`play/pause/seek/restart/jump_to_section`) for custom UI.
- Live interactivity: `on_pointer` events forwarded into `UpdaterCtx` input
  state (scene coords), enabling manim-as-interactive-widget.
- Resize: `ResizeObserver` → surface reconfigure, preserving frame aspect with
  letterboxing (background color from config).

## Native Dioxus

`ManimPlayer` also compiles for Dioxus desktop via `wgpu` on a child window /
texture import where supported; initial target is **web-first**, desktop
best-effort (tracked as separate issue).

## Crate boundaries

`manim-dioxus` depends only on the `manim` facade + `dioxus` + `web-sys`. No
render internals leak; everything flows through `RealtimePlayer` and
`SceneController`, which are plain `manim` APIs — so other UI frameworks
(leptos, egui) could wrap the same way later.

## SSR / hydration

Canvas is client-only: component renders an empty canvas server-side and boots
the player on hydration (`use_effect` runs client-side only — standard Dioxus
pattern). Optional `poster` prop: a pre-rendered PNG (from `OfflineRenderer`)
shown until wgpu initializes.
