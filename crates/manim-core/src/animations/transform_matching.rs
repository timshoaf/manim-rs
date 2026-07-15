//! [`TransformMatchingShapes`]: morph one group mobject into another by matching
//! their children on **shape**, regardless of position or scale. Port of manim
//! CE's `TransformMatchingShapes`.
//!
//! Each child is reduced to a scale/translation-invariant signature (a quantized,
//! normalized point set). Children whose signatures match are morphed with
//! [`Transform`]; unmatched source children [`FadeOut`] and unmatched target
//! children [`FadeIn`]. This composes existing animations — manim-text's
//! `TransformMatchingTex` uses the same technique specialized to glyphs; the two
//! are intentionally independent to avoid churn in that crate's API.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use manim_math::path::Path;
use manim_math::rate_functions::RateFn;

use super::{AnimationGroup, FadeIn, FadeOut, Transform};
use crate::animation::{AnimConfig, Animation};
use crate::mobject::AnyId;
use crate::scene_state::SceneState;

/// Points sampled per Bézier curve when building a child's shape signature.
const SIGNATURE_SAMPLES: usize = 6;
/// Grid resolution the normalized outline is quantized to.
const SIGNATURE_GRID: f32 = 24.0;

/// A scale/translation-invariant signature of an outline: points are sampled,
/// recentered, normalized by bounding-box size, quantized to a grid, sorted, and
/// hashed — so the same shape hashes identically at any position or scale.
fn signature(path: &Path) -> u64 {
    let pts = path.points(SIGNATURE_SAMPLES);
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
                (((p.x - center.x) / size) * SIGNATURE_GRID).round() as i32,
                (((p.y - center.y) / size) * SIGNATURE_GRID).round() as i32,
            )
        })
        .collect();
    quant.sort_unstable();
    quant.dedup();
    let mut hasher = DefaultHasher::new();
    quant.hash(&mut hasher);
    hasher.finish()
}

/// The drawable children of `id` (those with geometry), with their signatures.
fn child_signatures(state: &SceneState, id: AnyId) -> Vec<(AnyId, u64)> {
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

/// The result of matching two group mobjects' children.
#[derive(Debug, Clone, Default)]
pub struct MatchResult {
    /// `(source child, target child)` pairs with equal signatures.
    pub matched: Vec<(AnyId, AnyId)>,
    /// Source children with no match (they fade out).
    pub unmatched_source: Vec<AnyId>,
    /// Target children with no match (they fade in).
    pub unmatched_target: Vec<AnyId>,
}

/// Matches the children of `a` and `b` by shape signature (greedy 1-to-1).
/// Exposed so callers/tests can inspect the pairing.
pub fn match_shapes(state: &SceneState, a: AnyId, b: AnyId) -> MatchResult {
    let src = child_signatures(state, a);
    let tgt = child_signatures(state, b);
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

/// Morphs group mobject `a` into group mobject `b`, matching children of equal
/// shape and fading the rest. Port of manim CE's `TransformMatchingShapes`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::TransformMatchingShapes;
/// use manim_math::RIGHT;
/// let mut scene = Scene::new(Config::low());
/// // Two groups each holding a circle; the circles match by shape.
/// let a = scene.add(VGroup::new());
/// let ca = scene.add(Circle::new());
/// scene.state_mut().add_child(a.erase(), ca.erase());
/// let b = scene.add(VGroup::new());
/// let mut cb = Circle::new();
/// cb.shift(3.0 * RIGHT);
/// let cb = scene.add(cb);
/// scene.state_mut().add_child(b.erase(), cb.erase());
/// scene.play(TransformMatchingShapes::new(a.erase(), b.erase())).unwrap();
/// assert!(scene.total_duration() > 0.0);
/// ```
pub struct TransformMatchingShapes {
    a: AnyId,
    b: AnyId,
    config: AnimConfig,
    inner: Option<AnimationGroup>,
}

impl TransformMatchingShapes {
    /// Transforms `a` into `b` by shape-matching their children.
    pub fn new(a: impl Into<AnyId>, b: impl Into<AnyId>) -> Self {
        Self {
            a: a.into(),
            b: b.into(),
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

    /// The child match for `(a, b)` in `state` (for inspection / tests).
    pub fn analyze(state: &SceneState, a: impl Into<AnyId>, b: impl Into<AnyId>) -> MatchResult {
        match_shapes(state, a.into(), b.into())
    }
}

impl Animation for TransformMatchingShapes {
    fn begin(&mut self, state: &mut SceneState) {
        let m = match_shapes(state, self.a, self.b);
        // Hide the target's matched children; the moving source children take
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Circle, Square, VGroup};
    use crate::mobject::MobjectExt;

    fn group_with(state: &mut SceneState, children: Vec<AnyId>) -> AnyId {
        let g = state.add(VGroup::new()).erase();
        for c in children {
            state.add_child(g, c);
        }
        g
    }

    #[test]
    fn matches_same_shape_across_position() {
        let mut state = SceneState::new();
        let c1 = state.add(Circle::new()).erase();
        let a = group_with(&mut state, vec![c1]);

        let mut circle = Circle::new();
        circle.shift(3.0 * manim_math::RIGHT);
        let c2 = state.add(circle).erase();
        let b = group_with(&mut state, vec![c2]);

        let m = match_shapes(&state, a, b);
        assert_eq!(m.matched.len(), 1);
        assert!(m.unmatched_source.is_empty());
        assert!(m.unmatched_target.is_empty());
    }

    #[test]
    fn unmatched_shapes_fade() {
        let mut state = SceneState::new();
        let sq = state.add(Square::new()).erase();
        let a = group_with(&mut state, vec![sq]);
        let ci = state.add(Circle::new()).erase();
        let b = group_with(&mut state, vec![ci]);

        let m = match_shapes(&state, a, b);
        // A square and a circle do not share a signature.
        assert!(m.matched.is_empty());
        assert_eq!(m.unmatched_source.len(), 1);
        assert_eq!(m.unmatched_target.len(), 1);
    }
}
