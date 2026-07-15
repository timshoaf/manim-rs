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
