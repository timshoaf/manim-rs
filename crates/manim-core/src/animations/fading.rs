//! Fading animations: [`FadeIn`] and [`FadeOut`], with optional shift/scale.

use manim_math::Point;

use crate::animation::AnimConfig;
use crate::animation::{
    anim_builders, anim_config_accessors, morph_between, morph_from, Animation, FamilyMorph,
};
use crate::mobject::AnyId;
use crate::scene_state::SceneState;

/// Fades a mobject in from transparent, optionally drifting in from a
/// `shift` direction and/or growing from a `scale`. Port of manim CE's `FadeIn`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::FadeIn;
/// use manim_math::UP;
/// let mut scene = Scene::new(Config::default());
/// let c = scene.add(Circle::new().with_fill(BLUE, 1.0));
/// scene.play(FadeIn::new(c).shift(UP)).unwrap();
/// // Ends fully opaque at its target position.
/// assert_eq!(scene[c].data().style.fill_opacity, 1.0);
/// assert!(scene[c].get_center().length() < 1e-4);
/// ```
pub struct FadeIn {
    id: AnyId,
    shift: Point,
    scale: f32,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
}
anim_builders!(FadeIn);

impl FadeIn {
    /// Fades `id` in at its current position.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            shift: Point::ZERO,
            scale: 1.0,
            config: AnimConfig::default(),
            morph: None,
        }
    }

    /// Makes the mobject drift in from `-shift` as it fades (manim's `shift`).
    pub fn shift(mut self, shift: Point) -> Self {
        self.shift = shift;
        self
    }

    /// Makes the mobject grow from `scale`× its size as it fades (manim's
    /// `scale`).
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }
}

impl Animation for FadeIn {
    fn begin(&mut self, state: &mut SceneState) {
        let id = self.id;
        let shift = self.shift;
        let scale = self.scale;
        // Start = the target, moved back by `shift`, scaled down, transparent.
        self.morph = Some(morph_from(state, id, |s| {
            s.shift(id, -shift);
            if (scale - 1.0).abs() > 1e-9 {
                let c = s.family_bounding_box(id).center();
                s.scale_about(id, scale, c);
            }
            s.set_style_family(id, |st| {
                st.set_opacity(0.0);
            });
        }));
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

/// Fades a mobject out to transparent, optionally drifting toward a `shift`
/// direction and/or shrinking by `scale`, then hiding it. Port of manim CE's
/// `FadeOut`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::FadeOut;
/// let mut scene = Scene::new(Config::default());
/// let c = scene.add(Circle::new().with_fill(BLUE, 1.0));
/// scene.play(FadeOut::new(c)).unwrap();
/// // Hidden at the end, so it no longer draws.
/// assert!(scene.display_list().is_empty());
/// ```
pub struct FadeOut {
    id: AnyId,
    shift: Point,
    scale: f32,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
}
anim_builders!(FadeOut);

impl FadeOut {
    /// Fades `id` out in place.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            shift: Point::ZERO,
            scale: 1.0,
            config: AnimConfig::default(),
            morph: None,
        }
    }

    /// Makes the mobject drift toward `shift` as it fades (manim's `shift`).
    pub fn shift(mut self, shift: Point) -> Self {
        self.shift = shift;
        self
    }

    /// Makes the mobject shrink by `scale`× as it fades (manim's `scale`).
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }
}

impl Animation for FadeOut {
    fn begin(&mut self, state: &mut SceneState) {
        let id = self.id;
        let shift = self.shift;
        let scale = self.scale;
        state.set_visible(id, true);
        // End = the target, moved by `shift`, scaled, transparent.
        self.morph = Some(morph_between(state, id, |s| {
            s.shift(id, shift);
            if (scale - 1.0).abs() > 1e-9 {
                let c = s.family_bounding_box(id).center();
                s.scale_about(id, scale, c);
            }
            s.set_style_family(id, |st| {
                st.set_opacity(0.0);
            });
        }));
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(m) = &self.morph {
            m.apply(state, alpha);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
        state.set_visible(self.id, false);
    }
    anim_config_accessors!();
}
