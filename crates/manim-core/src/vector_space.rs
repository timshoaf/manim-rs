//! Vector-space scene helpers: quick vector/plane/axes construction and a
//! [`LinearTransformationScene`] that animates a plane and its basis vectors
//! through a 2×2 matrix. Port of the 2D half of manim CE's `VectorScene` /
//! `LinearTransformationScene` (FE-109).

use glam::Mat3;

use manim_color::{Color, GREEN, RED, YELLOW};
use manim_math::{Point, ORIGIN, RIGHT, UP};

use crate::animations::ApplyMatrix;
use crate::error::Result;
use crate::geometry::Arrow;
use crate::graphing::{Axes, NumberPlane};
use crate::mobject::{AnyId, Buildable, MobjectId};
use crate::scene::Scene;
use crate::scene_state::SceneState;

/// CE's `i_hat` (x basis) color.
pub const I_HAT_COLOR: Color = GREEN;
/// CE's `j_hat` (y basis) color.
pub const J_HAT_COLOR: Color = RED;
/// CE's default vector color.
pub const VECTOR_COLOR: Color = YELLOW;

/// Curves inserted into the plane grid before a transform (see
/// [`LinearTransformationScene::apply_matrix`]).
const PLANE_SUBDIVISIONS: usize = 8;

/// Adds a vector arrow from the origin to `coords`, in CE's default vector color.
///
/// ```
/// use manim_core::vector_space::add_vector;
/// use manim_core::scene_state::SceneState;
/// use manim_core::mobject::Mobject;
/// use manim_math::Point;
/// let mut scene = SceneState::new();
/// let v = add_vector(&mut scene, Point::new(2.0, 1.0, 0.0));
/// assert!((scene.get(v).get_end() - Point::new(2.0, 1.0, 0.0)).length() < 1e-6);
/// ```
pub fn add_vector(scene: &mut SceneState, coords: Point) -> MobjectId<Arrow> {
    scene.add(Arrow::new(ORIGIN, coords).with_color(VECTOR_COLOR))
}

/// Adds a full-frame coordinate plane (CE's `NumberPlane` default extent).
pub fn add_plane(scene: &mut SceneState) -> MobjectId<NumberPlane> {
    scene.add(NumberPlane::new([-7.0, 7.0, 1.0], [-4.0, 4.0, 1.0]))
}

/// Adds a full-frame set of axes.
pub fn add_axes(scene: &mut SceneState) -> MobjectId<Axes> {
    scene.add(Axes::new([-7.0, 7.0, 1.0], [-4.0, 4.0, 1.0]))
}

/// The current geometric tip of a vector arrow — the arrowhead apex in the
/// **live** path, so it stays correct after the arrow has been transformed
/// (unlike [`Arrow::get_end`], which returns the original construction endpoint).
///
/// The apex is the first vertex of the arrow's tip triangle (its last subpath).
/// Falls back to the origin for a mobject with no drawable geometry.
pub fn vector_tip(scene: &SceneState, id: AnyId) -> Point {
    scene
        .get_dyn(id)
        .data()
        .path
        .subpaths
        .last()
        .and_then(|sp| sp.curves.first())
        .map(|c| c.p0)
        .unwrap_or(ORIGIN)
}

/// Builds the 3×3 embedding of a 2×2 linear map `[[a, b], [c, d]]` (rows), i.e.
/// the matrix whose columns are the images of the x/y basis vectors.
fn mat2_to_mat3(m: [[f32; 2]; 2]) -> Mat3 {
    let [[a, b], [c, d]] = m;
    // glam is column-major: col0 = image of x̂ = (a, c), col1 = image of ŷ = (b, d).
    Mat3::from_cols_array(&[a, c, 0.0, b, d, 0.0, 0.0, 0.0, 1.0])
}

/// A scene set up for visualizing 2×2 linear transformations: a faded ghost plane
/// that stays put, a live coordinate plane, and the two basis vectors
/// `i_hat` (green) / `j_hat` (red). Port of the 2D core of manim CE's
/// `LinearTransformationScene`.
///
/// [`apply_matrix`](Self::apply_matrix) animates the live plane and every
/// registered mobject through a matrix simultaneously, while the ghost plane
/// remains as a reference (a simplification of CE's `show_ghost_vectors`).
pub struct LinearTransformationScene {
    plane: AnyId,
    ghost: AnyId,
    i_hat: MobjectId<Arrow>,
    j_hat: MobjectId<Arrow>,
    tracked: Vec<AnyId>,
}

impl LinearTransformationScene {
    /// Sets up the ghost plane, live plane, and basis vectors in `scene`.
    pub fn new(scene: &mut Scene) -> Self {
        // Faded reference plane that does not move.
        let ghost = scene.add(NumberPlane::new([-7.0, 7.0, 1.0], [-4.0, 4.0, 1.0]));
        scene.state_mut().set_style_family(ghost.erase(), |s| {
            s.set_opacity(0.25);
        });

        let plane = scene.add(NumberPlane::new([-7.0, 7.0, 1.0], [-4.0, 4.0, 1.0]));
        let i_hat = scene.add(Arrow::new(ORIGIN, RIGHT).with_color(I_HAT_COLOR));
        let j_hat = scene.add(Arrow::new(ORIGIN, UP).with_color(J_HAT_COLOR));

        Self {
            plane: plane.erase(),
            ghost: ghost.erase(),
            i_hat,
            j_hat,
            tracked: vec![plane.erase(), i_hat.erase(), j_hat.erase()],
        }
    }

    /// The live (transformed) plane.
    pub fn plane(&self) -> AnyId {
        self.plane
    }

    /// The faded reference plane that stays fixed.
    pub fn ghost_plane(&self) -> AnyId {
        self.ghost
    }

    /// The x basis vector.
    pub fn i_hat(&self) -> MobjectId<Arrow> {
        self.i_hat
    }

    /// The y basis vector.
    pub fn j_hat(&self) -> MobjectId<Arrow> {
        self.j_hat
    }

    /// Adds a vector that will be carried along by later
    /// [`apply_matrix`](Self::apply_matrix) calls.
    pub fn add_transformable_vector(
        &mut self,
        scene: &mut Scene,
        coords: Point,
    ) -> MobjectId<Arrow> {
        let v = add_vector(scene.state_mut(), coords);
        self.tracked.push(v.erase());
        v
    }

    /// Registers an already-added mobject to be transformed by later
    /// [`apply_matrix`](Self::apply_matrix) calls.
    pub fn add_transformable(&mut self, id: impl Into<AnyId>) {
        self.tracked.push(id.into());
    }

    /// Animates the live plane and every registered mobject through the 2×2
    /// `matrix` (rows `[[a, b], [c, d]]`) at once. The ghost plane is untouched.
    ///
    /// The plane grid is subdivided first (a few extra curves) to mirror CE.
    /// Note our [`ApplyMatrix`] interpolates as a *matrix blend*
    /// `((1-α)I + αM)`, so grid lines stay straight at every α — the subdivision
    /// is cosmetic here (it matters for CE's point-function homotopy), documented
    /// for parity.
    ///
    /// # Errors
    ///
    /// Propagates any [`Scene::play`] error.
    pub fn apply_matrix(&mut self, scene: &mut Scene, matrix: [[f32; 2]; 2]) -> Result<()> {
        let m = mat2_to_mat3(matrix);
        // Subdivide the plane grid before transforming (CE parity).
        if scene.state().contains(self.plane) {
            scene
                .state_mut()
                .get_dyn_mut(self.plane)
                .data_mut()
                .path
                .insert_n_curves(PLANE_SUBDIVISIONS);
        }
        let anims: Vec<ApplyMatrix> = self
            .tracked
            .iter()
            .filter(|id| scene.state().contains(**id))
            .map(|id| ApplyMatrix::new(*id, m))
            .collect();
        scene.play(anims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn basis_vectors_land_at_matrix_columns() {
        let mut scene = Scene::new(Config::low());
        let mut lts = LinearTransformationScene::new(&mut scene);
        // A shear + scale: x̂ -> (2, 1), ŷ -> (-1, 1).
        let matrix = [[2.0, -1.0], [1.0, 1.0]];
        lts.apply_matrix(&mut scene, matrix).unwrap();

        let i_tip = vector_tip(scene.state(), lts.i_hat().erase());
        let j_tip = vector_tip(scene.state(), lts.j_hat().erase());
        assert!(
            (i_tip - Point::new(2.0, 1.0, 0.0)).length() < 1e-3,
            "i_hat at {i_tip:?}"
        );
        assert!(
            (j_tip - Point::new(-1.0, 1.0, 0.0)).length() < 1e-3,
            "j_hat at {j_tip:?}"
        );
    }

    #[test]
    fn ghost_plane_unchanged() {
        let mut scene = Scene::new(Config::low());
        let mut lts = LinearTransformationScene::new(&mut scene);
        let ghost = lts.ghost_plane();
        let before = scene.state().family_bounding_box(ghost);
        lts.apply_matrix(&mut scene, [[2.0, 0.0], [0.0, 3.0]])
            .unwrap();
        let after = scene.state().family_bounding_box(ghost);
        assert!((before.min - after.min).length() < 1e-5);
        assert!((before.max - after.max).length() < 1e-5);
    }

    #[test]
    fn tracked_vector_transforms() {
        let mut scene = Scene::new(Config::low());
        let mut lts = LinearTransformationScene::new(&mut scene);
        let v = lts.add_transformable_vector(&mut scene, Point::new(1.0, 0.0, 0.0));
        // Scale x by 3: the vector tip moves from (1,0) to (3,0).
        lts.apply_matrix(&mut scene, [[3.0, 0.0], [0.0, 1.0]])
            .unwrap();
        let tip = vector_tip(scene.state(), v.erase());
        assert!(
            (tip - Point::new(3.0, 0.0, 0.0)).length() < 1e-3,
            "tip at {tip:?}"
        );
    }
}
