//! The [`Write`] animation: reveal a text's glyphs one after another.

use manim_core::animation::{AnimConfig, Animation};
use manim_core::mobject::AnyId;
use manim_core::scene_state::SceneState;
use manim_math::path::Path;
use manim_math::rate_functions::RateFn;

/// The visible portion `[a, b]` of a path, taken per-subpath.
fn partial(full: &Path, a: f32, b: f32) -> Path {
    if b <= a {
        return Path::default();
    }
    let subpaths = full
        .subpaths
        .iter()
        .filter(|s| !s.curves.is_empty())
        .filter_map(|s| {
            Path {
                subpaths: vec![s.clone()],
            }
            .get_subcurve(a, b)
            .subpaths
            .into_iter()
            .next()
        })
        .collect();
    Path { subpaths }
}

/// Progressively draws a text's glyphs left to right — the marquee text
/// animation. Port of manim CE's `Write` (a lagged per-glyph `Create`).
///
/// Works on any mobject family: each descendant with geometry is a glyph, drawn
/// with a staggered start.
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
    full: Vec<(AnyId, Path)>,
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
                let path = &state.get_dyn(m).data().path;
                if path.subpaths.iter().all(|s| s.curves.is_empty()) {
                    None
                } else {
                    Some((m, path.clone()))
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
            let local = (t - i as f32 * self.lag).clamp(0.0, 1.0);
            let data = state.get_dyn_mut(*id).data_mut();
            data.path = partial(full, 0.0, local);
            data.bump_generation();
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        for (id, full) in &self.full {
            if state.contains(*id) {
                let data = state.get_dyn_mut(*id).data_mut();
                data.path = full.clone();
                data.bump_generation();
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

/// The reverse of [`Write`]: un-draws a text's glyphs one after another. Port of
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
