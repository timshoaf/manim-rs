//! Numeric mobjects: [`DecimalNumber`], [`Integer`], [`Variable`], and the
//! [`ChangingDecimal`] / [`ChangeDecimalToValue`] animations.

use manim_color::{Color, WHITE};
use manim_core::animation::{AnimConfig, Animation};
use manim_core::animations::ValueTracker;
use manim_core::geometry::VGroup;
use manim_core::impl_mobject;
use manim_core::mobject::{bbox_of, AnyId, MobjectData, MobjectExt, MobjectId};
use manim_core::scene_state::SceneState;
use manim_core::style::Style;
use manim_math::path::Path;
use manim_math::rate_functions::RateFn;
use manim_math::{Point, LEFT};

use crate::digits;
use crate::text::{Text, DEFAULT_FONT_SIZE};

/// A number rendered with tabular (non-jittering) digits, re-typesettable in
/// place. Port of manim CE's `DecimalNumber`.
///
/// Digits share a fixed advance, so [`set_value`](Self::set_value) never shifts
/// the fixed edge (default the **left** edge — see [`edge_to_fix`](Self::edge_to_fix)):
/// a counter grows rightward while its left edge stays put.
///
/// ```
/// use manim_text::DecimalNumber;
/// let d = DecimalNumber::new(3.5);
/// // Two decimal places by default → "3.50".
/// assert_eq!(d.formatted(), "3.50");
/// ```
#[derive(Clone)]
pub struct DecimalNumber {
    data: MobjectData,
    value: f32,
    num_decimal_places: usize,
    include_sign: bool,
    group_with_commas: bool,
    unit: String,
    font_size: f32,
    color: Color,
    edge_to_fix: Point,
    glyph_count: usize,
}
impl_mobject!(DecimalNumber);

impl DecimalNumber {
    /// A number showing `value` (2 decimal places, no sign, white).
    pub fn new(value: f32) -> Self {
        let mut me = Self {
            data: MobjectData::new(Path::default(), Style::filled(WHITE)),
            value,
            num_decimal_places: 2,
            include_sign: false,
            group_with_commas: false,
            unit: String::new(),
            font_size: DEFAULT_FONT_SIZE,
            color: WHITE,
            edge_to_fix: LEFT,
            glyph_count: 0,
        };
        me.retypeset(None);
        me
    }

    /// Sets the number of decimal places (manim's `num_decimal_places`).
    pub fn num_decimal_places(mut self, n: usize) -> Self {
        self.num_decimal_places = n;
        self.retypeset(None);
        self
    }

    /// Always shows a leading `+`/`-` (reserves the sign slot).
    pub fn include_sign(mut self, yes: bool) -> Self {
        self.include_sign = yes;
        self.retypeset(None);
        self
    }

    /// Groups the integer part with thousands separators (`1,000`).
    pub fn group_with_commas(mut self, yes: bool) -> Self {
        self.group_with_commas = yes;
        self.retypeset(None);
        self
    }

    /// Appends a unit suffix (e.g. `"%"`).
    pub fn unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = unit.into();
        self.retypeset(None);
        self
    }

    /// Sets the font size.
    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self.retypeset(None);
        self
    }

    /// Sets the color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self.retypeset(None);
        self
    }

    /// Sets which edge stays fixed when the value re-typesets (default
    /// [`LEFT`]).
    pub fn edge_to_fix(mut self, edge: Point) -> Self {
        self.edge_to_fix = edge;
        self
    }

    /// The current value.
    pub fn value(&self) -> f32 {
        self.value
    }

    /// The formatted string this renders.
    pub fn formatted(&self) -> String {
        format_value(
            self.value,
            self.num_decimal_places,
            self.include_sign,
            self.group_with_commas,
            &self.unit,
        )
    }

    /// The number of drawn glyphs.
    pub fn glyph_count(&self) -> usize {
        self.glyph_count
    }

    /// Re-typesets to show `value`, keeping the fixed edge in place (manim's
    /// `set_value`). Works standalone or on an added mobject (`scene[id].set_value(v)`).
    ///
    /// ```
    /// use manim_text::DecimalNumber;
    /// use manim_core::mobject::MobjectExt;
    /// use manim_math::LEFT;
    /// let mut d = DecimalNumber::new(9.0);
    /// let left = d.get_corner(LEFT).x;
    /// d.set_value(10.0); // one more digit
    /// // The left edge did not move.
    /// assert!((d.get_corner(LEFT).x - left).abs() < 1e-4);
    /// ```
    pub fn set_value(&mut self, value: f32) {
        let anchor = if self.data.path.bounding_box().is_some() {
            Some(bbox_of(&self.data.path).point_in_direction(self.edge_to_fix))
        } else {
            None
        };
        self.value = value;
        self.retypeset(anchor);
    }

    /// Adds this number to `scene` (it is a single, self-drawing mobject, so no
    /// child glyphs), returning its handle.
    ///
    /// ```
    /// use manim_text::DecimalNumber;
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let d = DecimalNumber::new(42.0).add_to(&mut scene);
    /// assert!(scene.contains(d.erase()));
    /// ```
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<DecimalNumber> {
        scene.add(self.clone())
    }

    /// Rebuilds the outline; if `anchor` is given, keeps that world point of the
    /// fixed edge in place.
    fn retypeset(&mut self, anchor: Option<Point>) {
        let s = self.formatted();
        let layout = digits::layout(&s, self.font_size);
        self.data.path = layout.path;
        self.data.style = Style::filled(self.color);
        self.glyph_count = layout.glyph_count;
        if let Some(anchor) = anchor {
            let new_edge = bbox_of(&self.data.path).point_in_direction(self.edge_to_fix);
            self.data.path.apply(|p| p + (anchor - new_edge));
        }
        self.data.bump_generation();
    }
}

/// Formats a value into a display string.
fn format_value(
    value: f32,
    decimals: usize,
    include_sign: bool,
    group_commas: bool,
    unit: &str,
) -> String {
    let sign = if value < 0.0 {
        "-"
    } else if include_sign {
        "+"
    } else {
        ""
    };
    let mut body = format!("{:.*}", decimals, value.abs());
    if group_commas {
        body = group_thousands(&body);
    }
    format!("{sign}{body}{unit}")
}

/// Inserts thousands separators into the integer part of `s`.
fn group_thousands(s: &str) -> String {
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s, None),
    };
    let mut grouped = String::new();
    let n = int.len();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (n - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    match frac {
        Some(f) => format!("{grouped}.{f}"),
        None => grouped,
    }
}

/// A whole number (a [`DecimalNumber`] with zero decimal places). Port of manim
/// CE's `Integer`.
///
/// ```
/// use manim_text::Integer;
/// let n = Integer::new(1234).group_with_commas(true);
/// assert_eq!(n.formatted(), "1,234");
/// ```
pub struct Integer;

impl Integer {
    /// A whole-number [`DecimalNumber`].
    #[allow(clippy::new_ret_no_self)]
    pub fn new(value: i64) -> DecimalNumber {
        DecimalNumber::new(value as f32).num_decimal_places(0)
    }
}

/// A labeled, tracker-driven value display (`label = value`). Port of manim CE's
/// `Variable`.
///
/// [`Variable::of`] adds a label [`Text`] and a [`DecimalNumber`] to the scene,
/// wired to a [`ValueTracker`] by an updater, and returns the group.
pub struct Variable;

impl Variable {
    /// Adds `label = <tracker value>` to `scene`, kept in sync with `tracker`,
    /// and returns the group handle.
    ///
    /// ```
    /// use manim_text::Variable;
    /// use manim_core::animations::ValueTracker;
    /// use manim_core::scene_state::{SceneState, UpdaterCtx};
    /// let mut scene = SceneState::new();
    /// let t = scene.add(ValueTracker::new(3.0));
    /// let v = Variable::of(&mut scene, t, "x");
    /// scene.run_updaters(UpdaterCtx { dt: 0.0, time: 0.0 });
    /// // The group holds the label and the number.
    /// assert!(scene.family(v.erase()).len() >= 2);
    /// ```
    pub fn of(
        scene: &mut SceneState,
        tracker: MobjectId<ValueTracker>,
        label: &str,
    ) -> MobjectId<VGroup> {
        let label_id = Text::new(format!("{label} = ")).add_to(scene);
        let start = scene.try_get(tracker).map(|t| t.get_value()).unwrap_or(0.0);
        let mut number = DecimalNumber::new(start);
        // Place the number just right of the label.
        let right = scene.get(label_id).get_corner(manim_math::RIGHT);
        number.move_to(right + manim_math::RIGHT * (number.width() / 2.0 + 0.1));
        let num_id = scene.add(number);

        scene.add_updater(num_id.erase(), move |s, id, _ctx| {
            let v = s.try_get(tracker).map(|t| t.get_value()).unwrap_or(0.0);
            if let Some(dn) = s
                .get_dyn_mut(id)
                .as_any_mut()
                .downcast_mut::<DecimalNumber>()
            {
                dn.set_value(v);
            }
        });

        VGroup::of(scene, [label_id.erase(), num_id.erase()])
    }
}

/// Reads the [`DecimalNumber`] at `id` in `state`.
fn number_value(state: &SceneState, id: AnyId) -> f32 {
    state
        .get_dyn(id)
        .as_any()
        .downcast_ref::<DecimalNumber>()
        .map(|d| d.value())
        .unwrap_or(0.0)
}

/// Sets the [`DecimalNumber`] at `id` in `state` to `value`.
fn set_number(state: &mut SceneState, id: AnyId, value: f32) {
    if state.contains(id) {
        if let Some(d) = state
            .get_dyn_mut(id)
            .as_any_mut()
            .downcast_mut::<DecimalNumber>()
        {
            d.set_value(value);
        }
    }
}

/// Animates a [`DecimalNumber`] to a target value, re-typesetting each frame.
/// Port of manim CE's `ChangeDecimalToValue`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_text::{ChangeDecimalToValue, DecimalNumber};
/// let mut scene = Scene::new(Config::low());
/// let d = DecimalNumber::new(0.0).num_decimal_places(0).add_to(scene.state_mut());
/// scene.play(ChangeDecimalToValue::new(d, 100.0)).unwrap();
/// assert!((scene[d].value() - 100.0).abs() < 1e-3);
/// ```
pub struct ChangeDecimalToValue {
    id: AnyId,
    target: f32,
    start: f32,
    config: AnimConfig,
}

impl ChangeDecimalToValue {
    /// Animates the number at `id` toward `target`.
    pub fn new(id: impl Into<AnyId>, target: f32) -> Self {
        Self {
            id: id.into(),
            target,
            start: 0.0,
            config: AnimConfig::default(),
        }
    }

    /// Sets the run time in seconds.
    pub fn run_time(mut self, run_time: f32) -> Self {
        self.config.run_time = run_time;
        self
    }

    /// Sets the easing curve.
    pub fn rate_fn(mut self, rate_fn: RateFn) -> Self {
        self.config.rate_fn = rate_fn;
        self
    }
}

impl Animation for ChangeDecimalToValue {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = number_value(state, self.id);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        set_number(
            state,
            self.id,
            self.start + (self.target - self.start) * alpha,
        );
    }
    fn finish(&mut self, state: &mut SceneState) {
        set_number(state, self.id, self.target);
    }
    fn duration(&self) -> f32 {
        self.config.run_time
    }
    fn rate_fn(&self) -> RateFn {
        self.config.rate_fn.clone()
    }
}

/// The closure type driving [`ChangingDecimal`].
type ValueFn = Box<dyn Fn(f32) -> f32>;

/// Drives a [`DecimalNumber`] from an arbitrary function of `alpha`. Port of
/// manim CE's `ChangingDecimal`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_text::{ChangingDecimal, DecimalNumber};
/// let mut scene = Scene::new(Config::low());
/// let d = DecimalNumber::new(0.0).add_to(scene.state_mut());
/// scene.play(ChangingDecimal::new(d, |a| a * a * 50.0)).unwrap();
/// assert!((scene[d].value() - 50.0).abs() < 1e-3);
/// ```
pub struct ChangingDecimal {
    id: AnyId,
    func: ValueFn,
    config: AnimConfig,
}

impl ChangingDecimal {
    /// Drives the number at `id` with `func(alpha)`.
    pub fn new(id: impl Into<AnyId>, func: impl Fn(f32) -> f32 + 'static) -> Self {
        Self {
            id: id.into(),
            func: Box::new(func),
            config: AnimConfig::default(),
        }
    }

    /// Sets the run time in seconds.
    pub fn run_time(mut self, run_time: f32) -> Self {
        self.config.run_time = run_time;
        self
    }

    /// Sets the easing curve.
    pub fn rate_fn(mut self, rate_fn: RateFn) -> Self {
        self.config.rate_fn = rate_fn;
        self
    }
}

impl Animation for ChangingDecimal {
    fn begin(&mut self, _state: &mut SceneState) {}
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        set_number(state, self.id, (self.func)(alpha));
    }
    fn finish(&mut self, state: &mut SceneState) {
        set_number(state, self.id, (self.func)(1.0));
    }
    fn duration(&self) -> f32 {
        self.config.run_time
    }
    fn rate_fn(&self) -> RateFn {
        self.config.rate_fn.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        assert_eq!(DecimalNumber::new(1.5).formatted(), "1.50");
        assert_eq!(DecimalNumber::new(2.75).formatted(), "2.75");
        assert_eq!(Integer::new(42).formatted(), "42");
        assert_eq!(
            DecimalNumber::new(-5.0)
                .num_decimal_places(0)
                .include_sign(true)
                .formatted(),
            "-5"
        );
        assert_eq!(
            DecimalNumber::new(5.0)
                .num_decimal_places(0)
                .include_sign(true)
                .formatted(),
            "+5"
        );
        assert_eq!(
            DecimalNumber::new(1234567.0)
                .num_decimal_places(0)
                .group_with_commas(true)
                .formatted(),
            "1,234,567"
        );
        assert_eq!(
            DecimalNumber::new(50.0)
                .num_decimal_places(0)
                .unit("%")
                .formatted(),
            "50%"
        );
    }

    #[test]
    fn left_edge_is_stable_across_digit_count() {
        let mut d = DecimalNumber::new(9.0).num_decimal_places(0);
        let left = bbox_of(&d.data.path).point_in_direction(LEFT).x;
        d.set_value(1000.0);
        let left2 = bbox_of(&d.data.path).point_in_direction(LEFT).x;
        assert!((left - left2).abs() < 1e-4, "{left} vs {left2}");
    }
}
