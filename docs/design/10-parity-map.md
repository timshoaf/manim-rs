# Manim CE Parity Map

Target: manim CE v0.19 public API. Status: ⬜ planned · 🟨 partial · ✅ done.
This file is the source of truth for the Linear backlog; update as work lands.

Milestones **M0–M5 are complete**; **M6 (3D)** geometry is done and its renderer
(camera/projection/depth-sort, FE-107) is in flight. Statuses below reflect
`0.1.0-dev`.

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
| Union, Difference, Intersection, Exclusion, Cutout | `boolean` | ✅ (polyline result — smoothness gap, see below) |
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
Gaps: substring/sub-superscript **isolation** (FE-99) 🟨; `Code` (syntax
highlighting), monospace/`tt` runs (FE-100) ⬜; `SingleStringMathTex` folded into
`MathTex`.

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

### three_d (M6) — geometry ✅, renderer in flight
ThreeDVMobject 🟨 (faces-as-children model), Surface ✅, Sphere ✅, Dot3D ✅,
Cube ✅, Prism ✅, Cone ✅, Cylinder ✅, Line3D ✅, Arrow3D ✅, Torus ✅,
`ThreeDAxes` ✅, `rotate_about_axis` ✅. Geometry is camera-independent and
unit-tested headlessly; **rendering (3D camera/projection/depth-sort, FE-107) is
in flight**. `set_fill_by_value` (per-face value color) ⬜.

### others (M5) ✅
Matrix ✅, DecimalMatrix ✅, IntegerMatrix ✅, MobjectMatrix ✅,
Table (+ MathTable / DecimalTable) ✅, Graph / DiGraph (+ layouts) ✅,
VectorField ✅, ArrowVectorField ✅, StreamLines ✅ (animated flow via animate_flow ✅),
ValueTracker ✅, ComplexValueTracker ✅, TracedPath ✅.

## scene (M3 / M6)
Scene ✅, MovingCameraScene (moving camera) ✅, sections ✅,
VectorScene helpers ✅ (`vector_space`), LinearTransformationScene ✅.
ZoomedScene ⬜, ThreeDScene ✅ (FE-107 landed: 3D camera + orientation).

## camera (M3 done, 3D in flight)
2D camera frame center / width / zoom / rotation animatable ✅; background
color / opacity ✅. ThreeDCamera phi / theta / gamma / focal_distance — 🟨 in
flight (FE-107). Multi-camera zoomed display — ⬜ (ZoomedScene).

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
| sounds (`Scene.add_sound`) | manim-core (native feature) | ⬜ |
| tex / tex_templates | manim-text::typst mapping | ✅ (LaTeX-subset → typst) |

## Explicit deferrals (documented, issue-tracked)
- **Boolean smoothness**: `boolean` ops flatten to polylines (no skia-pathops
  equivalent); Bézier-preserving boolean is post-v1.
- **3D rendering** waits on FE-107 (camera/projection/depth-sort); the geometry is
  already headless-testable.
- **ZoomedScene** and **sound** (`add_sound`) — pending.
- **Text isolation / Code** (FE-99 / FE-100) — substring sub-super isolation and
  syntax-highlighted code blocks pending.
- OpenGL-renderer-specific API (CE's experimental opengl namespace): n/a — our
  renderer IS the GPU renderer.
- `manim cfg` / plugin system: replaced by Cargo features & Rust traits.
- IPython/Jupyter integration: out of scope; wasm/Dioxus embedding replaces it.
