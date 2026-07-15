# Scene & Runtime

## Scene API

`Scene` bundles: the mobject arena (`SceneState`), a `Timeline`, a `Camera2D`,
updater registry, and config. Parity surface:

- `add`, `remove`, `bring_to_front/back`, `clear`
- `play(anims)`, `wait(t)`, `wait_until(cond, max)`, `pause`
- `next_section(name, skip)` — sections partition the timeline for partial
  rendering, mirroring CE sections
- `scene.time()`, `scene.get(id)` / indexing

Scene types (CE parity), realized as capabilities rather than subclasses:

| manim CE | here |
|---|---|
| `Scene` | `Scene` (2D camera) |
| `MovingCameraScene` | built-in: camera is always animatable |
| `ZoomedScene` | `scene.add_zoom_window(...)` (secondary camera → texture inset) |
| `ThreeDScene` | `Scene::three_d()` → `Camera3D` (post-v1 milestone) |
| `VectorScene` / `LinearTransformationScene` | `vector_space` module helpers |

## Config

`ManimConfig` (a plain struct, `Default` = CE defaults): frame geometry,
pixel dimensions, fps, background color, quality presets (`-ql/-qm/-qh/-qk`
equivalents as constructors: `Config::low()`, `::high()`, ...). Passed
explicitly — no global mutable config (CE's `config` global is a testing
hazard we don't replicate). A thread-local default supports the common case.

## Entry points

```rust
// Offline render (native): CLI-style
manim::render(SquareToCircle, Config::high().output("out/"))?;   // → mp4/png

// Real-time preview window (native)
manim::preview(SquareToCircle, Config::default())?;              // winit + vsync

// Programmatic frames (any target incl. tests)
let mut player = OfflinePlayer::new(SquareToCircle, config)?;
while let Some(frame) = player.next_frame()? { /* RgbaImage */ }
```

A tiny `manim-cli` binary (post-v1) wraps `render`/`preview` with arg parsing
for the `manim render scene.rs`-style workflow; v1 users call these from `main`.

## Interactivity (real-time first)

Because the timeline is data (see 04), the runtime supports pause/resume,
seek/scrub, and playback rate — the substrate the Dioxus `ManimPlayer` builds on.
A live mode where updaters also receive input events (mouse position in scene
coords, key events) is planned; today `UpdaterCtx` carries `dt`/`time` only.
