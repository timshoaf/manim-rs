# Changelog

All notable changes to `manim_rust` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); this project is pre-1.0 and the
API may change between `0.x` releases.

## [0.1.0-dev] — unreleased

The first end-to-end milestone: a headless scene graph and animation engine, a
wgpu renderer, text/math typesetting, graphing, and 3D geometry — enough to port
most of the manim CE example gallery. Summarized by crate.

### Release readiness (FE-117)

- **Packaging metadata** in place on every crate: `description`, `keywords`,
  `categories`, `readme`, `homepage`, and the workspace `MIT` license (see
  [`LICENSE`](LICENSE)). Workspace version pinned at `0.1.0`.
- **Nothing is published to crates.io yet.** One decision remains before a first
  release: the facade crate name. **`manim` may be unavailable/reserved on
  crates.io** — if so the facade ships under a fallback (e.g. `manim-rs`) while
  the `manim-*` component crates keep their names. `manim-dioxus` is
  `publish = false` (a thin integration/demo crate over the whole stack).
- **CI** gained a software-rendered golden job (mesa lavapipe + `REQUIRE_GPU=1`,
  so a missing adapter fails loudly instead of skipping), an optional-feature
  matrix (`preview`, `code`), and `-D warnings` on rustdoc. Toolchain pinned via
  [`rust-toolchain.toml`](rust-toolchain.toml); contributor guide in
  [`CONTRIBUTING.md`](CONTRIBUTING.md).

### Mesh-pipeline follow-ons (FE-130, FE-131)

- **`set_z_test(true)`** ([#3]): opt a vector mobject into depth testing against
  the mesh pass, so meshes in front occlude it — for 2D content living *inside*
  a 3D scene (contour curves under a floating surface, wireframe parameter
  curves, world-pinned labels). Off by default and byte-identical when off (or
  in a mesh-less scene); ignored for image quads and `fixed_in_frame` HUD
  content. Opted-in items draw in their own read-only depth pass between the
  mesh pass and the plain vector pass, on native and canvas alike.
- **Live 3D in the browser** ([#2]): `ManimPlayer`'s `LiveUpdater` path now
  renders under the **live state's own camera** (plus its background and zoom
  window), so an updater driving `set_camera_orientation` — a timer, or a
  pointer-drag orbit — shows real depth-tested 3D. The dioxus gallery gained a
  "Live 3D (drag)" scene: an evolving `HeightField` wave you orbit by dragging.

[#2]: https://github.com/timshoaf/manim-rs/issues/2
[#3]: https://github.com/timshoaf/manim-rs/issues/3

### Depth-tested 3D mesh pipeline (FE-123…129)

A **second, parallel render path for triangle meshes** — depth-tested, per-pixel
shaded, GPU-instanced — layered *under* the existing 2D vector path. Spans
`manim-core`, `manim-render`, and the facade; see
[`docs/design/12-mesh-pipeline.md`](docs/design/12-mesh-pipeline.md) and the
[migration guide](docs/migration-guide.md#two-3d-paths-threed-vs-mesh) for how it
relates to the CE-shaped `threed` module.

- **Nothing existing changed semantics.** The `threed` project-and-sort path, the
  scene graph, the timeline, and every mobject keep working; a scene with no
  meshes produces byte-identical frames (guarded by the existing goldens). The
  new path is opt-in by using a mesh mobject.
- **New mobjects** (`manim_core::mesh`, wasm-clean): `Mesh` (a `TriMesh` +
  `MeshMaterial` + model transform), `Surface3D` (parametric `(u, v) → Vec3`,
  re-meshed on change, with CE-parity checkerboard fill), `InstancedMesh` (one
  base mesh at many transforms; `::spheres` / `::cylinders` helpers), and
  `HeightField` (a grid displaced by height data in the vertex shader).
- **Real occlusion and shading**: a `Depth32Float` attachment plus a WGSL mesh
  pipeline shading Blinn-Phong per pixel in linear space, with
  `Shading::{Flat, Smooth}`. Interpenetrating and self-occluding geometry — a
  closed torus hiding its own far half — now renders correctly, which the
  per-item depth sort cannot do.
- **Transparency**: opaque and translucent queues split on material opacity; the
  translucent queue draws after the opaque one, depth-tested read-only and sorted
  back-to-front per item (per instance for instanced meshes). Per-item sorting
  cannot resolve *self*-intersecting translucent geometry — documented, with
  weighted-blended OIT as the recorded upgrade path.
- **GPU instancing**: per-instance transform + color on a second vertex buffer.
  A 10k-atom molecule is **2 draw calls**; measured **0.8 ms/frame** for 10k
  instanced spheres (~5.76M tris) at 427×240 offscreen incl. readback, release,
  RTX 4090 / Vulkan.
- **Heightmap displacement**: an `R32Float` height texture sampled in the vertex
  shader, with in-shader finite-difference normals. A field evolving every frame
  costs one `nu × nv × 4 B` upload and zero CPU re-meshing. Neither CE nor
  ManimGL has an equivalent.
- **Animation**: `TriMesh::lerp` (same-topology vertex lerp) behind `MorphMesh`;
  `MorphSurface` tweens a `Surface3D` in **parameter space**, giving
  correspondence-free homeomorphism/isotopy animation. Ordinary transforms,
  updaters, `.animate()`, and `save_state`/`Restore` work on meshes unchanged.
  (Style setters do *not* — a mesh's appearance is its `MeshMaterial`.)
- **Caching**: a GPU buffer cache keyed like the tessellation cache, diffing per
  resource by `Arc` identity — static meshes upload once, and a heights-only
  change rewrites just the height texture. Renderer caches now key on
  `(arena, source, generation)`: a `DisplayList` carries its `SceneState`'s
  process-unique arena stamp (preserved by `Clone`, so timeline snapshots still
  hit the cache), which stops two independently built scenes from silently
  sharing entries through a shared renderer.
- **Portability**: no compute shaders and no storage buffers anywhere in the mesh
  path, so it runs on wgpu's WebGL2 backend as well as WebGPU. Verified through
  `manim-dioxus`'s `ManimPlayer` (which needed no code change — `DisplayList`
  carries the mesh channel and `CanvasSurface` already runs the pass) and the
  wasm32 example apps.
- **Gallery**: `mesh_surface_rotate` (shaded saddle, turntable camera),
  `mesh_molecule` (294 instanced atoms + bonds, translucent molecular surface),
  `mesh_heightfield_wave` (updater-driven traveling/standing wave), and
  `mesh_morph` (sheet → cylinder → torus homeomorphism), plus a `3D mesh` scene in
  the Dioxus gallery app.

### manim-math
- Vectors on `glam` (`Point` = `Vec3`), affine transforms, and `space_ops`
  (rotation matrices about arbitrary axes, angle helpers).
- Cubic Bézier paths: `Path` / `SubPath`, arc-length parameterization,
  `point_from_proportion`, `get_subcurve`, `insert_n_curves`, and `align_with`
  (the point-count alignment `Transform` depends on).
- The full CE rate-function catalog and transform path functions (straight / arc
  / spiral).

### manim-color
- `Color` type with linear/sRGB handling, hex/HSL conversions, and interpolation.
- The complete manim CE named-color catalog (including the `_A`…`_E` shade
  families) plus gradient support.

### manim-core
- **Scene graph**: a slotmap arena with cheap `Copy` typed handles
  (`MobjectId<M>` / `AnyId`), parent/child families, generation-stamped geometry
  for render caching, and a `Clone`-able `SceneState`.
- **Mobject model**: the `Mobject` trait, blanket `MobjectExt` (transform /
  position / size / style), builder (`with_*`) and mutate (`set_*`) styles, and
  dyn-callable style setters.
- **Geometry catalog**: arcs, circles/dots/ellipses/sectors, lines/arrows/angles,
  polygons/stars/polygrams, rectangles, plus `VGroup` / `VDict` /
  `DashedVMobject` / `CurvesAsSubmobjects` / `TracedPath`.
- **Boolean operations**: `Union` / `Difference` / `Intersection` / `Exclusion` /
  `Cutout`, curve-preserving via `flo_curves` (FE-121a); a hand-rolled
  Greiner–Hormann polyline clipper remains the fallback for degenerate inputs.
- **Animation engine**: the `Animation` trait (begin/interpolate/finish), the full
  CE catalog (creation, fading, transform family, indication, growing,
  movement/rotation, apply, composition, numbers, updaters), `.animate()`,
  `TransformMatchingShapes`, and `AnimatedBoundary`.
- **Timeline**: `construct` builds a snapshot timeline (eager end-state apply),
  enabling cheap scrubbing and re-rendering; `ValueTracker`, updaters, and
  `Scene::always_redraw`.
- **Graphing**: `NumberLine`, `Axes`, `ThreeDAxes`, `NumberPlane` / `ComplexPlane`
  / `PolarPlane`, plotting (`plot`, parametric curves, Riemann rectangles, area,
  secant-slope group), and `BarChart`.
- **Networks & fields**: `Graph` / `DiGraph` with layouts; `VectorField`,
  `ArrowVectorField`, `StreamLines`.
- **Vector spaces**: `add_vector` / `add_plane` / `add_axes` helpers and a
  `LinearTransformationScene` (ghost plane, basis vectors, `apply_matrix`).
- **3D geometry** (`threed`): parametric `Surface` with checkerboard faces;
  `Sphere` / `Cube` / `Prism` / `Cylinder` / `Cone` / `Torus` / `Dot3D` /
  `Line3D` / `Arrow3D`; `ThreeDAxes`; rotation helpers. Camera-independent and
  headless-tested; rendered via the perspective camera + per-item depth sort
  (FE-107/108). For depth-*tested* meshes see the mesh pipeline above.
- **Mesh mobjects** (`mesh`): `TriMesh`, `Mesh`, `Surface3D`, `InstancedMesh`,
  `HeightField`, `MeshMaterial`, `MorphMesh` / `MorphSurface`, and the
  `DisplayList` `meshes` channel — see the mesh pipeline section above.
- `Config` (resolution/fps presets), a moving 2D camera, and sections.
- `Result`-based errors (`CoreError`, with a `Text` variant wrapping typesetting
  failures).

### manim-text
- **Text** via `cosmic-text` shaping with bundled DejaVu faces; `Text`,
  `Paragraph`, `MarkupText`, `Write`.
- **Math** via `typst`: `MathTex` (LaTeX-subset → typst translation), `Tex`,
  `Typst`; `TransformMatchingTex` (shape-signature glyph matching).
- **Numbers**: `DecimalNumber`, `Integer`, `Variable`, `ChangingDecimal`.
- **Composites**: `BulletedList`, `Title`, `LabeledDot`, `BraceLabel`,
  `Matrix` / `DecimalMatrix` / `IntegerMatrix` / `MobjectMatrix`,
  `Table` / `MathTable` / `DecimalTable`.
- **Graph labels**: axis coordinates/labels, graph labels, bar-chart labels, and
  vector labels (extension traits over the graphing / vector-space types).

### manim-render
- A `wgpu` renderer consuming core display lists: `lyon` tessellation of
  fills/strokes, camera-aware projection, and gradient paint.
- Offscreen PNG rendering and `ffmpeg`-backed video export; golden-image tests.
- `SVGMobject` (via `usvg`) and textured `ImageMobject`.
- 3D camera (perspective orbit, `ThreeDParams`) with camera-space depth sorting
  for the `threed` path, and the depth-tested mesh pipeline (`mesh_pipeline`)
  described above.

### manim-dioxus
- A `ManimPlayer` Dioxus component that drives a scene on a canvas
  (`requestAnimationFrame` playback loop), plus a gallery app.

### manim (facade)
- `use manim::prelude::*;` re-exporting the scene, geometry, animation, color, and
  math surface; `manim::render(..)` (MP4) and `manim::preview(..)` (native window).
- A runnable example gallery mirroring the CE classics, with a construct-only
  smoke test guarding against example rot.

[0.1.0-dev]: https://github.com/
