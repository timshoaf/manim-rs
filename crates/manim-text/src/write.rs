//! The [`Write`] animation: reveal a text's glyphs one after another.

use manim_core::animation::{AnimConfig, Animation};
use manim_core::animations::{border_then_fill_frame, default_border_color, DEFAULT_BORDER_WIDTH};
use manim_core::mobject::{AnyId, MobjectData};
use manim_core::scene_state::SceneState;
use manim_math::rate_functions::RateFn;

/// Progressively writes a text's glyphs left to right — the iconic manim text
/// reveal. Port of manim CE's `Write` (a lagged per-glyph
/// [`DrawBorderThenFill`](manim_core::animations::DrawBorderThenFill)).
///
/// Works on any mobject family: each descendant with geometry is a glyph. Each
/// glyph's window traces its outline as a thin temporary stroke in the first
/// half (no fill), then fades the fill in while the temporary stroke fades out —
/// so a glyph never shows solid fill while it is still being traced.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_text::{Text, Write};
/// let mut scene = Scene::new(Config::low());
/// let t = Text::new("Hi").add_to(scene.state_mut());
/// scene.play(Write::new(t)).unwrap();
/// // Fully written at the end: both glyph outlines are complete.
/// let total: f32 = scene
///     .state()
///     .family(t.erase())
///     .iter()
///     .flat_map(|id| scene.state().get_dyn(*id).data().path.subpaths.iter())
///     .map(|s| s.arc_length())
///     .sum();
/// assert!(total > 0.0);
/// ```
pub struct Write {
    id: AnyId,
    lag: f32,
    config: AnimConfig,
    full: Vec<(AnyId, MobjectData)>,
}

impl Write {
    /// Writes the family rooted at `id`, one glyph at a time.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            lag: 0.2,
            config: AnimConfig::default(),
            full: Vec::new(),
        }
    }

    /// Sets the per-glyph lag (in glyph-draw units; smaller = more overlap).
    pub fn lag(mut self, lag: f32) -> Self {
        self.lag = lag.max(0.0);
        self
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

impl Animation for Write {
    fn begin(&mut self, state: &mut SceneState) {
        self.full = state
            .family(self.id)
            .into_iter()
            .filter_map(|m| {
                let data = state.get_dyn(m).data();
                if data.path.subpaths.iter().all(|s| s.curves.is_empty()) {
                    None
                } else {
                    Some((m, data.clone()))
                }
            })
            .collect();
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let n = self.full.len();
        if n == 0 {
            return;
        }
        let total_units = 1.0 + self.lag * (n - 1) as f32;
        let t = alpha.clamp(0.0, 1.0) * total_units;
        for (i, (id, full)) in self.full.iter().enumerate() {
            if !state.contains(*id) {
                continue;
            }
            // Each glyph runs the full draw-border-then-fill cycle over its own
            // one-unit window: trace in the first half, fill in the second.
            let local = (t - i as f32 * self.lag).clamp(0.0, 1.0);
            let border_color = default_border_color(&full.style);
            let data = state.get_dyn_mut(*id).data_mut();
            border_then_fill_frame(data, full, local, DEFAULT_BORDER_WIDTH, border_color);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        for (id, full) in &self.full {
            if state.contains(*id) {
                *state.get_dyn_mut(*id).data_mut() = full.clone();
            }
        }
    }
    fn duration(&self) -> f32 {
        self.config.run_time
    }
    fn rate_fn(&self) -> RateFn {
        self.config.rate_fn.clone()
    }
}

/// The reverse of [`Write`]: fades each glyph's fill back out and un-traces its
/// outline, one after another (time-reversed draw-border-then-fill). Port of
/// manim CE's `Unwrite`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_text::{Text, Unwrite};
/// let mut scene = Scene::new(Config::low());
/// let t = Text::new("bye").add_to(scene.state_mut());
/// scene.play(Unwrite::new(t)).unwrap();
/// assert!(scene.total_duration() > 0.0);
/// ```
pub struct Unwrite {
    inner: Write,
}

impl Unwrite {
    /// Un-writes the family rooted at `id`.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            inner: Write::new(id),
        }
    }

    /// Sets the run time in seconds.
    pub fn run_time(mut self, run_time: f32) -> Self {
        self.inner = self.inner.run_time(run_time);
        self
    }

    /// Sets the easing curve.
    pub fn rate_fn(mut self, rate_fn: RateFn) -> Self {
        self.inner = self.inner.rate_fn(rate_fn);
        self
    }
}

impl Animation for Unwrite {
    fn begin(&mut self, state: &mut SceneState) {
        Animation::begin(&mut self.inner, state);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        // Reverse Write: full at alpha 0, empty at alpha 1.
        Animation::interpolate(&mut self.inner, state, 1.0 - alpha);
    }
    fn finish(&mut self, state: &mut SceneState) {
        Animation::interpolate(&mut self.inner, state, 0.0);
    }
    fn duration(&self) -> f32 {
        Animation::duration(&self.inner)
    }
    fn rate_fn(&self) -> RateFn {
        Animation::rate_fn(&self.inner)
    }
}

/// Reveals a text's glyph children one letter at a time (visibility toggling,
/// not stroke-drawing). Port of manim CE's `AddTextLetterByLetter`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_text::{Text, AddTextLetterByLetter};
/// let mut scene = Scene::new(Config::low());
/// let t = Text::new("hi").add_to(scene.state_mut());
/// scene.play(AddTextLetterByLetter::new(t)).unwrap();
/// assert!(scene.total_duration() > 0.0);
/// ```
pub struct AddTextLetterByLetter {
    id: AnyId,
    config: AnimConfig,
    glyphs: Vec<AnyId>,
    removing: bool,
}

impl AddTextLetterByLetter {
    /// Reveals the glyphs of `id` one at a time.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            config: AnimConfig::default(),
            glyphs: Vec::new(),
            removing: false,
        }
    }

    /// Sets the run time in seconds.
    pub fn run_time(mut self, run_time: f32) -> Self {
        self.config.run_time = run_time;
        self
    }
}

/// Hides a text's glyph children one letter at a time. Port of manim CE's
/// `RemoveTextLetterByLetter`.
pub struct RemoveTextLetterByLetter {
    inner: AddTextLetterByLetter,
}

impl RemoveTextLetterByLetter {
    /// Hides the glyphs of `id` one at a time.
    pub fn new(id: impl Into<AnyId>) -> Self {
        let mut inner = AddTextLetterByLetter::new(id);
        inner.removing = true;
        Self { inner }
    }

    /// Sets the run time in seconds.
    pub fn run_time(mut self, run_time: f32) -> Self {
        self.inner = self.inner.run_time(run_time);
        self
    }
}

impl Animation for AddTextLetterByLetter {
    fn begin(&mut self, state: &mut SceneState) {
        self.glyphs = state
            .get_dyn(self.id)
            .data()
            .children
            .iter()
            .copied()
            .filter(|c| {
                let p = &state.get_dyn(*c).data().path;
                !p.subpaths.iter().all(|s| s.curves.is_empty())
            })
            .collect();
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let n = self.glyphs.len();
        let shown = ((alpha.clamp(0.0, 1.0) * n as f32).ceil() as usize).min(n);
        for (i, g) in self.glyphs.iter().enumerate() {
            // For removal, count down: hide the first `shown` instead of showing.
            let visible = if self.removing { i >= shown } else { i < shown };
            state.set_visible(*g, visible);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        for g in &self.glyphs {
            state.set_visible(*g, !self.removing);
        }
    }
    fn duration(&self) -> f32 {
        self.config.run_time
    }
    fn rate_fn(&self) -> RateFn {
        self.config.rate_fn.clone()
    }
}

impl Animation for RemoveTextLetterByLetter {
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
        self.inner.rate_fn()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Text;

    /// The glyph ids of `root` with geometry, in the same order `Write` uses.
    fn glyphs(scene: &SceneState, root: AnyId) -> Vec<AnyId> {
        scene
            .family(root)
            .into_iter()
            .filter(|m| {
                let p = &scene.get_dyn(*m).data().path;
                !p.subpaths.iter().all(|s| s.curves.is_empty())
            })
            .collect()
    }

    #[test]
    fn write_traces_before_it_fills_per_glyph() {
        let mut scene = SceneState::new();
        let t = Text::new("abcd").add_to(&mut scene);
        let ids = glyphs(&scene, t.erase());
        let n = ids.len();
        assert!(n >= 3, "need several glyphs, got {n}");

        let mut w = Write::new(t);
        w.begin(&mut scene);
        // At alpha 0.5 with the default lag, the earliest glyph is past its trace
        // midpoint (filling) while the latest is still outline-only.
        w.interpolate(&mut scene, 0.5);

        let first = &scene.get_dyn(ids[0]).data().style;
        assert!(first.fill_opacity > 0.0, "earliest glyph is filling");

        let last = &scene.get_dyn(ids[n - 1]).data().style;
        assert_eq!(last.fill_opacity, 0.0, "latest glyph is still outline-only");
        assert!(
            last.render_stroke().is_some(),
            "latest glyph shows a temporary border while tracing"
        );

        // Invariant: no glyph shows fill while it is still tracing (local <= 0.5).
        let lag = 0.2_f32;
        let total_units = 1.0 + lag * (n - 1) as f32;
        let t_units = 0.5 * total_units;
        for (i, id) in ids.iter().enumerate() {
            let local = (t_units - i as f32 * lag).clamp(0.0, 1.0);
            let fill = scene.get_dyn(*id).data().style.fill_opacity;
            if local <= 0.5 {
                assert_eq!(
                    fill, 0.0,
                    "glyph {i} (local {local}) shows fill while tracing"
                );
            } else {
                assert!(fill > 0.0, "glyph {i} (local {local}) should be filling");
            }
        }
    }

    #[test]
    fn write_finishes_with_every_glyph_at_its_target() {
        let mut scene = SceneState::new();
        let t = Text::new("ab").add_to(&mut scene);
        let ids = glyphs(&scene, t.erase());
        let targets: Vec<_> = ids
            .iter()
            .map(|id| scene.get_dyn(*id).data().style.clone())
            .collect();

        let mut w = Write::new(t);
        w.begin(&mut scene);
        w.interpolate(&mut scene, 0.7);
        w.finish(&mut scene);

        for (id, target) in ids.iter().zip(&targets) {
            assert_eq!(
                &scene.get_dyn(*id).data().style,
                target,
                "finish restores each glyph's exact target style"
            );
        }
    }

    #[test]
    fn unwrite_starts_full_and_ends_empty() {
        let mut scene = SceneState::new();
        let t = Text::new("hi").add_to(&mut scene);
        let ids = glyphs(&scene, t.erase());

        let mut u = Unwrite::new(t);
        u.begin(&mut scene);
        // alpha 0 → fully written (reverse of Write's end).
        u.interpolate(&mut scene, 0.0);
        for id in &ids {
            assert!(
                scene.get_dyn(*id).data().style.fill_opacity > 0.0,
                "Unwrite begins fully filled"
            );
        }
        // alpha 1 → un-traced away (empty paths).
        u.interpolate(&mut scene, 1.0);
        for id in &ids {
            let empty = scene
                .get_dyn(*id)
                .data()
                .path
                .subpaths
                .iter()
                .all(|s| s.curves.is_empty());
            assert!(empty, "Unwrite ends with the glyph un-drawn");
        }
    }
}
