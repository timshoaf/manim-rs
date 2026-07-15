//! Transform-family animations: [`Transform`], [`TransformInto`],
//! [`ReplacementTransform`], [`TransformFromCopy`], [`FadeTransform`],
//! [`Restore`], [`ScaleInPlace`], and [`ShrinkToCenter`].

use crate::animation::AnimConfig;
use crate::animation::{
    anim_builders, anim_config_accessors, family_data, morph_between, Animation, FamilyMorph,
    PathFn,
};
use crate::animations::paths::path_along_arc;
use crate::mobject::{AnyId, Mobject, MobjectData};
use crate::scene_state::SceneState;

/// Morphs one mobject into the shape and style of another scene mobject,
/// leaving the target untouched. Port of manim CE's `Transform`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Transform;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// let circle = scene.add(Circle::new());
/// scene.play(Transform::new(sq, circle)).unwrap();
/// // The square now matches the circle's bounding box (width 2).
/// assert!((scene[sq].bounding_box().width() - 2.0).abs() < 0.1);
/// ```
pub struct Transform {
    source: AnyId,
    target: AnyId,
    config: AnimConfig,
    path_fn: Option<PathFn>,
    morph: Option<FamilyMorph>,
}
anim_builders!(Transform);

impl Transform {
    /// Morphs `source` into `target`'s current shape and style.
    pub fn new(source: impl Into<AnyId>, target: impl Into<AnyId>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            config: AnimConfig::default(),
            path_fn: None,
            morph: None,
        }
    }

    /// Moves points along a circular arc of `angle` radians during the morph
    /// (manim's `path_arc`).
    pub fn path_arc(mut self, angle: f32) -> Self {
        self.path_fn = Some(path_along_arc(angle));
        self
    }

    /// Uses a custom transform path function (manim's `path_func`). See
    /// [`crate::animations::paths`].
    pub fn path_fn(mut self, path_fn: PathFn) -> Self {
        self.path_fn = Some(path_fn);
        self
    }
}

impl Animation for Transform {
    fn begin(&mut self, state: &mut SceneState) {
        let start = family_data(state, self.source);
        let end = family_data(state, self.target);
        self.morph = Some(FamilyMorph::build(start, end).with_path_fn(self.path_fn.clone()));
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

/// Morphs a scene mobject into a free (not-yet-added) target mobject's shape.
/// Port of manim CE's `Transform` with a target `Mobject`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::TransformInto;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// scene.play(TransformInto::new(sq, Circle::new())).unwrap();
/// assert!((scene[sq].bounding_box().width() - 2.0).abs() < 0.1);
/// ```
pub struct TransformInto {
    source: AnyId,
    target_data: MobjectData,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
}
anim_builders!(TransformInto);

impl TransformInto {
    /// Morphs `source` into the free mobject `target`'s own shape and style.
    pub fn new<M: Mobject>(source: impl Into<AnyId>, target: M) -> Self {
        Self {
            source: source.into(),
            target_data: target.data().clone(),
            config: AnimConfig::default(),
            morph: None,
        }
    }
}

impl Animation for TransformInto {
    fn begin(&mut self, state: &mut SceneState) {
        let start = vec![(self.source, state.get_dyn(self.source).data().clone())];
        let end = vec![(self.source, self.target_data.clone())];
        self.morph = Some(FamilyMorph::build(start, end));
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

/// Like [`Transform`], but the source is removed and the target revealed at the
/// end. Port of manim CE's `ReplacementTransform`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::ReplacementTransform;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// let circle = scene.add(Circle::new());
/// scene.play(ReplacementTransform::new(sq, circle)).unwrap();
/// // The square is gone; only the circle remains.
/// assert!(scene.state().try_get(sq).is_none());
/// assert!(scene.state().contains(circle.erase()));
/// ```
pub struct ReplacementTransform {
    source: AnyId,
    target: AnyId,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
}
anim_builders!(ReplacementTransform);

impl ReplacementTransform {
    /// Morphs `source` into `target`, then removes `source` and shows `target`.
    pub fn new(source: impl Into<AnyId>, target: impl Into<AnyId>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            config: AnimConfig::default(),
            morph: None,
        }
    }
}

impl Animation for ReplacementTransform {
    fn begin(&mut self, state: &mut SceneState) {
        let start = family_data(state, self.source);
        let end = family_data(state, self.target);
        self.morph = Some(FamilyMorph::build(start, end));
        // Hide the target while the source morphs into its place.
        state.set_visible(self.target, false);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(m) = &self.morph {
            m.apply(state, alpha);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
        state.remove(self.source);
        state.set_visible(self.target, true);
    }
    anim_config_accessors!();
}

/// The target mobject appears to emerge from a copy of the source's shape,
/// leaving the source untouched. Port of manim CE's `TransformFromCopy`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::TransformFromCopy;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// let circle = scene.add(Circle::new());
/// scene.play(TransformFromCopy::new(sq, circle)).unwrap();
/// // Both survive; the circle ends at its own shape.
/// assert!(scene.state().contains(sq.erase()));
/// assert!((scene[circle].bounding_box().width() - 2.0).abs() < 1e-3);
/// ```
pub struct TransformFromCopy {
    source: AnyId,
    target: AnyId,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
}
anim_builders!(TransformFromCopy);

impl TransformFromCopy {
    /// Animates `target` from a copy of `source`'s shape into `target`'s own.
    pub fn new(source: impl Into<AnyId>, target: impl Into<AnyId>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            config: AnimConfig::default(),
            morph: None,
        }
    }
}

impl Animation for TransformFromCopy {
    fn begin(&mut self, state: &mut SceneState) {
        let start = vec![(self.target, state.get_dyn(self.source).data().clone())];
        let end = vec![(self.target, state.get_dyn(self.target).data().clone())];
        self.morph = Some(FamilyMorph::build(start, end));
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

/// Cross-fades the source out (toward the target) while the target fades in
/// (from the source). Port of manim CE's `FadeTransform`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::FadeTransform;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new().with_fill(BLUE, 1.0));
/// let circle = scene.add(Circle::new().with_fill(RED, 1.0));
/// scene.play(FadeTransform::new(sq, circle)).unwrap();
/// assert!(scene.state().try_get(sq).is_none());
/// ```
pub struct FadeTransform {
    source: AnyId,
    target: AnyId,
    config: AnimConfig,
    source_morph: Option<FamilyMorph>,
    target_morph: Option<FamilyMorph>,
}
anim_builders!(FadeTransform);

impl FadeTransform {
    /// Cross-fades `source` into `target`.
    pub fn new(source: impl Into<AnyId>, target: impl Into<AnyId>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            config: AnimConfig::default(),
            source_morph: None,
            target_morph: None,
        }
    }
}

impl Animation for FadeTransform {
    fn begin(&mut self, state: &mut SceneState) {
        let source_center = state.family_bounding_box(self.source).center();
        let target_center = state.family_bounding_box(self.target).center();
        let source = self.source;
        let target = self.target;
        // Source fades out, drifting toward the target.
        self.source_morph = Some(morph_between(state, source, |s| {
            s.shift(source, target_center - source_center);
            s.set_style_family(source, |st| {
                st.set_opacity(0.0);
            });
        }));
        // Target fades in, drifting from the source.
        self.target_morph = Some(crate::animation::morph_from(state, target, |s| {
            s.shift(target, source_center - target_center);
            s.set_style_family(target, |st| {
                st.set_opacity(0.0);
            });
        }));
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(m) = &self.source_morph {
            m.apply(state, alpha);
        }
        if let Some(m) = &self.target_morph {
            m.apply(state, alpha);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
        state.remove(self.source);
    }
    anim_config_accessors!();
}

/// Animates a mobject back to its last [`save_state`](SceneState::save_state)
/// snapshot. Port of manim CE's `Restore`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Restore;
/// use manim_math::RIGHT;
/// let mut scene = Scene::new(Config::default());
/// let c = scene.add(Circle::new());
/// scene.state_mut().save_state(c.erase());
/// scene[c].shift(4.0 * RIGHT);
/// scene.play(Restore::new(c)).unwrap();
/// assert!(scene[c].get_center().length() < 1e-4);
/// ```
pub struct Restore {
    id: AnyId,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
}
anim_builders!(Restore);

impl Restore {
    /// Animates `id` back to its saved state.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            config: AnimConfig::default(),
            morph: None,
        }
    }
}

impl Animation for Restore {
    fn begin(&mut self, state: &mut SceneState) {
        let start = vec![(self.id, state.get_dyn(self.id).data().clone())];
        let end = match state.saved_state(self.id) {
            Some(saved) => vec![(self.id, saved.clone())],
            None => start.clone(),
        };
        self.morph = Some(FamilyMorph::build(start, end));
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

/// Scales a mobject in place by `factor`. Port of manim CE's `ScaleInPlace`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::ScaleInPlace;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new()); // width 2
/// scene.play(ScaleInPlace::new(sq, 2.0)).unwrap();
/// assert!((scene[sq].bounding_box().width() - 4.0).abs() < 1e-3);
/// ```
pub struct ScaleInPlace {
    id: AnyId,
    factor: f32,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
}
anim_builders!(ScaleInPlace);

impl ScaleInPlace {
    /// Scales `id`'s family in place by `factor`.
    pub fn new(id: impl Into<AnyId>, factor: f32) -> Self {
        Self {
            id: id.into(),
            factor,
            config: AnimConfig::default(),
            morph: None,
        }
    }
}

impl Animation for ScaleInPlace {
    fn begin(&mut self, state: &mut SceneState) {
        let id = self.id;
        let factor = self.factor;
        self.morph = Some(morph_between(state, id, |s| {
            s.scale(id, factor);
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

/// Transforms a mobject into the state of its
/// [`generate_target`](crate::scene_state::SceneState::generate_target) copy,
/// then removes the target. Port of manim CE's `MoveToTarget`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::MoveToTarget;
/// use manim_math::RIGHT;
/// let mut scene = Scene::new(Config::low());
/// let sq = scene.add(Square::new());
/// let target = scene.generate_target(sq);
/// scene.state_mut().get_mut(target).shift(3.0 * RIGHT).scale(2.0);
/// scene.play(MoveToTarget::new(sq)).unwrap();
/// // The original ended at the target's position/size; the target is gone.
/// assert!((scene[sq].get_center().x - 3.0).abs() < 1e-3);
/// assert!(scene.state().try_get(target).is_none());
/// ```
pub struct MoveToTarget {
    id: AnyId,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
    target: Option<AnyId>,
}
anim_builders!(MoveToTarget);

impl MoveToTarget {
    /// Moves `id` to its target's state.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            config: AnimConfig::default(),
            morph: None,
            target: None,
        }
    }
}

impl Animation for MoveToTarget {
    fn begin(&mut self, state: &mut SceneState) {
        self.target = state.target_of(self.id);
        if let Some(t) = self.target {
            let start = family_data(state, self.id);
            let end = family_data(state, t);
            self.morph = Some(FamilyMorph::build(start, end));
        }
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(m) = &self.morph {
            m.apply(state, alpha);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
        if let Some(t) = self.target.take() {
            state.take_target(self.id);
            state.remove(t);
        }
    }
    anim_config_accessors!();
}

/// Shrinks a mobject to nothing at its center. Port of manim CE's
/// `ShrinkToCenter` (a [`ScaleInPlace`] to factor `0`).
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::ShrinkToCenter;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// scene.play(ShrinkToCenter::new(sq)).unwrap();
/// assert!(scene[sq].bounding_box().width() < 1e-3);
/// ```
pub struct ShrinkToCenter {
    inner: ScaleInPlace,
}

impl ShrinkToCenter {
    /// Shrinks `id` to its center.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            inner: ScaleInPlace::new(id, 0.0),
        }
    }

    /// Sets the run time in seconds.
    pub fn run_time(mut self, run_time: f32) -> Self {
        self.inner = self.inner.run_time(run_time);
        self
    }

    /// Sets the easing curve.
    pub fn rate_fn(mut self, rate_fn: manim_math::rate_functions::RateFn) -> Self {
        self.inner = self.inner.rate_fn(rate_fn);
        self
    }
}

impl Animation for ShrinkToCenter {
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
    fn rate_fn(&self) -> manim_math::rate_functions::RateFn {
        Animation::rate_fn(&self.inner)
    }
}

/// Swaps the positions of two mobjects, each drifting along an arc to the
/// other's center. Port of manim CE's `Swap`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Swap;
/// use manim_math::RIGHT;
/// let mut scene = Scene::new(Config::default());
/// let a = scene.add(Circle::new());
/// let b = scene.add(Square::new().with_shift(4.0 * RIGHT));
/// scene.play(Swap::new(a, b)).unwrap();
/// // They exchange centers.
/// assert!((scene[a].get_center() - 4.0 * RIGHT).length() < 1e-3);
/// assert!(scene[b].get_center().length() < 1e-3);
/// ```
pub struct Swap {
    inner: CyclicReplace,
}

impl Swap {
    /// Swaps `a` and `b`.
    pub fn new(a: impl Into<AnyId>, b: impl Into<AnyId>) -> Self {
        Self {
            inner: CyclicReplace::new([a.into(), b.into()]),
        }
    }

    /// Sets the run time in seconds.
    pub fn run_time(mut self, run_time: f32) -> Self {
        self.inner = self.inner.run_time(run_time);
        self
    }
}

impl Animation for Swap {
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
    fn rate_fn(&self) -> manim_math::rate_functions::RateFn {
        Animation::rate_fn(&self.inner)
    }
}

/// Cyclically permutes a set of mobjects, each moving to the next one's center
/// along an arc. Port of manim CE's `CyclicReplace`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::CyclicReplace;
/// use manim_math::RIGHT;
/// let mut scene = Scene::new(Config::default());
/// let a = scene.add(Circle::new());
/// let b = scene.add(Square::new().with_shift(2.0 * RIGHT));
/// let c = scene.add(Circle::new().with_shift(4.0 * RIGHT));
/// scene.play(CyclicReplace::new([a.erase(), b.erase(), c.erase()])).unwrap();
/// // a→b's spot, b→c's spot, c→a's spot.
/// assert!((scene[a].get_center() - 2.0 * RIGHT).length() < 1e-3);
/// assert!((scene[c].get_center()).length() < 1e-3);
/// ```
pub struct CyclicReplace {
    ids: Vec<AnyId>,
    config: AnimConfig,
    path_fn: PathFn,
    morph: Option<FamilyMorph>,
}

impl CyclicReplace {
    /// Cyclically replaces the given mobjects (each moves to the next's center).
    pub fn new(ids: impl IntoIterator<Item = AnyId>) -> Self {
        Self {
            ids: ids.into_iter().collect(),
            config: AnimConfig::default(),
            path_fn: path_along_arc(std::f32::consts::PI),
            morph: None,
        }
    }

    /// Sets the run time in seconds.
    pub fn run_time(mut self, run_time: f32) -> Self {
        self.config.run_time = run_time;
        self
    }

    /// Sets the easing curve.
    pub fn rate_fn(mut self, rate_fn: manim_math::rate_functions::RateFn) -> Self {
        self.config.rate_fn = rate_fn;
        self
    }
}

impl Animation for CyclicReplace {
    fn begin(&mut self, state: &mut SceneState) {
        let n = self.ids.len();
        if n == 0 {
            self.morph = None;
            return;
        }
        // Each mobject's end center is the next one's current center.
        let centers: Vec<_> = self
            .ids
            .iter()
            .map(|id| state.family_bounding_box(*id).center())
            .collect();
        let mut start = Vec::new();
        let mut end = Vec::new();
        for (i, id) in self.ids.iter().enumerate() {
            for (mid, data) in family_data(state, *id) {
                let mut ed = data.clone();
                let delta = centers[(i + 1) % n] - centers[i];
                ed.path.apply(|p| p + delta);
                start.push((mid, data));
                end.push((mid, ed));
            }
        }
        self.morph = Some(FamilyMorph::build(start, end).with_path_fn(Some(self.path_fn.clone())));
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
