//! [`ThreeDAxes`]: three [`NumberLine`]s along x/y/z with 3D coordinate mapping.

use std::f32::consts::FRAC_PI_2;

use manim_math::{Point, ORIGIN, OUT, RIGHT, UP};

use crate::geometry::VGroup;
use crate::graphing::NumberLine;
use crate::mobject::{MobjectExt, MobjectId};
use crate::scene_state::SceneState;

/// A 3D coordinate system: three [`NumberLine`]s along the x, y, and z axes, with
/// `(x, y, z) ↔ point` mapping. Port of manim CE's `ThreeDAxes`.
///
/// [`coords_to_point`](Self::coords_to_point) maps data to scene space along
/// `RIGHT`/`UP`/`OUT`; [`add_to`](Self::add_to) materializes the three axis
/// lines (the y and z lines are the x-line rotated into place).
///
/// ```
/// use manim_core::threed::ThreeDAxes;
/// use manim_math::{OUT, RIGHT, UP};
/// let axes = ThreeDAxes::new();
/// // Unit steps map along the three scene-space axes.
/// assert!((axes.c2p(1.0, 0.0, 0.0) - RIGHT).length() < 1e-6);
/// assert!((axes.c2p(0.0, 1.0, 0.0) - UP).length() < 1e-6);
/// assert!((axes.c2p(0.0, 0.0, 1.0) - OUT).length() < 1e-6);
/// ```
pub struct ThreeDAxes {
    x_range: [f32; 3],
    y_range: [f32; 3],
    z_range: [f32; 3],
    x_unit: f32,
    y_unit: f32,
    z_unit: f32,
}

impl Default for ThreeDAxes {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreeDAxes {
    /// Axes with manim CE's default ranges (`x,y ∈ [-6,6]`/`[-5,5]`, `z ∈ [-4,4]`)
    /// at unit scale.
    pub fn new() -> Self {
        Self::with_ranges([-6.0, 6.0, 1.0], [-5.0, 5.0, 1.0], [-4.0, 4.0, 1.0])
    }

    /// Axes over explicit `[min, max, step]` ranges (unit scale).
    pub fn with_ranges(x_range: [f32; 3], y_range: [f32; 3], z_range: [f32; 3]) -> Self {
        Self {
            x_range,
            y_range,
            z_range,
            x_unit: 1.0,
            y_unit: 1.0,
            z_unit: 1.0,
        }
    }

    /// Maps data coordinates `(x, y, z)` to a scene point (3D `c2p`).
    pub fn coords_to_point(&self, x: f32, y: f32, z: f32) -> Point {
        RIGHT * ((x - center(self.x_range)) * self.x_unit)
            + UP * ((y - center(self.y_range)) * self.y_unit)
            + OUT * ((z - center(self.z_range)) * self.z_unit)
    }

    /// Alias for [`coords_to_point`](Self::coords_to_point).
    pub fn c2p(&self, x: f32, y: f32, z: f32) -> Point {
        self.coords_to_point(x, y, z)
    }

    /// Maps a scene point back to data coordinates `(x, y, z)` (3D `p2c`).
    pub fn point_to_coords(&self, p: Point) -> (f32, f32, f32) {
        (
            p.x / self.x_unit + center(self.x_range),
            p.y / self.y_unit + center(self.y_range),
            p.z / self.z_unit + center(self.z_range),
        )
    }

    /// Alias for [`point_to_coords`](Self::point_to_coords).
    pub fn p2c(&self, p: Point) -> (f32, f32, f32) {
        self.point_to_coords(p)
    }

    /// Adds the three axis lines to `scene` (x along `RIGHT`, y along `UP`, z
    /// along `OUT`), returning the group.
    ///
    /// ```
    /// use manim_core::threed::ThreeDAxes;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// let mut scene = SceneState::new();
    /// let axes = ThreeDAxes::new();
    /// let group = axes.add_to(&mut scene);
    /// assert_eq!(scene.get_dyn(group.erase()).data().children.len(), 3);
    /// ```
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        let x_axis = scene.add(line(self.x_range, self.x_unit));

        let mut y = line(self.y_range, self.y_unit);
        y.rotate_about(FRAC_PI_2, ORIGIN, OUT); // RIGHT → UP
        let y_axis = scene.add(y);

        let mut z = line(self.z_range, self.z_unit);
        z.rotate_about(-FRAC_PI_2, ORIGIN, UP); // RIGHT → OUT
        let z_axis = scene.add(z);

        VGroup::of(scene, [x_axis.erase(), y_axis.erase(), z_axis.erase()])
    }
}

/// A number line over `range` at the given unit size.
fn line(range: [f32; 3], unit: f32) -> NumberLine {
    NumberLine::new(range[0], range[1], range[2]).with_unit_size(unit)
}

/// The midpoint of a `[min, max, step]` range.
fn center(range: [f32; 3]) -> f32 {
    (range[0] + range[1]) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c2p_p2c_round_trip_3d() {
        let axes = ThreeDAxes::new();
        for (x, y, z) in [(0.0, 0.0, 0.0), (2.0, -3.0, 1.5), (-5.0, 4.0, -2.0)] {
            let (rx, ry, rz) = axes.point_to_coords(axes.coords_to_point(x, y, z));
            assert!((rx - x).abs() < 1e-4 && (ry - y).abs() < 1e-4 && (rz - z).abs() < 1e-4);
        }
    }

    #[test]
    fn basis_directions() {
        let axes = ThreeDAxes::new();
        assert!((axes.c2p(1.0, 0.0, 0.0) - RIGHT).length() < 1e-6);
        assert!((axes.c2p(0.0, 1.0, 0.0) - UP).length() < 1e-6);
        assert!((axes.c2p(0.0, 0.0, 1.0) - OUT).length() < 1e-6);
    }
}
