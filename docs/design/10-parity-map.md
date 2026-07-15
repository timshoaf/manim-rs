# Manim CE Parity Map

Target: manim CE v0.19 public API. Status: ⬜ planned · 🟨 partial · ✅ done.
This file is the source of truth for the Linear backlog; update as work lands.

## mobject

### geometry (M1)
| CE | Rust | Status |
|---|---|---|
| Arc, ArcBetweenPoints, CurvedArrow, CurvedDoubleArrow | `geometry::arc` | ⬜ |
| Circle, Dot, AnnotationDot, LabeledDot, Ellipse, Annulus, AnnularSector, Sector | `geometry::arc` | ⬜ |
| Line, DashedLine, TangentLine, Elbow, Arrow, Vector, DoubleArrow | `geometry::line` | ⬜ |
| Angle, RightAngle | `geometry::line` | ⬜ |
| Polygram, Polygon, RegularPolygram, RegularPolygon, Star, Triangle | `geometry::polygram` | ⬜ |
| Rectangle, Square, RoundedRectangle, Cutout | `geometry::polygram` | ⬜ |
| ArrowTip variants (triangle/square/circle/stealth, filled/open) | `geometry::tips` | ⬜ |
| ArcPolygon, ArcPolygonFromArcs | `geometry::arc` | ⬜ |
| Union, Difference, Intersection, Exclusion (boolean_ops) | `geometry::boolean_ops` | ⬜ (post-v1, via lyon/kurbo boolean) |

### types (M1)
VMobject, VGroup, VDict, VectorizedPoint, CurvesAsSubmobjects, DashedVMobject,
Group, Mobject, PMobject/Point (point clouds), ImageMobject — ⬜

### text (M4)
Text, Paragraph, MarkupText, Tex, MathTex, SingleStringMathTex, BulletedList,
Title, DecimalNumber, Integer, Variable, Code (syntect highlighting) — ⬜

### svg (M4)
SVGMobject, Brace, BraceLabel, BraceText, BraceBetweenPoints — ⬜

### graphing (M5)
NumberLine, UnitInterval, Axes, ThreeDAxes, NumberPlane, ComplexPlane,
PolarPlane, CoordinateSystem methods (plot, plot_parametric_curve,
plot_implicit_curve, get_graph_label, get_riemann_rectangles, get_area,
get_secant_slope_group, coords_to_point, point_to_coords, add_coordinates),
ParametricFunction, FunctionGraph, ImplicitFunction, BarChart — ⬜

### three_d (M6)
ThreeDVMobject, Surface, Sphere, Dot3D, Cube, Prism, Cone, Cylinder, Line3D,
Arrow3D, Torus — ⬜

### others (M5/M6)
Matrix, DecimalMatrix, IntegerMatrix, MobjectMatrix, Table (+variants),
Graph/DiGraph (+layouts), VectorField, ArrowVectorField, StreamLines,
ValueTracker, ComplexValueTracker, TracedPath — ⬜ (ValueTracker in M2)

## animation
See [04-animation-system.md](04-animation-system.md) for the full catalog —
creation/transform/fading/indication/growing/movement/rotation/composition/
numbers/updaters/changing/speedmodifier/specialized/transform_matching_parts.
M2 covers the core (Create, Write-less creation set, Transform family, Fade,
Rotate, composition, .animate()); the long tail lands in M3.

## scene (M3/M6)
Scene, MovingCameraScene (M3), ZoomedScene (M6), ThreeDScene (M6),
VectorScene, LinearTransformationScene (M6), sections (M3).

## camera (M3, 3D in M6)
Camera frame center/width/zoom/rotation animatable; background color/opacity;
multi-camera (zoomed display); ThreeDCamera phi/theta/gamma/focal_distance.

## utils
| CE module | Rust home | Milestone |
|---|---|---|
| bezier | manim-math::bezier | M0 ✅ |
| rate_functions | manim-math::rate_functions | M0 ✅ |
| space_ops | manim-math::space_ops | M0 ✅ |
| color | manim-color | M0 ✅ (XKCD/X11 feature catalogs pending) |
| paths (straight/arc/spiral path funcs for transforms) | manim-math::paths | M2 |
| config | manim-core::config | M1 |
| images/ipython/hashing/caching | n/a (Python-specific) | — |
| sounds (`Scene.add_sound`) | manim-core (native feature) | M6 |
| tex/tex_templates | manim-text::typst mapping | M4 |

## Explicit deferrals (documented, issue-tracked)
- OpenGL-renderer-specific API (CE's experimental opengl namespace): n/a — our
  renderer IS the gpu renderer.
- `manim cfg` / plugin system: replaced by Cargo features & Rust plugins (traits).
- IPython/Jupyter integration: out of scope; wasm/Dioxus embedding replaces it.
