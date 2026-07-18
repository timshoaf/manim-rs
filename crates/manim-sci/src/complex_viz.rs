//! A complex-analysis visualization kit: conformal grid images, zero/pole
//! markers, branch-cut indicators, and the Riemann sphere.
//!
//! Domain-coloring *materials* (phase→hue shading) arrive with the render-side
//! material system (S1); this module builds the geometry those visualizers sit
//! on, plus the stereographic projection that ties the plane to the sphere.

use manim_core::geometry::{DashedLine, Dot, VGroup};
use manim_core::mesh::Mesh;
use manim_core::mobject::{AnyId, Buildable, MobjectId};
use manim_core::prelude::{Point, GREEN, RED, YELLOW};
use manim_core::scene_state::SceneState;

use manim_fields::ad::Scalar;
use manim_fields::complex::Complex;
use manim_fields::map::{MapClosure, SpaceMap};

use crate::deform::DeformationGrid;

/// A complex number as a scene-space point in the `xy`-plane.
fn complex_point(z: Complex) -> Point {
    Point::new(z.re as f32, z.im as f32, 0.0)
}

/// Builds the **conformal grid image** of a [`SpaceMap`] over a rectangle: a
/// [`DeformationGrid`] pre-deformed by the map (with a faded undeformed ghost),
/// adaptively subdivided so the curved images stay smooth.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_sci::complex_viz::conformal_grid_image;
/// use manim_fields::map::SpaceMap;
/// let mut scene = Scene::new(Config::default());
/// // The image of the grid under z ↦ z².
/// let g = conformal_grid_image(scene.state_mut(), &SpaceMap::complex_power(2), [-1.5, 1.5], [-1.5, 1.5], 0.5);
/// assert!(!scene.state().get_dyn(g).data().children.is_empty());
/// ```
pub fn conformal_grid_image(
    scene: &mut SceneState,
    map: &SpaceMap,
    x_range: [f64; 2],
    y_range: [f64; 2],
    step: f64,
) -> MobjectId<VGroup> {
    DeformationGrid::new(x_range, y_range, step)
        .with_map(map)
        .pre_deformed()
        .with_ghost()
        .add_to(scene)
}

/// Places markers at a rational function's zeros (green dots) and poles (red
/// dots), grouped together.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_sci::complex_viz::zeros_poles_markers;
/// use manim_fields::complex::Complex;
/// let mut scene = Scene::new(Config::default());
/// let zeros = [Complex::new(1.0, 0.0)];
/// let poles = [Complex::new(0.0, 1.0), Complex::new(0.0, -1.0)];
/// let g = zeros_poles_markers(scene.state_mut(), &zeros, &poles);
/// // One zero + two poles = three marker dots.
/// assert_eq!(scene.state().get_dyn(g).data().children.len(), 3);
/// ```
pub fn zeros_poles_markers(
    scene: &mut SceneState,
    zeros: &[Complex],
    poles: &[Complex],
) -> MobjectId<VGroup> {
    let mut ids: Vec<AnyId> = Vec::new();
    for &z in zeros {
        ids.push(
            scene
                .add(Dot::at(complex_point(z)).with_fill(GREEN, 1.0))
                .erase(),
        );
    }
    for &p in poles {
        ids.push(
            scene
                .add(Dot::at(complex_point(p)).with_fill(RED, 1.0))
                .erase(),
        );
    }
    VGroup::of(scene, ids)
}

/// Draws a branch-cut indicator as a dashed segment between two complex points
/// (e.g. the principal `ln`'s cut along the negative real axis).
///
/// ```
/// use manim_core::prelude::*;
/// use manim_sci::complex_viz::branch_cut;
/// use manim_fields::complex::Complex;
/// let mut scene = Scene::new(Config::default());
/// let cut = branch_cut(scene.state_mut(), Complex::new(-3.0, 0.0), Complex::new(0.0, 0.0));
/// assert!(scene.state().contains(cut));
/// ```
pub fn branch_cut(scene: &mut SceneState, from: Complex, to: Complex) -> AnyId {
    scene
        .add(DashedLine::new(complex_point(from), complex_point(to)).with_stroke(YELLOW, 3.0, 1.0))
        .erase()
}

/// The Riemann sphere: a unit mesh sphere plus the stereographic maps that carry
/// the extended complex plane onto it (projection from the north pole).
///
/// Texturing (domain coloring wrapped on the sphere) is deferred to the material
/// system (S2b); this provides the geometry and the exact projection.
pub struct RiemannSphere;

impl RiemannSphere {
    /// Adds the unit mesh sphere to the scene.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_sci::complex_viz::RiemannSphere;
    /// let mut scene = Scene::new(Config::default());
    /// let s = RiemannSphere::add_to(scene.state_mut());
    /// assert!(scene.state().contains(s));
    /// ```
    pub fn add_to(scene: &mut SceneState) -> MobjectId<Mesh> {
        scene.add(Mesh::sphere())
    }

    /// The stereographic projection **plane → sphere** (from the north pole): a
    /// plane point `(X, Y)` maps onto the unit sphere. Jacobian is exact (AD).
    ///
    /// ```
    /// use manim_sci::complex_viz::RiemannSphere;
    /// use manim_fields::Point;
    /// // The origin of the plane maps to the south pole (0,0,−1).
    /// let s = RiemannSphere::stereographic().apply(Point::ZERO);
    /// assert!((s - Point::new(0.0, 0.0, -1.0)).length() < 1e-12);
    /// ```
    pub fn stereographic() -> SpaceMap {
        SpaceMap::from_closure(PlaneToSphere)
    }

    /// The inverse stereographic projection **sphere → plane**.
    ///
    /// ```
    /// use manim_sci::complex_viz::RiemannSphere;
    /// use manim_fields::Point;
    /// // Project the plane point (2,1) up and back down — a round trip.
    /// let p = Point::new(2.0, 1.0, 0.0);
    /// let up = RiemannSphere::stereographic().apply(p);
    /// let down = RiemannSphere::inverse_stereographic().apply(up);
    /// assert!((down - p).length() < 1e-9);
    /// ```
    pub fn inverse_stereographic() -> SpaceMap {
        SpaceMap::from_closure(SphereToPlane)
    }
}

/// Plane → unit sphere (north-pole stereographic).
struct PlaneToSphere;
impl MapClosure for PlaneToSphere {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> [S; 3] {
        let (x, y) = (p[0], p[1]);
        let r2 = x * x + y * y;
        let denom = r2 + S::constant(1.0);
        [
            x.scale(2.0) / denom,
            y.scale(2.0) / denom,
            (r2 - S::constant(1.0)) / denom,
        ]
    }
}

/// Unit sphere → plane (inverse north-pole stereographic).
struct SphereToPlane;
impl MapClosure for SphereToPlane {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> [S; 3] {
        let (x, y, z) = (p[0], p[1], p[2]);
        let denom = S::constant(1.0) - z;
        [x / denom, y / denom, S::constant(0.0)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_fields::Point;

    #[test]
    fn stereographic_round_trips() {
        let m = RiemannSphere::stereographic();
        let inv = RiemannSphere::inverse_stereographic();
        for p in [
            Point::new(0.0, 0.0, 0.0),
            Point::new(2.0, -1.0, 0.0),
            Point::new(-0.5, 0.3, 0.0),
        ] {
            let sphere = m.apply(p);
            // Image lies on the unit sphere.
            assert!(
                (sphere.length() - 1.0).abs() < 1e-12,
                "off sphere: {sphere:?}"
            );
            // Round trip back to the plane.
            assert!(
                (inv.apply(sphere) - p).length() < 1e-9,
                "round trip failed at {p:?}"
            );
        }
    }

    #[test]
    fn conformal_map_preserves_angles() {
        // z² is holomorphic ⇒ conformal: the images of two grid directions meet
        // at the same angle as the originals (a right angle here).
        let sq = SpaceMap::complex_power(2);
        let p = Point::new(0.8, 0.5, 0.0);
        let j = sq.jacobian(p);
        let du = manim_fields::Point::new(j.x_axis.x, j.x_axis.y, 0.0); // image of x-dir
        let dv = manim_fields::Point::new(j.y_axis.x, j.y_axis.y, 0.0); // image of y-dir
        let cos = du.dot(dv) / (du.length() * dv.length());
        // Originals (x-dir, y-dir) are orthogonal; images stay orthogonal.
        assert!(cos.abs() < 1e-3, "angle not preserved: cos={cos}");
    }
}
