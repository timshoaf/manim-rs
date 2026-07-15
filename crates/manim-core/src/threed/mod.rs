//! 3D mobjects: parametric [`Surface`]s, solids ([`Sphere`], [`Cube`], …), and
//! [`ThreeDAxes`]. Port of manim CE's `three_d` mobjects (FE-108).
//!
//! # Model
//!
//! [`Point`] is already 3D, and mobjects emit 3D paths, so this geometry is
//! **camera-independent** — it is built and unit-tested headlessly. A 3D camera
//! (FE-107) projects and depth-sorts these paths at render time.
//!
//! Following manim CE, a curved surface is a **group of flat quad faces**: each
//! face is a closed 4-corner [`VMobject`] child of a [`VGroup`], so the renderer
//! can depth-sort per face and faces can be individually colored (the default
//! checkerboard alternates `BLUE_D`/`BLUE_E`). The materializing method is
//! `add_to(scene) -> MobjectId<VGroup>`, mirroring `Text::add_to`.
//! Face geometry is also exposed via `faces()` for headless inspection/tests.

mod axes3d;
mod solids;
mod surface;

pub use axes3d::ThreeDAxes;
pub use solids::{Arrow3D, Cone, Cube, Cylinder, Dot3D, Line3D, Prism, Sphere, Torus};
pub use surface::{Surface, DEFAULT_SURFACE_RESOLUTION};

use manim_color::Color;
use manim_math::path::{Path, SubPath};
use manim_math::space_ops::rotation_matrix;
use manim_math::Point;

use crate::geometry::{VGroup, VMobject};
use crate::mobject::MobjectId;
use crate::scene_state::SceneState;
use crate::style::Style;

/// The default face checkerboard colors (manim CE's `Surface` default).
pub fn default_checkerboard() -> Vec<Color> {
    vec![manim_color::BLUE_D, manim_color::BLUE_E]
}

/// A closed 4-corner (or n-corner) face path from its ordered `corners`.
pub(crate) fn face_path(corners: &[Point]) -> Path {
    if corners.len() < 3 {
        return Path::default();
    }
    let mut pts = corners.to_vec();
    pts.push(corners[0]);
    let mut sp = SubPath::from_corners(&pts);
    sp.closed = true;
    Path { subpaths: vec![sp] }
}

/// Adds a face group to `scene`: a [`VGroup`] parent with one filled face child
/// per polygon in `faces`, colored by `colors[i % colors.len()]`. Returns the
/// group.
pub(crate) fn add_face_group(
    scene: &mut SceneState,
    faces: &[Vec<Point>],
    colors: &[Color],
    fill_opacity: f32,
) -> MobjectId<VGroup> {
    let group = scene.add(VGroup::new());
    let palette = if colors.is_empty() {
        default_checkerboard()
    } else {
        colors.to_vec()
    };
    for (i, face) in faces.iter().enumerate() {
        let mut style = Style::filled(palette[i % palette.len()]);
        style.fill_opacity = fill_opacity;
        let child = scene.add(VMobject::new(face_path(face), style));
        scene.add_child(group.erase(), child.erase());
    }
    group
}

/// Rotates `point` by `angle` radians about the line through the origin with
/// direction `axis` (manim's arbitrary-axis rotation).
///
/// ```
/// use manim_core::threed::rotate_about_axis;
/// use manim_math::{Point, OUT, RIGHT, UP};
/// // A quarter turn about the z-axis sends x̂ to ŷ.
/// let p = rotate_about_axis(RIGHT, std::f32::consts::FRAC_PI_2, OUT);
/// assert!((p - UP).length() < 1e-6);
/// ```
pub fn rotate_about_axis(point: Point, angle: f32, axis: Point) -> Point {
    rotation_matrix(angle, axis) * point
}

/// Rotates every control point of `path` about the origin around `axis` by
/// `angle` (in place).
///
/// ```
/// use manim_core::threed::rotate_path_about_axis;
/// use manim_core::geometry::Line;
/// use manim_core::mobject::Mobject;
/// use manim_math::{Point, OUT, RIGHT, UP};
/// let mut line = Line::new(Point::ZERO, RIGHT);
/// rotate_path_about_axis(&mut line.data_mut().path, std::f32::consts::FRAC_PI_2, OUT);
/// // The endpoint x̂ has rotated to ŷ.
/// assert!((line.data().path.point_from_proportion(1.0) - UP).length() < 1e-5);
/// ```
pub fn rotate_path_about_axis(path: &mut Path, angle: f32, axis: Point) {
    let m = rotation_matrix(angle, axis);
    path.apply(|p| m * p);
}
