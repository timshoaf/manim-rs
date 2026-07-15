//! [`Brace`]: a parametric curly brace spanning an extent.

use manim_color::WHITE;
use manim_math::bezier::CubicBezier;
use manim_math::path::{Path, SubPath};
use manim_math::space_ops::normalize_or_zero;
use manim_math::{Point, DOWN};

use crate::impl_mobject;
use crate::mobject::MobjectData;
use crate::style::Style;

/// Default brace protrusion (depth) in scene units.
pub const DEFAULT_BRACE_DEPTH: f32 = 0.25;

/// A curly brace spanning `start → end`, bulging toward `direction`. Port of
/// manim CE's `Brace`.
///
/// The shape is two mirrored cubic hooks meeting at a central tip (a parametric
/// approximation of CE's filled `}` glyph — ours is a stroked curve). The
/// numeric/text label a `BraceLabel` would attach is left to a later text pass;
/// [`brace_label_point`](Self::brace_label_point) gives its anchor.
///
/// ```
/// use manim_core::geometry::Brace;
/// use manim_math::{Point, DOWN, RIGHT};
/// // Brace under a segment from the origin to (4, 0), pointing down.
/// let b = Brace::new(Point::ZERO, 4.0 * RIGHT, DOWN);
/// let tip = b.get_tip();
/// // The tip is centered under the span and offset downward.
/// assert!((tip.x - 2.0).abs() < 1e-5);
/// assert!(tip.y < 0.0);
/// ```
#[derive(Clone)]
pub struct Brace {
    data: MobjectData,
    start: Point,
    end: Point,
    direction: Point,
    depth: f32,
}
impl_mobject!(Brace);

impl Brace {
    /// A brace spanning `start → end` that bulges toward `direction` (which need
    /// not be normalized), with the default depth.
    pub fn new(start: Point, end: Point, direction: Point) -> Self {
        Self::with_depth(start, end, direction, DEFAULT_BRACE_DEPTH)
    }

    /// A brace spanning the horizontal extent `[x_min, x_max]` at height `y`,
    /// pointing [`DOWN`](manim_math::DOWN) — the common "brace under a row" case.
    pub fn horizontal(x_min: f32, x_max: f32, y: f32) -> Self {
        Self::new(Point::new(x_min, y, 0.0), Point::new(x_max, y, 0.0), DOWN)
    }

    /// A brace with an explicit protrusion `depth`.
    pub fn with_depth(start: Point, end: Point, direction: Point, depth: f32) -> Self {
        let mut b = Self {
            data: MobjectData::new(Path::default(), Style::stroked(WHITE)),
            start,
            end,
            direction: normalize_or_zero(direction),
            depth,
        };
        b.rebuild();
        b
    }

    /// The central tip point (where the brace points).
    pub fn get_tip(&self) -> Point {
        let mid = (self.start + self.end) * 0.5;
        mid + self.direction * self.depth
    }

    /// The anchor point for a label, `buff` beyond the tip along the brace
    /// direction. Text rendering (`BraceLabel`) is a later pass.
    ///
    /// ```
    /// use manim_core::geometry::Brace;
    /// use manim_math::{Point, DOWN, RIGHT};
    /// let b = Brace::new(Point::ZERO, 2.0 * RIGHT, DOWN);
    /// let label = b.brace_label_point(0.3);
    /// assert!(label.y < b.get_tip().y); // further past the tip
    /// ```
    pub fn brace_label_point(&self, buff: f32) -> Point {
        self.get_tip() + self.direction * buff
    }

    /// Rebuilds the brace path from its extent, direction, and depth.
    fn rebuild(&mut self) {
        let along = self.end - self.start;
        let len = along.length();
        if len < 1e-9 {
            self.data.path = Path::default();
            return;
        }
        let u = along / len; // unit along the span
        let d = self.direction; // unit protrusion
                                // Local (lx along span, ly protrusion) → world.
        let w = |lx: f32, ly: f32| self.start + u * lx + d * ly;
        let depth = self.depth;
        let half = len * 0.5;

        // Left hook: left end → central tip.
        let left = CubicBezier::new(w(0.0, 0.0), w(0.0, depth), w(half, 0.0), w(half, depth));
        // Right hook: central tip → right end (mirror of the left, reversed).
        let right = CubicBezier::new(w(half, depth), w(half, 0.0), w(len, depth), w(len, 0.0));

        self.data.path = Path {
            subpaths: vec![SubPath {
                curves: vec![left, right],
                closed: false,
            }],
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::{Mobject, MobjectExt};
    use manim_math::{RIGHT, UP};

    #[test]
    fn tip_is_centered_and_on_the_direction_side() {
        let b = Brace::new(Point::ZERO, 4.0 * RIGHT, DOWN);
        let tip = b.get_tip();
        assert!((tip.x - 2.0).abs() < 1e-5); // centered
        assert!(tip.y < 0.0); // below (DOWN)
    }

    #[test]
    fn direction_flips_the_tip() {
        let down = Brace::new(Point::ZERO, 2.0 * RIGHT, DOWN);
        let up = Brace::new(Point::ZERO, 2.0 * RIGHT, UP);
        assert!(down.get_tip().y < 0.0);
        assert!(up.get_tip().y > 0.0);
    }

    #[test]
    fn brace_spans_the_extent() {
        let b = Brace::new(Point::ZERO, 4.0 * RIGHT, DOWN);
        let bb = b.bounding_box();
        // Spans the full horizontal extent of the target.
        assert!((bb.min.x - 0.0).abs() < 1e-4);
        assert!((bb.max.x - 4.0).abs() < 1e-4);
    }

    #[test]
    fn degenerate_span_is_empty() {
        let b = Brace::new(Point::ZERO, Point::ZERO, DOWN);
        assert!(b.data().path.subpaths.is_empty());
    }
}
