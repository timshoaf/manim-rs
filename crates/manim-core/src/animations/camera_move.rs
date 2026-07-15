//! Camera animation: [`CameraMove`] and the [`CameraFrameHandle`] returned by
//! [`Scene::camera_frame`](crate::scene::Scene::camera_frame).
//!
//! Maps manim CE's `self.camera.frame.animate.scale(..).move_to(..)` onto our
//! arena model: the camera lives in the scene state, so animating it rides the
//! same snapshot machinery as mobjects.

use manim_color::Color;
use manim_math::Point;

use crate::animation::AnimConfig;
use crate::animation::{anim_config_accessors, Animation};
use crate::camera::Camera2D;
use crate::scene_state::SceneState;

/// A recorded camera mutation, replayed on a clone to find the end state.
type CameraOp = Box<dyn Fn(&mut Camera2D)>;

/// A builder-animation that tweens the camera from its current state to the one
/// produced by the recorded operations. Port of manim CE's animated camera
/// frame.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_math::UP;
/// let mut scene = Scene::new(Config::default());
/// let _ = scene.add(Circle::new());
/// // Zoom in to half size and recenter up.
/// scene.play(scene.camera_frame().animate().scale(0.5).move_to(2.0 * UP)).unwrap();
/// assert!((scene.state().camera().frame_width - Config::default().frame_width * 0.5).abs() < 1e-4);
/// assert!((scene.state().camera().frame_center - 2.0 * UP).length() < 1e-4);
/// ```
pub struct CameraMove {
    ops: Vec<CameraOp>,
    config: AnimConfig,
    start: Camera2D,
    end: Camera2D,
}

impl Default for CameraMove {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraMove {
    /// A camera animation with no operations yet.
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            config: AnimConfig::default(),
            start: Camera2D::default(),
            end: Camera2D::default(),
        }
    }

    /// Sets the run time in seconds.
    pub fn run_time(mut self, run_time: f32) -> Self {
        self.config.run_time = run_time;
        self
    }

    /// Sets the easing curve.
    pub fn rate_fn(mut self, rate_fn: manim_math::rate_functions::RateFn) -> Self {
        self.config.rate_fn = rate_fn;
        self
    }

    /// Zooms by `factor` (values < 1 zoom in). manim's `camera.frame.scale`.
    pub fn scale(mut self, factor: f32) -> Self {
        self.ops.push(Box::new(move |c| {
            c.frame_width *= factor;
            c.frame_height *= factor;
        }));
        self
    }

    /// Recenters the frame on `point` (manim's `camera.frame.move_to`).
    pub fn move_to(mut self, point: Point) -> Self {
        self.ops.push(Box::new(move |c| c.frame_center = point));
        self
    }

    /// Shifts the frame by `delta` (manim's `camera.frame.shift`).
    pub fn shift(mut self, delta: Point) -> Self {
        self.ops.push(Box::new(move |c| c.frame_center += delta));
        self
    }

    /// Sets the frame width, preserving aspect ratio (manim's
    /// `camera.frame.set_width`).
    pub fn set_width(mut self, width: f32) -> Self {
        self.ops.push(Box::new(move |c| {
            let aspect = if c.frame_width.abs() > 1e-9 {
                c.frame_height / c.frame_width
            } else {
                1.0
            };
            c.frame_width = width;
            c.frame_height = width * aspect;
        }));
        self
    }

    /// Rolls the frame by `angle` radians (manim's `camera.frame.rotate`).
    pub fn rotate(mut self, angle: f32) -> Self {
        self.ops.push(Box::new(move |c| c.rotation += angle));
        self
    }

    /// Sets the background color.
    pub fn set_background(mut self, color: Color) -> Self {
        self.ops.push(Box::new(move |c| c.background = color));
        self
    }
}

impl Animation for CameraMove {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = state.camera().clone();
        let mut end = self.start.clone();
        for op in &self.ops {
            op(&mut end);
        }
        self.end = end;
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        *state.camera_mut() = Camera2D::lerp(&self.start, &self.end, alpha);
    }
    fn finish(&mut self, state: &mut SceneState) {
        *state.camera_mut() = self.end.clone();
    }
    anim_config_accessors!();
}

/// A handle to the camera frame, mirroring manim's `self.camera.frame`. Its
/// [`animate`](CameraFrameHandle::animate) begins a [`CameraMove`].
///
/// ```
/// use manim_core::prelude::*;
/// let mut scene = Scene::new(Config::default());
/// let move_anim = scene.camera_frame().animate().scale(2.0);
/// scene.play(move_anim).unwrap();
/// // Zoomed out to double size.
/// assert!((scene.state().camera().frame_width - Config::default().frame_width * 2.0).abs() < 1e-4);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct CameraFrameHandle;

impl CameraFrameHandle {
    /// Begins a camera animation (manim's `camera.frame.animate`).
    pub fn animate(self) -> CameraMove {
        CameraMove::new()
    }
}
