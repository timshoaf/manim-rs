# manim-math & manim-color

## manim-math

### Points and coordinates

manim CE represents points as numpy `[x, y, z]` in "scene units" (frame height
8.0, origin center, +y up). We keep the same coordinate convention with
`glam::Vec3` (`f32`).

```rust
pub type Point = glam::Vec3;   // scene-space point
pub const ORIGIN: Point; pub const UP/DOWN/LEFT/RIGHT/IN/OUT: Point;
pub const UL/UR/DL/DR: Point;  // corner combos, as in manim
```

f32 vs f64: manim uses f64 via numpy. We choose f32 — GPU-native, 2× memory
bandwidth, adequate for animation (positions span ~±10 units; f32 gives ~1e-6
relative precision). Accumulation-sensitive code (arc length tables, boolean
ops) uses f64 internally where needed.

### Bezier (`bezier.rs`)

The heart of manim. All parity functions:

- `bezier(points) -> impl Fn(f32) -> Point` — general de Casteljau evaluation
- `CubicBezier` type: `eval`, `split(t)`, `subdivide(n)`, `partial(a, b)`
  (manim's `partial_bezier_points`), derivative, arc-length (adaptive Gauss–Legendre)
- `interpolate`, `inverse_interpolate`, `match_interpolate`
- `integer_interpolate` (for submobject family indexing)
- `get_smooth_cubic_bezier_handle_points` — smooth spline through anchors
  (manim's smoothing for `set_points_smoothly`)
- `is_closed`, `proportions_along_bezier_curve_for_point`, point-curve projection

### Paths (`path.rs`)

`Path` = `Vec<SubPath>`, `SubPath` = anchors+handles in manim's layout
(consecutive cubic segments sharing anchors) + `closed` flag. Operations:
`point_from_proportion`, `get_subcurve`, insertion of curves for
`align_points` (needed by `Transform`), winding/orientation, bounding box.

### Space ops (`space_ops.rs`)

Parity with `manim.utils.space_ops`: `rotation_matrix`, `rotate_vector`,
`angle_of_vector`, `angle_between_vectors`, `normalize`, `cross2d`,
`find_intersection`, `line_intersection`, `midpoint`, `perpendicular_bisector`,
`compass_directions`, `regular_vertices`, quaternion helpers, 2D↔3D shuffles.

### Rate functions (`rate_functions.rs`)

All manim CE rate functions as `fn(f32) -> f32` + a `RateFn` enum (so they're
data, serializable, and pattern-matchable) with `Custom(Arc<dyn Fn>)` escape hatch:
`linear`, `smooth`, `smoothstep`/`smootherstep`/`smoothererstep`, `rush_into`,
`rush_from`, `slow_into`, `double_smooth`, `there_and_back`,
`there_and_back_with_pause`, `running_start`, `not_quite_there`, `wiggle`,
`squish_rate_func`, `lingering`, `exponential_decay`, plus the full easing suite
(`ease_in_sine` … `ease_in_out_bounce`, all 30).

Property tests: every rate fn maps [0,1]→ℝ with f(0)=0, f(1)=1 (except
`there_and_back` family and `wiggle`, asserted separately).

## manim-color

`Color` is linear-RGBA f32 internally (GPU-ready), with sRGB conversions at the
edges:

```rust
pub struct Color { r: f32, g: f32, b: f32, a: f32 } // linear light
impl Color {
    pub fn from_hex(s: &str) -> Result<Self, ColorError>;  // "#RRGGBB[AA]", "#RGB"
    pub fn to_hex(&self) -> String;
    pub fn from_rgb/rgba/hsv/hsl(..);
    pub fn lighter/darker(&self, amount: f32) -> Self;     // manim's lighter/darker
    pub fn interpolate(&self, other: &Self, t: f32) -> Self;
    pub fn invert(&self), opacity(&self, a: f32), ...
}
```

- Full manim CE named-color catalog as consts: `BLUE_A..E`, `TEAL_A..E`,
  `GREEN_A..E`, `YELLOW_A..E`, `GOLD_A..E`, `RED_A..E`, `MAROON_A..E`,
  `PURPLE_A..E`, `GRAY_A..E` (+ `GREY` aliases), `WHITE`, `BLACK`, `PINK`,
  `ORANGE`, `PURE_RED/GREEN/BLUE`, `LIGHT_BROWN`, `DARK_BROWN`, XKCD/X11/DVIPS
  catalogs behind feature flags (`colors-xkcd`, `colors-x11`).
- `color_gradient(&[Color], n)`, `average_color`, `random_bright_color`,
  `random_color` (seeded RNG passed in — determinism for tests).
- Interpolation happens in linear space by default; an `interpolate_srgb`
  variant matches manim's visual results where golden parity matters.

Everything here is `no_std`-adjacent (std but zero heavy deps) and fully
unit/property tested (hex round-trips, HSV↔RGB round-trips, gradient endpoints).
