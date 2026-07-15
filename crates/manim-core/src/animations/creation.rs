//! Creation animations: [`Create`], [`Uncreate`], [`DrawBorderThenFill`],
//! [`ShowIncreasingSubsets`], and [`ShowSubmobjectsOneByOne`].

use manim_math::path::Path;

use crate::animation::AnimConfig;
use crate::animation::{anim_builders, anim_config_accessors, Animation};
use crate::mobject::{AnyId, MobjectData};
use crate::scene_state::SceneState;

/// The portion of `path` between arc-length proportions `a` and `b`, taken
/// per-subpath so each outline is drawn to the same proportion.
fn partial_path(path: &Path, a: f32, b: f32) -> Path {
    let subpaths = path
        .subpaths
        .iter()
        .filter(|s| !s.curves.is_empty())
        .filter_map(|s| {
            let whole = Path {
                subpaths: vec![s.clone()],
            };
            whole.get_subcurve(a, b).subpaths.into_iter().next()
        })
        .collect();
    Path { subpaths }
}

/// Snapshots the full path of every family member with geometry.
fn full_paths(state: &SceneState, id: AnyId) -> Vec<(AnyId, Path)> {
    state
        .family(id)
        .into_iter()
        .filter_map(|m| {
            let path = &state.get_dyn(m).data().path;
            if path.subpaths.iter().all(|s| s.curves.is_empty()) {
                None
            } else {
                Some((m, path.clone()))
            }
        })
        .collect()
}

/// Draws a mobject by progressively tracing its outline. Port of manim CE's
/// `Create`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Create;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// scene.play(Create::new(sq)).unwrap();
/// // Fully drawn at the end (perimeter 8 for a side-2 square).
/// let len: f32 = scene[sq].data().path.subpaths.iter()
///     .map(|s| s.arc_length()).sum();
/// assert!((len - 8.0).abs() < 1e-2);
/// ```
pub struct Create {
    id: AnyId,
    config: AnimConfig,
    full: Vec<(AnyId, Path)>,
}
anim_builders!(Create);

impl Create {
    /// Creates (draws in) `id`.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            config: AnimConfig::default(),
            full: Vec::new(),
        }
    }
}

impl Animation for Create {
    fn begin(&mut self, state: &mut SceneState) {
        self.full = full_paths(state, self.id);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        for (id, full) in &self.full {
            if state.contains(*id) {
                let data = state.get_dyn_mut(*id).data_mut();
                data.path = partial_path(full, 0.0, alpha.clamp(0.0, 1.0));
                data.bump_generation();
            }
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
    anim_config_accessors!();
}

/// Erases a mobject by progressively un-tracing its outline, then hiding it.
/// Port of manim CE's `Uncreate`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Uncreate;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// scene.play(Uncreate::new(sq)).unwrap();
/// // Hidden at the end.
/// assert!(scene.display_list().is_empty());
/// ```
pub struct Uncreate {
    id: AnyId,
    config: AnimConfig,
    full: Vec<(AnyId, Path)>,
}
anim_builders!(Uncreate);

impl Uncreate {
    /// Un-creates (erases) `id`.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            config: AnimConfig::default(),
            full: Vec::new(),
        }
    }
}

impl Animation for Uncreate {
    fn begin(&mut self, state: &mut SceneState) {
        self.full = full_paths(state, self.id);
        state.set_visible(self.id, true);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        for (id, full) in &self.full {
            if state.contains(*id) {
                let data = state.get_dyn_mut(*id).data_mut();
                data.path = partial_path(full, 0.0, (1.0 - alpha).clamp(0.0, 1.0));
                data.bump_generation();
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        state.set_visible(self.id, false);
    }
    anim_config_accessors!();
}

/// Draws a mobject's border in the first half, then fills it in the second.
/// Port of manim CE's `DrawBorderThenFill`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::DrawBorderThenFill;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new().with_fill(BLUE, 0.8));
/// scene.play(DrawBorderThenFill::new(sq)).unwrap();
/// // Ends at its full fill opacity.
/// assert!((scene[sq].data().style.fill_opacity - 0.8).abs() < 1e-6);
/// ```
pub struct DrawBorderThenFill {
    id: AnyId,
    config: AnimConfig,
    full: Vec<(AnyId, MobjectData)>,
}
anim_builders!(DrawBorderThenFill);

impl DrawBorderThenFill {
    /// Draws the border of `id`, then fills it.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            config: AnimConfig::default(),
            full: Vec::new(),
        }
    }
}

impl Animation for DrawBorderThenFill {
    fn begin(&mut self, state: &mut SceneState) {
        self.full = state
            .family(self.id)
            .into_iter()
            .filter(|m| {
                !state
                    .get_dyn(*m)
                    .data()
                    .path
                    .subpaths
                    .iter()
                    .all(|s| s.curves.is_empty())
            })
            .map(|m| (m, state.get_dyn(m).data().clone()))
            .collect();
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let alpha = alpha.clamp(0.0, 1.0);
        for (id, full) in &self.full {
            if !state.contains(*id) {
                continue;
            }
            let target_fill = full.style.fill_opacity;
            let data = state.get_dyn_mut(*id).data_mut();
            if alpha < 0.5 {
                data.path = partial_path(&full.path, 0.0, alpha * 2.0);
                data.style.fill_opacity = 0.0;
            } else {
                data.path = full.path.clone();
                data.style.fill_opacity = target_fill * (alpha - 0.5) * 2.0;
            }
            data.bump_generation();
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        for (id, full) in &self.full {
            if state.contains(*id) {
                *state.get_dyn_mut(*id).data_mut() = full.clone();
            }
        }
    }
    anim_config_accessors!();
}

/// Reveals a group's submobjects one cumulative subset at a time. Port of manim
/// CE's `ShowIncreasingSubsets`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::ShowIncreasingSubsets;
/// let mut scene = Scene::new(Config::default());
/// let a = scene.add(Circle::new());
/// let b = scene.add(Square::new());
/// let g = VGroup::of(scene.state_mut(), [a.erase(), b.erase()]);
/// scene.play(ShowIncreasingSubsets::new(g)).unwrap();
/// // Both children visible at the end.
/// assert_eq!(scene.display_list().len(), 2);
/// ```
pub struct ShowIncreasingSubsets {
    id: AnyId,
    config: AnimConfig,
    children: Vec<AnyId>,
}
anim_builders!(ShowIncreasingSubsets);

impl ShowIncreasingSubsets {
    /// Reveals the submobjects of `id` one at a time.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            config: AnimConfig::default(),
            children: Vec::new(),
        }
    }
}

impl Animation for ShowIncreasingSubsets {
    fn begin(&mut self, state: &mut SceneState) {
        self.children = state.get_dyn(self.id).data().children.clone();
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let n = self.children.len();
        let k = ((alpha.clamp(0.0, 1.0) * n as f32).ceil() as usize).min(n);
        for (i, child) in self.children.iter().enumerate() {
            state.set_visible(*child, i < k);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        for child in &self.children {
            state.set_visible(*child, true);
        }
    }
    anim_config_accessors!();
}

/// Flips through a group's submobjects one at a time. Port of manim CE's
/// `ShowSubmobjectsOneByOne`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::ShowSubmobjectsOneByOne;
/// let mut scene = Scene::new(Config::default());
/// let a = scene.add(Circle::new());
/// let b = scene.add(Square::new());
/// let g = VGroup::of(scene.state_mut(), [a.erase(), b.erase()]);
/// scene.play(ShowSubmobjectsOneByOne::new(g)).unwrap();
/// // Ends showing just the last child.
/// assert_eq!(scene.display_list().len(), 1);
/// ```
pub struct ShowSubmobjectsOneByOne {
    id: AnyId,
    config: AnimConfig,
    children: Vec<AnyId>,
}
anim_builders!(ShowSubmobjectsOneByOne);

impl ShowSubmobjectsOneByOne {
    /// Flips through the submobjects of `id` one at a time.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            config: AnimConfig::default(),
            children: Vec::new(),
        }
    }
}

impl Animation for ShowSubmobjectsOneByOne {
    fn begin(&mut self, state: &mut SceneState) {
        self.children = state.get_dyn(self.id).data().children.clone();
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let n = self.children.len();
        if n == 0 {
            return;
        }
        let idx = ((alpha.clamp(0.0, 1.0) * n as f32).floor() as usize).min(n - 1);
        for (i, child) in self.children.iter().enumerate() {
            state.set_visible(*child, i == idx);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        let n = self.children.len();
        for (i, child) in self.children.iter().enumerate() {
            state.set_visible(*child, i + 1 == n);
        }
    }
    anim_config_accessors!();
}
