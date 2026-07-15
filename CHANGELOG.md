# Changelog

All notable changes to `manim_rust` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); this project is pre-1.0 and the
API may change between `0.x` releases.

## [0.1.0-dev] — unreleased

The first end-to-end milestone: a headless scene graph and animation engine, a
wgpu renderer, text/math typesetting, graphing, and 3D geometry — enough to port
most of the manim CE example gallery. Summarized by crate.

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
  `Cutout` via a hand-rolled Greiner–Hormann polygon clipper (polyline results).
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
  headless-tested (3D rendering pending FE-107).
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
- 3D camera / depth pipeline — in progress.

### manim-dioxus
- A `ManimPlayer` Dioxus component that drives a scene on a canvas
  (`requestAnimationFrame` playback loop), plus a gallery app.

### manim (facade)
- `use manim::prelude::*;` re-exporting the scene, geometry, animation, color, and
  math surface; `manim::render(..)` (MP4) and `manim::preview(..)` (native window).
- A runnable example gallery mirroring the CE classics, with a construct-only
  smoke test guarding against example rot.

[0.1.0-dev]: https://github.com/
