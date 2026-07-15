//! 3D solids: [`Sphere`], [`Torus`], [`Dot3D`], [`Cylinder`], [`Cone`], [`Cube`],
//! [`Prism`], [`Line3D`], and [`Arrow3D`].

use manim_color::Color;
use manim_math::{Point, OUT, TAU};

use super::{add_face_group, default_checkerboard, Surface};
use crate::geometry::{Line, VGroup, VMobject};
use crate::mobject::{MobjectExt, MobjectId};
use crate::scene_state::SceneState;
use crate::style::Style;

/// Default angular resolution for round solids.
pub const DEFAULT_SOLID_RESOLUTION: usize = 24;

// ---------------------------------------------------------------------------
// Parametric solids (built directly on `Surface`).
// ---------------------------------------------------------------------------

/// A sphere of the given radius, centered at the origin, as a parametric
/// [`Surface`] (u = azimuth θ, v = polar φ). Port of manim CE's `Sphere`.
///
/// ```
/// use manim_core::threed::Sphere;
/// // Every meshed vertex lies on the sphere of radius 1.5.
/// let s = Sphere::new(1.5);
/// for face in s.faces() {
///     for c in face {
///         assert!((c.length() - 1.5).abs() < 1e-3);
///     }
/// }
/// ```
pub struct Sphere;

impl Sphere {
    /// A sphere of `radius` as a [`Surface`].
    #[allow(clippy::new_ret_no_self)]
    pub fn new(radius: f32) -> Surface {
        Surface::new(
            move |theta, phi| {
                Point::new(
                    radius * phi.sin() * theta.cos(),
                    radius * phi.sin() * theta.sin(),
                    radius * phi.cos(),
                )
            },
            [0.0, TAU],
            [0.0, std::f32::consts::PI],
        )
    }
}

/// A torus with the given major (ring) and minor (tube) radii, as a parametric
/// [`Surface`]. Port of manim CE's `Torus`.
pub struct Torus;

impl Torus {
    /// A torus with `major` ring radius and `minor` tube radius.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(major: f32, minor: f32) -> Surface {
        Surface::new(
            move |u, v| {
                let ring = major + minor * v.cos();
                Point::new(ring * u.cos(), ring * u.sin(), minor * v.sin())
            },
            [0.0, TAU],
            [0.0, TAU],
        )
    }
}

/// A small sphere marking a point in 3D. Port of manim CE's `Dot3D`.
pub struct Dot3D;

/// The default radius of a [`Dot3D`].
pub const DOT3D_RADIUS: f32 = 0.08;

impl Dot3D {
    /// A dot at the origin.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Surface {
        Self::at(Point::ZERO)
    }

    /// A dot centered at `center`.
    pub fn at(center: Point) -> Surface {
        let r = DOT3D_RADIUS;
        Surface::new(
            move |theta, phi| {
                center
                    + Point::new(
                        r * phi.sin() * theta.cos(),
                        r * phi.sin() * theta.sin(),
                        r * phi.cos(),
                    )
            },
            [0.0, TAU],
            [0.0, std::f32::consts::PI],
        )
        .with_resolution(8, 6)
    }
}

// ---------------------------------------------------------------------------
// Face-list solids (lateral surface + caps).
// ---------------------------------------------------------------------------

/// A cylinder of the given radius and height, centered at the origin with its
/// axis along `OUT` (z), including top and bottom caps. Port of manim CE's
/// `Cylinder`.
///
/// ```
/// use manim_core::threed::Cylinder;
/// use manim_core::mobject::Mobject;
/// use manim_core::scene_state::SceneState;
/// let mut scene = SceneState::new();
/// let cyl = Cylinder::new(1.0, 2.0);
/// // res lateral quads + 2 caps.
/// let group = cyl.add_to(&mut scene);
/// assert!(scene.get_dyn(group.erase()).data().children.len() >= 3);
/// ```
pub struct Cylinder {
    radius: f32,
    height: f32,
    resolution: usize,
    colors: Vec<Color>,
}

impl Cylinder {
    /// A cylinder of `radius` and `height`.
    pub fn new(radius: f32, height: f32) -> Self {
        Self {
            radius,
            height,
            resolution: DEFAULT_SOLID_RESOLUTION,
            colors: default_checkerboard(),
        }
    }

    /// Sets the angular resolution.
    pub fn with_resolution(mut self, resolution: usize) -> Self {
        self.resolution = resolution.max(3);
        self
    }

    /// The lateral quad faces plus the two cap polygons.
    pub fn faces(&self) -> Vec<Vec<Point>> {
        let hz = self.height / 2.0;
        let bottom = ring(Point::new(0.0, 0.0, -hz), self.radius, OUT, self.resolution);
        let top = ring(Point::new(0.0, 0.0, hz), self.radius, OUT, self.resolution);
        let mut faces = Vec::new();
        for k in 0..self.resolution {
            let k1 = (k + 1) % self.resolution;
            faces.push(vec![bottom[k], bottom[k1], top[k1], top[k]]);
        }
        faces.push(bottom); // bottom cap
        faces.push(top); // top cap
        faces
    }

    /// Adds the cylinder to `scene`, returning the face group.
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        add_face_group(scene, &self.faces(), &self.colors, 1.0)
    }
}

/// A cone with a circular base (radius `base_radius`) at `z = 0` and apex at
/// `z = height`, including the base cap. Port of manim CE's `Cone`.
pub struct Cone {
    base_radius: f32,
    height: f32,
    resolution: usize,
    colors: Vec<Color>,
}

impl Cone {
    /// A cone of `base_radius` and `height`.
    pub fn new(base_radius: f32, height: f32) -> Self {
        Self {
            base_radius,
            height,
            resolution: DEFAULT_SOLID_RESOLUTION,
            colors: default_checkerboard(),
        }
    }

    /// Sets the angular resolution.
    pub fn with_resolution(mut self, resolution: usize) -> Self {
        self.resolution = resolution.max(3);
        self
    }

    /// The lateral triangle faces plus the base polygon.
    pub fn faces(&self) -> Vec<Vec<Point>> {
        let base = ring(Point::ZERO, self.base_radius, OUT, self.resolution);
        let apex = Point::new(0.0, 0.0, self.height);
        let mut faces = Vec::new();
        for k in 0..self.resolution {
            let k1 = (k + 1) % self.resolution;
            faces.push(vec![apex, base[k], base[k1]]);
        }
        faces.push(base);
        faces
    }

    /// Adds the cone to `scene`, returning the face group.
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        add_face_group(scene, &self.faces(), &self.colors, 1.0)
    }
}

/// An axis-aligned cube of the given side length, centered at the origin (6
/// square faces). Port of manim CE's `Cube`.
///
/// ```
/// use manim_core::threed::Cube;
/// let cube = Cube::new(2.0);
/// // Six faces, each centroid one unit out along an axis.
/// let faces = cube.faces();
/// assert_eq!(faces.len(), 6);
/// for face in &faces {
///     let c = face.iter().copied().fold(manim_math::Point::ZERO, |a, b| a + b) / 4.0;
///     assert!((c.length() - 1.0).abs() < 1e-5);
/// }
/// ```
pub struct Cube {
    side: f32,
    colors: Vec<Color>,
}

impl Cube {
    /// A cube of `side` length.
    pub fn new(side: f32) -> Self {
        Self {
            side,
            colors: default_checkerboard(),
        }
    }

    /// The six square faces (outward-wound).
    pub fn faces(&self) -> Vec<Vec<Point>> {
        box_faces([self.side, self.side, self.side])
    }

    /// Adds the cube to `scene`, returning the face group.
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        add_face_group(scene, &self.faces(), &self.colors, 1.0)
    }
}

/// An axis-aligned rectangular box of the given `[width, height, depth]`,
/// centered at the origin. Port of manim CE's `Prism`.
pub struct Prism {
    dims: [f32; 3],
    colors: Vec<Color>,
}

impl Prism {
    /// A box of `[width, height, depth]`.
    pub fn new(dims: [f32; 3]) -> Self {
        Self {
            dims,
            colors: default_checkerboard(),
        }
    }

    /// The six rectangular faces (outward-wound).
    pub fn faces(&self) -> Vec<Vec<Point>> {
        box_faces(self.dims)
    }

    /// Adds the prism to `scene`, returning the face group.
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        add_face_group(scene, &self.faces(), &self.colors, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Line / arrow.
// ---------------------------------------------------------------------------

/// A straight line between two 3D points. Port of manim CE's `Line3D`.
///
/// This returns an ordinary [`Line`] whose endpoints carry z-coordinates. We
/// have no true tube geometry, so 3D "thickness" is the 2D stroke width; a
/// cylindrical `Line3D` body is deferred.
pub struct Line3D;

impl Line3D {
    /// A 3D line from `start` to `end`, with a slightly thicker default stroke.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(start: Point, end: Point) -> Line {
        let mut line = Line::new(start, end);
        line.set_stroke(manim_color::BLUE_D, 6.0, 1.0);
        line
    }
}

/// A 3D arrow: a line shaft with a conical tip. Port of manim CE's `Arrow3D`.
pub struct Arrow3D;

/// Default tip length of an [`Arrow3D`].
pub const ARROW3D_TIP_LENGTH: f32 = 0.35;
/// Default tip base radius of an [`Arrow3D`].
pub const ARROW3D_TIP_RADIUS: f32 = 0.12;

impl Arrow3D {
    /// Adds a 3D arrow from `start` to `end` (shaft line + conical tip) to
    /// `scene`, returning the group.
    ///
    /// ```
    /// use manim_core::threed::Arrow3D;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// use manim_math::{Point, OUT};
    /// let mut scene = SceneState::new();
    /// let arrow = Arrow3D::of(&mut scene, Point::ZERO, 3.0 * OUT);
    /// // Shaft + tip faces under the group.
    /// assert!(scene.family(arrow.erase()).len() > 2);
    /// ```
    pub fn of(scene: &mut SceneState, start: Point, end: Point) -> MobjectId<VGroup> {
        let dir = normalize_or(end - start, OUT);
        let tip_base = end - dir * ARROW3D_TIP_LENGTH;
        let shaft = Line3D::new(start, tip_base);
        let shaft_id = scene.add(shaft).erase();

        // Conical tip: apex at `end`, base ring perpendicular to `dir`.
        let base = ring(tip_base, ARROW3D_TIP_RADIUS, dir, DEFAULT_SOLID_RESOLUTION);
        let mut tip_faces: Vec<Vec<Point>> = Vec::new();
        for k in 0..base.len() {
            let k1 = (k + 1) % base.len();
            tip_faces.push(vec![end, base[k], base[k1]]);
        }
        tip_faces.push(base);

        let group = scene.add(VGroup::new());
        scene.add_child(group.erase(), shaft_id);
        for face in &tip_faces {
            let child = scene.add(VMobject::new(
                super::face_path(face),
                Style::filled(manim_color::BLUE_D),
            ));
            scene.add_child(group.erase(), child.erase());
        }
        group
    }
}

// ---------------------------------------------------------------------------
// Geometry helpers.
// ---------------------------------------------------------------------------

/// The six outward-wound faces of an axis-aligned box of `[w, h, d]` centered at
/// the origin.
fn box_faces(dims: [f32; 3]) -> Vec<Vec<Point>> {
    let [w, h, d] = dims;
    let (x, y, z) = (w / 2.0, h / 2.0, d / 2.0);
    let p = |sx: f32, sy: f32, sz: f32| Point::new(sx * x, sy * y, sz * z);
    vec![
        vec![
            p(1.0, -1.0, -1.0),
            p(1.0, 1.0, -1.0),
            p(1.0, 1.0, 1.0),
            p(1.0, -1.0, 1.0),
        ], // +x
        vec![
            p(-1.0, -1.0, -1.0),
            p(-1.0, -1.0, 1.0),
            p(-1.0, 1.0, 1.0),
            p(-1.0, 1.0, -1.0),
        ], // -x
        vec![
            p(-1.0, 1.0, -1.0),
            p(-1.0, 1.0, 1.0),
            p(1.0, 1.0, 1.0),
            p(1.0, 1.0, -1.0),
        ], // +y
        vec![
            p(-1.0, -1.0, -1.0),
            p(1.0, -1.0, -1.0),
            p(1.0, -1.0, 1.0),
            p(-1.0, -1.0, 1.0),
        ], // -y
        vec![
            p(-1.0, -1.0, 1.0),
            p(1.0, -1.0, 1.0),
            p(1.0, 1.0, 1.0),
            p(-1.0, 1.0, 1.0),
        ], // +z
        vec![
            p(-1.0, -1.0, -1.0),
            p(-1.0, 1.0, -1.0),
            p(1.0, 1.0, -1.0),
            p(1.0, -1.0, -1.0),
        ], // -z
    ]
}

/// A ring of `res` points of `radius` around `center`, in the plane whose normal
/// is `axis`.
fn ring(center: Point, radius: f32, axis: Point, res: usize) -> Vec<Point> {
    let n = normalize_or(axis, OUT);
    // Two unit vectors spanning the plane perpendicular to `n`.
    let seed = if n.x.abs() < 0.9 {
        Point::new(1.0, 0.0, 0.0)
    } else {
        Point::new(0.0, 1.0, 0.0)
    };
    let a = normalize_or(n.cross(seed), Point::new(1.0, 0.0, 0.0));
    let b = n.cross(a);
    (0..res)
        .map(|k| {
            let ang = TAU * k as f32 / res as f32;
            center + (a * ang.cos() + b * ang.sin()) * radius
        })
        .collect()
}

/// Normalizes `v`, falling back to `fallback` for a near-zero vector.
fn normalize_or(v: Point, fallback: Point) -> Point {
    let len = v.length();
    if len > 1e-9 {
        v / len
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_radii() {
        let s = Sphere::new(2.5);
        for face in s.faces() {
            for c in face {
                assert!((c.length() - 2.5).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn cube_has_six_faces_with_unit_centroids() {
        let cube = Cube::new(2.0);
        let faces = cube.faces();
        assert_eq!(faces.len(), 6);
        for face in &faces {
            let centroid = face.iter().copied().fold(Point::ZERO, |a, b| a + b) / 4.0;
            assert!(
                (centroid.length() - 1.0).abs() < 1e-5,
                "centroid {centroid:?}"
            );
        }
    }

    #[test]
    fn torus_bbox_matches_major_plus_minor() {
        let (major, minor) = (3.0, 1.0);
        let t = Torus::new(major, minor);
        let mut max_xy = 0.0_f32;
        let mut max_z = 0.0_f32;
        for face in t.faces() {
            for c in face {
                max_xy = max_xy.max((c.x * c.x + c.y * c.y).sqrt());
                max_z = max_z.max(c.z.abs());
            }
        }
        assert!((max_xy - (major + minor)).abs() < 1e-2, "xy {max_xy}");
        assert!((max_z - minor).abs() < 1e-2, "z {max_z}");
    }

    #[test]
    fn cylinder_face_count() {
        let cyl = Cylinder::new(1.0, 2.0).with_resolution(12);
        // 12 lateral + 2 caps.
        assert_eq!(cyl.faces().len(), 14);
    }

    #[test]
    fn cone_apex_and_base() {
        let cone = Cone::new(1.0, 3.0).with_resolution(10);
        let faces = cone.faces();
        assert_eq!(faces.len(), 11); // 10 lateral + base
                                     // Every lateral triangle shares the apex at (0,0,3).
        let apex = Point::new(0.0, 0.0, 3.0);
        for face in faces.iter().take(10) {
            assert!(face.iter().any(|p| (*p - apex).length() < 1e-5));
        }
    }

    #[test]
    fn arrow3d_tip_reaches_end() {
        let mut scene = SceneState::new();
        let end = 3.0 * OUT;
        let arrow = Arrow3D::of(&mut scene, Point::ZERO, end);
        // The group's bounding box reaches the arrow tip.
        assert!((scene.family_bounding_box(arrow.erase()).max.z - end.z).abs() < 1e-4);
    }

    #[test]
    fn dot3d_is_small_sphere() {
        let d = Dot3D::at(Point::new(1.0, 0.0, 0.0));
        for face in d.faces() {
            for c in face {
                assert!(((c - Point::new(1.0, 0.0, 0.0)).length() - DOT3D_RADIUS).abs() < 1e-3);
            }
        }
    }
}
