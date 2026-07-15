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

/// A zoomed inset window (manim CE's `ZoomedScene`): the renderer draws the scene
/// a second time through a magnifying camera over a small `region_*` of scene
/// space, into an `inset_*` rectangle of the output, framed by a border.
///
/// The `inset_*` rectangle is in **normalized viewport coordinates** — `(0, 0)`
/// top-left, `(1, 1)` bottom-right of the drawable (letterboxed) area — so it
/// stays put regardless of output resolution. `region_center`/`region_width` are
/// scene-space; the region's height is derived from the inset's aspect at render
/// time so the magnified content is never distorted. All the geometric fields
/// interpolate, so the window can pan/zoom across a `play`.
///
/// ```
/// use manim_core::camera::ZoomWindow;
/// use manim_math::ORIGIN;
/// let zw = ZoomWindow::new(ORIGIN, 1.0, [0.62, 0.05, 0.33, 0.33]);
/// assert_eq!(zw.region_width, 1.0);
/// assert_eq!(zw.inset_w, 0.33);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomWindow {
    /// Scene-space center of the magnified region.
    pub region_center: Point,
    /// Scene-space width of the magnified region (height follows the inset aspect).
    pub region_width: f32,
    /// Inset left edge, as a fraction `[0, 1]` of the viewport width.
    pub inset_x: f32,
    /// Inset top edge, as a fraction `[0, 1]` of the viewport height.
    pub inset_y: f32,
    /// Inset width, as a fraction `[0, 1]` of the viewport width.
    pub inset_w: f32,
    /// Inset height, as a fraction `[0, 1]` of the viewport height.
    pub inset_h: f32,
    /// Border color drawn around the inset.
    pub border_color: Color,
    /// Border thickness in output pixels.
    pub border_width: f32,
}

impl ZoomWindow {
    /// A zoom window over `region_center`/`region_width`, placed at the
    /// normalized `inset = [x, y, w, h]` rectangle, with a default white border.
    pub fn new(region_center: Point, region_width: f32, inset: [f32; 4]) -> Self {
        Self {
            region_center,
            region_width: region_width.max(1e-4),
            inset_x: inset[0],
            inset_y: inset[1],
            inset_w: inset[2],
            inset_h: inset[3],
            border_color: Color::from_rgba(1.0, 1.0, 1.0, 1.0),
            border_width: 3.0,
        }
    }

    /// Sets the border color and pixel width (builder style).
    pub fn with_border(mut self, color: Color, width: f32) -> Self {
        self.border_color = color;
        self.border_width = width;
        self
    }

    /// Linearly interpolates the geometric fields; the border is carried from `a`.
    pub fn lerp(a: &ZoomWindow, b: &ZoomWindow, t: f32) -> ZoomWindow {
        let l = |x: f32, y: f32| x + (y - x) * t;
        ZoomWindow {
            region_center: a.region_center + (b.region_center - a.region_center) * t,
            region_width: l(a.region_width, b.region_width),
            inset_x: l(a.inset_x, b.inset_x),
            inset_y: l(a.inset_y, b.inset_y),
            inset_w: l(a.inset_w, b.inset_w),
            inset_h: l(a.inset_h, b.inset_h),
            border_color: a.border_color.interpolate(&b.border_color, t),
            border_width: l(a.border_width, b.border_width),
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
    /// A magnifying inset window (manim's `ZoomedScene`), or `None`.
    pub zoom_window: Option<ZoomWindow>,
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
            zoom_window: None,
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
            zoom_window: None,
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
        let zoom_window = match (a.zoom_window, b.zoom_window) {
            (Some(za), Some(zb)) => Some(ZoomWindow::lerp(&za, &zb, t)),
            (Some(za), None) => Some(za),
            (None, Some(zb)) => Some(zb),
            (None, None) => None,
        };
        Camera2D {
            frame_center: a.frame_center + (b.frame_center - a.frame_center) * t,
            frame_width: l(a.frame_width, b.frame_width),
            frame_height: l(a.frame_height, b.frame_height),
            rotation: l(a.rotation, b.rotation),
            background: a.background.interpolate(&b.background, t),
            three_d,
            zoom_window,
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
    /// A magnifying inset window (manim's `ZoomedScene`), or `None`.
    pub zoom_window: Option<ZoomWindow>,
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
            zoom_window: c.zoom_window,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_math::{ORIGIN, RIGHT};

    #[test]
    fn zoom_window_lerp_interpolates_geometry() {
        let a = ZoomWindow::new(ORIGIN, 2.0, [0.0, 0.0, 0.2, 0.2]);
        let b = ZoomWindow::new(4.0 * RIGHT, 1.0, [0.6, 0.6, 0.4, 0.4]);
        let mid = ZoomWindow::lerp(&a, &b, 0.5);
        assert!((mid.region_center.x - 2.0).abs() < 1e-6);
        assert!((mid.region_width - 1.5).abs() < 1e-6);
        assert!((mid.inset_x - 0.3).abs() < 1e-6);
        assert!((mid.inset_w - 0.3).abs() < 1e-6);
    }

    #[test]
    fn camera_lerp_carries_and_blends_zoom_window() {
        let mut a = Camera2D::default();
        // Present on only one side → carried through.
        let b = Camera2D {
            zoom_window: Some(ZoomWindow::new(ORIGIN, 2.0, [0.5, 0.5, 0.3, 0.3])),
            ..Camera2D::default()
        };
        let mid = Camera2D::lerp(&a, &b, 0.5);
        assert!(mid.zoom_window.is_some());
        // Present on both → interpolated.
        a.zoom_window = Some(ZoomWindow::new(ORIGIN, 4.0, [0.0, 0.0, 0.3, 0.3]));
        let mid2 = Camera2D::lerp(&a, &b, 0.5);
        assert!((mid2.zoom_window.unwrap().region_width - 3.0).abs() < 1e-6);
    }

    #[test]
    fn camera_frame_carries_zoom_window() {
        let cam = Camera2D {
            zoom_window: Some(ZoomWindow::new(ORIGIN, 1.0, [0.6, 0.05, 0.3, 0.3])),
            ..Camera2D::default()
        };
        let frame = CameraFrame::from(&cam);
        assert!(frame.zoom_window.is_some());
    }
}
