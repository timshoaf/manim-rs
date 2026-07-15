//! Creation animations: [`Create`], [`Uncreate`], [`DrawBorderThenFill`],
//! [`ShowIncreasingSubsets`], and [`ShowSubmobjectsOneByOne`].

use manim_color::{Color, WHITE};
use manim_math::path::Path;

use crate::animation::AnimConfig;
use crate::animation::{anim_builders, anim_config_accessors, Animation};
use crate::mobject::{AnyId, MobjectData};
use crate::scene_state::SceneState;
use crate::style::Style;

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

/// CE's default temporary-border stroke width (in manim "stroke points") for
/// [`DrawBorderThenFill`] and [`Write`](../../manim_text/struct.Write.html).
pub const DEFAULT_BORDER_WIDTH: f32 = 2.0;

/// The default temporary-border color: the mobject's own fill color, falling
/// back to its stroke color, then white. Mirrors CE's `DrawBorderThenFill`,
/// whose `stroke_color` parameter defaults to the mobject's fill color.
pub fn default_border_color(style: &Style) -> Color {
    style.fill_color.or(style.stroke_color).unwrap_or(WHITE)
}

/// Applies the CE "draw border, then fill" look to one mobject at local progress
/// `t ∈ [0, 1]`, given its target style/path in `full`.
///
/// - **Trace phase** (`t ≤ 0.5`): the outline is traced to proportion `2t` as a
///   thin *temporary* stroke (`border_color` / `border_width`, fully opaque) with
///   **no fill** — even for strokeless mobjects (text glyphs, fill-only shapes),
///   which is exactly what CE synthesizes.
/// - **Fill phase** (`t > 0.5`): the full path is shown, the fill opacity ramps
///   `0 → target`, and the temporary stroke crossfades (color, width, opacity) to
///   the mobject's *real* target stroke — which for a strokeless target means the
///   border simply fades away.
///
/// This is the single source of truth shared by [`DrawBorderThenFill`] and the
/// per-glyph `Write` animation, so both read identically.
pub fn border_then_fill_frame(
    data: &mut MobjectData,
    full: &MobjectData,
    t: f32,
    border_width: f32,
    border_color: Color,
) {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.5 {
        // Trace the outline as the temporary border stroke, no fill yet.
        let drawn = t * 2.0;
        data.path = if drawn <= 0.0 {
            Path::default()
        } else {
            partial_path(&full.path, 0.0, drawn)
        };
        data.style.fill_opacity = 0.0;
        data.style.stroke_color = Some(border_color);
        data.style.stroke_opacity = 1.0;
        data.style.stroke_width = border_width;
    } else {
        // Fill ramps up; the temporary border crossfades to the target stroke.
        let v = (t - 0.5) * 2.0;
        data.path = full.path.clone();
        data.style.fill_opacity = full.style.fill_opacity * v;

        // "Has a real stroke" means the target actually paints one (visible
        // color, width, and opacity) — not merely a stroke_color that renders to
        // nothing. A strokeless target just fades the temporary border away.
        let has_target_stroke = full.style.render_stroke().is_some();
        let target_color = if has_target_stroke {
            full.style.stroke_color.unwrap_or(border_color)
        } else {
            border_color
        };
        let target_opacity = if has_target_stroke {
            full.style.stroke_opacity
        } else {
            0.0
        };
        let target_width = if has_target_stroke {
            full.style.stroke_width
        } else {
            border_width
        };
        data.style.stroke_color = Some(border_color.interpolate(&target_color, v));
        data.style.stroke_opacity = 1.0 + (target_opacity - 1.0) * v;
        data.style.stroke_width = border_width + (target_width - border_width) * v;
    }
    data.bump_generation();
}

/// Traces a mobject's outline as a temporary stroke, then crossfades to its fill.
/// Port of manim CE's `DrawBorderThenFill`.
///
/// For a strokeless filled mobject the first half shows a thin border in the
/// fill color (there is no real stroke to draw), and the second half fades that
/// border out as the fill ramps in — the iconic manim reveal. Override the
/// temporary border with [`border_width`](Self::border_width) /
/// [`border_color`](Self::border_color).
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
    border_width: Option<f32>,
    border_color: Option<Color>,
    full: Vec<(AnyId, MobjectData)>,
}
anim_builders!(DrawBorderThenFill);

impl DrawBorderThenFill {
    /// Traces the border of `id`, then fills it.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            config: AnimConfig::default(),
            border_width: None,
            border_color: None,
            full: Vec::new(),
        }
    }

    /// Overrides the temporary border stroke width (default
    /// [`DEFAULT_BORDER_WIDTH`]).
    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = Some(width);
        self
    }

    /// Overrides the temporary border stroke color (default: the mobject's fill
    /// color — see [`default_border_color`]).
    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = Some(color);
        self
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
            let border_width = self.border_width.unwrap_or(DEFAULT_BORDER_WIDTH);
            let border_color = self
                .border_color
                .unwrap_or_else(|| default_border_color(&full.style));
            let data = state.get_dyn_mut(*id).data_mut();
            border_then_fill_frame(data, full, alpha, border_width, border_color);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Square;
    use crate::mobject::Buildable;
    use manim_color::{GREEN, RED};

    /// A filled square with no *visible* stroke — the case CE's synthesized
    /// border targets (text glyphs behave the same way).
    fn strokeless_filled_square(state: &mut SceneState) -> AnyId {
        state
            .add(Square::new().with_fill(RED, 1.0).with_stroke(RED, 0.0, 0.0))
            .erase()
    }

    #[test]
    fn dbtf_trace_phase_shows_temp_border_and_no_fill() {
        let mut state = SceneState::new();
        let id = strokeless_filled_square(&mut state);
        let mut anim = DrawBorderThenFill::new(id);
        anim.begin(&mut state);
        anim.interpolate(&mut state, 0.25);

        let style = &state.get_dyn(id).data().style;
        assert_eq!(style.fill_opacity, 0.0, "no fill during the trace phase");
        assert_eq!(style.stroke_color, Some(RED), "border takes the fill color");
        assert_eq!(style.stroke_width, DEFAULT_BORDER_WIDTH);
        assert!(
            style.render_stroke().is_some(),
            "a temporary border actually renders on the strokeless shape"
        );
    }

    #[test]
    fn dbtf_fill_phase_ramps_fill_and_fades_border() {
        let mut state = SceneState::new();
        let id = strokeless_filled_square(&mut state);
        let mut anim = DrawBorderThenFill::new(id);
        anim.begin(&mut state);
        anim.interpolate(&mut state, 0.75);

        let style = &state.get_dyn(id).data().style;
        assert!(
            style.fill_opacity > 0.0 && style.fill_opacity < 1.0,
            "fill is ramping in: {}",
            style.fill_opacity
        );
        assert!(
            style.stroke_opacity > 0.0 && style.stroke_opacity < 1.0,
            "the temporary border is fading out: {}",
            style.stroke_opacity
        );
    }

    #[test]
    fn dbtf_finish_restores_exact_target_style() {
        let mut state = SceneState::new();
        let id = strokeless_filled_square(&mut state);
        let target = state.get_dyn(id).data().style.clone();

        let mut anim = DrawBorderThenFill::new(id);
        anim.begin(&mut state);
        anim.interpolate(&mut state, 0.6);
        anim.finish(&mut state);

        let style = &state.get_dyn(id).data().style;
        assert_eq!(*style, target, "finish restores the exact target style");
        assert_eq!(style.fill_opacity, 1.0);
        assert!(style.render_stroke().is_none(), "target is strokeless");
    }

    #[test]
    fn dbtf_border_overrides_apply() {
        let mut state = SceneState::new();
        let id = strokeless_filled_square(&mut state);
        let mut anim = DrawBorderThenFill::new(id)
            .border_color(GREEN)
            .border_width(5.0);
        anim.begin(&mut state);
        anim.interpolate(&mut state, 0.2);

        let style = &state.get_dyn(id).data().style;
        assert_eq!(style.stroke_color, Some(GREEN));
        assert_eq!(style.stroke_width, 5.0);
    }
}
