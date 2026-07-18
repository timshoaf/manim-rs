# Manim CE Parity Map

Target: manim CE v0.19 public API. Status: ⬜ planned · 🟨 partial · ✅ done.
This file is the source of truth for the Linear backlog; update as work lands.

Milestones **M0–M5 are complete**; **M6 (3D)** is functional end to end —
geometry, perspective camera, depth sort, and blessed goldens (FE-107/108) — and
is now joined by a second, depth-tested **mesh pipeline** (FE-123…129,
`docs/design/12-mesh-pipeline.md`) offering real per-pixel occlusion and shading,
GPU instancing, and heightfield displacement. Both 3D paths ship; neither
replaces the other. Statuses below reflect `0.1.0-dev`.

Note on scope: this file tracks parity with CE's public API. The mesh pipeline is
largely *beyond* that surface — it has no CE counterpart to be at parity with —
so it appears here only where it changes a CE-facing story.

## mobject

### geometry (M1) ✅
| CE | Rust | Status |
|---|---|---|
| Arc, ArcBetweenPoints, CurvedArrow, CurvedDoubleArrow | `geometry::arc`, `geometry::line` | ✅ |
| Circle, Dot, AnnotationDot, LabeledDot, Ellipse, Annulus, AnnularSector, Sector | `geometry::arc` (+ `manim-text::LabeledDot`) | ✅ |
| Line, DashedLine, TangentLine, Elbow, Arrow, Vector, DoubleArrow | `geometry::line` | ✅ |
| Angle, RightAngle | `geometry::line` | ✅ |
| Polygram, Polygon, RegularPolygram, RegularPolygon, Star, Triangle | `geometry::polygram` | ✅ |
| Rectangle, Square, RoundedRectangle | `geometry::polygram` | ✅ |
| Union, Difference, Intersection, Exclusion, Cutout | `boolean` | ✅ (curve-preserving via `flo_curves`; GH polyline fallback) |
| ArrowTip variants (triangle/square/circle/stealth, filled/open) | `geometry::line::TipShape` | 🟨 (4 shapes; open outline for pointed tips, round/square solid-only) |
| ArcPolygon, ArcPolygonFromArcs | `geometry::arc` | ✅ |

### types (M1) ✅
VMobject ✅, VGroup ✅, VDict ✅, VectorizedPoint ✅, CurvesAsSubmobjects ✅,
DashedVMobject ✅, TracedPath ✅, Mobject ✅, ImageMobject ✅, SVGMobject ✅,
Group ✅ (alias of `VGroup` — the type-erased arena holds any mobject).
PMobject/Point (point clouds) — ⬜.

### animation (M2) ✅ — full core catalog
creation / fading / transform / movement / rotation / growing / apply /
composition / indication families, `.animate()`, updaters, ValueTracker,
`TransformMatchingShapes` / `TransformMatchingTex`, `AnimatedBoundary`,
`MoveToTarget` / `generate_target`, transform path functions — all landed;
`Unwrite` / `AddTextLetterByLetter` / `RemoveTextLetterByLetter` ✅ (manim-text).
Remaining tail: `PhaseFlow` / `ComplexHomotopy` variants — ⬜.

### text (M4) ✅ (with gaps)
Text ✅, Paragraph ✅, MarkupText ✅, Tex ✅, MathTex ✅, Typst ✅,
BulletedList ✅, Title ✅, DecimalNumber ✅, Integer ✅, Variable ✅, Write ✅.
MathTex substring **isolation** ✅ (FE-99: `get_parts_by_tex` / `set_color_by_tex`
via typst glyph spans, occurrence-level; synthesized glyphs fall back to
shape-matching). `Code` ✅ + monospace/`tt` markup runs ✅ (FE-100: syntect
highlighting behind the off-by-default `code` feature; `<tt>` uses bundled DejaVu
Sans Mono). `SingleStringMathTex` folded into `MathTex`.

### svg / braces (M4) ✅
SVGMobject ✅, Brace ✅ (+ `Brace::attached_to`), BraceLabel ✅ (`manim-text`).
BraceText / BraceBetweenPoints — 🟨 (compose `Brace` + `Text` manually).

### graphing (M5) ✅ (with gaps)
NumberLine ✅, UnitInterval ✅, Axes ✅, ThreeDAxes ✅, NumberPlane ✅,
ComplexPlane ✅, PolarPlane ✅; CoordinateSystem methods — plot ✅,
plot_parametric_curve ✅, get_graph_label ✅, get_riemann_rectangles ✅,
get_area ✅, get_secant_slope_group ✅, c2p/p2c ✅, add_coordinates ✅;
ParametricFunction ✅, FunctionGraph ✅, BarChart ✅.
Gaps: some auto-layouts (FE-105) 🟨. plot_implicit_curve / ImplicitFunction ✅.

### three_d (M6) ✅ (rendered, goldens blessed)

**Two 3D paths ship, and both are supported** — see `docs/migration-guide.md`
for which to reach for, and `docs/design/12-mesh-pipeline.md` for the second.

*Path 1 — project-and-sort (CE-shaped, the parity surface).* The CE catalog maps
1:1 and is unchanged by the mesh work: ThreeDVMobject 🟨 (faces-as-children
model), Surface ✅, Sphere ✅, Dot3D ✅, Cube ✅, Prism ✅, Cone ✅,
Cylinder ✅, Line3D ✅, Arrow3D ✅, Torus ✅, `ThreeDAxes` ✅,
`rotate_about_axis` ✅. Geometry is camera-independent and unit-tested
headlessly; rendering is a perspective orbit camera + camera-space depth **sort**
with plane-fitted tessellation (sphere/cube/axes-surface/torus goldens). This is
CE's model, including its limits: whole items sort per frame, so interpenetrating
geometry cannot occlude correctly and there is no per-pixel shading.

*Path 2 — depth-tested meshes (beyond CE, FE-123…129).* `mesh::{Mesh, Surface3D,
InstancedMesh, HeightField}` ✅ carry real indexed geometry through a second,
depth-**tested** render pass with per-pixel Blinn-Phong shading, a sorted
translucent queue, and GPU instancing. Real occlusion of interpenetrating
geometry ✅; `MorphMesh` / `MorphSurface` ✅ (parameter-space tweening — CE has
no equivalent). No CE counterpart exists for these, so they are *not* parity
items:
- **Instancing** — one base mesh at many transforms; 10k spheres ≈ 0.8 ms/frame,
  2 draw calls. CE would need 10k mobjects.
- **Heightfield displacement** — vertex-shader displacement from an `R32Float`
  texture; a per-frame-evolving field costs one `nu × nv × 4 B` upload and zero
  CPU re-meshing. Neither CE nor ManimGL has an equivalent.

*`set_fill_by_value`* (per-face value color) ✅ (S1/FE-136) —
`Surface3D::set_fill_by_value(f, colormap, min, max)` colors each vertex by a
scalar function of its position through a [`Colormap`](../../crates/manim-core/src/display.rs)
(viridis / magma / coolwarm / turbo), recomputed on re-mesh; clears the M6
deferral. (The CE `threed::Surface` project-and-sort path keeps its checkerboard;
value coloring lives on the depth-tested `Surface3D`.)

### others (M5) ✅
Matrix ✅, DecimalMatrix ✅, IntegerMatrix ✅, MobjectMatrix ✅,
Table (+ MathTable / DecimalTable) ✅, Graph / DiGraph (+ layouts) ✅,
VectorField ✅, ArrowVectorField ✅, StreamLines ✅ (animated flow via animate_flow ✅),
ValueTracker ✅, ComplexValueTracker ✅, TracedPath ✅.

## scene (M3 / M6)
Scene ✅, MovingCameraScene (moving camera) ✅, sections ✅,
VectorScene helpers ✅ (`vector_space`), LinearTransformationScene ✅.
ZoomedScene ⬜, ThreeDScene ✅ (FE-107 landed: 3D camera + orientation).

## camera (M3 + 3D done)
2D camera frame center / width / zoom / rotation animatable ✅; background
color / opacity ✅. ThreeDCamera phi / theta / gamma / focal_distance ✅
(FE-107; set_camera_orientation, ambient rotation, fixed-in-frame HUD).
Multi-camera zoomed display — ⬜ (ZoomedScene, FE-120).

## utils
| CE module | Rust home | Status |
|---|---|---|
| bezier | manim-math::bezier | ✅ |
| rate_functions | manim-math::rate_functions | ✅ |
| space_ops | manim-math::space_ops | ✅ |
| color | manim-color | ✅ (full CE catalog; XKCD/X11 extras ⬜) |
| paths (straight/arc/spiral path funcs) | manim-math::paths / core `animations::paths` | ✅ |
| config | manim-core::config | ✅ |
| images/ipython/hashing/caching | n/a (Python-specific) | — |
| sounds (`Scene.add_sound`) | manim-core cues + manim-render ffmpeg mux | ✅ (native video export) |
| tex / tex_templates | manim-text::typst mapping | ✅ (LaTeX-subset → typst) |

## Explicit deferrals (documented, issue-tracked)
- **Boolean smoothness**: ✅ resolved (FE-121a) — `boolean` ops are now
  curve-preserving via `flo_curves` (pure-Rust path arithmetic), keeping Bézier
  arcs; the Greiner–Hormann polyline clip remains a documented fallback for
  degenerate inputs.
- **3D rendering**: ✅ resolved. FE-107 landed the CE-shaped project-and-sort path
  (camera/projection/depth-sort); FE-123…129 added the depth-tested mesh
  pipeline alongside it (`docs/design/12-mesh-pipeline.md`). Both ship; the
  geometry of both is headless-testable.
- **Vector strokes occluded *by* meshes**: ⬜ deliberate. The 2D vector pass draws
  over the mesh pass with no depth test — 2D content is annotation (CE's
  `add_fixed_in_frame_mobjects` semantics). Mixed scenes needing strokes *inside*
  the 3D scene use the project-and-sort path; a `z_test` opt-in on `DrawItem` is
  recorded future work.
- **Self-intersecting translucent meshes**: ⬜ known limit. The translucent queue
  sorts per *item* (per instance for instanced meshes), which cannot resolve a
  translucent surface intersecting itself. Weighted-blended OIT is the recorded
  upgrade path.
- OpenGL-renderer-specific API (CE's experimental opengl namespace): n/a — our
  renderer IS the GPU renderer.
- `manim cfg` / plugin system: replaced by Cargo features & Rust traits.
- IPython/Jupyter integration: out of scope; wasm/Dioxus embedding replaces it.
