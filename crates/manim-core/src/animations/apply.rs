//! Function-application animations: [`Homotopy`], [`ApplyPointwiseFunction`],
//! [`ApplyFunction`], [`ApplyMatrix`], and [`MaintainPositionRelativeTo`].

use glam::Mat3;
use manim_math::Point;

use crate::animation::AnimConfig;
use crate::animation::{anim_builders, anim_config_accessors, family_data, Animation};
use crate::mobject::{AnyId, MobjectData};
use crate::scene_state::SceneState;

/// The closure type driving [`Homotopy`]: `(point, t) → point`.
type HomotopyFn = Box<dyn Fn(Point, f32) -> Point>;

/// Continuously deforms a mobject by a homotopy `h(point, t)`. Port of manim
/// CE's `Homotopy`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Homotopy;
/// use manim_math::{Point, UP};
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// // Slide everything up over time.
/// scene.play(Homotopy::new(sq, |p, t| p + t * UP)).unwrap();
/// assert!((scene[sq].get_center() - UP).length() < 1e-4);
/// ```
pub struct Homotopy {
    id: AnyId,
    func: HomotopyFn,
    config: AnimConfig,
    start: Vec<(AnyId, MobjectData)>,
}
anim_builders!(Homotopy);

impl Homotopy {
    /// Deforms `id` by `func(point, t)`.
    pub fn new(id: impl Into<AnyId>, func: impl Fn(Point, f32) -> Point + 'static) -> Self {
        Self {
            id: id.into(),
            func: Box::new(func),
            config: AnimConfig::default(),
            start: Vec::new(),
        }
    }
}

impl Animation for Homotopy {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = family_data(state, self.id);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        for (id, data) in &self.start {
            if state.contains(*id) {
                let out = state.get_dyn_mut(*id).data_mut();
                out.path = data.path.clone();
                out.path.apply(|p| (self.func)(p, alpha));
                out.bump_generation();
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    anim_config_accessors!();
}

/// The closure type driving [`ApplyPointwiseFunction`]: `point → point`.
type PointFn = Box<dyn Fn(Point) -> Point>;

/// Animates a mobject from its current points to `func` applied to them. Port of
/// manim CE's `ApplyPointwiseFunction`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::ApplyPointwiseFunction;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// scene.play(ApplyPointwiseFunction::new(sq, |p| p * 2.0)).unwrap();
/// assert!((scene[sq].bounding_box().width() - 4.0).abs() < 1e-4);
/// ```
pub struct ApplyPointwiseFunction {
    id: AnyId,
    func: PointFn,
    config: AnimConfig,
    start: Vec<(AnyId, MobjectData)>,
}
anim_builders!(ApplyPointwiseFunction);

impl ApplyPointwiseFunction {
    /// Applies `func` to every point of `id`.
    pub fn new(id: impl Into<AnyId>, func: impl Fn(Point) -> Point + 'static) -> Self {
        Self {
            id: id.into(),
            func: Box::new(func),
            config: AnimConfig::default(),
            start: Vec::new(),
        }
    }
}

impl Animation for ApplyPointwiseFunction {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = family_data(state, self.id);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        for (id, data) in &self.start {
            if state.contains(*id) {
                let out = state.get_dyn_mut(*id).data_mut();
                out.path = data.path.clone();
                out.path.apply(|p| p + ((self.func)(p) - p) * alpha);
                out.bump_generation();
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    anim_config_accessors!();
}

/// Alias for [`ApplyPointwiseFunction`], matching manim CE's `ApplyFunction`.
pub type ApplyFunction = ApplyPointwiseFunction;

/// Applies a linear map (a 3×3 matrix) to a mobject, about a point. Port of
/// manim CE's `ApplyMatrix`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::ApplyMatrix;
/// use glam::Mat3;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// // Scale x by 2 via a diagonal matrix.
/// let m = Mat3::from_cols_array(&[2.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
/// scene.play(ApplyMatrix::new(sq, m)).unwrap();
/// assert!((scene[sq].bounding_box().width() - 4.0).abs() < 1e-4);
/// ```
pub struct ApplyMatrix {
    id: AnyId,
    matrix: Mat3,
    about: Point,
    config: AnimConfig,
    start: Vec<(AnyId, MobjectData)>,
}
anim_builders!(ApplyMatrix);

impl ApplyMatrix {
    /// Applies `matrix` about the origin.
    pub fn new(id: impl Into<AnyId>, matrix: Mat3) -> Self {
        Self {
            id: id.into(),
            matrix,
            about: Point::ZERO,
            config: AnimConfig::default(),
            start: Vec::new(),
        }
    }

    /// Sets the point the transformation is applied about (manim's
    /// `about_point`).
    pub fn about_point(mut self, point: Point) -> Self {
        self.about = point;
        self
    }
}

impl Animation for ApplyMatrix {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = family_data(state, self.id);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        for (id, data) in &self.start {
            if state.contains(*id) {
                let out = state.get_dyn_mut(*id).data_mut();
                out.path = data.path.clone();
                let about = self.about;
                let m = self.matrix;
                out.path.apply(|p| {
                    let mapped = about + m * (p - about);
                    p + (mapped - p) * alpha
                });
                out.bump_generation();
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    anim_config_accessors!();
}

/// Keeps `id` at a fixed offset from `anchor` as `anchor` moves during the same
/// play group. Port of manim CE's `MaintainPositionRelativeTo`.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::{MaintainPositionRelativeTo, MoveTo};
/// use manim_math::{RIGHT, UP};
/// let mut scene = Scene::new(Config::default());
/// let anchor = scene.add(Dot::new());
/// let follower = scene.add(Dot::at(UP)); // one unit above the anchor
/// scene.play((
///     MoveTo::new(anchor, 4.0 * RIGHT),
///     MaintainPositionRelativeTo::new(follower, anchor),
/// )).unwrap();
/// // Follower keeps its +UP offset from the anchor's new spot.
/// assert!((scene[follower].get_center() - (4.0 * RIGHT + UP)).length() < 1e-3);
/// ```
pub struct MaintainPositionRelativeTo {
    id: AnyId,
    anchor: AnyId,
    config: AnimConfig,
    start: Vec<(AnyId, MobjectData)>,
    start_center: Point,
    offset: Point,
}
anim_builders!(MaintainPositionRelativeTo);

impl MaintainPositionRelativeTo {
    /// Keeps `id` at its current offset from `anchor`.
    pub fn new(id: impl Into<AnyId>, anchor: impl Into<AnyId>) -> Self {
        Self {
            id: id.into(),
            anchor: anchor.into(),
            config: AnimConfig::default(),
            start: Vec::new(),
            start_center: Point::ZERO,
            offset: Point::ZERO,
        }
    }
}

impl Animation for MaintainPositionRelativeTo {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = family_data(state, self.id);
        self.start_center = state.family_bounding_box(self.id).center();
        let anchor_center = state.family_bounding_box(self.anchor).center();
        self.offset = self.start_center - anchor_center;
    }
    fn interpolate(&mut self, state: &mut SceneState, _alpha: f32) {
        let anchor_center = state.family_bounding_box(self.anchor).center();
        let desired = anchor_center + self.offset;
        let delta = desired - self.start_center;
        for (id, data) in &self.start {
            if state.contains(*id) {
                let out = state.get_dyn_mut(*id).data_mut();
                out.path = data.path.clone();
                out.path.apply(|p| p + delta);
                out.bump_generation();
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    anim_config_accessors!();
}
