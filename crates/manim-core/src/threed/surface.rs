//! [`Surface`]: a parametric `(u, v) → Point` surface meshed into quad faces.

use manim_color::Color;
use manim_math::Point;

use super::{add_face_group, default_checkerboard};
use crate::geometry::VGroup;
use crate::mobject::MobjectId;
use crate::scene_state::SceneState;

/// Default number of faces along each parameter axis (kept modest; CE's default
/// is far higher but headless meshes stay small).
pub const DEFAULT_SURFACE_RESOLUTION: usize = 24;

/// A parametric surface sampled over `[u_min, u_max] × [v_min, v_max]` and
/// meshed into a `u_res × v_res` grid of flat quad faces. Port of manim CE's
/// `Surface`.
///
/// This is a builder: [`add_to`](Self::add_to) materializes the mesh into a
/// [`VGroup`] of checkerboard-colored face children; [`faces`](Self::faces)
/// exposes the raw quad corners for headless use.
///
/// ```
/// use manim_core::threed::Surface;
/// use manim_math::{Point, PI, TAU};
/// // A unit sphere as an explicit parametric surface.
/// let surface = Surface::new(
///     |theta, phi| Point::new(phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos()),
///     [0.0, TAU],
///     [0.0, PI],
/// )
/// .with_resolution(8, 6);
/// assert_eq!(surface.resolution(), (8, 6));
/// assert_eq!(surface.faces().len(), 8 * 6);
/// ```
pub struct Surface {
    sampler: Box<dyn Fn(f32, f32) -> Point + Send + Sync>,
    u_range: [f32; 2],
    v_range: [f32; 2],
    u_res: usize,
    v_res: usize,
    checkerboard: Vec<Color>,
    fill_opacity: f32,
}

impl Surface {
    /// A surface from a `(u, v) → Point` sampler over the given ranges, at the
    /// default resolution and checkerboard.
    pub fn new(
        sampler: impl Fn(f32, f32) -> Point + Send + Sync + 'static,
        u_range: [f32; 2],
        v_range: [f32; 2],
    ) -> Self {
        Self {
            sampler: Box::new(sampler),
            u_range,
            v_range,
            u_res: DEFAULT_SURFACE_RESOLUTION,
            v_res: DEFAULT_SURFACE_RESOLUTION,
            checkerboard: default_checkerboard(),
            fill_opacity: 1.0,
        }
    }

    /// Sets the number of faces along `u` and `v`.
    pub fn with_resolution(mut self, u_res: usize, v_res: usize) -> Self {
        self.u_res = u_res.max(1);
        self.v_res = v_res.max(1);
        self
    }

    /// Sets the face checkerboard colors.
    pub fn with_checkerboard(mut self, colors: &[Color]) -> Self {
        if !colors.is_empty() {
            self.checkerboard = colors.to_vec();
        }
        self
    }

    /// Sets the face fill opacity.
    pub fn with_fill_opacity(mut self, opacity: f32) -> Self {
        self.fill_opacity = opacity;
        self
    }

    /// The `(u_res, v_res)` face resolution.
    pub fn resolution(&self) -> (usize, usize) {
        (self.u_res, self.v_res)
    }

    /// Samples the surface at parameters `(u, v)`.
    pub fn sample(&self, u: f32, v: f32) -> Point {
        (self.sampler)(u, v)
    }

    /// The `u_res × v_res` quad faces, each as four corner points wound
    /// consistently.
    pub fn faces(&self) -> Vec<Vec<Point>> {
        let mut faces = Vec::with_capacity(self.u_res * self.v_res);
        let u_at = |i: usize| lerp(self.u_range, i, self.u_res);
        let v_at = |j: usize| lerp(self.v_range, j, self.v_res);
        for i in 0..self.u_res {
            for j in 0..self.v_res {
                let (u0, u1) = (u_at(i), u_at(i + 1));
                let (v0, v1) = (v_at(j), v_at(j + 1));
                faces.push(vec![
                    self.sample(u0, v0),
                    self.sample(u1, v0),
                    self.sample(u1, v1),
                    self.sample(u0, v1),
                ]);
            }
        }
        faces
    }

    /// The checkerboard color for each face, parallel to [`faces`](Self::faces).
    pub fn face_colors(&self) -> Vec<Color> {
        let n = self.checkerboard.len().max(1);
        let mut colors = Vec::with_capacity(self.u_res * self.v_res);
        for i in 0..self.u_res {
            for j in 0..self.v_res {
                colors.push(self.checkerboard[(i + j) % n]);
            }
        }
        colors
    }

    /// Adds the meshed surface to `scene` as a [`VGroup`] of face children,
    /// returning the group.
    ///
    /// ```
    /// use manim_core::threed::Surface;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// use manim_math::{Point, PI, TAU};
    /// let mut scene = SceneState::new();
    /// let surface = Surface::new(
    ///     |t, p| Point::new(p.sin() * t.cos(), p.sin() * t.sin(), p.cos()),
    ///     [0.0, TAU],
    ///     [0.0, PI],
    /// )
    /// .with_resolution(6, 4);
    /// let group = surface.add_to(&mut scene);
    /// // 6 × 4 face children.
    /// assert_eq!(scene.family(group.erase()).len(), 1 + 24);
    /// ```
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        add_face_group(scene, &self.faces(), &self.face_colors(), self.fill_opacity)
    }
}

/// Linearly interpolates `range` at grid index `i` of `res` divisions.
fn lerp(range: [f32; 2], i: usize, res: usize) -> f32 {
    range[0] + (range[1] - range[0]) * (i as f32 / res as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_math::{PI, TAU};

    #[test]
    fn resolution_sets_face_count() {
        let s = Surface::new(|u, v| Point::new(u, v, 0.0), [0.0, 1.0], [0.0, 1.0])
            .with_resolution(5, 7);
        assert_eq!(s.faces().len(), 35);
        assert_eq!(s.face_colors().len(), 35);
    }

    #[test]
    fn sphere_surface_samples_lie_on_radius() {
        let r = 2.0;
        let s = Surface::new(
            move |t, p| Point::new(r * p.sin() * t.cos(), r * p.sin() * t.sin(), r * p.cos()),
            [0.0, TAU],
            [0.0, PI],
        );
        for face in s.faces() {
            for c in face {
                assert!((c.length() - r).abs() < 1e-4, "radius {}", c.length());
            }
        }
    }
}
