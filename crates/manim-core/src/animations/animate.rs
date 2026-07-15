//! The `.animate()` API: an [`AnimBuilder`] that records mobject mutations and
//! plays them as a start→end interpolation.
//!
//! Like manim's beloved `mobject.animate.shift(…)`, this interpolates the
//! *states* before and after the recorded calls, not the path of method calls.
//! Because our handles are arena ids, the entry point is
//! [`Animate::animate`] on a [`MobjectId`], e.g. `circle.animate().shift(RIGHT)`.

use manim_color::Color;
use manim_math::Point;

use crate::animation::AnimConfig;
use crate::animation::{anim_config_accessors, morph_between, Animation, FamilyMorph};
use crate::mobject::{Mobject, MobjectId};
use crate::scene_state::SceneState;

/// A recorded family-aware mutation, replayed on a scene clone to find the
/// morph's end state.
type SceneOp = Box<dyn Fn(&mut SceneState)>;

/// A recorder of family-aware mutations that becomes a morph animation.
///
/// Build it with [`Animate::animate`], chain mutation methods, and pass it to
/// [`Scene::play`](crate::scene::Scene::play).
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Animate;
/// use manim_math::RIGHT;
/// let mut scene = Scene::new(Config::default());
/// let c = scene.add(Circle::new());
/// scene.play(c.animate().shift(2.0 * RIGHT).set_color(RED)).unwrap();
/// assert!((scene[c].get_center() - 2.0 * RIGHT).length() < 1e-4);
/// assert_eq!(scene[c].data().style.stroke_color, Some(RED));
/// ```
pub struct AnimBuilder<M> {
    id: MobjectId<M>,
    ops: Vec<SceneOp>,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
}

impl<M: Mobject> AnimBuilder<M> {
    /// Starts recording mutations for `id`.
    pub fn new(id: MobjectId<M>) -> Self {
        Self {
            id,
            ops: Vec::new(),
            config: AnimConfig::default(),
            morph: None,
        }
    }

    /// Sets the run time in seconds (manim's `run_time`).
    pub fn run_time(mut self, run_time: f32) -> Self {
        self.config.run_time = run_time;
        self
    }

    /// Sets the easing curve (manim's `rate_func`).
    pub fn rate_fn(mut self, rate_fn: manim_math::rate_functions::RateFn) -> Self {
        self.config.rate_fn = rate_fn;
        self
    }

    /// Records a family shift.
    pub fn shift(mut self, delta: Point) -> Self {
        let id = self.id.erase();
        self.ops.push(Box::new(move |s| s.shift(id, delta)));
        self
    }

    /// Records a family move-to.
    pub fn move_to(mut self, point: Point) -> Self {
        let id = self.id.erase();
        self.ops.push(Box::new(move |s| s.move_to(id, point)));
        self
    }

    /// Records a family scale about its center.
    pub fn scale(mut self, factor: f32) -> Self {
        let id = self.id.erase();
        self.ops.push(Box::new(move |s| s.scale(id, factor)));
        self
    }

    /// Records a family rotation about its center (interpolated as a state
    /// morph, per manim's `.animate` semantics).
    pub fn rotate(mut self, angle: f32) -> Self {
        let id = self.id.erase();
        self.ops.push(Box::new(move |s| s.rotate(id, angle)));
        self
    }

    /// Records a fill change.
    pub fn set_fill(mut self, color: Color, opacity: f32) -> Self {
        let id = self.id.erase();
        self.ops.push(Box::new(move |s| {
            s.set_style_family(id, |st| {
                st.set_fill(color, opacity);
            })
        }));
        self
    }

    /// Records a stroke change.
    pub fn set_stroke(mut self, color: Color, width: f32, opacity: f32) -> Self {
        let id = self.id.erase();
        self.ops.push(Box::new(move |s| {
            s.set_style_family(id, |st| {
                st.set_stroke(color, width, opacity);
            })
        }));
        self
    }

    /// Records a color change (both fill and stroke).
    pub fn set_color(mut self, color: Color) -> Self {
        let id = self.id.erase();
        self.ops.push(Box::new(move |s| {
            s.set_style_family(id, |st| {
                st.set_color(color);
            })
        }));
        self
    }

    /// Records an opacity change (both fill and stroke).
    pub fn set_opacity(mut self, opacity: f32) -> Self {
        let id = self.id.erase();
        self.ops.push(Box::new(move |s| {
            s.set_style_family(id, |st| {
                st.set_opacity(opacity);
            })
        }));
        self
    }
}

impl<M: Mobject> Animation for AnimBuilder<M> {
    fn begin(&mut self, state: &mut SceneState) {
        let ops = &self.ops;
        let morph = morph_between(state, self.id.erase(), |s| {
            for op in ops {
                op(s);
            }
        });
        self.morph = Some(morph);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(m) = &self.morph {
            m.apply(state, alpha);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    anim_config_accessors!();
}

/// Adds `.animate()` to a mobject handle (manim's `mobject.animate`).
pub trait Animate<M> {
    /// Begins recording an [`AnimBuilder`] for this handle.
    fn animate(self) -> AnimBuilder<M>;
}

impl<M: Mobject> Animate<M> for MobjectId<M> {
    fn animate(self) -> AnimBuilder<M> {
        AnimBuilder::new(self)
    }
}
