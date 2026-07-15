//! [`NumberLine`]: a 1-D axis with ticks and an optional arrow tip.

use manim_color::WHITE;
use manim_math::path::{Path, SubPath};
use manim_math::{Point, DOWN, ORIGIN, RIGHT, UP};

use crate::impl_mobject;
use crate::mobject::MobjectData;
use crate::style::Style;

/// manim CE's default tick length (scene units).
pub const DEFAULT_TICK_SIZE: f32 = 0.1;

/// A number line: a straight axis from `x_min` to `x_max`, centered on the
/// origin, with evenly spaced ticks and an optional arrow tip. Port of manim
/// CE's `NumberLine`.
///
/// Numeric tick labels need `DecimalNumber` (M4); this builds tick geometry and
/// [label attachment points](Self::number_label_point) but defers the text.
///
/// ```
/// use manim_core::graphing::NumberLine;
/// use manim_math::RIGHT;
/// let nl = NumberLine::new(-3.0, 3.0, 1.0).with_length(6.0);
/// // Unit size is length / span = 6 / 6 = 1.
/// assert!((nl.number_to_point(1.0) - RIGHT).length() < 1e-5);
/// assert!((nl.point_to_number(2.0 * RIGHT) - 2.0).abs() < 1e-5);
/// ```
#[derive(Clone)]
pub struct NumberLine {
    data: MobjectData,
    x_min: f32,
    x_max: f32,
    x_step: f32,
    unit_size: f32,
    include_ticks: bool,
    tick_size: f32,
    include_tip: bool,
    tip_length: f32,
}
impl_mobject!(NumberLine);

impl NumberLine {
    /// A number line over `[x_min, x_max]` with tick spacing `x_step`, unit size
    /// `1.0`, ticks on, no tip.
    pub fn new(x_min: f32, x_max: f32, x_step: f32) -> Self {
        let mut nl = Self {
            data: MobjectData::new(Path::default(), Style::stroked(WHITE)),
            x_min,
            x_max,
            x_step,
            unit_size: 1.0,
            include_ticks: true,
            tick_size: DEFAULT_TICK_SIZE,
            include_tip: false,
            tip_length: 0.25,
        };
        nl.rebuild();
        nl
    }

    /// manim CE's `UnitInterval`: a number line over `[0, 1]` stepping by `0.1`.
    ///
    /// ```
    /// use manim_core::graphing::NumberLine;
    /// let ui = NumberLine::unit_interval();
    /// assert_eq!(ui.get_tick_range().len(), 11); // 0.0 … 1.0
    /// ```
    pub fn unit_interval() -> Self {
        Self::new(0.0, 1.0, 0.1)
    }

    /// Sets the on-screen length, deriving the unit size from the span.
    pub fn with_length(mut self, length: f32) -> Self {
        let span = self.x_max - self.x_min;
        if span.abs() > 1e-9 {
            self.unit_size = length / span;
        }
        self.rebuild();
        self
    }

    /// Sets the scene units per data unit directly.
    pub fn with_unit_size(mut self, unit_size: f32) -> Self {
        self.unit_size = unit_size;
        self.rebuild();
        self
    }

    /// Adds an arrow tip at the positive end.
    pub fn with_tip(mut self) -> Self {
        self.include_tip = true;
        self.rebuild();
        self
    }

    /// Removes the ticks.
    pub fn without_ticks(mut self) -> Self {
        self.include_ticks = false;
        self.rebuild();
        self
    }

    /// Sets the tick length in scene units.
    pub fn with_tick_size(mut self, tick_size: f32) -> Self {
        self.tick_size = tick_size;
        self.rebuild();
        self
    }

    /// The data value at the center of the range (which maps to the origin).
    pub fn x_center(&self) -> f32 {
        0.5 * (self.x_min + self.x_max)
    }

    /// Scene units per data unit.
    pub fn get_unit_size(&self) -> f32 {
        self.unit_size
    }

    /// The scene point for data value `x` (manim's `number_to_point` / `n2p`).
    pub fn number_to_point(&self, x: f32) -> Point {
        ORIGIN + RIGHT * ((x - self.x_center()) * self.unit_size)
    }

    /// The data value at scene point `p` (manim's `point_to_number` / `p2n`);
    /// the inverse of [`number_to_point`](Self::number_to_point) along the axis.
    pub fn point_to_number(&self, p: Point) -> f32 {
        self.x_center() + p.x / self.unit_size
    }

    /// The tick values from `x_min` to `x_max`, aligned to multiples of the step.
    pub fn get_tick_range(&self) -> Vec<f32> {
        let mut out = Vec::new();
        if self.x_step <= 0.0 {
            return out;
        }
        let mut i = (self.x_min / self.x_step).ceil() as i64;
        loop {
            let v = i as f32 * self.x_step;
            if v > self.x_max + 1e-6 {
                break;
            }
            if v >= self.x_min - 1e-6 {
                out.push(v);
            }
            i += 1;
        }
        out
    }

    /// The tick mark segments as `(bottom, top)` scene-point pairs.
    pub fn get_tick_marks(&self) -> Vec<(Point, Point)> {
        let h = self.tick_size / 2.0;
        self.get_tick_range()
            .iter()
            .map(|&x| {
                let p = self.number_to_point(x);
                (p - UP * h, p + UP * h)
            })
            .collect()
    }

    /// The anchor point for a numeric label at `x`, just below the axis.
    ///
    /// Text rendering is deferred to M4 (`DecimalNumber`); this is where such a
    /// label would attach.
    pub fn number_label_point(&self, x: f32) -> Point {
        self.number_to_point(x) + DOWN * (self.tick_size / 2.0 + 0.2)
    }

    /// Rebuilds the path (main line + ticks + optional tip) from the config.
    fn rebuild(&mut self) {
        let mut subpaths = Vec::new();
        let a = self.number_to_point(self.x_min);
        let b = self.number_to_point(self.x_max);
        subpaths.push(SubPath::from_corners(&[a, b]));

        if self.include_ticks {
            for (lo, hi) in self.get_tick_marks() {
                subpaths.push(SubPath::from_corners(&[lo, hi]));
            }
        }

        if self.include_tip {
            let tl = self.tip_length;
            let base = b - RIGHT * tl;
            let hw = tl / 2.0;
            let mut tip = SubPath::from_corners(&[b, base + UP * hw, base - UP * hw]);
            tip.closed = true;
            subpaths.push(tip);
        }

        self.data.path = Path { subpaths };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::Mobject;

    #[test]
    fn number_to_point_is_linear_and_invertible() {
        let nl = NumberLine::new(-4.0, 4.0, 1.0).with_unit_size(0.5);
        // Linear: equal steps in x map to equal steps in point.
        let d0 = nl.number_to_point(1.0) - nl.number_to_point(0.0);
        let d1 = nl.number_to_point(3.0) - nl.number_to_point(2.0);
        assert!((d0 - d1).length() < 1e-6);
        // Center of range maps to the origin.
        assert!(nl.number_to_point(0.0).length() < 1e-6);
        // Round trip.
        for x in [-4.0, -1.5, 0.0, 2.25, 4.0] {
            assert!((nl.point_to_number(nl.number_to_point(x)) - x).abs() < 1e-5);
        }
    }

    #[test]
    fn ticks_and_labels() {
        let nl = NumberLine::new(0.0, 5.0, 1.0);
        assert_eq!(nl.get_tick_range(), vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(nl.get_tick_marks().len(), 6);
        // Label anchor sits below its tick point.
        let lp = nl.number_label_point(2.0);
        assert!(lp.y < nl.number_to_point(2.0).y);
    }

    #[test]
    fn tip_adds_a_closed_subpath() {
        let plain = NumberLine::new(-1.0, 1.0, 1.0).without_ticks();
        let tipped = NumberLine::new(-1.0, 1.0, 1.0).without_ticks().with_tip();
        assert!(tipped.data().path.subpaths.len() > plain.data().path.subpaths.len());
        assert!(tipped.data().path.subpaths.last().unwrap().closed);
    }
}
