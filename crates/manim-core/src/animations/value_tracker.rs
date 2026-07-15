//! [`ValueTracker`] — a mobject holding an animatable scalar — and
//! [`SetValue`], the animation that tweens it.

use crate::animation::AnimConfig;
use crate::animation::{anim_builders, anim_config_accessors, Animation};
use crate::impl_mobject;
use crate::mobject::{MobjectData, MobjectId};
use crate::scene_state::SceneState;
use crate::style::Style;

/// A mobject with no geometry that holds a single `f32`, for driving other
/// mobjects via updaters. Port of manim CE's `ValueTracker`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::ValueTracker;
/// let mut scene = Scene::new(Config::default());
/// let t = scene.add(ValueTracker::new(2.0));
/// assert_eq!(scene[t].get_value(), 2.0);
/// scene[t].increment_value(0.5);
/// assert_eq!(scene[t].get_value(), 2.5);
/// ```
#[derive(Clone)]
pub struct ValueTracker {
    data: MobjectData,
    value: f32,
}
impl_mobject!(ValueTracker);

impl ValueTracker {
    /// A tracker initialized to `value`.
    pub fn new(value: f32) -> Self {
        Self {
            data: MobjectData::new(Default::default(), Style::default()),
            value,
        }
    }

    /// The current value (manim's `get_value`).
    pub fn get_value(&self) -> f32 {
        self.value
    }

    /// Sets the value (manim's `set_value`).
    pub fn set_value(&mut self, value: f32) {
        self.value = value;
    }

    /// Adds `delta` to the value (manim's `increment_value`).
    pub fn increment_value(&mut self, delta: f32) {
        self.value += delta;
    }
}

/// Tweens a [`ValueTracker`] from its current value to `target`. Serves as
/// manim's `tracker.animate.set_value(target)`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::{SetValue, ValueTracker};
/// let mut scene = Scene::new(Config::default());
/// let t = scene.add(ValueTracker::new(0.0));
/// scene.play(SetValue::new(t, 10.0)).unwrap();
/// assert!((scene[t].get_value() - 10.0).abs() < 1e-6);
/// ```
pub struct SetValue {
    tracker: MobjectId<ValueTracker>,
    target: f32,
    start: f32,
    config: AnimConfig,
}
anim_builders!(SetValue);

impl SetValue {
    /// Animates `tracker` toward `target`.
    pub fn new(tracker: MobjectId<ValueTracker>, target: f32) -> Self {
        Self {
            tracker,
            target,
            start: 0.0,
            config: AnimConfig::default(),
        }
    }
}

impl Animation for SetValue {
    fn begin(&mut self, state: &mut SceneState) {
        if let Some(t) = state.try_get(self.tracker) {
            self.start = t.get_value();
        }
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(t) = state.try_get_mut(self.tracker) {
            t.set_value(self.start + (self.target - self.start) * alpha);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        if let Some(t) = state.try_get_mut(self.tracker) {
            t.set_value(self.target);
        }
    }
    anim_config_accessors!();
}
