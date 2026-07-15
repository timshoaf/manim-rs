//! Direction and frame constants matching manim CE's `constants`.

use crate::Point;

/// The center of the scene, `(0, 0, 0)`.
///
/// ```
/// use manim_math::ORIGIN;
/// assert_eq!(ORIGIN.length(), 0.0);
/// ```
pub const ORIGIN: Point = Point::ZERO;

/// Unit vector pointing up (`+y`).
pub const UP: Point = Point::new(0.0, 1.0, 0.0);
/// Unit vector pointing down (`-y`).
pub const DOWN: Point = Point::new(0.0, -1.0, 0.0);
/// Unit vector pointing right (`+x`).
pub const RIGHT: Point = Point::new(1.0, 0.0, 0.0);
/// Unit vector pointing left (`-x`).
pub const LEFT: Point = Point::new(-1.0, 0.0, 0.0);
/// Unit vector pointing out of the screen toward the viewer (`+z`).
pub const OUT: Point = Point::new(0.0, 0.0, 1.0);
/// Unit vector pointing into the screen away from the viewer (`-z`).
pub const IN: Point = Point::new(0.0, 0.0, -1.0);

/// Diagonal direction up-left, `UP + LEFT`.
pub const UL: Point = Point::new(-1.0, 1.0, 0.0);
/// Diagonal direction up-right, `UP + RIGHT`.
pub const UR: Point = Point::new(1.0, 1.0, 0.0);
/// Diagonal direction down-left, `DOWN + LEFT`.
pub const DL: Point = Point::new(-1.0, -1.0, 0.0);
/// Diagonal direction down-right, `DOWN + RIGHT`.
pub const DR: Point = Point::new(1.0, -1.0, 0.0);

/// π, as an `f32` (manim exposes `PI` directly).
pub const PI: f32 = std::f32::consts::PI;
/// τ = 2π, a full turn (manim exposes `TAU` directly).
pub const TAU: f32 = std::f32::consts::TAU;
/// One degree in radians; write `90.0 * DEGREES` like in manim.
pub const DEGREES: f32 = TAU / 360.0;

/// Default frame height in scene units (manim CE default).
pub const FRAME_HEIGHT: f32 = 8.0;
/// Default frame width in scene units (16:9 of [`FRAME_HEIGHT`]).
pub const FRAME_WIDTH: f32 = FRAME_HEIGHT * 16.0 / 9.0;

/// Default small buffer distance used by positioning methods.
pub const SMALL_BUFF: f32 = 0.1;
/// Default medium-small buffer distance (manim's `MED_SMALL_BUFF`), the
/// default for `next_to`.
pub const MED_SMALL_BUFF: f32 = 0.25;
/// Default medium-large buffer distance.
pub const MED_LARGE_BUFF: f32 = 0.5;
/// Default large buffer distance, the default for `to_edge`.
pub const LARGE_BUFF: f32 = 1.0;
