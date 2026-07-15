//! Updater-driven animation and the re-exported [`UpdaterCtx`].
//!
//! The updater *registry* lives on [`SceneState`](crate::scene_state::SceneState)
//! (`add_updater` / `remove_updaters` / `run_updaters`) so family transforms and
//! updaters stay coherent. This module re-exports [`UpdaterCtx`] and adds
//! [`UpdateFromFunc`], a one-shot animation whose every frame is produced by a
//! closure of `alpha`.

pub use crate::scene_state::UpdaterCtx;

use crate::animation::AnimConfig;
use crate::animation::{anim_builders, anim_config_accessors, Animation};
use crate::mobject::AnyId;
use crate::scene_state::SceneState;

/// The per-frame closure `UpdateFromFunc` drives, `(state, id, alpha)`.
type AlphaFn = Box<dyn FnMut(&mut SceneState, AnyId, f32)>;

/// An animation that calls a closure `(state, id, alpha)` each frame. Port of
/// manim CE's `UpdateFromAlphaFunc`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::UpdateFromFunc;
/// use manim_math::RIGHT;
/// let mut scene = Scene::new(Config::default());
/// let d = scene.add(Dot::new());
/// scene.play(UpdateFromFunc::new(d, |s, id, alpha| {
///     let base = 4.0 * RIGHT;
///     s.get_dyn_mut(id).data_mut().path.apply(|_| base * alpha);
/// })).unwrap();
/// // The closure drove the dot to 4·RIGHT by alpha = 1.
/// assert!((scene[d].get_center() - 4.0 * RIGHT).length() < 1e-4);
/// ```
pub struct UpdateFromFunc {
    id: AnyId,
    func: AlphaFn,
    config: AnimConfig,
}
anim_builders!(UpdateFromFunc);

impl UpdateFromFunc {
    /// Drives `id` each frame with `func(state, id, alpha)`.
    pub fn new(
        id: impl Into<AnyId>,
        func: impl FnMut(&mut SceneState, AnyId, f32) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            func: Box::new(func),
            config: AnimConfig::default(),
        }
    }
}

impl Animation for UpdateFromFunc {
    fn begin(&mut self, _state: &mut SceneState) {}
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        (self.func)(state, self.id, alpha);
    }
    fn finish(&mut self, state: &mut SceneState) {
        (self.func)(state, self.id, 1.0);
    }
    anim_config_accessors!();
}
