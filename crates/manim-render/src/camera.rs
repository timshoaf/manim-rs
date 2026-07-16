//! The render camera: a scene rectangle (2-D) or spherical orbit (3-D) mapped to
//! normalized device coords.
//!
//! [`Camera2D`] describes the visible frame — center, size, roll, and an optional
//! [`ThreeDParams`] orientation. [`Camera2D::view_proj`] returns the `mat4`
//! uniform the vertex shader multiplies each world position by. With no 3-D part
//! it is an **orthographic** map onto `[-1, 1]²` NDC (y-up) — identical to the
//! 2-D renderer, so 2-D scenes are byte-for-byte unchanged. With a 3-D part it is
//! `perspective · look_at`, orbiting `frame_center` by `(phi, theta, gamma)`.

use glam::{Mat4, Vec3};
use manim_core::camera::ThreeDParams;
use manim_core::config::Config;
use manim_math::{Point, ORIGIN};

/// How far out along `+z` a 2-D (orthographic) camera's notional eye sits, for
/// the shading terms that need a view direction. It matches
/// [`ThreeDParams`]'s default focal distance, so a mesh shaded under a 2-D
/// camera looks like the same mesh at `phi = 0` in 3-D.
const DEFAULT_ORTHO_EYE_DISTANCE: f32 = 16.0;

/// The half-depth, in scene units, that a 2-D camera's mesh pass keeps: world
/// `z` outside `±16` is clipped. It matches
/// [`DEFAULT_ORTHO_EYE_DISTANCE`], so a mesh visible under the 2-D camera is
/// exactly one that would be in front of the equivalent 3-D camera at `phi = 0`.
pub const ORTHO_DEPTH_RANGE: f32 = DEFAULT_ORTHO_EYE_DISTANCE;

/// A render camera over a slice of scene space (2-D ortho or 3-D perspective).
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
///     three_d: None,
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
    /// 3-D orientation, or `None` for an orthographic 2-D camera.
    pub three_d: Option<ThreeDParams>,
}

impl Camera2D {
    /// Whether the camera is in 3-D (perspective) mode.
    pub fn is_3d(&self) -> bool {
        self.three_d.is_some()
    }

    /// The view-projection matrix. Orthographic when 2-D (see
    /// [`ortho_view_proj`](Self::ortho_view_proj)); perspective orbit when 3-D.
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
    ///     three_d: None,
    /// };
    /// let m = cam.view_proj();
    /// // Top-right frame corner (2, 1) → NDC (1, 1).
    /// let c = m.project_point3(Point::new(2.0, 1.0, 0.0));
    /// assert!((c.x - 1.0).abs() < 1e-6 && (c.y - 1.0).abs() < 1e-6);
    /// ```
    pub fn view_proj(&self) -> Mat4 {
        match self.three_d {
            Some(p) => self.perspective_proj(&p) * self.view_matrix(),
            None => self.ortho_view_proj(),
        }
    }

    /// The view-projection the **mesh pass** uses: identical to
    /// [`view_proj`](Self::view_proj) in 3-D, but in 2-D it also maps depth into
    /// the `[0, 1]` NDC z range wgpu clips against.
    ///
    /// [`ortho_view_proj`](Self::ortho_view_proj) passes world `z` through
    /// untouched, which is exactly right for the vector pass — it has no depth
    /// attachment, so `z` only ever fed the (unused) depth output. The moment a
    /// mesh is drawn with a real depth buffer, though, anything off the `z = 0`
    /// plane falls outside `[0, 1]` and is clipped away entirely. This maps
    /// `z ∈ [-`[`ORTHO_DEPTH_RANGE`]`, +`[`ORTHO_DEPTH_RANGE`]`]` onto
    /// `[1, 0]` (nearer is smaller, matching the `LessEqual` depth test) and
    /// leaves x/y identical, so meshes and vector content still line up exactly.
    ///
    /// ```
    /// use manim_render::camera::Camera2D;
    /// use manim_math::{ORIGIN, Point};
    ///
    /// let cam = Camera2D {
    ///     frame_center: ORIGIN,
    ///     frame_width: 8.0,
    ///     frame_height: 8.0,
    ///     rotation: 0.0,
    ///     three_d: None,
    /// };
    /// let m = cam.mesh_view_proj();
    /// // x/y map exactly as the 2-D camera always did.
    /// let c = m.project_point3(Point::new(4.0, 4.0, 0.0));
    /// assert!((c.x - 1.0).abs() < 1e-6 && (c.y - 1.0).abs() < 1e-6);
    /// // The z = 0 plane sits mid-depth, and nearer geometry gets a smaller z.
    /// assert!((c.z - 0.5).abs() < 1e-6);
    /// assert!(m.project_point3(Point::new(0.0, 0.0, 2.0)).z < 0.5);
    /// ```
    pub fn mesh_view_proj(&self) -> Mat4 {
        match self.three_d {
            Some(p) => self.perspective_proj(&p) * self.view_matrix(),
            None => {
                // Nearer (larger world z) must give a smaller NDC z, hence the
                // negative scale; the +0.5 recenters the range on the z=0 plane.
                let depth = Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5))
                    * Mat4::from_scale(Vec3::new(1.0, 1.0, -0.5 / ORTHO_DEPTH_RANGE));
                depth * self.ortho_view_proj()
            }
        }
    }

    /// The orthographic view-projection, mapping the frame rectangle onto
    /// `[-1, 1]²` NDC (y-up). Used for 2-D content and for `fixed_in_frame` HUD
    /// overlays under a 3-D camera.
    ///
    /// Depth is passed through unchanged — see
    /// [`mesh_view_proj`](Self::mesh_view_proj) for why the mesh pass needs a
    /// different matrix.
    pub fn ortho_view_proj(&self) -> Mat4 {
        let scale = Mat4::from_scale(Vec3::new(
            2.0 / self.frame_width,
            2.0 / self.frame_height,
            1.0,
        ));
        let rotate = Mat4::from_rotation_z(-self.rotation);
        let translate = Mat4::from_translation(-self.frame_center);
        scale * rotate * translate
    }

    /// The 3-D view (look-at) matrix — world → camera space. Identity-like
    /// (translate to center) when 2-D; used to depth-sort items by camera-space
    /// z. Panics-free: falls back to the 2-D translation when not 3-D.
    pub fn view_matrix(&self) -> Mat4 {
        match self.three_d {
            Some(p) => {
                let eye = self.eye(&p);
                Mat4::look_at_rh(eye, self.frame_center, self.up(&p))
            }
            None => Mat4::from_translation(-self.frame_center),
        }
    }

    /// The camera eye in world space — where the viewer is.
    ///
    /// The mesh pass needs this for its specular half-vector and for the
    /// two-sided normal flip (see [`mesh_pipeline`](crate::mesh_pipeline)). In
    /// 3-D it is the orbit position; in 2-D there is no eye, so this reports the
    /// point the orthographic camera looks *from* — one focal distance out along
    /// `+z`, which gives a sensible view direction for shading a mesh under a
    /// flat camera.
    ///
    /// ```
    /// use manim_core::camera::ThreeDParams;
    /// use manim_render::camera::Camera2D;
    /// use manim_math::ORIGIN;
    ///
    /// let mut cam = Camera2D {
    ///     frame_center: ORIGIN,
    ///     frame_width: 8.0,
    ///     frame_height: 8.0,
    ///     rotation: 0.0,
    ///     three_d: None,
    /// };
    /// // 2-D: straight out along +z.
    /// assert!(cam.eye_position().z > 0.0);
    /// // 3-D: on the orbit sphere, one focal distance from the frame center.
    /// let p = ThreeDParams::default();
    /// cam.three_d = Some(p);
    /// assert!((cam.eye_position().length() - p.focal_distance).abs() < 1e-4);
    /// ```
    pub fn eye_position(&self) -> Vec3 {
        match self.three_d {
            Some(p) => self.eye(&p),
            None => self.frame_center + Vec3::Z * DEFAULT_ORTHO_EYE_DISTANCE,
        }
    }

    /// The camera eye position for 3-D params `p`.
    fn eye(&self, p: &ThreeDParams) -> Vec3 {
        let dir = Vec3::new(
            p.phi.sin() * p.theta.cos(),
            p.phi.sin() * p.theta.sin(),
            p.phi.cos(),
        );
        self.frame_center + dir * p.focal_distance
    }

    /// The rolled up-vector for 3-D params `p`.
    fn up(&self, p: &ThreeDParams) -> Vec3 {
        let eye = self.eye(p);
        let forward = (self.frame_center - eye).normalize_or_zero();
        // Reference up: +y when looking near the pole (phi≈0), else +z.
        let reference = if p.phi.abs() < 1e-3 { Vec3::Y } else { Vec3::Z };
        let right = forward.cross(reference).normalize_or_zero();
        let up0 = right.cross(forward).normalize_or_zero();
        // Roll about the view axis by gamma.
        (up0 * p.gamma.cos() + right * p.gamma.sin()).normalize_or_zero()
    }

    /// The perspective projection for 3-D params `p`, matching the frame's
    /// visible height at the focus plane.
    fn perspective_proj(&self, p: &ThreeDParams) -> Mat4 {
        let aspect = self.frame_width / self.frame_height;
        let half_h = (self.frame_height * 0.5) / p.zoom.max(1e-3);
        let fovy = 2.0 * (half_h / p.focal_distance.max(1e-3)).atan();
        let far = p.focal_distance * 4.0 + 100.0;
        Mat4::perspective_rh(fovy, aspect, 0.05, far)
    }
}

impl From<&Config> for Camera2D {
    /// Builds the default 2-D camera for a [`Config`].
    ///
    /// ```
    /// use manim_core::config::Config;
    /// use manim_render::camera::Camera2D;
    ///
    /// let cam = Camera2D::from(&Config::default());
    /// assert_eq!(cam.frame_height, 8.0);
    /// assert!(!cam.is_3d());
    /// ```
    fn from(config: &Config) -> Self {
        Self {
            frame_center: ORIGIN,
            frame_width: config.frame_width,
            frame_height: config.frame_height,
            rotation: 0.0,
            three_d: None,
        }
    }
}

impl From<&manim_core::camera::CameraFrame> for Camera2D {
    /// Builds the render camera from a per-frame
    /// [`CameraFrame`](manim_core::camera::CameraFrame), carrying the 3-D
    /// orientation so renderers follow animated 2-D *and* 3-D camera motion.
    ///
    /// ```
    /// use manim_core::camera::{Camera2D as CoreCamera, CameraFrame};
    /// use manim_render::camera::Camera2D;
    ///
    /// let mut core = CoreCamera::default();
    /// core.frame_width = 4.0;
    /// let cam = Camera2D::from(&CameraFrame::from(&core));
    /// assert_eq!(cam.frame_width, 4.0);
    /// ```
    fn from(c: &manim_core::camera::CameraFrame) -> Self {
        Self {
            frame_center: c.center,
            frame_width: c.width,
            frame_height: c.height,
            rotation: c.rotation,
            three_d: c.three_d,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_math::Point;

    fn cam_2d(center: Point, w: f32, h: f32, rotation: f32) -> Camera2D {
        Camera2D {
            frame_center: center,
            frame_width: w,
            frame_height: h,
            rotation,
            three_d: None,
        }
    }

    #[test]
    fn frame_corners_map_to_ndc_unit_square() {
        let cam = cam_2d(Point::new(1.0, 2.0, 0.0), 8.0, 4.0, 0.0);
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
        assert!(up.y > 0.0);
    }

    #[test]
    fn rotation_rolls_the_world() {
        let cam = cam_2d(Point::ZERO, 2.0, 2.0, std::f32::consts::FRAC_PI_2);
        let c = cam.view_proj().project_point3(Point::new(1.0, 0.0, 0.0));
        assert!(c.x.abs() < 1e-6);
        assert!((c.y.abs() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn default_3d_orientation_matches_2d_axes() {
        // phi=0 (default) looks down +z with +y up and +x right — like the 2-D
        // view. A point on +x maps to positive NDC x, on +y to positive NDC y.
        let mut cam = cam_2d(Point::ZERO, 8.0, 8.0, 0.0);
        cam.three_d = Some(ThreeDParams::default());
        let px = cam.view_proj().project_point3(Point::new(1.0, 0.0, 0.0));
        let py = cam.view_proj().project_point3(Point::new(0.0, 1.0, 0.0));
        assert!(px.x > 0.0, "px.x = {}", px.x);
        assert!(py.y > 0.0, "py.y = {}", py.y);
    }

    #[test]
    fn mesh_view_proj_keeps_2d_geometry_inside_the_depth_range() {
        // The regression this matrix exists for: with the plain ortho matrix a
        // mesh at z = 2 lands at NDC z = 2 and is clipped away entirely once the
        // mesh pass attaches a depth buffer.
        let cam = cam_2d(Point::ZERO, 8.0, 8.0, 0.0);
        assert_eq!(
            cam.ortho_view_proj()
                .project_point3(Point::new(0.0, 0.0, 2.0))
                .z,
            2.0
        );

        let m = cam.mesh_view_proj();
        for z in [-ORTHO_DEPTH_RANGE, -2.0, 0.0, 2.0, ORTHO_DEPTH_RANGE] {
            let ndc = m.project_point3(Point::new(0.0, 0.0, z));
            assert!(
                (0.0..=1.0).contains(&ndc.z),
                "world z {z} → NDC z {} is outside the clip range",
                ndc.z
            );
        }
        // Nearer geometry (larger world z) must come out with a smaller NDC z,
        // so the LessEqual depth test keeps it.
        let near = m.project_point3(Point::new(0.0, 0.0, 2.0)).z;
        let far = m.project_point3(Point::new(0.0, 0.0, -2.0)).z;
        assert!(near < far, "near {near} should test closer than far {far}");
    }

    #[test]
    fn mesh_view_proj_matches_view_proj_in_3d() {
        // In 3-D the perspective matrix already produces a [0, 1] NDC z, so the
        // mesh pass and the vector pass share one matrix exactly.
        let mut cam = cam_2d(Point::ZERO, 8.0, 8.0, 0.0);
        cam.three_d = Some(ThreeDParams::default());
        assert_eq!(cam.mesh_view_proj(), cam.view_proj());
    }

    #[test]
    fn mesh_view_proj_leaves_xy_untouched_in_2d() {
        // Meshes and vector content must land on the same pixels.
        let cam = cam_2d(Point::new(1.0, 2.0, 0.0), 8.0, 4.0, 0.3);
        let mesh = cam.mesh_view_proj();
        let vector = cam.view_proj();
        for p in [
            Point::new(3.0, 1.0, 0.0),
            Point::new(-2.0, 4.0, 1.5),
            Point::new(0.0, 0.0, -3.0),
        ] {
            let a = mesh.project_point3(p);
            let b = vector.project_point3(p);
            assert!((a.x - b.x).abs() < 1e-6, "x: {} vs {}", a.x, b.x);
            assert!((a.y - b.y).abs() < 1e-6, "y: {} vs {}", a.y, b.y);
        }
    }

    #[test]
    fn eye_position_is_the_orbit_point_in_3d() {
        let mut cam = cam_2d(Point::new(1.0, 0.0, 0.0), 8.0, 8.0, 0.0);
        assert_eq!(
            cam.eye_position(),
            Point::new(1.0, 0.0, DEFAULT_ORTHO_EYE_DISTANCE)
        );

        let p = ThreeDParams::default();
        cam.three_d = Some(p);
        let eye = cam.eye_position();
        // On the orbit sphere, one focal distance from the frame center.
        assert!((eye.distance(cam.frame_center) - p.focal_distance).abs() < 1e-3);
    }

    #[test]
    fn perspective_selected_only_when_3d() {
        let cam2 = cam_2d(Point::ZERO, 8.0, 8.0, 0.0);
        assert!(!cam2.is_3d());
        // 2-D: orthographic — depth (z) does not affect x/y.
        let near = cam2.view_proj().project_point3(Point::new(1.0, 0.0, 0.0));
        let far = cam2.view_proj().project_point3(Point::new(1.0, 0.0, -3.0));
        assert!((near.x - far.x).abs() < 1e-6);

        let mut cam3 = cam2;
        cam3.three_d = Some(ThreeDParams::default());
        assert!(cam3.is_3d());
        // 3-D: perspective — a point farther from the camera projects smaller.
        let a = cam3.view_proj().project_point3(Point::new(1.0, 0.0, 0.0));
        let b = cam3.view_proj().project_point3(Point::new(1.0, 0.0, -3.0));
        assert!(
            b.x.abs() < a.x.abs(),
            "far {} should be smaller than near {}",
            b.x,
            a.x
        );
    }
}
