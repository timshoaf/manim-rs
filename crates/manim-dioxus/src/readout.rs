//! Live readouts for interactive figures (FE-145): a coordinate label that
//! follows a handle, a tracker-bound decimal, and an angle marker between three
//! points.
//!
//! # Why the text mobject is a parameter
//!
//! Every readout takes a `build: impl Fn(&str) -> M` closure instead of naming
//! a text type. Typesetting lives in `manim-text` (typst + cosmic-text, a heavy
//! dependency); making the web component crate depend on it would put a
//! typesetter in every figure's wasm bundle, including the many that never show
//! a number. The caller passes `Text::new` (or `MathTex::new`, or anything else
//! that is a mobject) and the kit owns only *placement and formatting* — which
//! is the part with the fiddly rules, and the part that is unit-tested here.
//!
//! # One mobject, one arena slot
//!
//! The builder's mobject is added with [`SceneState::add`] and, on every later
//! change, *replaced in place* with its geometry generation bumped. That is not
//! an optimization but a correctness requirement: the renderer's tessellation
//! cache is keyed on `(arena, source, generation)`, and removing a mobject frees
//! its arena slot for reuse — a fresh mobject landing in that slot with
//! generation `0` would hit the departed one's cache entry and draw its glyphs.
//!
//! One consequence for the caller: pass a builder that returns the mobject
//! (`|s| Text::new(s)`), not one that adds it (`Text::add_to`, which splits a
//! string into per-glyph children). A single-mobject `Text` still draws its whole
//! string — the glyph outlines are all in its own path.
//!
//! # Why rebuilds are change-gated
//!
//! Laying out text costs far more than moving a mobject, and a readout's string
//! changes on maybe one frame in ten of a drag. Every readout therefore
//! remembers its last string and rebuilds only when it actually differs; the
//! rest of the time it just moves the existing mobject.

use manim_core::mobject::{AnyId, Mobject, MobjectId};
use manim_core::prelude::Point;
use manim_core::scene_state::SceneState;

/// How a coordinate pair is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoordStyle {
    /// `(1.20, -0.35)`
    #[default]
    Cartesian,
    /// `1.20 − 0.35i` — the natural reading on a complex plane.
    Complex,
    /// `r = 1.25, θ = -0.28` (radians).
    Polar,
}

/// Formatting for a coordinate readout: style, precision, and optional
/// surrounding text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CoordFormat {
    /// Fractional digits (default 2).
    pub decimals: usize,
    /// How the pair is written.
    pub style: CoordStyle,
    /// Text before the value (e.g. `"z₀ = "`).
    pub prefix: String,
    /// Text after the value.
    pub suffix: String,
}

impl CoordFormat {
    /// Two-decimal cartesian, no affixes.
    pub fn new() -> Self {
        Self {
            decimals: 2,
            ..Default::default()
        }
    }

    /// Sets the fractional digit count.
    pub fn decimals(mut self, n: usize) -> Self {
        self.decimals = n;
        self
    }

    /// Sets the pair style.
    pub fn style(mut self, style: CoordStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets a label prefix (e.g. `"z₀ = "`).
    pub fn prefix(mut self, s: impl Into<String>) -> Self {
        self.prefix = s.into();
        self
    }

    /// Sets a suffix (e.g. a unit).
    pub fn suffix(mut self, s: impl Into<String>) -> Self {
        self.suffix = s.into();
        self
    }

    /// Renders `p` per this format.
    pub fn render(&self, p: Point) -> String {
        let d = self.decimals;
        let body = match self.style {
            CoordStyle::Cartesian => {
                format!("({}, {})", format_scalar(p.x, d), format_scalar(p.y, d))
            }
            CoordStyle::Complex => {
                // A signed imaginary part reads as a sum, not as "+ -0.35i".
                let sign = if p.y < 0.0 { "−" } else { "+" };
                format!(
                    "{} {} {}i",
                    format_scalar(p.x, d),
                    sign,
                    format_scalar(p.y.abs(), d)
                )
            }
            CoordStyle::Polar => {
                let r = (p.x * p.x + p.y * p.y).sqrt();
                format!(
                    "r = {}, θ = {}",
                    format_scalar(r, d),
                    format_scalar(p.y.atan2(p.x), d)
                )
            }
        };
        format!("{}{}{}", self.prefix, body, self.suffix)
    }
}

/// Formats a scalar at `decimals` places, normalizing `-0.00` to `0.00`.
///
/// A value drifting a hair below zero otherwise flickers a minus sign on and
/// off under the reader's finger, which looks like a bug in the mathematics.
///
/// ```
/// use manim_dioxus::readout::format_scalar;
/// assert_eq!(format_scalar(-0.001, 2), "0.00");
/// assert_eq!(format_scalar(1.2345, 3), "1.235");
/// ```
pub fn format_scalar(v: f32, decimals: usize) -> String {
    if !v.is_finite() {
        return "—".to_string();
    }
    let s = format!("{v:.decimals$}");
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') {
        s[1..].to_string()
    } else {
        s
    }
}

/// A mobject that follows a point and re-renders when its text changes.
///
/// Generic over the mobject the caller's builder produces, so no text engine is
/// named here (see the [module docs](self)).
#[derive(Debug)]
pub struct Readout<M: Mobject> {
    id: Option<MobjectId<M>>,
    offset: Point,
    last: Option<String>,
}

impl<M: Mobject> Readout<M> {
    /// A readout drawn at `offset` from whatever point it is synced to.
    pub fn new(offset: Point) -> Self {
        Self {
            id: None,
            offset,
            last: None,
        }
    }

    /// The readout mobject, once it has been built.
    pub fn id(&self) -> Option<AnyId> {
        self.id.map(|i| i.erase())
    }

    /// Moves the readout to `anchor + offset` and, if `text` differs from what
    /// is displayed, rebuilds it with `build`. Returns whether it rebuilt.
    ///
    /// The rebuild replaces the mobject's contents in place and bumps its
    /// geometry generation, so the renderer's per-mobject caches invalidate
    /// exactly once — recreating the mobject instead would leak a fresh arena
    /// slot on every changed digit.
    pub fn sync(
        &mut self,
        state: &mut SceneState,
        anchor: Point,
        text: &str,
        build: impl FnOnce(&str) -> M,
    ) -> bool {
        let pos = anchor + self.offset;
        let changed = self.last.as_deref() != Some(text);
        match self.id {
            None => {
                let id = state.add(build(text));
                self.id = Some(id);
                self.last = Some(text.to_string());
                state.move_to(id, pos);
                return true;
            }
            Some(id) if changed => {
                let generation = state.get(id).data().generation;
                let fresh = build(text);
                let slot = state.get_mut(id);
                *slot = fresh;
                slot.data_mut().generation = generation + 1;
                self.last = Some(text.to_string());
                state.move_to(id, pos);
                return true;
            }
            Some(id) => state.move_to(id, pos),
        };
        false
    }

    /// Removes the readout from the scene (e.g. the handle it labels vanished).
    pub fn clear(&mut self, state: &mut SceneState) {
        if let Some(id) = self.id.take() {
            state.remove(id);
        }
        self.last = None;
    }
}

/// A coordinate label that follows a handle (FE-145).
///
/// ```no_run
/// # use manim_dioxus::readout::{CoordinateReadout, CoordStyle};
/// # use manim_core::prelude::{Point, Circle};
/// # let mut state = manim_core::scene_state::SceneState::new();
/// let mut readout = CoordinateReadout::new(Point::new(0.0, 0.4, 0.0))
///     .with_format(|f| f.style(CoordStyle::Complex).prefix("z₀ = "));
/// // In the live updater, each frame — `build` is your text mobject:
/// readout.sync(&mut state, Point::new(1.0, 2.0, 0.0), |_s| Circle::new());
/// ```
#[derive(Debug)]
pub struct CoordinateReadout<M: Mobject> {
    readout: Readout<M>,
    format: CoordFormat,
}

impl<M: Mobject> CoordinateReadout<M> {
    /// A two-decimal cartesian readout `offset` from the point it labels.
    pub fn new(offset: Point) -> Self {
        Self {
            readout: Readout::new(offset),
            format: CoordFormat::new(),
        }
    }

    /// Adjusts the format (builder style).
    pub fn with_format(mut self, f: impl FnOnce(CoordFormat) -> CoordFormat) -> Self {
        self.format = f(self.format);
        self
    }

    /// The text this readout would show for `p`.
    pub fn text(&self, p: Point) -> String {
        self.format.render(p)
    }

    /// The readout mobject, once built.
    pub fn id(&self) -> Option<AnyId> {
        self.readout.id()
    }

    /// Labels `p` (the readout also *sits* relative to `p`). Returns whether the
    /// text changed this frame.
    pub fn sync(
        &mut self,
        state: &mut SceneState,
        p: Point,
        build: impl FnOnce(&str) -> M,
    ) -> bool {
        let text = self.format.render(p);
        self.readout.sync(state, p, &text, build)
    }

    /// Removes it from the scene.
    pub fn clear(&mut self, state: &mut SceneState) {
        self.readout.clear(state);
    }
}

/// A single tracked number rendered at a fixed place (FE-145) — manim's
/// `DecimalNumber` bound to a value tracker, in this crate's builder-closure
/// idiom.
#[derive(Debug)]
pub struct DecimalReadout<M: Mobject> {
    readout: Readout<M>,
    at: Point,
    decimals: usize,
    prefix: String,
    suffix: String,
}

impl<M: Mobject> DecimalReadout<M> {
    /// A two-decimal number drawn at `at`.
    pub fn new(at: Point) -> Self {
        Self {
            readout: Readout::new(Point::ZERO),
            at,
            decimals: 2,
            prefix: String::new(),
            suffix: String::new(),
        }
    }

    /// Sets the fractional digit count.
    pub fn decimals(mut self, n: usize) -> Self {
        self.decimals = n;
        self
    }

    /// Sets text before the number (e.g. `"|f(z)| = "`).
    pub fn prefix(mut self, s: impl Into<String>) -> Self {
        self.prefix = s.into();
        self
    }

    /// Sets text after the number (e.g. `"°"`).
    pub fn suffix(mut self, s: impl Into<String>) -> Self {
        self.suffix = s.into();
        self
    }

    /// The text this readout would show for `v`.
    pub fn text(&self, v: f32) -> String {
        format!(
            "{}{}{}",
            self.prefix,
            format_scalar(v, self.decimals),
            self.suffix
        )
    }

    /// The readout mobject, once built.
    pub fn id(&self) -> Option<AnyId> {
        self.readout.id()
    }

    /// Displays `v`. Returns whether the text changed this frame.
    pub fn sync(&mut self, state: &mut SceneState, v: f32, build: impl FnOnce(&str) -> M) -> bool {
        let text = self.text(v);
        self.readout.sync(state, self.at, &text, build)
    }

    /// Moves the readout.
    pub fn move_to(&mut self, at: Point) {
        self.at = at;
    }

    /// Removes it from the scene.
    pub fn clear(&mut self, state: &mut SceneState) {
        self.readout.clear(state);
    }
}

/// The unsigned angle `∠(a, vertex, b)` in radians, or `0` if either arm is
/// degenerate (two coincident handles must not produce a `NaN` label).
///
/// ```
/// use manim_dioxus::readout::angle_between;
/// use manim_core::prelude::Point;
/// let a = Point::new(1.0, 0.0, 0.0);
/// let b = Point::new(0.0, 1.0, 0.0);
/// assert!((angle_between(a, Point::ZERO, b) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
/// ```
pub fn angle_between(a: Point, vertex: Point, b: Point) -> f32 {
    let u = a - vertex;
    let v = b - vertex;
    let (lu, lv) = (u.length(), v.length());
    if lu <= f32::EPSILON || lv <= f32::EPSILON {
        return 0.0;
    }
    (u.dot(v) / (lu * lv)).clamp(-1.0, 1.0).acos()
}

/// The signed angle from `a` to `b` about `vertex`, in `(-π, π]` (positive =
/// counter-clockwise). Useful when the sign is the point (a phase, a winding).
pub fn signed_angle(a: Point, vertex: Point, b: Point) -> f32 {
    let u = a - vertex;
    let v = b - vertex;
    if u.length() <= f32::EPSILON || v.length() <= f32::EPSILON {
        return 0.0;
    }
    (u.x * v.y - u.y * v.x).atan2(u.x * v.x + u.y * v.y)
}

/// A live angle arc between three points, with an optional degree readout
/// (FE-145).
///
/// The arc is a core [`Angle`](manim_core::geometry::Angle) rebuilt in place
/// each frame the geometry moves; the label (if any) is a [`DecimalReadout`]
/// placed on the bisector just outside the arc, where it does not sit under the
/// arms.
#[derive(Debug)]
pub struct AngleMarker {
    id: Option<MobjectId<manim_core::geometry::Angle>>,
    radius: f32,
    color: manim_core::prelude::Color,
    value: f32,
}

impl AngleMarker {
    /// An angle arc of the given radius and stroke color.
    pub fn new(radius: f32, color: manim_core::prelude::Color) -> Self {
        Self {
            id: None,
            radius: radius.max(1e-3),
            color,
            value: 0.0,
        }
    }

    /// The last measured angle, in radians.
    pub fn value(&self) -> f32 {
        self.value
    }

    /// The last measured angle, in degrees (what a label usually wants).
    pub fn degrees(&self) -> f32 {
        self.value.to_degrees()
    }

    /// The arc mobject, once built.
    pub fn id(&self) -> Option<AnyId> {
        self.id.map(|i| i.erase())
    }

    /// Where a label belongs: just outside the arc, on the bisector of the two
    /// arms — so it clears both of them at any opening angle.
    pub fn label_anchor(&self, a: Point, vertex: Point, b: Point) -> Point {
        let u = normalize_or_zero(a - vertex);
        let v = normalize_or_zero(b - vertex);
        let bisector = normalize_or_zero(u + v);
        if bisector.length() <= f32::EPSILON {
            // A straight angle: the arms cancel, so step off perpendicular.
            return vertex + Point::new(-u.y, u.x, 0.0) * (self.radius * 1.6);
        }
        vertex + bisector * (self.radius * 1.6)
    }

    /// Rebuilds the arc for the current three points. Returns the measured
    /// angle in radians.
    pub fn sync(&mut self, state: &mut SceneState, a: Point, vertex: Point, b: Point) -> f32 {
        use manim_core::geometry::{Angle, Line};
        use manim_core::Buildable;

        self.value = angle_between(a, vertex, b);
        let fresh = Angle::with_radius(&Line::new(vertex, a), &Line::new(vertex, b), self.radius)
            .with_stroke(self.color, 3.0, 1.0);
        match self.id {
            None => self.id = Some(state.add(fresh)),
            Some(id) => {
                // In-place replacement with a bumped generation: same arena
                // slot, caches invalidated exactly once. See `Readout::sync`.
                let generation = state.get(id).data().generation;
                let slot = state.get_mut(id);
                *slot = fresh;
                slot.data_mut().generation = generation + 1;
            }
        }
        self.value
    }

    /// Removes the arc from the scene.
    pub fn clear(&mut self, state: &mut SceneState) {
        if let Some(id) = self.id.take() {
            state.remove(id);
        }
    }
}

/// `v` normalized, or the zero vector when it is too short to have a direction.
fn normalize_or_zero(v: Point) -> Point {
    let l = v.length();
    if l <= f32::EPSILON {
        Point::ZERO
    } else {
        v / l
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_core::prelude::{Circle, RED};
    use manim_core::Buildable;

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y, 0.0)
    }

    /// A stand-in "text" mobject whose scale encodes the string length, so a
    /// test can see *which* string was built without a typesetter.
    fn fake_text(s: &str) -> Circle {
        Circle::new().with_scale(s.len() as f32)
    }

    #[test]
    fn scalar_formatting_normalizes_negative_zero() {
        assert_eq!(format_scalar(-0.0001, 2), "0.00");
        assert_eq!(format_scalar(0.0, 2), "0.00");
        assert_eq!(format_scalar(-1.5, 1), "-1.5");
        assert_eq!(format_scalar(f32::NAN, 2), "—");
    }

    #[test]
    fn coordinate_styles_read_correctly() {
        let v = p(1.2345, -0.35);
        assert_eq!(CoordFormat::new().render(v), "(1.23, -0.35)");
        assert_eq!(
            CoordFormat::new().style(CoordStyle::Complex).render(v),
            "1.23 − 0.35i"
        );
        assert_eq!(
            CoordFormat::new()
                .style(CoordStyle::Complex)
                .render(p(1.0, 0.5)),
            "1.00 + 0.50i"
        );
        assert_eq!(
            CoordFormat::new()
                .decimals(1)
                .style(CoordStyle::Polar)
                .render(p(3.0, 4.0)),
            "r = 5.0, θ = 0.9"
        );
    }

    #[test]
    fn affixes_wrap_the_value() {
        let f = CoordFormat::new().decimals(0).prefix("z = ").suffix(" ✓");
        assert_eq!(f.render(p(1.0, 2.0)), "z = (1, 2) ✓");
    }

    #[test]
    fn a_readout_is_created_once_then_only_moves() {
        let mut state = SceneState::new();
        let mut r = CoordinateReadout::new(p(0.0, 0.5));
        assert!(
            r.sync(&mut state, p(1.0, 2.0), fake_text),
            "first frame builds"
        );
        let id = r.id().expect("built");
        // An imperceptible move keeps the same text → no rebuild.
        assert!(!r.sync(&mut state, p(1.0001, 2.0), fake_text));
        assert_eq!(r.id(), Some(id), "and the same arena slot");
        // A real move changes the digits → one rebuild.
        assert!(r.sync(&mut state, p(1.5, 2.0), fake_text));
        assert_eq!(r.id(), Some(id), "still the same slot");
    }

    #[test]
    fn a_readout_sits_at_its_offset_from_the_point() {
        let mut state = SceneState::new();
        let mut r = CoordinateReadout::new(p(0.0, 0.5));
        r.sync(&mut state, p(1.0, 2.0), |_| Circle::new());
        let id = r.id().unwrap();
        let center = state.family_bounding_box(id).center();
        assert!(
            (center.x - 1.0).abs() < 1e-4 && (center.y - 2.5).abs() < 1e-4,
            "{center:?}"
        );
    }

    #[test]
    fn rebuilding_bumps_the_generation_so_caches_invalidate() {
        let mut state = SceneState::new();
        let mut r = DecimalReadout::new(p(0.0, 0.0));
        r.sync(&mut state, 1.0, fake_text);
        let id = r.id().unwrap();
        let g0 = state.get_dyn(id).data().generation;
        r.sync(&mut state, 2.0, fake_text);
        assert!(state.get_dyn(id).data().generation > g0);
    }

    #[test]
    fn clearing_removes_the_readout() {
        let mut state = SceneState::new();
        let mut r = DecimalReadout::new(p(0.0, 0.0));
        r.sync(&mut state, 1.0, fake_text);
        let id = r.id().unwrap();
        r.clear(&mut state);
        assert!(!state.contains(id));
        assert!(r.id().is_none());
        // ...and it can come back.
        r.sync(&mut state, 1.0, fake_text);
        assert!(r.id().is_some());
    }

    #[test]
    fn decimal_readout_text_carries_its_affixes() {
        let r: DecimalReadout<Circle> = DecimalReadout::new(Point::ZERO).decimals(1).suffix("°");
        assert_eq!(r.text(90.0), "90.0°");
    }

    #[test]
    fn angles_are_measured_unsigned_and_signed() {
        let (a, b) = (p(1.0, 0.0), p(0.0, 1.0));
        assert!((angle_between(a, Point::ZERO, b) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        // Unsigned ignores the order; signed does not.
        assert!((angle_between(b, Point::ZERO, a) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!(signed_angle(a, Point::ZERO, b) > 0.0);
        assert!(signed_angle(b, Point::ZERO, a) < 0.0);
    }

    #[test]
    fn a_degenerate_arm_measures_zero_not_nan() {
        assert_eq!(angle_between(Point::ZERO, Point::ZERO, p(1.0, 0.0)), 0.0);
        assert_eq!(signed_angle(p(1.0, 0.0), Point::ZERO, Point::ZERO), 0.0);
    }

    #[test]
    fn the_angle_marker_tracks_moving_arms_in_one_slot() {
        let mut state = SceneState::new();
        let mut m = AngleMarker::new(0.5, RED);
        let v = angle_between(p(1.0, 0.0), Point::ZERO, p(0.0, 1.0));
        assert!((m.sync(&mut state, p(1.0, 0.0), Point::ZERO, p(0.0, 1.0)) - v).abs() < 1e-6);
        let id = m.id().expect("built");
        assert!((m.degrees() - 90.0).abs() < 1e-3);
        // Move an arm: same slot, new measurement.
        let half = m.sync(&mut state, p(1.0, 0.0), Point::ZERO, p(1.0, 1.0));
        assert_eq!(m.id(), Some(id));
        assert!((half - std::f32::consts::FRAC_PI_4).abs() < 1e-5, "{half}");
    }

    #[test]
    fn the_label_anchor_clears_both_arms() {
        let m = AngleMarker::new(0.5, RED);
        // A right angle in the first quadrant → the bisector points up-right.
        let at = m.label_anchor(p(1.0, 0.0), Point::ZERO, p(0.0, 1.0));
        assert!(at.x > 0.0 && at.y > 0.0);
        assert!((at.length() - 0.8).abs() < 1e-4, "{at:?}");
        // A straight angle has no bisector; it must still land off the line.
        let at = m.label_anchor(p(1.0, 0.0), Point::ZERO, p(-1.0, 0.0));
        assert!(at.y.abs() > 0.1, "{at:?}");
    }
}
