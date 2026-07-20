//! Separatrices: the four orbits that leave (or enter) a saddle.
//!
//! A saddle's stable manifold is the set of points that flow *into* it, its
//! unstable manifold the set that flows out. Both are tangent at the saddle to
//! the corresponding eigenvector, which gives the standard construction: step a
//! hair off the saddle along `±v`, then integrate — forward along the unstable
//! eigenvector, **backward** along the stable one, since the stable manifold is
//! the unstable manifold of the time-reversed system.
//!
//! Those four branches are the skeleton of a phase portrait: they partition the
//! plane into basins, and every other orbit is trapped between them.

use manim_core::geometry::{VGroup, VMobject};
use manim_core::graphing::Axes;
use manim_core::mobject::{AnyId, Buildable, MobjectId};
use manim_core::prelude::{Color, YELLOW};
use manim_core::scene_state::SceneState;
use manim_math::path::Path;

use crate::equilibria::Equilibrium;
use crate::{trajectory, PlanarSystem};

/// Which manifold a branch belongs to.
///
/// ```
/// use manim_dynamics::separatrix::Manifold;
/// assert!(Manifold::Stable.is_backwards());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Manifold {
    /// Orbits that flow **into** the saddle (traced by integrating backwards).
    Stable,
    /// Orbits that flow **out of** the saddle (traced forwards).
    Unstable,
}

impl Manifold {
    /// Whether tracing this manifold means integrating backwards in time.
    ///
    /// ```
    /// use manim_dynamics::separatrix::Manifold;
    /// assert!(!Manifold::Unstable.is_backwards());
    /// ```
    pub fn is_backwards(self) -> bool {
        self == Manifold::Stable
    }
}

/// One separatrix branch: a manifold, a side, and the orbit itself.
#[derive(Clone, Debug)]
pub struct Branch {
    /// Which manifold this branch belongs to.
    pub manifold: Manifold,
    /// `+1` for the `+v` side of the eigenvector, `−1` for the `−v` side.
    pub side: i32,
    /// The orbit in data coordinates, starting at the offset point.
    pub points: Vec<[f64; 2]>,
}

/// Traces all four separatrix branches of a saddle.
///
/// `offset` is how far along the eigenvector to start (small enough that the
/// linearisation is still accurate, large enough to escape the saddle's
/// stagnation); `dt` and `steps` set the integration length. Returns an empty
/// vector for any equilibrium that is not a saddle.
///
/// ```
/// use manim_dynamics::equilibria::find_equilibria;
/// use manim_dynamics::separatrix::{separatrices, Manifold};
/// use manim_dynamics::Pendulum;
/// let p = Pendulum { damping: 0.0 };
/// let saddle = find_equilibria(&p, (2.0, 4.0), (-1.0, 1.0), 9)[0];
/// let branches = separatrices(&p, &saddle, 1e-4, 0.02, 200);
/// assert_eq!(branches.len(), 4);
/// assert_eq!(branches.iter().filter(|b| b.manifold == Manifold::Stable).count(), 2);
/// ```
pub fn separatrices<Sy: PlanarSystem + ?Sized>(
    system: &Sy,
    saddle: &Equilibrium,
    offset: f64,
    dt: f64,
    steps: usize,
) -> Vec<Branch> {
    let Some((stable_dir, unstable_dir)) = saddle.saddle_directions() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(4);
    for (manifold, dir) in [
        (Manifold::Stable, stable_dir),
        (Manifold::Unstable, unstable_dir),
    ] {
        for side in [1, -1] {
            let s = side as f64 * offset;
            let start = [saddle.point[0] + dir[0] * s, saddle.point[1] + dir[1] * s];
            let step = if manifold.is_backwards() { -dt } else { dt };
            out.push(Branch {
                manifold,
                side,
                points: trajectory(system, start, step, steps),
            });
        }
    }
    out
}

/// Draws separatrix branches on `axes`, clipped to the axes' window.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_dynamics::equilibria::find_equilibria;
/// use manim_dynamics::separatrix::{add_separatrices, separatrices};
/// use manim_dynamics::Pendulum;
/// let axes = Axes::new([-4.0, 4.0, 1.0], [-3.0, 3.0, 1.0]);
/// let p = Pendulum { damping: 0.0 };
/// let saddle = find_equilibria(&p, (2.0, 4.0), (-1.0, 1.0), 9)[0];
/// let branches = separatrices(&p, &saddle, 1e-4, 0.02, 400);
/// let mut scene = SceneState::new();
/// let g = add_separatrices(&mut scene, &axes, &branches, YELLOW);
/// assert!(scene.contains(g));
/// ```
pub fn add_separatrices(
    scene: &mut SceneState,
    axes: &Axes,
    branches: &[Branch],
    color: Color,
) -> MobjectId<VGroup> {
    let coords = axes.coords();
    let (x0, x1) = (coords.x_range[0] as f64, coords.x_range[1] as f64);
    let (y0, y1) = (coords.y_range[0] as f64, coords.y_range[1] as f64);
    let mut members: Vec<AnyId> = Vec::new();
    for branch in branches {
        let pts: Vec<_> = branch
            .points
            .iter()
            .take_while(|p| p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1)
            .map(|p| axes.c2p(p[0] as f32, p[1] as f32))
            .collect();
        if pts.len() < 2 {
            continue;
        }
        members.push(
            scene
                .add(
                    VMobject::from_path(Path::from_corners(&pts, false))
                        .with_stroke(color, 3.5, 1.0),
                )
                .erase(),
        );
    }
    VGroup::of(scene, members)
}

/// The default separatrix colour (yellow, matching the saddle marker).
///
/// ```
/// use manim_core::prelude::YELLOW;
/// assert_eq!(manim_dynamics::separatrix::default_color(), YELLOW);
/// ```
pub fn default_color() -> Color {
    YELLOW
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equilibria::find_equilibria;
    use crate::{value, Pendulum};
    use std::f64::consts::PI;

    fn pendulum_saddle() -> (Pendulum, Equilibrium) {
        let p = Pendulum { damping: 0.0 };
        let eq = find_equilibria(&p, (2.0, 4.0), (-1.0, 1.0), 9)[0];
        (p, eq)
    }

    #[test]
    fn the_saddle_has_four_branches_two_per_manifold() {
        let (p, saddle) = pendulum_saddle();
        let branches = separatrices(&p, &saddle, 1e-5, 0.02, 100);
        assert_eq!(branches.len(), 4);
        for m in [Manifold::Stable, Manifold::Unstable] {
            let sides: Vec<i32> = branches
                .iter()
                .filter(|b| b.manifold == m)
                .map(|b| b.side)
                .collect();
            assert_eq!(sides.len(), 2);
            assert!(sides.contains(&1) && sides.contains(&-1));
        }
    }

    #[test]
    fn unstable_branches_leave_and_stable_branches_arrive() {
        let (p, saddle) = pendulum_saddle();
        let branches = separatrices(&p, &saddle, 1e-5, 0.02, 500);
        for b in &branches {
            let d = |q: &[f64; 2]| {
                ((q[0] - saddle.point[0]).powi(2) + (q[1] - saddle.point[1]).powi(2)).sqrt()
            };
            let start = d(&b.points[0]);
            let end = d(b.points.last().unwrap());
            // Both manifolds are traced *away* from the saddle (the stable one in
            // reverse time), so the orbit always gets farther from it.
            assert!(
                end > start * 10.0,
                "{:?}/{}: {start} → {end}",
                b.manifold,
                b.side
            );
        }
    }

    #[test]
    fn the_pendulum_separatrix_has_the_energy_of_the_inverted_state() {
        // The undamped pendulum conserves E = ½ω² − cos θ. The separatrix leaves
        // the saddle at (π, 0), where E = 1, so every point on it has E = 1 —
        // that is exactly the orbit dividing libration from rotation.
        let (p, saddle) = pendulum_saddle();
        assert!((saddle.point[0] - PI).abs() < 1e-9);
        let branches = separatrices(&p, &saddle, 1e-6, 0.01, 600);
        for b in &branches {
            for q in &b.points {
                let e = 0.5 * q[1] * q[1] - q[0].cos();
                assert!((e - 1.0).abs() < 1e-5, "E = {e} at {q:?}");
            }
        }
    }

    #[test]
    fn branches_start_along_their_eigenvectors() {
        let (p, saddle) = pendulum_saddle();
        let (stable, unstable) = saddle.saddle_directions().unwrap();
        let branches = separatrices(&p, &saddle, 1e-4, 0.02, 10);
        for b in &branches {
            let dir = match b.manifold {
                Manifold::Stable => stable,
                Manifold::Unstable => unstable,
            };
            let off = [
                b.points[0][0] - saddle.point[0],
                b.points[0][1] - saddle.point[1],
            ];
            let n = (off[0] * off[0] + off[1] * off[1]).sqrt();
            let cos = (off[0] * dir[0] + off[1] * dir[1]) / n;
            assert!((cos.abs() - 1.0).abs() < 1e-9, "not along the eigenvector");
            assert!((n - 1e-4).abs() < 1e-12);
        }
    }

    #[test]
    fn a_non_saddle_has_no_separatrices() {
        let p = Pendulum { damping: 0.4 };
        let eq = find_equilibria(&p, (-1.0, 1.0), (-1.0, 1.0), 9)[0];
        assert!(separatrices(&p, &eq, 1e-4, 0.02, 10).is_empty());
        // Sanity: the field really does vanish there.
        assert!(value(&p, eq.point[0], eq.point[1])[1].abs() < 1e-12);
    }
}
