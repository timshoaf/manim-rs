//! Math foundations for `manim_rust`.
//!
//! Ports the numerical heart of manim CE's `utils`: bezier curves
//! ([`bezier`]), vectorized paths ([`path`]), spatial operations
//! ([`space_ops`]), and the full rate-function catalog
//! ([`rate_functions`]).
//!
//! Points live in manim's scene space: origin at frame center, `+y` up,
//! frame height 8.0 scene units. See [`Point`].

pub mod bezier;
pub mod constants;
pub mod path;
pub mod rate_functions;
pub mod space_ops;

pub use constants::*;

/// A point (or direction) in 3D scene space.
///
/// manim uses 3D points everywhere, even for 2D scenes (`z = 0`).
///
/// ```
/// use manim_math::{Point, ORIGIN, RIGHT, UP};
/// let p: Point = ORIGIN + 2.0 * RIGHT + UP;
/// assert_eq!(p, Point::new(2.0, 1.0, 0.0));
/// ```
pub type Point = glam::Vec3;
