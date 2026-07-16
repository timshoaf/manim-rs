# Manim CE Parity Audit (FE-111)

A systematic sweep of the manim CE **v0.19** public API against the
`manim_rust` implemented surface, done by reading the actual crate exports (not
doc examples). Status legend:

- ✅ **full** — present with matching semantics
- 🟨 **partial** — present but missing behavior (noted)
- ⬜ **absent** — not implemented
- ❓ **unsure** — couldn't confirm CE has it, or whether ours matches semantics

Severity: **core** (blocks common workflows) · **common** (frequently used) ·
**niche** (rarely used / specialized).

> This is the exit-criterion document for the "full parity" claim. It supersedes
> the summary in [design/10-parity-map.md](design/10-parity-map.md) where they
> disagree (see [§3 corrections](#3-corrections-to-existing-docs)).

---

## 1. Module-by-module tables

### mobject.geometry — arcs / lines / polygons

| CE | Ours | Status | Sev. |
|---|---|---|---|
| Arc, ArcBetweenPoints | `geometry::arc::{Arc, ArcBetweenPoints}` | ✅ | common |
| CurvedArrow, CurvedDoubleArrow | — | ⬜ | common |
| Circle, Dot | `Circle`, `Dot` | ✅ | core |
| AnnotationDot | — | ⬜ | niche |
| LabeledDot | `manim-text::LabeledDot` | ✅ | niche |
| Ellipse, Annulus, AnnularSector, Sector | all present | ✅ | common |
| Line, DashedLine, Elbow | present | ✅ | core |
| TangentLine | `geometry::line::TangentLine` | ✅ | niche |
| Arrow, Vector, DoubleArrow | present | ✅ | core |
| LabeledLine, LabeledArrow | — | ⬜ | niche |
| Angle, RightAngle | present | ✅ | common |
| Polygon, Polygram, RegularPolygon, RegularPolygram, Star, Triangle | present | ✅ | common |
| Rectangle, Square, RoundedRectangle | present | ✅ | core |
| Cutout | `boolean::Cutout` | 🟨 (polyline, not Bézier) | common |
| ArcPolygon, ArcPolygonFromArcs | — | ⬜ | niche |
| ArrowTip family (Stealth / Triangle / Circle / Square, filled+open) | inline filled-triangle tip | 🟨 (one tip style) | common |

### mobject.types — vectorized containers

| CE | Ours | Status | Sev. |
|---|---|---|---|
| VMobject | `geometry::VMobject` | ✅ | core |
| VGroup | `VGroup` | ✅ | core |
| VDict | `VDict` | ✅ | niche |
| VectorizedPoint | `VectorizedPoint` | ✅ | niche |
| CurvesAsSubmobjects | `CurvesAsSubmobjects` | ✅ | niche |
| DashedVMobject | `DashedVMobject` | ✅ | common |
| Group (non-vectorized) | — (only `VGroup`) | ⬜ | common |
| PMobject / point clouds (Mobject1D/2D, PGroup) | — | ⬜ | niche |
| ThreeDVMobject | face-group model (no named type) | 🟨 | niche |

### mobject.text — text & numbers

| CE | Ours | Status | Sev. |
|---|---|---|---|
| Text | `text::Text` (t2c/t2w/t2s, line_spacing, alignment) | ✅ | core |
| Paragraph | `Paragraph` | ✅ | common |
| MarkupText | `MarkupText` (b/i/u/s/span, sub/sup, size) | ✅ | common |
| Tex, MathTex | `Tex`, `MathTex` (LaTeX-subset→typst) | 🟨 (no substring isolation) | core |
| SingleStringMathTex | folded into `MathTex` | ✅ | niche |
| MathTex `set_color_by_tex` / `.set_color_by_tex_to_color_map` / `index_labels` | — | ⬜ | common |
| Code (syntax highlight) | — | ⬜ | common |
| Text `set_color_by_gradient` (whole word) | family `set_color_by_gradient` on scene | 🟨 (no Text builder) | common |
| BulletedList, Title | present | ✅ | common |
| DecimalNumber, Integer, Variable | present (commas/sign/places/unit) | ✅ | common |

### mobject.svg — SVG & braces

| CE | Ours | Status | Sev. |
|---|---|---|---|
| SVGMobject | `svg::SVGMobject` (usvg) | ✅ | common |
| ImageMobject | `image_mobject::ImageMobject` | ✅ | common |
| Brace | `geometry::Brace` (+ `attached_to`) | ✅ | common |
| BraceLabel | `manim-text::BraceLabel` | ✅ | common |
| BraceText | compose `Brace`+`Text` manually | 🟨 | niche |
| BraceBetweenPoints | — (use `Brace::new`) | 🟨 | niche |

### mobject.three_d

| CE | Ours | Status | Sev. |
|---|---|---|---|
| Surface | `threed::Surface` (checkerboard faces) | ✅ | common |
| Sphere, Cube, Prism, Cone, Cylinder, Torus, Dot3D | all present | ✅ | common |
| Line3D, Arrow3D | present | ✅ | common |
| ThreeDAxes | `threed::ThreeDAxes` | ✅ | common |
| ThreeDVMobject | face-group model | 🟨 | niche |
| Surface `set_fill_by_value` | — | ⬜ | niche |
| Text3D | — | ⬜ | niche |

### mobject.graphing (coordinate_systems / functions / number_line)

| CE | Ours | Status | Sev. |
|---|---|---|---|
| NumberLine, UnitInterval | `NumberLine`, `NumberLine::unit_interval` | ✅ | core |
| Axes, ThreeDAxes | present | ✅ | core |
| NumberPlane, ComplexPlane, PolarPlane | present | ✅ | common |
| CoordinateSystem `plot`, `plot_parametric_curve` | present | ✅ | core |
| `plot_implicit_curve` / ImplicitFunction | — | ⬜ | common |
| `c2p`/`p2c`, `input_to_graph_point` | present | ✅ | core |
| `get_graph_label`, `add_coordinates`, `get_axis_labels` | `manim-text` traits | ✅ | core |
| `get_area`, `get_riemann_rectangles`, `get_secant_slope_group` | present | ✅ | common |
| `get_vertical_line`, `get_horizontal_line`, `get_lines_to_point` | first two ✅; `get_lines_to_point` ⬜ | 🟨 | niche |
| `angle_of_tangent` / `slope_of_tangent` / `get_T_label` | — | ⬜ | niche |
| ParametricFunction, FunctionGraph | present | ✅ | common |
| BarChart | `graphing::BarChart` | ✅ | common |

### mobject.matrix / table

| CE | Ours | Status | Sev. |
|---|---|---|---|
| Matrix, DecimalMatrix, IntegerMatrix, MobjectMatrix | present (`of`, get_rows/columns/brackets) | ✅ | common |
| Matrix `get_det_text`, `add_background_to_entries` | — | ⬜ | niche |
| Table, MathTable, DecimalTable | present (`with_lines`, `highlight_cell`) | ✅ | common |
| IntegerTable, MobjectTable | — (use `Table`/`MathTable`) | 🟨 | niche |
| Table row/col labels, `get_row_labels` | partial | 🟨 | niche |

### mobject.vector_field / graph / value_tracker

| CE | Ours | Status | Sev. |
|---|---|---|---|
| VectorField, ArrowVectorField, StreamLines | present | ✅ | common |
| StreamLines animated flow (`start_animation`) | static only | 🟨 | common |
| Graph, DiGraph (+ layouts) | `network::{Graph, DiGraph, GraphLayout}` | ✅ | common |
| ValueTracker, ComplexValueTracker | present | ✅ | core |

### animation.* — creation / transform / fading / indication / …

| CE | Ours | Status | Sev. |
|---|---|---|---|
| Create, Uncreate, DrawBorderThenFill | present | ✅ | core |
| Write | `manim-text::Write` | ✅ | core |
| Unwrite, AddTextLetterByLetter, AddTextWordByWord, RemoveTextLetterByLetter | — | ⬜ | common |
| ShowIncreasingSubsets, ShowSubmobjectsOneByOne, SpiralIn | present | ✅ | niche |
| FadeIn, FadeOut | present (+ shift/scale/target) | ✅ | core |
| Transform, ReplacementTransform, TransformFromCopy, TransformInto | present | ✅ | core |
| ClockwiseTransform, CounterclockwiseTransform | — | ⬜ | niche |
| MoveToTarget / `generate_target` pattern | — (use `.animate()`/`TransformInto`) | 🟨 | common |
| ApplyMatrix, ApplyFunction, ApplyPointwiseFunction, Homotopy | present | ✅ | common |
| ApplyComplexFunction, ComplexHomotopy, PhaseFlow | — | ⬜ | niche |
| FadeTransform, Restore, ScaleInPlace, ShrinkToCenter, Swap, CyclicReplace | present | ✅ | common |
| TransformMatchingShapes, TransformMatchingTex | present | ✅ | common |
| Indicate, Flash, FocusOn, Circumscribe, Wiggle, ApplyWave, ShowPassingFlash | present | ✅ | common |
| Blink, Broadcast | — | ⬜ | niche |
| GrowFromPoint/Center/Edge, GrowArrow, SpinInFromNothing | present | ✅ | common |
| Rotate, Rotating, MoveAlongPath (+ `along`) | present | ✅ | core |
| AnimationGroup, Succession, LaggedStart, LaggedStartMap | present | ✅ | core |
| ChangingDecimal, ChangeDecimalToValue | present | ✅ | common |
| UpdateFromFunc, UpdateFromAlphaFunc, MaintainPositionRelativeTo | present | ✅ | common |
| AnimatedBoundary, TracedPath | present | ✅ | common |
| ChangeSpeed | present | ✅ | niche |

### scene.* / camera

| CE | Ours | Status | Sev. |
|---|---|---|---|
| Scene (`add`/`play`/`wait`/`remove`) | `scene::Scene` | ✅ | core |
| Scene positioning shortcuts (`shift`/`scale`/`move_to`/`to_edge`/`next_to`) | present on `Scene` | ✅ | common |
| sections (`next_section`) | present | ✅ | common |
| MovingCameraScene | camera frame animatable (`camera_frame`, `CameraMove`) | ✅ | common |
| ThreeDScene (`set_camera_orientation`, `move_camera`, ambient rotation) | `Scene::set_camera_orientation`/`rotate_camera`/`camera.rotate_ambient` | ✅ | common |
| ZoomedScene | — | ⬜ | common |
| SpecialThreeDScene | — | ⬜ | niche |
| VectorScene, LinearTransformationScene | `vector_space` helpers + `LinearTransformationScene` | ✅ | common |
| `interactive_embed` / `embed` | — | ⬜ | niche |
| `add_sound` | `Scene::add_sound` / `add_sound_at` (ffmpeg mux) | ✅ | niche |
| ThreeDCamera phi/theta/gamma/focal_distance | `ThreeDParams` (all four) | ✅ | common |
| MultiCamera / zoomed display / MappingCamera | — | ⬜ | niche |

### utils.*

| CE | Ours | Status | Sev. |
|---|---|---|---|
| bezier, rate_functions, space_ops | `manim-math` | ✅ | core |
| color (+ full named catalog) | `manim-color` (`_A`…`_E` families) | ✅ | core |
| paths (straight/arc/spiral path funcs) | `manim-math::paths` + `animations::paths` | ✅ | common |
| config | `manim-core::config` (`Config::low/medium/high`) | ✅ | common |
| tex_templates | typst mapping (LaTeX subset) | 🟨 | common |
| sounds | — | ⬜ | niche |
| images / hashing / caching / ipython | n/a (Python-specific) | — | — |

---

## 2. Top-15 gaps (ranked by severity × effort)

Ranked for maximum real-world parity per unit of work — highest-leverage first.

1. **CurvedArrow / CurvedDoubleArrow** — curved arrow variants of `ArcBetweenPoints` with a tip. *(common, small; manim-core geometry/arc.)* Also fixes a parity-map error.
2. **MathTex substring coloring** — `set_color_by_tex` / color map + `index_labels`; needs span→glyph tracking (the isolation the translator already loses). *(core, medium; manim-text.)*
3. **ArrowTip variants** — StealthTip + circle/square + open (unfilled) tips, selectable on `Arrow`/`Line`. *(common, medium; manim-core geometry/line.)*
4. **`Text` gradient builder** — a `Text::with_gradient` that applies the (already-implemented) family gradient at add-time. *(common, small; manim-text.)*
5. **Unwrite / AddTextLetterByLetter / RemoveTextLetterByLetter** — the text-reveal animation set. *(common, small; manim-core animations + manim-text.)*
6. **`plot_implicit_curve` / ImplicitFunction** — marching-squares implicit plot. *(common, medium; manim-core graphing.)*
7. **MoveToTarget / `generate_target`** — the `mob.target = mob.copy()` → `MoveToTarget` workflow; pairs with the existing `r#become`. *(common, medium; manim-core animations.)*
8. **ZoomedScene** — inset zoomed camera display. *(common, medium-high; manim-core scene + manim-render multi-viewport.)*
9. **non-vectorized `Group`** — grouping mixed mobjects (e.g. ImageMobject + VMobject) that `VGroup` can't hold. *(common, small; manim-core.)*
10. **Code mobject** — syntax-highlighted code blocks (syntect). *(common for CS content, high; manim-text.)*
11. **Animated `StreamLines`** — `start_animation` / flowing dots along the field. *(common, medium; manim-core vector_field.)*
12. **Boolean Bézier smoothness** — keep curves through boolean ops instead of polyline flattening. *(common, high; manim-core boolean.)*
13. **AnnotationDot + LabeledLine / LabeledArrow** — small labeled-geometry conveniences. *(niche, small; manim-core geometry + manim-text.)*
14. ~~**`add_sound`**~~ — ✅ done (FE-121): timeline sound cues + ffmpeg audio mux in `render_to_mp4`.
15. **ArcPolygon / ArcPolygonFromArcs** — polygons with arc edges. *(niche, medium; manim-core geometry.)*

---

## 3. Corrections to existing docs

Found by this audit; **not edited** — listed for routing.

1. **parity map** (`10-parity-map.md`, geometry table) claims
   `CurvedArrow, CurvedDoubleArrow … ✅`. They are **absent** (no such types).
   → change to ⬜, or drop them from the "done" row.
2. **migration guide** (`migration-guide.md`, naming-deltas table) says
   `mob.become(other)` → *"no same-named method"*. In fact **`MobjectExt::r#become(&other)`
   exists** (mobject.rs:1173). → correct the row to
   `scene[m].r#become(&other)`.
3. **parity map** scene row says `ThreeDScene 🟨 (geometry ready; awaits FE-107
   camera)`. FE-107 has landed — `Scene::set_camera_orientation` / `rotate_camera`
   and the full `ThreeDParams` (phi/theta/gamma/focal_distance) exist. → upgrade
   ThreeDScene toward ✅ (pending golden verification).
4. **migration guide** "Not yet ported" omits that the **Scene positioning
   shortcuts** (`scene.shift/scale/move_to/to_edge/next_to`) now exist — wart 1
   is effectively resolved. → the guide's post-add-positioning "wart" note is now
   stale; positioning can use `scene.shift(id, ..)` directly.
5. **parity map** lists `TangentLine pending` / `RegularPolygram pending` in a
   sub-note — both are **done**. (The main table is correct; a parenthetical is
   stale.)

---

## 4. Counts

Tallied across the module tables above (CE entries; grouped rows counted once):

| Status | Count |
|---|---|
| ✅ full | 58 |
| 🟨 partial | 13 |
| ⬜ absent | 22 |
| ❓ unsure | 0 |

Roughly **≈80% of surveyed CE surface is ✅ or 🟨**, with the ⬜ set concentrated
in niche geometry (ArcPolygon, point clouds), specialized animations
(Clockwise/Complex/Broadcast), and a handful of common-but-unbuilt features
(CurvedArrow, ArrowTip variants, implicit plots, Code, ZoomedScene, text-reveal
animations, MathTex coloring).

---

## 5. What surprised me

- **`r#become` already exists** — the migration guide (which I wrote) wrongly said
  it didn't. Good example of why this audit reads exports, not docs.
- **CurvedArrow/CurvedDoubleArrow are missing** despite the parity map marking
  them done — the single most concrete parity-map error, and an easy fix.
- **The 3D camera is genuinely complete** — phi/theta/gamma **and**
  focal_distance perspective, plus `rotate_ambient`. ThreeDScene parity is much
  closer than the parity map implies now that FE-107 landed.
- **Scene ergonomic shortcuts already landed** (render-agent's #32) —
  `scene.shift/scale/move_to/to_edge/next_to/set_fill`, so the biggest DX wart
  from the gallery is already resolved; the migration guide hasn't caught up.
- **MobjectExt is broader than CE in places** — `align_on_frame`, `critical_point`,
  `get_corner`, `stretch`, `flip_about`, `set_points_smoothly` are all present;
  positioning/alignment parity is essentially complete.
- **The real parity gap is text-semantic, not geometric.** Geometry/animation
  coverage is high; the felt gaps for a real user are MathTex substring coloring,
  Code, and the text-reveal animation family — i.e. *text*, not shapes.
