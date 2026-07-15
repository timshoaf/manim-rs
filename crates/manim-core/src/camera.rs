//! The animatable 2D camera state stored inside a
//! [`SceneState`](crate::scene_state::SceneState).
//!
//! Keeping the camera in the scene state means timeline snapshots capture camera
//! motion automatically — camera animation rides the same snapshot-seek
//! machinery as everything else (see [`Timeline`](crate::timeline::Timeline)).
//! Port of manim CE's `MovingCamera` frame.

use manim_color::Color;
use manim_math::{Point, FRAME_HEIGHT, FRAME_WIDTH};

use crate::config::Config;

/// The 3-D orientation of a camera (manim CE's `ThreeDScene` spherical
/// conventions): `phi` is the polar angle from `+z`, `theta` the azimuth, and
/// `gamma` the roll about the view axis. `focal_distance` sets perspective
/// strength (larger = flatter), and `zoom` scales the frame.
///
/// A [`Camera2D`] with `three_d: None` renders orthographically (all 2-D scenes,
/// byte-identically); `Some` switches the renderer to a perspective orbit
/// camera.
///
/// ```
/// use manim_core::camera::ThreeDParams;
/// let p = ThreeDParams::default();
/// assert_eq!(p.phi, 0.0); // looking straight down +z, like the 2-D view
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThreeDParams {
    /// Polar angle from `+z`, in radians (`0` looks down the +z axis).
    pub phi: f32,
    /// Azimuth, in radians.
    pub theta: f32,
    /// Roll about the view axis, in radians.
    pub gamma: f32,
    /// Perspective focal distance in scene units (larger flattens perspective).
    pub focal_distance: f32,
    /// Frame zoom factor.
    pub zoom: f32,
}

impl Default for ThreeDParams {
    fn default() -> Self {
        Self {
            phi: 0.0,
            theta: -std::f32::consts::FRAC_PI_2,
            gamma: 0.0,
            focal_distance: 16.0,
            zoom: 1.0,
        }
    }
}

impl ThreeDParams {
    /// Linearly interpolates all fields.
    pub fn lerp(a: &ThreeDParams, b: &ThreeDParams, t: f32) -> ThreeDParams {
        let l = |x: f32, y: f32| x + (y - x) * t;
        ThreeDParams {
            phi: l(a.phi, b.phi),
            theta: l(a.theta, b.theta),
            gamma: l(a.gamma, b.gamma),
            focal_distance: l(a.focal_distance, b.focal_distance),
            zoom: l(a.zoom, b.zoom),
        }
    }
}

/// The camera / frame state: what region of scene space is visible, its
/// rotation, and the background color.
///
/// ```
/// use manim_core::camera::Camera2D;
/// use manim_core::config::Config;
/// let cam = Camera2D::from_config(&Config::default());
/// assert_eq!(cam.frame_height, 8.0);
/// assert_eq!(cam.rotation, 0.0);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Camera2D {
    /// Center of the visible frame in scene units.
    pub frame_center: Point,
    /// Width of the visible frame in scene units.
    pub frame_width: f32,
    /// Height of the visible frame in scene units.
    pub frame_height: f32,
    /// Frame rotation in radians (manim's camera roll).
    pub rotation: f32,
    /// Background color.
    pub background: Color,
    /// 3-D orientation, or `None` for an orthographic 2-D camera (the default).
    pub three_d: Option<ThreeDParams>,
}

impl Default for Camera2D {
    fn default() -> Self {
        Self {
            frame_center: Point::ZERO,
            frame_width: FRAME_WIDTH,
            frame_height: FRAME_HEIGHT,
            rotation: 0.0,
            background: Color::from_rgba(0.0, 0.0, 0.0, 1.0),
            three_d: None,
        }
    }
}

impl Camera2D {
    /// Builds a camera from a [`Config`].
    pub fn from_config(config: &Config) -> Self {
        Self {
            frame_center: Point::ZERO,
            frame_width: config.frame_width,
            frame_height: config.frame_height,
            rotation: 0.0,
            background: config.background_color,
            three_d: None,
        }
    }

    /// Whether the camera is in 3-D (perspective orbit) mode.
    pub fn is_3d(&self) -> bool {
        self.three_d.is_some()
    }

    /// Switches to 3-D and sets the orbit angles (manim's
    /// `set_camera_orientation(phi, theta)`), keeping any other 3-D params.
    ///
    /// ```
    /// use manim_core::camera::Camera2D;
    /// let mut cam = Camera2D::default();
    /// cam.set_camera_orientation(1.2, 0.5);
    /// assert!(cam.is_3d());
    /// assert!((cam.three_d.unwrap().phi - 1.2).abs() < 1e-6);
    /// ```
    pub fn set_camera_orientation(&mut self, phi: f32, theta: f32) {
        let mut p = self.three_d.unwrap_or_default();
        p.phi = phi;
        p.theta = theta;
        self.three_d = Some(p);
    }

    /// Sets the full 3-D orientation params at once.
    pub fn set_three_d(&mut self, params: ThreeDParams) {
        self.three_d = Some(params);
    }

    /// Advances ambient camera rotation by `d_theta` radians (the incremental
    /// step manim's `begin_ambient_camera_rotation(rate)` applies per frame).
    /// No-op in 2-D mode.
    pub fn rotate_ambient(&mut self, d_theta: f32) {
        if let Some(p) = &mut self.three_d {
            p.theta += d_theta;
        }
    }

    /// The frame center (manim's `camera.frame.get_center()`).
    pub fn get_center(&self) -> Point {
        self.frame_center
    }

    /// The frame width.
    pub fn width(&self) -> f32 {
        self.frame_width
    }

    /// The frame height.
    pub fn height(&self) -> f32 {
        self.frame_height
    }

    /// Linearly interpolates between two camera states.
    ///
    /// ```
    /// use manim_core::camera::Camera2D;
    /// let a = Camera2D::default();
    /// let mut b = Camera2D::default();
    /// b.frame_width = 4.0;
    /// let mid = Camera2D::lerp(&a, &b, 0.5);
    /// assert!((mid.frame_width - (a.frame_width + 4.0) / 2.0).abs() < 1e-6);
    /// ```
    pub fn lerp(a: &Camera2D, b: &Camera2D, t: f32) -> Camera2D {
        let l = |x: f32, y: f32| x + (y - x) * t;
        let three_d = match (a.three_d, b.three_d) {
            (Some(pa), Some(pb)) => Some(ThreeDParams::lerp(&pa, &pb, t)),
            (Some(pa), None) => Some(pa),
            (None, Some(pb)) => Some(pb),
            (None, None) => None,
        };
        Camera2D {
            frame_center: a.frame_center + (b.frame_center - a.frame_center) * t,
            frame_width: l(a.frame_width, b.frame_width),
            frame_height: l(a.frame_height, b.frame_height),
            rotation: l(a.rotation, b.rotation),
            background: a.background.interpolate(&b.background, t),
            three_d,
        }
    }
}

/// A lightweight per-frame copy of the camera, emitted by
/// [`Scene::frames_with_camera`](crate::scene::Scene::frames_with_camera).
///
/// ```
/// use manim_core::camera::{Camera2D, CameraFrame};
/// let frame = CameraFrame::from(&Camera2D::default());
/// assert_eq!(frame.height, 8.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraFrame {
    /// Frame center in scene units.
    pub center: Point,
    /// Frame width in scene units.
    pub width: f32,
    /// Frame height in scene units.
    pub height: f32,
    /// Frame rotation in radians.
    pub rotation: f32,
    /// Background color.
    pub background: Color,
    /// 3-D orientation, or `None` for an orthographic 2-D frame.
    pub three_d: Option<ThreeDParams>,
}

impl From<&Camera2D> for CameraFrame {
    fn from(c: &Camera2D) -> Self {
        Self {
            center: c.frame_center,
            width: c.frame_width,
            height: c.frame_height,
            rotation: c.rotation,
            background: c.background,
            three_d: c.three_d,
        }
    }
}
