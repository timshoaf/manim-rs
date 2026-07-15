# 11 · Dioxus desktop: assessment & recommendation (FE-115)

**Status:** assessment only — no desktop rendering is built yet. This note records
why the web `<ManimPlayer>` path does not carry over to Dioxus desktop unchanged,
the options considered, and the recommended v1 approach.

## The mismatch

On the web, [`ManimPlayer`](../../crates/manim-dioxus/src/lib.rs) mounts a
`<canvas>` and hands it to
[`CanvasSurface`](../../crates/manim-render/src/canvas.rs), which builds a wgpu
surface via `wgpu::SurfaceTarget::Canvas(HtmlCanvasElement)` and drives a
`requestAnimationFrame` loop. This works because on the web our Rust is compiled
to **wasm running inside the browser's JS context**, so `web_sys::HtmlCanvasElement`
is a real handle wgpu can render into.

Dioxus **desktop** (0.6) is different: it is a **WebView** shell (tao + wry —
WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux) whose UI is HTML,
but our Rust runs **natively**, not as wasm inside the webview. Consequences:

- There is no `HtmlCanvasElement` reachable from native Rust — `web_sys` types
  only exist on the wasm target. `CanvasSurface::new` cannot be called at all
  (the whole `canvas` module is `#[cfg(target_arch = "wasm32")]`).
- The webview's own JS engine *could* run WebGL/WebGPU against a canvas, but our
  renderer is native wgpu, not JS — the two live on opposite sides of the
  native/JS boundary.

So "just render into the canvas" is not available: native code owns the GPU, the
webview owns the canvas, and nothing bridges them out of the box.

## Options considered

1. **Native child GPU window composited into the webview.** Create a native
   winit/tao surface (we already render into one via
   [`RealtimePlayer`](../../crates/manim-render/src/preview.rs)) and position it
   as a child window over a placeholder `<div>`, syncing geometry on resize/scroll.
   - *Verdict:* highest fidelity and performance, but brittle and
     platform-specific (child-window z-order, DPI, scroll/resize sync, occlusion
     by other DOM). High effort, poor portability. **Not recommended for v1.**

2. **Offscreen render → image bridge.** Reuse the native
   [`OffscreenRenderer`](../../crates/manim-render/src/renderer.rs) (already built,
   golden-tested) to render each frame to RGBA, and display it in the webview via
   either:
   - a base64 PNG **`data:` URL** swapped into an `<img>` each frame (simplest,
     but PNG-encode + base64 + DOM churn caps practical FPS), or
   - a JS **`putImageData`** bridge: push raw RGBA bytes through Dioxus desktop's
     `eval`/asset channel into a real `<canvas>` in the webview via
     `ctx.putImageData(...)` (skips PNG encode; much faster).
   - *Verdict:* reuses all existing native rendering, fully cross-platform, no new
     GPU surface code. The `putImageData` variant is fast enough for the short,
     modest-resolution playback the player targets. **Recommended primary path.**

3. **Package the web build inside the webview.** Ship the existing wasm web app
   (which already works — `CanvasSurface` + rAF) and load it in the desktop
   webview as a local page. Desktop becomes "a packaged web app."
   - *Verdict:* zero new rendering code and identical behavior to the web player,
     at the cost of shipping the wasm toolchain output and losing native-Rust
     integration in the same process. **Recommended fallback / fast path to a
     shippable desktop demo.**

4. **Wait for a native Dioxus renderer (Blitz).** Dioxus is developing a
   native/wgpu VDOM renderer (Blitz). A custom canvas-like element could
   eventually integrate our wgpu output directly.
   - *Verdict:* not available in 0.6; revisit when Blitz stabilizes.

## Recommendation

For v1 desktop support, implement **option 2 with the `putImageData` bridge**:
drive `OffscreenRenderer` from a native timer, push raw RGBA into a webview
`<canvas>` per frame. It reuses the golden-tested offscreen path, stays
cross-platform, and needs no fragile native-window compositing. Keep **option 3**
(packaged web app) as the quickest route to a shippable desktop build if a
native-process integration is not required. Explicitly **defer option 1**; revisit
**option 4** when Blitz lands.

Because the render side is not yet wired for desktop, the FE-114 `ZoomWindow`
support in `RealtimePlayer`/`CanvasSurface` is the reusable substrate here: the
`putImageData` bridge would render through `OffscreenRenderer::render_frame`,
which already honors zoom windows.
