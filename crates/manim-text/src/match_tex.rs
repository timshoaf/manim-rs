//! [`TransformMatchingTex`]: morph one [`MathTex`](crate::MathTex) into another
//! by matching glyphs on **shape**, without substring isolation.
//!
//! Each glyph child is reduced to a scale/translation-invariant signature (a
//! quantized, normalized point set). Glyphs whose signatures match are morphed
//! with [`Transform`]; unmatched source glyphs [`FadeOut`] and unmatched target
//! glyphs [`FadeIn`]. This composes existing core animations — no core changes.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use manim_core::animation::{AnimConfig, Animation};
use manim_core::animations::{AnimationGroup, FadeIn, FadeOut, Transform};
use manim_core::mobject::AnyId;
use manim_core::scene_state::SceneState;
use manim_math::path::Path;
use manim_math::rate_functions::RateFn;

use crate::math::MathTex;
use manim_core::mobject::MobjectId;

/// A scale/translation-invariant signature of a glyph outline.
///
/// Points are sampled along the outline, recentered, normalized by the bounding
/// box, quantized to a grid, sorted, and hashed — so the same character (same
/// font, any position/scale) hashes identically.
fn signature(path: &Path) -> u64 {
    let pts = path.points(6);
    if pts.is_empty() {
        return 0;
    }
    let (mut min, mut max) = (pts[0], pts[0]);
    for p in &pts {
        min = min.min(*p);
        max = max.max(*p);
    }
    let center = (min + max) * 0.5;
    let size = (max - min).max_element().max(1e-6);
    let mut quant: Vec<(i32, i32)> = pts
        .iter()
        .map(|p| {
            (
                (((p.x - center.x) / size) * 24.0).round() as i32,
                (((p.y - center.y) / size) * 24.0).round() as i32,
            )
        })
        .collect();
    quant.sort_unstable();
    quant.dedup();
    let mut hasher = DefaultHasher::new();
    quant.hash(&mut hasher);
    hasher.finish()
}

/// The glyph children of a math mobject that have geometry, with signatures.
fn glyph_signatures(state: &SceneState, id: AnyId) -> Vec<(AnyId, u64)> {
    state
        .get_dyn(id)
        .data()
        .children
        .clone()
        .into_iter()
        .filter_map(|c| {
            let path = &state.get_dyn(c).data().path;
            if path.subpaths.iter().all(|s| s.curves.is_empty()) {
                None
            } else {
                Some((c, signature(path)))
            }
        })
        .collect()
}

/// The result of matching two formulas' glyphs.
#[derive(Debug, Clone, Default)]
pub struct MatchResult {
    /// `(source glyph, target glyph)` pairs with equal signatures.
    pub matched: Vec<(AnyId, AnyId)>,
    /// Source glyphs with no match (they fade out).
    pub unmatched_source: Vec<AnyId>,
    /// Target glyphs with no match (they fade in).
    pub unmatched_target: Vec<AnyId>,
}

/// Matches the glyphs of the math mobjects `a` and `b` by shape signature
/// (greedy 1-to-1). Exposed so callers/tests can inspect the pairing.
pub fn match_glyphs(state: &SceneState, a: AnyId, b: AnyId) -> MatchResult {
    let src = glyph_signatures(state, a);
    let tgt = glyph_signatures(state, b);
    let mut used: HashSet<usize> = HashSet::new();
    let mut result = MatchResult::default();
    for (aid, asig) in &src {
        let hit = tgt
            .iter()
            .enumerate()
            .find(|(j, (_, bsig))| !used.contains(j) && bsig == asig);
        match hit {
            Some((j, (bid, _))) => {
                used.insert(j);
                result.matched.push((*aid, *bid));
            }
            None => result.unmatched_source.push(*aid),
        }
    }
    for (j, (bid, _)) in tgt.iter().enumerate() {
        if !used.contains(&j) {
            result.unmatched_target.push(*bid);
        }
    }
    result
}

/// Morphs math `a` into math `b`, matching shared glyphs and fading the rest.
/// Port of manim CE's `TransformMatchingTex` (shape-matched, isolation-free).
///
/// ```
/// use manim_core::prelude::*;
/// use manim_text::{MathTex, TransformMatchingTex};
/// let mut scene = Scene::new(Config::low());
/// let a = MathTex::new(r"e^{i\pi} + 1 = 0").unwrap().add_to(scene.state_mut());
/// let b = MathTex::new(r"e^{i\pi} = -1").unwrap().add_to(scene.state_mut());
/// scene.play(TransformMatchingTex::new(a, b)).unwrap();
/// // The shared e, i, π, =, 1 glyphs are matched.
/// assert!(scene.total_duration() > 0.0);
/// ```
pub struct TransformMatchingTex {
    a: AnyId,
    b: AnyId,
    config: AnimConfig,
    inner: Option<AnimationGroup>,
}

impl TransformMatchingTex {
    /// Transforms `a` into `b` by shape-matching their glyphs.
    pub fn new(a: MobjectId<MathTex>, b: MobjectId<MathTex>) -> Self {
        Self {
            a: a.erase(),
            b: b.erase(),
            config: AnimConfig::default(),
            inner: None,
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

    /// The glyph match for `(a, b)` in `state` (for inspection / tests).
    pub fn analyze(
        state: &SceneState,
        a: MobjectId<MathTex>,
        b: MobjectId<MathTex>,
    ) -> MatchResult {
        match_glyphs(state, a.erase(), b.erase())
    }
}

impl Animation for TransformMatchingTex {
    fn begin(&mut self, state: &mut SceneState) {
        let m = match_glyphs(state, self.a, self.b);
        // The target's matched glyphs are hidden; the moving source glyphs take
        // their place.
        for (_, bid) in &m.matched {
            state.set_visible(*bid, false);
        }
        let mut anims: Vec<Box<dyn Animation>> = Vec::new();
        for (aid, bid) in &m.matched {
            anims.push(Box::new(Transform::new(*aid, *bid)));
        }
        for aid in &m.unmatched_source {
            anims.push(Box::new(FadeOut::new(*aid)));
        }
        for bid in &m.unmatched_target {
            anims.push(Box::new(FadeIn::new(*bid)));
        }
        let mut group = AnimationGroup::new(anims);
        group.begin(state);
        self.inner = Some(group);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(g) = &mut self.inner {
            g.interpolate(state, alpha);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        if let Some(g) = &mut self.inner {
            g.finish(state);
        }
    }
    fn duration(&self) -> f32 {
        self.config.run_time
    }
    fn rate_fn(&self) -> RateFn {
        self.config.rate_fn.clone()
    }
}
