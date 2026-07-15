//! Glyph-outline extraction: `ttf-parser` outline callbacks → cubic Bézier
//! [`SubPath`]s, with quadratics elevated to cubics.

use manim_math::bezier::CubicBezier;
use manim_math::path::SubPath;
use manim_math::Point;

/// A [`ttf_parser::OutlineBuilder`] that accumulates cubic subpaths, mapping each
/// on-curve/off-curve font-unit coordinate through a placement closure.
pub(crate) struct GlyphOutline<F: Fn(f32, f32) -> Point> {
    place: F,
    subpaths: Vec<SubPath>,
    current: Vec<CubicBezier>,
    start: Point,
    pen: Point,
}

impl<F: Fn(f32, f32) -> Point> GlyphOutline<F> {
    /// A builder that places font-unit coordinates with `place`.
    pub(crate) fn new(place: F) -> Self {
        Self {
            place,
            subpaths: Vec::new(),
            current: Vec::new(),
            start: Point::ZERO,
            pen: Point::ZERO,
        }
    }

    /// The finished subpaths (closes any dangling contour).
    pub(crate) fn finish(mut self) -> Vec<SubPath> {
        self.flush(true);
        self.subpaths
    }

    /// Pushes the current contour (marking it closed if `closed`).
    fn flush(&mut self, closed: bool) {
        if !self.current.is_empty() {
            self.subpaths.push(SubPath {
                curves: std::mem::take(&mut self.current),
                closed,
            });
        }
    }
}

impl<F: Fn(f32, f32) -> Point> ttf_parser::OutlineBuilder for GlyphOutline<F> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush(true);
        let p = (self.place)(x, y);
        self.start = p;
        self.pen = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = (self.place)(x, y);
        self.current.push(CubicBezier::line(self.pen, p));
        self.pen = p;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        // Elevate the quadratic (P0, Q, P2) to a cubic.
        let q = (self.place)(x1, y1);
        let p2 = (self.place)(x, y);
        let p0 = self.pen;
        let c1 = p0 + (q - p0) * (2.0 / 3.0);
        let c2 = p2 + (q - p2) * (2.0 / 3.0);
        self.current.push(CubicBezier::new(p0, c1, c2, p2));
        self.pen = p2;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let c1 = (self.place)(x1, y1);
        let c2 = (self.place)(x2, y2);
        let p2 = (self.place)(x, y);
        self.current.push(CubicBezier::new(self.pen, c1, c2, p2));
        self.pen = p2;
    }

    fn close(&mut self) {
        self.flush(true);
        self.pen = self.start;
    }
}
