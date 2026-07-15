//! Movement and rotation animations: [`Shift`], [`MoveTo`], [`Rotate`],
//! [`Rotating`], and [`MoveAlongPath`].

use manim_math::path::Path;
use manim_math::rate_functions::RateFn;
use manim_math::space_ops::rotation_matrix;
use manim_math::{Point, OUT, TAU};

use crate::animation::AnimConfig;
use crate::animation::{
    anim_builders, anim_config_accessors, family_data, morph_between, Animation, FamilyMorph,
};
use crate::mobject::{AnyId, MobjectData};
use crate::scene_state::SceneState;

/// Translates a mobject by `delta`. Port of manim's `.animate.shift`, as a
/// standalone animation.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Shift;
/// use manim_math::RIGHT;
/// let mut scene = Scene::new(Config::default());
/// let c = scene.add(Circle::new());
/// scene.play(Shift::new(c, 3.0 * RIGHT)).unwrap();
/// assert!((scene[c].get_center() - 3.0 * RIGHT).length() < 1e-4);
/// ```
pub struct Shift {
    id: AnyId,
    delta: Point,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
}
anim_builders!(Shift);

impl Shift {
    /// Shifts `id`'s family by `delta`.
    pub fn new(id: impl Into<AnyId>, delta: Point) -> Self {
        Self {
            id: id.into(),
            delta,
            config: AnimConfig::default(),
            morph: None,
        }
    }
}

impl Animation for Shift {
    fn begin(&mut self, state: &mut SceneState) {
        let (id, delta) = (self.id, self.delta);
        self.morph = Some(morph_between(state, id, |s| s.shift(id, delta)));
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(m) = &self.morph {
            m.apply(state, alpha);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    anim_config_accessors!();
}

/// Moves a mobject so its center lands on `point`. Port of manim's
/// `.animate.move_to`, as a standalone animation.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::MoveTo;
/// use manim_math::UP;
/// let mut scene = Scene::new(Config::default());
/// let c = scene.add(Circle::new());
/// scene.play(MoveTo::new(c, 2.0 * UP)).unwrap();
/// assert!((scene[c].get_center() - 2.0 * UP).length() < 1e-4);
/// ```
pub struct MoveTo {
    id: AnyId,
    point: Point,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
}
anim_builders!(MoveTo);

impl MoveTo {
    /// Moves `id`'s family so its center reaches `point`.
    pub fn new(id: impl Into<AnyId>, point: Point) -> Self {
        Self {
            id: id.into(),
            point,
            config: AnimConfig::default(),
            morph: None,
        }
    }
}

impl Animation for MoveTo {
    fn begin(&mut self, state: &mut SceneState) {
        let (id, point) = (self.id, self.point);
        self.morph = Some(morph_between(state, id, |s| s.move_to(id, point)));
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(m) = &self.morph {
            m.apply(state, alpha);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    anim_config_accessors!();
}

/// Rotates a mobject by `angle` radians about a pivot (its center by default).
/// Port of manim CE's `Rotate`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Rotate;
/// use manim_math::{TAU, Point, RIGHT};
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// // A quarter turn about the origin.
/// scene.play(Rotate::new(sq, TAU / 4.0).about_point(Point::ZERO)).unwrap();
/// assert!((scene[sq].bounding_box().width() - 2.0).abs() < 1e-3);
/// ```
pub struct Rotate {
    id: AnyId,
    angle: f32,
    about_point: Option<Point>,
    config: AnimConfig,
    start: Vec<(AnyId, MobjectData)>,
    pivot: Point,
}
anim_builders!(Rotate);

impl Rotate {
    /// Rotates `id` by `angle` radians about its center.
    pub fn new(id: impl Into<AnyId>, angle: f32) -> Self {
        Self {
            id: id.into(),
            angle,
            about_point: None,
            config: AnimConfig::default(),
            start: Vec::new(),
            pivot: Point::ZERO,
        }
    }

    /// Sets an explicit pivot point (manim's `about_point`).
    pub fn about_point(mut self, point: Point) -> Self {
        self.about_point = Some(point);
        self
    }
}

/// Rotates a path clone by `angle` about `pivot`.
fn rotated(path: &Path, angle: f32, pivot: Point) -> Path {
    let m = rotation_matrix(angle, OUT);
    let mut p = path.clone();
    p.apply(|q| pivot + m * (q - pivot));
    p
}

impl Animation for Rotate {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = family_data(state, self.id);
        self.pivot = self
            .about_point
            .unwrap_or_else(|| state.family_bounding_box(self.id).center());
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        for (id, data) in &self.start {
            if state.contains(*id) {
                let out = state.get_dyn_mut(*id).data_mut();
                out.path = rotated(&data.path, self.angle * alpha, self.pivot);
                out.bump_generation();
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    anim_config_accessors!();
}

/// Continuous rotation, defaulting to a full turn over 5 s with linear timing.
/// Port of manim CE's `Rotating`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Rotating;
/// use manim_math::TAU;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// scene.play(Rotating::new(sq).angle(TAU)).unwrap();
/// // A full turn returns a square to its footprint.
/// assert!((scene[sq].bounding_box().width() - 2.0).abs() < 1e-3);
/// ```
pub struct Rotating {
    inner: Rotate,
}

impl Rotating {
    /// A rotation of `id` by a full turn (`TAU`) over 5 s, linear.
    pub fn new(id: impl Into<AnyId>) -> Self {
        let inner = Rotate::new(id, TAU).run_time(5.0).rate_fn(RateFn::Linear);
        Self { inner }
    }

    /// Sets the total rotation angle in radians.
    pub fn angle(mut self, angle: f32) -> Self {
        self.inner.angle = angle;
        self
    }

    /// Sets an explicit pivot point.
    pub fn about_point(mut self, point: Point) -> Self {
        self.inner = self.inner.about_point(point);
        self
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

impl Animation for Rotating {
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
        Animation::rate_fn(&self.inner)
    }
}

/// Moves a mobject so its center travels along a given [`Path`]. Port of manim
/// CE's `MoveAlongPath`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::MoveAlongPath;
/// use manim_math::path::Path;
/// use manim_math::{Point, RIGHT};
/// let mut scene = Scene::new(Config::default());
/// let d = scene.add(Dot::new());
/// let track = Path::from_corners(&[Point::ZERO, 4.0 * RIGHT], false);
/// scene.play(MoveAlongPath::new(d, track)).unwrap();
/// assert!((scene[d].get_center() - 4.0 * RIGHT).length() < 1e-3);
/// ```
pub struct MoveAlongPath {
    id: AnyId,
    path: Path,
    config: AnimConfig,
    start: Vec<(AnyId, MobjectData)>,
    base: Point,
}
anim_builders!(MoveAlongPath);

impl MoveAlongPath {
    /// Moves `id` along `path`.
    pub fn new(id: impl Into<AnyId>, path: Path) -> Self {
        Self {
            id: id.into(),
            path,
            config: AnimConfig::default(),
            start: Vec::new(),
            base: Point::ZERO,
        }
    }
}

impl Animation for MoveAlongPath {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = family_data(state, self.id);
        self.base = self.path.point_from_proportion(0.0);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let shift = self.path.point_from_proportion(alpha.clamp(0.0, 1.0)) - self.base;
        for (id, data) in &self.start {
            if state.contains(*id) {
                let out = state.get_dyn_mut(*id).data_mut();
                out.path = data.path.clone();
                out.path.apply(|p| p + shift);
                out.bump_generation();
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    anim_config_accessors!();
}
