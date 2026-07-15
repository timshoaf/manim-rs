# Rendering (wgpu)

## Contract: DisplayList in, pixels out

`manim-render` consumes the `DisplayList` produced by `manim-core` each frame:

```rust
pub struct DrawItem {
    pub path: Path,            // resolved world-space bezier subpaths
    pub fill: Option<FillStyle>,    // color | linear gradient, opacity
    pub stroke: Option<StrokeStyle>,// color, width (scene units), opacity, cap/join, dashes
    pub transform: Mat4,       // usually identity; 3D mobjects use it
    pub z_index: f32,
    pub generation: u64,       // cache key: bumped when the mobject's geometry changes
}
```

## v1 pipeline: CPU tessellation (lyon) + cached GPU buffers

1. **Tessellate**: each `DrawItem`'s path → `lyon_tessellation` fill and stroke
   into an interleaved vertex buffer (`pos: vec2f`, `color: vec4f` premultiplied).
   Tolerance derived from camera zoom so curves stay smooth when zoomed.
2. **Cache**: tessellation keyed by `(mobject key, generation, zoom bucket)`.
   Static mobjects tessellate once; only animating mobjects re-tessellate.
   During `Transform`-heavy frames this is the hot path — lyon handles ~10⁵
   segments/ms, ample for manim-scale scenes at 60fps.
3. **Batch**: all items concatenated into one vertex/index buffer pair per frame
   (rewritten only for dirty ranges), drawn back-to-front by `z_index` with a
   single pipeline. One draw call for typical scenes.
4. **Pipeline**: alpha blending (premultiplied), MSAA 4x render target,
   camera uniform (view-projection from `Camera2D`: frame center, zoom,
   rotation → NDC). WGSL shaders are trivial by design.

Stroke rendering: lyon `StrokeTessellator` with width in scene units
(manim semantics: stroke width visually scales with zoom, like CE's renderer;
a `ScreenSpaceStroke` opt-out exists for UI-ish overlays).

Gradients (v1): linear gradients evaluated per-vertex during tessellation
(manim's `set_color_by_gradient` maps color along path proportion — computed
at tessellation time, matching CE's per-anchor colors).

## Why not GPU curves in v1

Loop-Blinn / compute rasterization (à la vello) render beziers exactly on-GPU
and would eliminate re-tessellation during transforms. Costs: complexity,
wgpu compute limits on some web targets, and stroke semantics get hard. The
`DisplayList` contract means we can swap the backend later without touching
core — tracked as a post-v1 issue. (Using `vello` directly is also an option
behind the same trait if its API stabilizes for our needs.)

## Targets

```rust
pub trait RenderTarget { fn begin_frame(&mut self) -> TextureView; fn present(&mut self); ... }
```

- **SurfaceTarget**: winit window (native) or canvas (wasm). Resize-aware,
  vsync. Used by `RealtimePlayer` and `manim-dioxus`.
- **TextureTarget**: offscreen `wgpu::Texture` + readback buffer → `image::RgbaImage`.
  Used by `OfflineRenderer` (PNG sequence, piped to ffmpeg for video) and by
  golden-image tests. Works headless (no window system) via wgpu's
  fallback adapters, including lavapipe/llvmpipe in CI.

## Camera

`Camera2D`: frame_center (Point), frame_width/height (default 14.222×8 scene
units, manim's config), rotation, background color. It is itself scene state so
`MovingCameraScene` parity (`self.camera.frame.animate.scale(0.5)`) works by
animating camera fields with the standard animation machinery.

`Camera3D` (post-v1 milestone): perspective projection, phi/theta/gamma
orbiting, `ThreeDScene` parity. The Mat4 plumbing exists from day one
(`DrawItem.transform`), so 3D slots in without a redesign.

## Frame export & video

- `render_to_png(scene, path)` — single frame.
- PNG sequence + `ffmpeg` subprocess (native) for `.mp4`/`.gif`/`.webm`;
  wasm export via `MediaRecorder` on the canvas stream (Dioxus layer, post-v1).

## Golden-image testing

`TextureTarget` at fixed size (854×480), deterministic offline clock, compare
against checked-in PNGs with perceptual tolerance (per-channel δ + % differing
pixels threshold to absorb driver AA differences). `BLESS=1 cargo test`
regenerates. CI runs on lavapipe for reproducibility.
