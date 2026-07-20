//! Nullclines: the curves where one component of the field vanishes.
//!
//! On the `x`-nullcline `ẋ = 0` the flow is purely vertical; on the
//! `y`-nullcline `ẏ = 0` it is purely horizontal. Their intersections are
//! exactly the equilibria, and the regions they cut the plane into each have a
//! fixed sign pattern `(sign ẋ, sign ẏ)` — which is why sketching nullclines
//! first is the standard way to read a portrait by hand.
//!
//! Both curves are traced by marching squares over the sign of the relevant
//! component, emitting line-segment subpaths so disconnected branches (the
//! pendulum's `ẏ = 0` is a whole comb of vertical lines) come out naturally.

use manim_core::geometry::VMobject;
use manim_core::graphing::Axes;
use manim_core::mobject::{Buildable, MobjectId};
use manim_core::prelude::{Color, GREEN, PURPLE};
use manim_core::scene_state::SceneState;
use manim_math::path::{Path, SubPath};

use crate::{value, PlanarSystem};

/// Which component's zero set to trace.
///
/// ```
/// use manim_dynamics::nullclines::Component;
/// assert_eq!(Component::X.index(), 0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    /// `ẋ = 0` — the flow crosses it vertically.
    X,
    /// `ẏ = 0` — the flow crosses it horizontally.
    Y,
}

impl Component {
    /// The index into the field value (`0` for `ẋ`, `1` for `ẏ`).
    ///
    /// ```
    /// use manim_dynamics::nullclines::Component;
    /// assert_eq!(Component::Y.index(), 1);
    /// ```
    pub fn index(self) -> usize {
        match self {
            Component::X => 0,
            Component::Y => 1,
        }
    }

    /// The conventional colour: green for the `ẋ` nullcline, purple for `ẏ`.
    ///
    /// ```
    /// use manim_core::prelude::GREEN;
    /// use manim_dynamics::nullclines::Component;
    /// assert_eq!(Component::X.color(), GREEN);
    /// ```
    pub fn color(self) -> Color {
        match self {
            Component::X => GREEN,
            Component::Y => PURPLE,
        }
    }
}

/// Marching-squares zero contour of one field component, as data-space segments.
///
/// `resolution` is the number of cells per axis. Each cell contributes 0, 1, or
/// (at a saddle of the component) 2 segments, with crossings placed by linear
/// interpolation along the cell edges.
///
/// ```
/// use manim_dynamics::nullclines::{nullcline_segments, Component};
/// use manim_dynamics::Pendulum;
/// // The pendulum's ẋ = y nullcline is the line y = 0.
/// let segs = nullcline_segments(&Pendulum { damping: 0.0 },
///                               Component::X, (-2.0, 2.0), (-1.0, 1.0), 40);
/// assert!(!segs.is_empty());
/// assert!(segs.iter().all(|[a, b]| a[1].abs() < 1e-9 && b[1].abs() < 1e-9));
/// ```
pub fn nullcline_segments<Sy: PlanarSystem + ?Sized>(
    system: &Sy,
    component: Component,
    x_range: (f64, f64),
    y_range: (f64, f64),
    resolution: usize,
) -> Vec<[[f64; 2]; 2]> {
    let res = resolution.max(2);
    let k = component.index();
    let dx = (x_range.1 - x_range.0) / res as f64;
    let dy = (y_range.1 - y_range.0) / res as f64;
    let f = |x: f64, y: f64| value(system, x, y)[k];

    let mut segs = Vec::new();
    for i in 0..res {
        for j in 0..res {
            let (x0, y0) = (x_range.0 + i as f64 * dx, y_range.0 + j as f64 * dy);
            let (x1, y1) = (x0 + dx, y0 + dy);
            let (bl, br, tr, tl) = (f(x0, y0), f(x1, y0), f(x1, y1), f(x0, y1));
            let mut pts: Vec<[f64; 2]> = Vec::new();
            let mut edge = |fa: f64, fb: f64, ax: f64, ay: f64, bx: f64, by: f64| {
                if (fa > 0.0) != (fb > 0.0) && (fa - fb).abs() > 1e-15 {
                    let t = fa / (fa - fb);
                    pts.push([ax + t * (bx - ax), ay + t * (by - ay)]);
                }
            };
            edge(bl, br, x0, y0, x1, y0);
            edge(br, tr, x1, y0, x1, y1);
            edge(tr, tl, x1, y1, x0, y1);
            edge(tl, bl, x0, y1, x0, y0);
            match pts.len() {
                2 => segs.push([pts[0], pts[1]]),
                4 => {
                    // Ambiguous (saddle) cell: pair the crossings up.
                    segs.push([pts[0], pts[1]]);
                    segs.push([pts[2], pts[3]]);
                }
                _ => {}
            }
        }
    }
    segs
}

/// A nullcline as a mobject on `axes`, ready to add to a scene.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_dynamics::nullclines::{nullcline, Component};
/// use manim_dynamics::VanDerPol;
/// let axes = Axes::new([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0]);
/// let m = nullcline(&axes, &VanDerPol { mu: 1.0 }, Component::X, 60);
/// assert!(!m.data().path.subpaths.is_empty());
/// ```
pub fn nullcline<Sy: PlanarSystem + ?Sized>(
    axes: &Axes,
    system: &Sy,
    component: Component,
    resolution: usize,
) -> VMobject {
    let (x_range, y_range) = axes_ranges(axes);
    let segs = nullcline_segments(system, component, x_range, y_range, resolution);
    let subpaths = segs
        .into_iter()
        .map(|[a, b]| {
            SubPath::from_corners(&[
                axes.c2p(a[0] as f32, a[1] as f32),
                axes.c2p(b[0] as f32, b[1] as f32),
            ])
        })
        .collect();
    VMobject::from_path(Path { subpaths }).with_stroke(component.color(), 3.0, 1.0)
}

/// Adds both nullclines to `scene`, returning `(x_nullcline, y_nullcline)`.
///
/// Where the two curves cross, the field vanishes entirely: those crossings are
/// the equilibria.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_dynamics::nullclines::add_nullclines;
/// use manim_dynamics::Pendulum;
/// let axes = Axes::new([-4.0, 4.0, 1.0], [-2.0, 2.0, 1.0]);
/// let mut scene = SceneState::new();
/// let (nx, ny) = add_nullclines(&mut scene, &axes, &Pendulum { damping: 0.0 }, 80);
/// assert!(scene.contains(nx) && scene.contains(ny));
/// ```
pub fn add_nullclines<Sy: PlanarSystem + ?Sized>(
    scene: &mut SceneState,
    axes: &Axes,
    system: &Sy,
    resolution: usize,
) -> (MobjectId<VMobject>, MobjectId<VMobject>) {
    let nx = nullcline(axes, system, Component::X, resolution);
    let ny = nullcline(axes, system, Component::Y, resolution);
    (scene.add(nx), scene.add(ny))
}

/// The data ranges an [`Axes`] spans, recovered from its corner mappings.
fn axes_ranges(axes: &Axes) -> ((f64, f64), (f64, f64)) {
    let coords = axes.coords();
    (
        (coords.x_range[0] as f64, coords.x_range[1] as f64),
        (coords.y_range[0] as f64, coords.y_range[1] as f64),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Pendulum, VanDerPol};

    #[test]
    fn every_emitted_point_is_a_zero_of_its_component() {
        let sys = VanDerPol { mu: 1.0 };
        for comp in [Component::X, Component::Y] {
            let segs = nullcline_segments(&sys, comp, (-3.0, 3.0), (-3.0, 3.0), 80);
            assert!(!segs.is_empty(), "{comp:?} nullcline empty");
            for [a, b] in segs {
                for p in [a, b] {
                    let v = value(&sys, p[0], p[1])[comp.index()];
                    assert!(v.abs() < 5e-2, "{comp:?} at {p:?}: value {v}");
                }
            }
        }
    }

    #[test]
    fn pendulum_x_nullcline_is_the_theta_axis() {
        let segs = nullcline_segments(
            &Pendulum { damping: 0.0 },
            Component::X,
            (-4.0, 4.0),
            (-2.0, 2.0),
            40,
        );
        assert!(!segs.is_empty());
        for [a, b] in segs {
            assert!(a[1].abs() < 1e-12 && b[1].abs() < 1e-12);
        }
    }

    #[test]
    fn pendulum_y_nullcline_is_a_comb_of_vertical_lines() {
        // ẏ = −sin θ = 0 at θ = kπ, for every ω.
        let segs = nullcline_segments(
            &Pendulum { damping: 0.0 },
            Component::Y,
            (-4.0, 4.0),
            (-2.0, 2.0),
            80,
        );
        assert!(!segs.is_empty());
        for [a, b] in segs {
            for p in [a, b] {
                let nearest = (p[0] / std::f64::consts::PI).round() * std::f64::consts::PI;
                // Marching squares places the crossing by linear interpolation, so
                // it lands within a fraction of a cell (here 0.1 wide) of kπ.
                assert!((p[0] - nearest).abs() < 1e-4, "x = {} off the comb", p[0]);
            }
        }
    }

    #[test]
    fn van_der_pol_y_nullcline_is_the_cubic() {
        // ẏ = 0 ⇔ y = x / (μ(1 − x²)).
        let mu = 1.0;
        let segs = nullcline_segments(
            &VanDerPol { mu },
            Component::Y,
            (-2.5, 2.5),
            (-4.0, 4.0),
            200,
        );
        for [a, b] in segs {
            for p in [a, b] {
                let denom = mu * (1.0 - p[0] * p[0]);
                if denom.abs() > 0.3 {
                    let want = p[0] / denom;
                    assert!(
                        (p[1] - want).abs() < 0.1,
                        "at x = {}: {} vs {want}",
                        p[0],
                        p[1]
                    );
                }
            }
        }
    }

    #[test]
    fn nullclines_cross_at_the_equilibria() {
        use crate::equilibria::find_equilibria;
        let sys = Pendulum { damping: 0.3 };
        let eqs = find_equilibria(&sys, (-4.0, 4.0), (-2.0, 2.0), 25);
        let xs = nullcline_segments(&sys, Component::X, (-4.0, 4.0), (-2.0, 2.0), 120);
        let ys = nullcline_segments(&sys, Component::Y, (-4.0, 4.0), (-2.0, 2.0), 120);
        for eq in eqs {
            // Each equilibrium has segments of *both* nullclines nearby.
            let near = |segs: &Vec<[[f64; 2]; 2]>| {
                segs.iter().any(|[a, _]| {
                    (a[0] - eq.point[0]).abs() < 0.15 && (a[1] - eq.point[1]).abs() < 0.15
                })
            };
            assert!(near(&xs) && near(&ys), "no crossing at {:?}", eq.point);
        }
    }

    #[test]
    fn mobject_geometry_lands_inside_the_axes() {
        let axes = Axes::new([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0]);
        let mut scene = SceneState::new();
        let (nx, _ny) = add_nullclines(&mut scene, &axes, &VanDerPol { mu: 1.0 }, 60);
        use manim_core::mobject::MobjectExt;
        let bb = scene.get(nx).bounding_box();
        assert!(bb.width() <= 6.0 + 1e-3 && bb.height() <= 6.0 + 1e-3);
    }
}
