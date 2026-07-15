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
}

impl Default for Camera2D {
    fn default() -> Self {
        Self {
            frame_center: Point::ZERO,
            frame_width: FRAME_WIDTH,
            frame_height: FRAME_HEIGHT,
            rotation: 0.0,
            background: Color::from_rgba(0.0, 0.0, 0.0, 1.0),
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
        Camera2D {
            frame_center: a.frame_center + (b.frame_center - a.frame_center) * t,
            frame_width: l(a.frame_width, b.frame_width),
            frame_height: l(a.frame_height, b.frame_height),
            rotation: l(a.rotation, b.rotation),
            background: a.background.interpolate(&b.background, t),
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
}

impl From<&Camera2D> for CameraFrame {
    fn from(c: &Camera2D) -> Self {
        Self {
            center: c.frame_center,
            width: c.frame_width,
            height: c.frame_height,
            rotation: c.rotation,
            background: c.background,
        }
    }
}
