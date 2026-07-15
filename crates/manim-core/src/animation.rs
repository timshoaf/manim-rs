//! The [`Animation`] trait, its per-animation [`AnimConfig`], the
//! [`IntoAnimations`] conversion used by `play`, and the shared interpolation
//! helpers every animation is built from.
//!
//! An animation is data: a target, a duration, a rate function, and a rule for
//! interpolating the scene at progress `alpha ∈ [0, 1]`. The lifecycle
//! ([`begin`](Animation::begin) → [`interpolate`](Animation::interpolate) →
//! [`finish`](Animation::finish)) mirrors manim CE's
//! `begin`/`interpolate_mobject`/`finish` exactly. See
//! `docs/design/04-animation-system.md`.

use manim_math::path::Path;
use manim_math::rate_functions::RateFn;
use manim_math::Point;

use crate::mobject::{AnyId, MobjectData};
use crate::scene_state::SceneState;
use crate::style::Style;

/// Per-animation timing configuration: run time (seconds) and easing.
///
/// ```
/// use manim_core::animation::AnimConfig;
/// use manim_math::rate_functions::RateFn;
/// let cfg = AnimConfig::default();
/// assert_eq!(cfg.run_time, 1.0);
/// assert!(matches!(cfg.rate_fn, RateFn::Smooth));
/// ```
#[derive(Debug, Clone)]
pub struct AnimConfig {
    /// Duration in seconds (manim's `run_time`).
    pub run_time: f32,
    /// The easing curve applied to progress before interpolation.
    pub rate_fn: RateFn,
}

impl Default for AnimConfig {
    fn default() -> Self {
        Self {
            run_time: 1.0,
            rate_fn: RateFn::Smooth,
        }
    }
}

/// Anything that can drive the scene forward over a normalized progress.
///
/// Implementors snapshot their start state in [`begin`](Self::begin) (re-run on
/// every seek, so it must recompute from the current scene), mutate the scene in
/// [`interpolate`](Self::interpolate) as a pure function of `alpha`, and commit
/// final state in [`finish`](Self::finish). The `alpha` passed to `interpolate`
/// already has the [`rate_fn`](Self::rate_fn) applied.
pub trait Animation: 'static {
    /// Snapshot start state and prepare (e.g. align point counts).
    fn begin(&mut self, state: &mut SceneState);
    /// Drive the animation to progress `alpha` (rate function already applied).
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32);
    /// Commit final state and clean up temporaries.
    fn finish(&mut self, state: &mut SceneState);
    /// The run time in seconds (default `1.0`).
    fn duration(&self) -> f32 {
        1.0
    }
    /// The easing curve (default [`RateFn::Smooth`]).
    fn rate_fn(&self) -> RateFn {
        RateFn::Smooth
    }
}

/// Generates the `run_time` and `rate_fn` consuming builders for an animation
/// struct that has a `config: AnimConfig` field.
macro_rules! anim_builders {
    ($t:ty) => {
        impl $t {
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
        }
    };
}
pub(crate) use anim_builders;

/// Expands to the [`Animation::duration`] / [`Animation::rate_fn`] method bodies
/// for an animation whose timing lives in a `config: AnimConfig` field.
macro_rules! anim_config_accessors {
    () => {
        fn duration(&self) -> f32 {
            self.config.run_time
        }
        fn rate_fn(&self) -> manim_math::rate_functions::RateFn {
            self.config.rate_fn.clone()
        }
    };
}
pub(crate) use anim_config_accessors;

/// Conversion into a flat list of boxed animations, accepted by
/// [`Scene::play`](crate::scene::Scene::play).
///
/// Implemented for a single animation, a `Box<dyn Animation>`, a `Vec` of
/// either, and tuples up to six elements — so concurrent animations read
/// naturally: `scene.play((Create::new(a), FadeIn::new(b)))`.
pub trait IntoAnimations {
    /// Flattens `self` into boxed animations, run concurrently in one segment.
    fn into_animations(self) -> Vec<Box<dyn Animation>>;
}

impl<A: Animation> IntoAnimations for A {
    fn into_animations(self) -> Vec<Box<dyn Animation>> {
        vec![Box::new(self)]
    }
}

impl IntoAnimations for Box<dyn Animation> {
    fn into_animations(self) -> Vec<Box<dyn Animation>> {
        vec![self]
    }
}

impl<A: IntoAnimations> IntoAnimations for Vec<A> {
    fn into_animations(self) -> Vec<Box<dyn Animation>> {
        self.into_iter()
            .flat_map(IntoAnimations::into_animations)
            .collect()
    }
}

macro_rules! tuple_into_animations {
    ($($name:ident),+) => {
        impl<$($name: IntoAnimations),+> IntoAnimations for ($($name,)+) {
            fn into_animations(self) -> Vec<Box<dyn Animation>> {
                #[allow(non_snake_case)]
                let ($($name,)+) = self;
                let mut out = Vec::new();
                $( out.extend($name.into_animations()); )+
                out
            }
        }
    };
}
tuple_into_animations!(A);
tuple_into_animations!(A, B);
tuple_into_animations!(A, B, C);
tuple_into_animations!(A, B, C, D);
tuple_into_animations!(A, B, C, D, E);
tuple_into_animations!(A, B, C, D, E, F);

// ---------------------------------------------------------------------------
// Shared interpolation helpers.
// ---------------------------------------------------------------------------

/// Linearly interpolates two structurally-aligned paths pointwise.
///
/// Both paths must already have equal subpath and per-subpath curve counts
/// (see [`Path::align_with`]); mismatched extras are ignored.
///
/// ```
/// use manim_core::animation::lerp_aligned_path;
/// use manim_math::path::Path;
/// use manim_math::{Point, RIGHT};
/// let a = Path::from_corners(&[Point::ZERO, RIGHT], false);
/// let b = Path::from_corners(&[RIGHT, 3.0 * RIGHT], false);
/// let mid = lerp_aligned_path(&a, &b, 0.5);
/// assert!((mid.point_from_proportion(0.0) - 0.5 * RIGHT).length() < 1e-6);
/// ```
pub fn lerp_aligned_path(a: &Path, b: &Path, t: f32) -> Path {
    let subpaths = a
        .subpaths
        .iter()
        .zip(&b.subpaths)
        .map(|(sa, sb)| {
            let curves = sa
                .curves
                .iter()
                .zip(&sb.curves)
                .map(|(ca, cb)| manim_math::bezier::CubicBezier {
                    p0: lerp_point(ca.p0, cb.p0, t),
                    p1: lerp_point(ca.p1, cb.p1, t),
                    p2: lerp_point(ca.p2, cb.p2, t),
                    p3: lerp_point(ca.p3, cb.p3, t),
                })
                .collect();
            manim_math::path::SubPath {
                curves,
                closed: sa.closed,
            }
        })
        .collect();
    Path { subpaths }
}

/// Componentwise point interpolation.
fn lerp_point(a: Point, b: Point, t: f32) -> Point {
    a + (b - a) * t
}

fn lerp_scalar(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Linearly interpolates two styles (colors, opacities, stroke width).
///
/// A color that is set on only one side is carried through unchanged; the dash
/// pattern follows `b`.
///
/// ```
/// use manim_core::animation::lerp_style;
/// use manim_core::style::Style;
/// let mut a = Style::default();
/// a.set_fill(manim_color::RED, 0.0);
/// let mut b = Style::default();
/// b.set_fill(manim_color::RED, 1.0);
/// assert!((lerp_style(&a, &b, 0.25).fill_opacity - 0.25).abs() < 1e-6);
/// ```
pub fn lerp_style(a: &Style, b: &Style, t: f32) -> Style {
    let mix = |x: Option<manim_color::Color>, y: Option<manim_color::Color>| match (x, y) {
        (Some(x), Some(y)) => Some(x.interpolate(&y, t)),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        (None, None) => None,
    };
    Style {
        fill_color: mix(a.fill_color, b.fill_color),
        fill_opacity: lerp_scalar(a.fill_opacity, b.fill_opacity, t),
        stroke_color: mix(a.stroke_color, b.stroke_color),
        stroke_opacity: lerp_scalar(a.stroke_opacity, b.stroke_opacity, t),
        stroke_width: lerp_scalar(a.stroke_width, b.stroke_width, t),
        dash_pattern: b.dash_pattern.clone().or_else(|| a.dash_pattern.clone()),
    }
}

/// A per-mobject start→end pair, with paths already aligned for interpolation.
///
/// This is the workhorse behind [`Transform`](crate::animations::Transform),
/// the fades, and `.animate()`: capture family start data, compute end data,
/// then [`build`](FamilyMorph::build) once and [`apply`](FamilyMorph::apply) at
/// each `alpha`.
pub struct FamilyMorph {
    entries: Vec<MorphEntry>,
    path_fn: Option<PathFn>,
}

struct MorphEntry {
    id: AnyId,
    start: MobjectData,
    end: MobjectData,
}

/// A transform path function `(start, end, alpha) → point`, used to move each
/// control point along a curve (e.g. an arc) instead of a straight line. Port
/// of manim CE's `path_func`. See [`crate::animations::paths`].
pub type PathFn = std::sync::Arc<dyn Fn(Point, Point, f32) -> Point + Send + Sync>;

impl FamilyMorph {
    /// Aligns each `(id, start, end)` triple's paths and stores them for
    /// interpolation. Entries are matched positionally between `start` and
    /// `end`.
    pub fn build(start: Vec<(AnyId, MobjectData)>, end: Vec<(AnyId, MobjectData)>) -> Self {
        let entries = start
            .into_iter()
            .zip(end)
            .map(|((id, mut sd), (_, mut ed))| {
                sd.path.align_with(&mut ed.path);
                MorphEntry {
                    id,
                    start: sd,
                    end: ed,
                }
            })
            .collect();
        Self {
            entries,
            path_fn: None,
        }
    }

    /// Sets a transform path function, moving control points along its curve
    /// rather than straight lines (manim's `path_func`).
    pub fn with_path_fn(mut self, path_fn: Option<PathFn>) -> Self {
        self.path_fn = path_fn;
        self
    }

    /// Interpolates every entry into `state` at progress `alpha`.
    pub fn apply(&self, state: &mut SceneState, alpha: f32) {
        for e in &self.entries {
            if !state.contains(e.id) {
                continue;
            }
            let data = state.get_dyn_mut(e.id).data_mut();
            data.path = match &self.path_fn {
                Some(pf) => lerp_path_with(&e.start.path, &e.end.path, alpha, pf),
                None => lerp_aligned_path(&e.start.path, &e.end.path, alpha),
            };
            data.style = lerp_style(&e.start.style, &e.end.style, alpha);
            data.bump_generation();
        }
    }
}

/// Like [`lerp_aligned_path`], but each control point is moved by `path_fn`.
fn lerp_path_with(a: &Path, b: &Path, t: f32, path_fn: &PathFn) -> Path {
    let subpaths = a
        .subpaths
        .iter()
        .zip(&b.subpaths)
        .map(|(sa, sb)| {
            let curves = sa
                .curves
                .iter()
                .zip(&sb.curves)
                .map(|(ca, cb)| manim_math::bezier::CubicBezier {
                    p0: path_fn(ca.p0, cb.p0, t),
                    p1: path_fn(ca.p1, cb.p1, t),
                    p2: path_fn(ca.p2, cb.p2, t),
                    p3: path_fn(ca.p3, cb.p3, t),
                })
                .collect();
            manim_math::path::SubPath {
                curves,
                closed: sa.closed,
            }
        })
        .collect();
    Path { subpaths }
}

/// Collects `(id, data-clone)` for every member of `id`'s family.
pub(crate) fn family_data(state: &SceneState, id: AnyId) -> Vec<(AnyId, MobjectData)> {
    state
        .family(id)
        .into_iter()
        .map(|m| (m, state.get_dyn(m).data().clone()))
        .collect()
}

/// Builds a [`FamilyMorph`] from `id`'s current family state (start) to the
/// state after applying `f` to a throwaway clone (end).
///
/// This is how position/scale/style animations are expressed: describe the end
/// with ordinary scene mutations, and the morph interpolates toward it.
pub(crate) fn morph_between(
    state: &SceneState,
    id: AnyId,
    f: impl FnOnce(&mut SceneState),
) -> FamilyMorph {
    let start = family_data(state, id);
    let mut clone = state.clone();
    f(&mut clone);
    let end = family_data(&clone, id);
    FamilyMorph::build(start, end)
}

/// Builds a [`FamilyMorph`] whose *start* is the state after applying `f` to a
/// clone and whose *end* is the current state — the reverse of
/// [`morph_between`], used by entrance animations like `FadeIn`.
pub(crate) fn morph_from(
    state: &SceneState,
    id: AnyId,
    f: impl FnOnce(&mut SceneState),
) -> FamilyMorph {
    let end = family_data(state, id);
    let mut clone = state.clone();
    f(&mut clone);
    let start = family_data(&clone, id);
    FamilyMorph::build(start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Circle;

    #[test]
    fn into_animations_flattens_tuple() {
        use crate::animations::{Create, FadeIn};
        let mut scene = SceneState::new();
        let a = scene.add(Circle::new());
        let b = scene.add(Circle::new());
        let anims = (Create::new(a), FadeIn::new(b)).into_animations();
        assert_eq!(anims.len(), 2);
    }

    #[test]
    fn style_lerp_endpoints() {
        let mut a = Style::default();
        a.set_fill(manim_color::RED, 0.0);
        let mut b = Style::default();
        b.set_fill(manim_color::RED, 1.0);
        assert!((lerp_style(&a, &b, 0.0).fill_opacity).abs() < 1e-6);
        assert!((lerp_style(&a, &b, 1.0).fill_opacity - 1.0).abs() < 1e-6);
    }
}
