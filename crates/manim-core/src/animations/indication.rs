//! Indication and "long-tail" animations: [`Indicate`], [`Flash`], [`FocusOn`],
//! [`Circumscribe`], [`Wiggle`], [`ApplyWave`], [`ShowPassingFlash`], and
//! [`ChangeSpeed`].
//!
//! Animations that need helper geometry add it in `begin` and remove it in
//! `finish`, so timeline snapshots taken between segments stay clean.

use manim_color::{Color, YELLOW};
use manim_math::path::Path;
use manim_math::rate_functions::{there_and_back, RateFn};
use manim_math::{Point, TAU, UP};

use crate::animation::AnimConfig;
use crate::animation::{
    anim_builders, anim_config_accessors, family_data, morph_between, Animation, FamilyMorph,
};
use crate::geometry::{Circle, Line, Rectangle};
use crate::mobject::{AnyId, Mobject, MobjectData, MobjectExt};
use crate::scene_state::SceneState;

/// The visible window `[lo, hi]` of each subpath, taken per-subpath.
fn window_path(full: &Path, lo: f32, hi: f32) -> Path {
    if hi <= lo {
        return Path::default();
    }
    let subpaths = full
        .subpaths
        .iter()
        .filter(|s| !s.curves.is_empty())
        .filter_map(|s| {
            let whole = Path {
                subpaths: vec![s.clone()],
            };
            whole.get_subcurve(lo, hi).subpaths.into_iter().next()
        })
        .collect();
    Path { subpaths }
}

/// Briefly scales a mobject up and tints it, then returns it to normal. Port of
/// manim CE's `Indicate` (scale 1.2, color `YELLOW`, there-and-back).
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Indicate;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// scene.play(Indicate::new(sq)).unwrap();
/// // Ends back at its original size and color.
/// assert!((scene[sq].bounding_box().width() - 2.0).abs() < 1e-3);
/// ```
pub struct Indicate {
    id: AnyId,
    color: Color,
    scale_factor: f32,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
}

impl Indicate {
    /// Indicates `id` with the manim defaults.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            color: YELLOW,
            scale_factor: 1.2,
            config: AnimConfig {
                rate_fn: RateFn::ThereAndBack,
                ..AnimConfig::default()
            },
            morph: None,
        }
    }

    /// Sets the indication color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the peak scale factor.
    pub fn scale_factor(mut self, scale_factor: f32) -> Self {
        self.scale_factor = scale_factor;
        self
    }

    /// Sets the run time in seconds.
    pub fn run_time(mut self, run_time: f32) -> Self {
        self.config.run_time = run_time;
        self
    }
}

impl Animation for Indicate {
    fn begin(&mut self, state: &mut SceneState) {
        let (id, color, factor) = (self.id, self.color, self.scale_factor);
        self.morph = Some(morph_between(state, id, |s| {
            s.scale(id, factor);
            s.set_style_family(id, |st| {
                st.set_color(color);
            });
        }));
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(m) = &self.morph {
            m.apply(state, alpha);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        // there_and_back(1) == 0, so end state is the original.
        if let Some(m) = &self.morph {
            m.apply(state, 0.0);
        }
    }
    anim_config_accessors!();
}

/// Radiates fading lines outward from a point. Port of manim CE's `Flash`. The
/// helper lines are temporary — added in `begin`, removed in `finish`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Flash;
/// let mut scene = Scene::new(Config::default());
/// let _ = scene.add(Circle::new());
/// scene.play(Flash::new(manim_math::ORIGIN)).unwrap();
/// // The flash leaves no residue: only the circle remains.
/// assert_eq!(scene.display_list().len(), 1);
/// ```
pub struct Flash {
    point: Point,
    color: Color,
    num_lines: usize,
    line_length: f32,
    flash_radius: f32,
    config: AnimConfig,
    temp: Vec<AnyId>,
}
anim_builders!(Flash);

impl Flash {
    /// A flash centered at `point` with the manim defaults.
    pub fn new(point: Point) -> Self {
        Self {
            point,
            color: YELLOW,
            num_lines: 12,
            line_length: 0.2,
            flash_radius: 0.1,
            config: AnimConfig::default(),
            temp: Vec::new(),
        }
    }

    /// Sets the flash color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Animation for Flash {
    fn begin(&mut self, state: &mut SceneState) {
        self.temp.clear();
        for k in 0..self.num_lines {
            let angle = k as f32 * TAU / self.num_lines as f32;
            let dir = Point::new(angle.cos(), angle.sin(), 0.0);
            let inner = self.point + dir * self.flash_radius;
            let outer = self.point + dir * (self.flash_radius + self.line_length);
            let mut line = Line::new(inner, outer);
            line.set_stroke(self.color, 4.0, 1.0);
            self.temp.push(state.add(line).erase());
        }
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let opacity = (1.0 - alpha).clamp(0.0, 1.0);
        for id in &self.temp {
            if state.contains(*id) {
                state.get_dyn_mut(*id).data_mut().style.stroke_opacity = opacity;
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        for id in self.temp.drain(..) {
            state.remove(id);
        }
    }
    anim_config_accessors!();
}

/// A translucent spotlight that shrinks onto a point. Port of manim CE's
/// `FocusOn`. The overlay is temporary.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::FocusOn;
/// let mut scene = Scene::new(Config::default());
/// let _ = scene.add(Circle::new());
/// scene.play(FocusOn::new(manim_math::ORIGIN)).unwrap();
/// assert_eq!(scene.display_list().len(), 1); // overlay removed
/// ```
pub struct FocusOn {
    point: Point,
    color: Color,
    opacity: f32,
    start_radius: f32,
    config: AnimConfig,
    temp: Option<AnyId>,
}
anim_builders!(FocusOn);

impl FocusOn {
    /// Focuses on `point` with the manim defaults.
    pub fn new(point: Point) -> Self {
        Self {
            point,
            color: Color::from_srgb(0.5, 0.5, 0.5),
            opacity: 0.2,
            start_radius: 2.0,
            config: AnimConfig::default(),
            temp: None,
        }
    }
}

impl Animation for FocusOn {
    fn begin(&mut self, state: &mut SceneState) {
        let mut circle = Circle::new().radius(self.start_radius);
        circle.set_fill(self.color, self.opacity);
        circle.set_stroke(self.color, 0.0, 0.0);
        circle.move_to(self.point);
        self.temp = Some(state.add(circle).erase());
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(id) = self.temp {
            if state.contains(id) {
                // Rebuild from a fresh full-size circle each frame, then shrink.
                let factor = 1.0 + (0.01 - 1.0) * alpha.clamp(0.0, 1.0);
                let mut circle = Circle::new().radius(self.start_radius);
                circle.set_fill(self.color, self.opacity);
                circle.set_stroke(self.color, 0.0, 0.0);
                circle.move_to(self.point);
                circle.scale_about(factor, self.point);
                *state.get_dyn_mut(id).data_mut() = circle.data().clone();
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        if let Some(id) = self.temp.take() {
            state.remove(id);
        }
    }
    anim_config_accessors!();
}

/// Draws a shape around a mobject, then fades it. Port of manim CE's
/// `Circumscribe`. The surrounding shape is temporary.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Circumscribe;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// scene.play(Circumscribe::new(sq)).unwrap();
/// assert_eq!(scene.display_list().len(), 1); // frame removed
/// ```
pub struct Circumscribe {
    id: AnyId,
    color: Color,
    buff: f32,
    config: AnimConfig,
    temp: Option<AnyId>,
}
anim_builders!(Circumscribe);

impl Circumscribe {
    /// Circumscribes `id` with the manim defaults.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            color: YELLOW,
            buff: 0.1,
            config: AnimConfig::default(),
            temp: None,
        }
    }

    /// Sets the surrounding-shape color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Animation for Circumscribe {
    fn begin(&mut self, state: &mut SceneState) {
        let bb = state.family_bounding_box(self.id);
        let w = bb.width() + 2.0 * self.buff;
        let h = bb.height() + 2.0 * self.buff;
        let mut rect = Rectangle::with_size(w, h);
        rect.set_stroke(self.color, 4.0, 1.0);
        rect.move_to(bb.center());
        self.temp = Some(state.add(rect).erase());
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(id) = self.temp {
            if state.contains(id) {
                // Triangular envelope: appear then disappear.
                let a = alpha.clamp(0.0, 1.0);
                let opacity = if a < 0.5 { a * 2.0 } else { (1.0 - a) * 2.0 };
                state.get_dyn_mut(id).data_mut().style.stroke_opacity = opacity;
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        if let Some(id) = self.temp.take() {
            state.remove(id);
        }
    }
    anim_config_accessors!();
}

/// Wiggles a mobject with a small scale-and-rotate oscillation. Port of manim
/// CE's `Wiggle`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Wiggle;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// scene.play(Wiggle::new(sq)).unwrap();
/// // Returns to its original footprint.
/// assert!((scene[sq].bounding_box().width() - 2.0).abs() < 1e-3);
/// ```
pub struct Wiggle {
    id: AnyId,
    scale_value: f32,
    rotation_angle: f32,
    n_wiggles: f32,
    config: AnimConfig,
    start: Vec<(AnyId, MobjectData)>,
    center: Point,
}

impl Wiggle {
    /// Wiggles `id` with the manim defaults.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            scale_value: 1.1,
            rotation_angle: 0.01 * TAU,
            n_wiggles: 6.0,
            config: AnimConfig {
                rate_fn: RateFn::Linear,
                ..AnimConfig::default()
            },
            start: Vec::new(),
            center: Point::ZERO,
        }
    }

    /// Sets the run time in seconds.
    pub fn run_time(mut self, run_time: f32) -> Self {
        self.config.run_time = run_time;
        self
    }
}

impl Animation for Wiggle {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = family_data(state, self.id);
        self.center = state.family_bounding_box(self.id).center();
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let env = there_and_back(alpha.clamp(0.0, 1.0));
        let scale_f = 1.0 + (self.scale_value - 1.0) * env;
        let rot = self.rotation_angle * env * (alpha * self.n_wiggles * TAU).sin();
        let m = manim_math::space_ops::rotation_matrix(rot, manim_math::OUT);
        let center = self.center;
        for (id, data) in &self.start {
            if state.contains(*id) {
                let out = state.get_dyn_mut(*id).data_mut();
                out.path = data.path.clone();
                out.path.apply(|p| center + (m * (p - center)) * scale_f);
                out.bump_generation();
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    anim_config_accessors!();
}

/// Ripples a sinusoidal wave through a mobject (an x-dependent y displacement),
/// then settles. Port of manim CE's `ApplyWave`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::ApplyWave;
/// let mut scene = Scene::new(Config::default());
/// let line = scene.add(Line::new(manim_math::ORIGIN, 4.0 * RIGHT));
/// scene.play(ApplyWave::new(line)).unwrap();
/// // Settles back to the flat line.
/// assert!(scene[line].get_center().y.abs() < 1e-3);
/// ```
pub struct ApplyWave {
    id: AnyId,
    amplitude: f32,
    frequency: f32,
    config: AnimConfig,
    start: Vec<(AnyId, MobjectData)>,
}
anim_builders!(ApplyWave);

impl ApplyWave {
    /// Applies a wave to `id` with the manim defaults.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            amplitude: 0.2,
            frequency: 2.0,
            config: AnimConfig::default(),
            start: Vec::new(),
        }
    }
}

impl Animation for ApplyWave {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = family_data(state, self.id);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let env = there_and_back(alpha.clamp(0.0, 1.0));
        let (amp, freq) = (self.amplitude, self.frequency);
        for (id, data) in &self.start {
            if state.contains(*id) {
                let out = state.get_dyn_mut(*id).data_mut();
                out.path = data.path.clone();
                out.path
                    .apply(|p| p + UP * (amp * (freq * p.x).sin() * env));
                out.bump_generation();
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    anim_config_accessors!();
}

/// Sweeps a short glowing window of stroke along a mobject's outline. Port of
/// manim CE's `ShowPassingFlash`. Restores the full outline at the end.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::ShowPassingFlash;
/// let mut scene = Scene::new(Config::low());
/// let sq = scene.add(Square::new());
/// scene.play(ShowPassingFlash::new(sq)).unwrap();
/// // The full square is restored at the end.
/// let len: f32 = scene[sq].data().path.subpaths.iter()
///     .map(|s| s.arc_length()).sum();
/// assert!((len - 8.0).abs() < 1e-2);
/// ```
pub struct ShowPassingFlash {
    id: AnyId,
    time_width: f32,
    config: AnimConfig,
    full: Vec<(AnyId, Path)>,
}
anim_builders!(ShowPassingFlash);

impl ShowPassingFlash {
    /// A passing flash along `id` with the manim default window (0.1).
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            time_width: 0.1,
            config: AnimConfig::default(),
            full: Vec::new(),
        }
    }

    /// Sets the fraction of the path lit at once.
    pub fn time_width(mut self, time_width: f32) -> Self {
        self.time_width = time_width.clamp(0.0, 1.0);
        self
    }
}

impl Animation for ShowPassingFlash {
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
        let tw = self.time_width;
        let upper = (alpha * (1.0 + tw)).clamp(0.0, 1.0);
        let lower = (alpha * (1.0 + tw) - tw).clamp(0.0, 1.0);
        for (id, full) in &self.full {
            if state.contains(*id) {
                let data = state.get_dyn_mut(*id).data_mut();
                data.path = window_path(full, lower, upper);
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

/// Plays another animation at a constant speed multiple by scaling its run
/// time. Port of manim CE's `ChangeSpeed` (constant-factor case).
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::{ChangeSpeed, FadeIn};
/// let mut scene = Scene::new(Config::default());
/// let c = scene.add(Circle::new().with_fill(BLUE, 1.0));
/// // Play a 1 s fade twice as fast → 0.5 s.
/// scene.play(ChangeSpeed::new(FadeIn::new(c), 2.0)).unwrap();
/// assert!((scene.total_duration() - 0.5).abs() < 1e-6);
/// ```
pub struct ChangeSpeed {
    inner: Box<dyn Animation>,
    factor: f32,
}

impl ChangeSpeed {
    /// Plays `anim` at `factor`× speed (values > 1 are faster).
    pub fn new(anim: impl Animation, factor: f32) -> Self {
        Self {
            inner: Box::new(anim),
            factor: factor.max(1e-3),
        }
    }
}

impl Animation for ChangeSpeed {
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
        self.inner.duration() / self.factor
    }
    fn rate_fn(&self) -> RateFn {
        self.inner.rate_fn()
    }
}
