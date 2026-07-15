//! Animation composition: [`AnimationGroup`], [`Succession`], and
//! [`LaggedStart`].

use manim_math::rate_functions::RateFn;

use crate::animation::AnimConfig;
use crate::animation::{Animation, IntoAnimations};
use crate::scene_state::SceneState;

/// manim CE's default `LaggedStart` lag ratio.
pub const DEFAULT_LAGGED_START_LAG_RATIO: f32 = 0.05;

/// Plays several animations together with a configurable stagger. Port of manim
/// CE's `AnimationGroup`.
///
/// With `lag_ratio = 0` (the default) all children start at once; with `1.0`
/// they run strictly back-to-back ([`Succession`]); values between stagger the
/// starts, exactly as CE's `build_animations_with_timings`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::{AnimationGroup, FadeIn};
/// let mut scene = Scene::new(Config::default());
/// let a = scene.add(Circle::new().with_fill(BLUE, 1.0));
/// let b = scene.add(Square::new().with_fill(RED, 1.0));
/// // Concurrent group runs in 1 s; a sequence would take 2 s.
/// scene.play(AnimationGroup::new((FadeIn::new(a), FadeIn::new(b)))).unwrap();
/// assert!((scene.total_duration() - 1.0).abs() < 1e-6);
/// ```
pub struct AnimationGroup {
    anims: Vec<Box<dyn Animation>>,
    timings: Vec<(f32, f32)>,
    duration: f32,
    lag_ratio: f32,
    config: AnimConfig,
}

impl AnimationGroup {
    /// Groups `anims` to play concurrently (lag ratio `0`).
    pub fn new(anims: impl IntoAnimations) -> Self {
        let mut group = Self {
            anims: anims.into_animations(),
            timings: Vec::new(),
            duration: 0.0,
            lag_ratio: 0.0,
            // Group timing is applied by the sub-timings; ease each child, not
            // the group as a whole.
            config: AnimConfig {
                rate_fn: RateFn::Linear,
                ..AnimConfig::default()
            },
        };
        group.rebuild();
        group
    }

    /// Sets the lag ratio (manim's `lag_ratio`) and recomputes timings.
    pub fn lag_ratio(mut self, lag_ratio: f32) -> Self {
        self.lag_ratio = lag_ratio;
        self.rebuild();
        self
    }

    /// Sets the easing applied to the group's overall progress.
    pub fn rate_fn(mut self, rate_fn: RateFn) -> Self {
        self.config.rate_fn = rate_fn;
        self
    }

    /// Recomputes per-child `(start, end)` windows and the total duration, using
    /// CE's rule `next_start = start + lag_ratio * run_time`.
    fn rebuild(&mut self) {
        let mut timings = Vec::with_capacity(self.anims.len());
        let mut curr = 0.0_f32;
        let mut max_end = 0.0_f32;
        for anim in &self.anims {
            let start = curr;
            let end = start + anim.duration();
            timings.push((start, end));
            max_end = max_end.max(end);
            curr = start + self.lag_ratio * (end - start);
        }
        self.timings = timings;
        self.duration = max_end;
    }
}

impl Animation for AnimationGroup {
    fn begin(&mut self, state: &mut SceneState) {
        for anim in &mut self.anims {
            anim.begin(state);
        }
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let t = alpha.clamp(0.0, 1.0) * self.duration;
        for (anim, (start, end)) in self.anims.iter_mut().zip(&self.timings) {
            let local = if end > start {
                ((t - start) / (end - start)).clamp(0.0, 1.0)
            } else if t >= *start {
                1.0
            } else {
                0.0
            };
            anim.interpolate(state, anim.rate_fn().apply(local));
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        for anim in &mut self.anims {
            anim.finish(state);
        }
    }
    fn duration(&self) -> f32 {
        self.duration
    }
    fn rate_fn(&self) -> RateFn {
        self.config.rate_fn.clone()
    }
}

/// Plays animations strictly one after another. Port of manim CE's `Succession`
/// (an [`AnimationGroup`] with `lag_ratio = 1`).
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::{Succession, FadeIn, FadeOut};
/// let mut scene = Scene::new(Config::default());
/// let a = scene.add(Circle::new().with_fill(BLUE, 1.0));
/// scene.play(Succession::new((FadeIn::new(a), FadeOut::new(a)))).unwrap();
/// // Two 1 s animations back-to-back → 2 s total.
/// assert!((scene.total_duration() - 2.0).abs() < 1e-6);
/// ```
pub struct Succession {
    inner: AnimationGroup,
}

impl Succession {
    /// Plays `anims` back-to-back.
    pub fn new(anims: impl IntoAnimations) -> Self {
        Self {
            inner: AnimationGroup::new(anims).lag_ratio(1.0),
        }
    }
}

impl Animation for Succession {
    fn begin(&mut self, state: &mut SceneState) {
        self.inner.begin(state);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        self.inner.interpolate(state, alpha);
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.inner.finish(state);
    }
    fn duration(&self) -> f32 {
        self.inner.duration()
    }
    fn rate_fn(&self) -> RateFn {
        Animation::rate_fn(&self.inner)
    }
}

/// Plays animations with a small default stagger. Port of manim CE's
/// `LaggedStart` (lag ratio `0.05`).
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::{LaggedStart, FadeIn};
/// let mut scene = Scene::new(Config::default());
/// let a = scene.add(Circle::new().with_fill(BLUE, 1.0));
/// let b = scene.add(Square::new().with_fill(RED, 1.0));
/// scene.play(LaggedStart::new((FadeIn::new(a), FadeIn::new(b)))).unwrap();
/// // Second starts 0.05 of the way in → total 1.05 s.
/// assert!((scene.total_duration() - 1.05).abs() < 1e-4);
/// ```
pub struct LaggedStart {
    inner: AnimationGroup,
}

impl LaggedStart {
    /// Plays `anims` with the default lag ratio (`0.05`).
    pub fn new(anims: impl IntoAnimations) -> Self {
        Self {
            inner: AnimationGroup::new(anims).lag_ratio(DEFAULT_LAGGED_START_LAG_RATIO),
        }
    }

    /// Sets an explicit lag ratio.
    pub fn lag_ratio(mut self, lag_ratio: f32) -> Self {
        self.inner = self.inner.lag_ratio(lag_ratio);
        self
    }
}

impl Animation for LaggedStart {
    fn begin(&mut self, state: &mut SceneState) {
        self.inner.begin(state);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        self.inner.interpolate(state, alpha);
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.inner.finish(state);
    }
    fn duration(&self) -> f32 {
        self.inner.duration()
    }
    fn rate_fn(&self) -> RateFn {
        Animation::rate_fn(&self.inner)
    }
}

/// Applies an animation-generating closure to each of several mobjects, played
/// with a lag. Port of manim CE's `LaggedStartMap`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::{LaggedStartMap, FadeIn};
/// let mut scene = Scene::new(Config::default());
/// let ids: Vec<_> = (0..3)
///     .map(|_| scene.add(Circle::new().with_fill(BLUE, 1.0)).erase())
///     .collect();
/// scene.play(LaggedStartMap::new(ids, |id| Box::new(FadeIn::new(id)))).unwrap();
/// // Three 1 s fades at lag 0.05 → 1 + 2·0.05 = 1.1 s.
/// assert!((scene.total_duration() - 1.1).abs() < 1e-4);
/// ```
pub struct LaggedStartMap {
    inner: AnimationGroup,
}

impl LaggedStartMap {
    /// Maps `map` over `ids` and plays the results with the default lag (0.05).
    pub fn new(
        ids: impl IntoIterator<Item = crate::mobject::AnyId>,
        map: impl Fn(crate::mobject::AnyId) -> Box<dyn Animation>,
    ) -> Self {
        let anims: Vec<Box<dyn Animation>> = ids.into_iter().map(map).collect();
        Self {
            inner: AnimationGroup::new(anims).lag_ratio(DEFAULT_LAGGED_START_LAG_RATIO),
        }
    }

    /// Sets an explicit lag ratio.
    pub fn lag_ratio(mut self, lag_ratio: f32) -> Self {
        self.inner = self.inner.lag_ratio(lag_ratio);
        self
    }
}

impl Animation for LaggedStartMap {
    fn begin(&mut self, state: &mut SceneState) {
        self.inner.begin(state);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        self.inner.interpolate(state, alpha);
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.inner.finish(state);
    }
    fn duration(&self) -> f32 {
        self.inner.duration()
    }
    fn rate_fn(&self) -> RateFn {
        Animation::rate_fn(&self.inner)
    }
}
