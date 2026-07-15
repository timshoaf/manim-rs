//! The 2-D camera: a scene-space rectangle mapped to normalized device coords.
//!
//! [`Camera2D`] describes the visible frame — its center, width, height, and
//! roll — exactly like manim CE's `camera.frame`. [`Camera2D::view_proj`] turns
//! that into the `mat4` uniform the vertex shader multiplies each world-space
//! position by, mapping the frame rectangle onto the `[-1, 1]²` clip cube with
//! **y pointing up** (matching both scene convention and wgpu's NDC).

use glam::{Mat4, Vec3};
use manim_core::config::Config;
use manim_math::{Point, ORIGIN};

/// A 2-D camera over a rectangular slice of scene space.
///
/// The camera sees a `frame_width × frame_height` rectangle centered on
/// `frame_center`, optionally rolled by `rotation` radians. Animating these
/// fields with the ordinary animation machinery gives manim's
/// `MovingCameraScene` behavior (`self.camera.frame.animate.scale(0.5)`).
///
/// ```
/// use manim_render::camera::Camera2D;
/// use manim_math::ORIGIN;
///
/// let cam = Camera2D {
///     frame_center: ORIGIN,
///     frame_width: 14.222,
///     frame_height: 8.0,
///     rotation: 0.0,
/// };
/// // The frame center maps to the middle of clip space.
/// let clip = cam.view_proj().project_point3(ORIGIN);
/// assert!(clip.x.abs() < 1e-6 && clip.y.abs() < 1e-6);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera2D {
    /// The scene-space point at the center of the frame.
    pub frame_center: Point,
    /// The frame width in scene units.
    pub frame_width: f32,
    /// The frame height in scene units.
    pub frame_height: f32,
    /// Camera roll in radians (counter-clockwise), about `frame_center`.
    pub rotation: f32,
}

impl Camera2D {
    /// The view-projection matrix mapping world space to `[-1, 1]²` NDC (y-up).
    ///
    /// A world point `p` is translated so `frame_center` is the origin, rolled
    /// by `-rotation` (rolling the camera one way scrolls the world the other),
    /// then scaled so the frame half-extents land on `±1`. Points outside the
    /// frame fall outside the clip cube and are clipped by the GPU.
    ///
    /// ```
    /// use manim_render::camera::Camera2D;
    /// use manim_math::Point;
    ///
    /// let cam = Camera2D {
    ///     frame_center: Point::ZERO,
    ///     frame_width: 4.0,
    ///     frame_height: 2.0,
    ///     rotation: 0.0,
    /// };
    /// let m = cam.view_proj();
    /// // Top-right frame corner (2, 1) → NDC (1, 1).
    /// let c = m.project_point3(Point::new(2.0, 1.0, 0.0));
    /// assert!((c.x - 1.0).abs() < 1e-6 && (c.y - 1.0).abs() < 1e-6);
    /// ```
    pub fn view_proj(&self) -> Mat4 {
        let scale = Mat4::from_scale(Vec3::new(
            2.0 / self.frame_width,
            2.0 / self.frame_height,
            1.0,
        ));
        let rotate = Mat4::from_rotation_z(-self.rotation);
        let translate = Mat4::from_translation(-self.frame_center);
        scale * rotate * translate
    }
}

impl From<&Config> for Camera2D {
    /// Builds the default camera for a [`Config`]: centered at the origin,
    /// unrolled, with the config's frame dimensions.
    ///
    /// ```
    /// use manim_core::config::Config;
    /// use manim_render::camera::Camera2D;
    ///
    /// let cam = Camera2D::from(&Config::default());
    /// assert_eq!(cam.frame_height, 8.0);
    /// assert_eq!(cam.rotation, 0.0);
    /// ```
    fn from(config: &Config) -> Self {
        Self {
            frame_center: ORIGIN,
            frame_width: config.frame_width,
            frame_height: config.frame_height,
            rotation: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_math::Point;

    #[test]
    fn frame_corners_map_to_ndc_unit_square() {
        let cam = Camera2D {
            frame_center: Point::new(1.0, 2.0, 0.0),
            frame_width: 8.0,
            frame_height: 4.0,
            rotation: 0.0,
        };
        let m = cam.view_proj();
        let corners = [
            (Point::new(1.0 + 4.0, 2.0 + 2.0, 0.0), (1.0, 1.0)),
            (Point::new(1.0 - 4.0, 2.0 - 2.0, 0.0), (-1.0, -1.0)),
            (Point::new(1.0 + 4.0, 2.0 - 2.0, 0.0), (1.0, -1.0)),
        ];
        for (world, (nx, ny)) in corners {
            let c = m.project_point3(world);
            assert!((c.x - nx).abs() < 1e-6, "x: {} vs {nx}", c.x);
            assert!((c.y - ny).abs() < 1e-6, "y: {} vs {ny}", c.y);
        }
    }

    #[test]
    fn y_axis_points_up() {
        let cam = Camera2D::from(&Config::default());
        let up = cam.view_proj().project_point3(Point::new(0.0, 1.0, 0.0));
        // A point above center has positive NDC y (wgpu NDC is y-up).
        assert!(up.y > 0.0);
    }

    #[test]
    fn rotation_rolls_the_world() {
        // A 90° camera roll sends the +x frame axis onto the -y (or +y) NDC axis.
        let cam = Camera2D {
            frame_center: Point::ZERO,
            frame_width: 2.0,
            frame_height: 2.0,
            rotation: std::f32::consts::FRAC_PI_2,
        };
        let c = cam.view_proj().project_point3(Point::new(1.0, 0.0, 0.0));
        assert!(c.x.abs() < 1e-6);
        assert!((c.y.abs() - 1.0).abs() < 1e-6);
    }
}
