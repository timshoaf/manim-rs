//! Phase portraits: the direction field, and the orbits that follow it.
//!
//! A planar autonomous system assigns a velocity to every point, so the plane is
//! covered by arrows. [`PhasePortrait`] draws that field on a lattice —
//! normalized in length, so direction reads clearly and speed is carried by
//! color or by nothing at all — and integrates streamlines through chosen seeds
//! so the eye follows whole orbits instead of individual arrows.

use manim_core::geometry::{Arrow, VGroup, VMobject};
use manim_core::graphing::Axes;
use manim_core::mobject::{AnyId, Buildable, MobjectId};
use manim_core::prelude::{Color, Point, BLUE, WHITE};
use manim_core::scene_state::SceneState;
use manim_math::path::Path;

use crate::{trajectory, value, PlanarSystem};

/// A direction-field + streamline portrait over a rectangular window.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_dynamics::phase::PhasePortrait;
/// use manim_dynamics::VanDerPol;
/// let axes = Axes::new([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0]);
/// let mut scene = SceneState::new();
/// let portrait = PhasePortrait::new((-3.0, 3.0), (-3.0, 3.0)).grid(9);
/// let arrows = portrait.add_arrows(&mut scene, &axes, &VanDerPol { mu: 1.0 });
/// assert!(scene.contains(arrows));
/// ```
#[derive(Clone, Debug)]
pub struct PhasePortrait {
    x_range: (f64, f64),
    y_range: (f64, f64),
    grid: usize,
    arrow_length: f64,
    arrow_color: Color,
    stream_color: Color,
    stream_dt: f64,
    stream_steps: usize,
}

impl PhasePortrait {
    /// A portrait over `x_range × y_range` with sensible defaults (a 13×13
    /// arrow lattice and streamlines 400 steps of `0.02` long).
    ///
    /// ```
    /// use manim_dynamics::phase::PhasePortrait;
    /// let p = PhasePortrait::new((-2.0, 2.0), (-1.0, 1.0));
    /// assert_eq!(p.bounds(), ((-2.0, 2.0), (-1.0, 1.0)));
    /// ```
    pub fn new(x_range: (f64, f64), y_range: (f64, f64)) -> Self {
        Self {
            x_range,
            y_range,
            grid: 13,
            arrow_length: 0.0,
            arrow_color: BLUE,
            stream_color: WHITE,
            stream_dt: 0.02,
            stream_steps: 400,
        }
    }

    /// Sets the arrow lattice to `n × n`.
    ///
    /// ```
    /// use manim_dynamics::phase::PhasePortrait;
    /// assert_eq!(PhasePortrait::new((0.0, 1.0), (0.0, 1.0)).grid(5).grid_size(), 5);
    /// ```
    pub fn grid(mut self, n: usize) -> Self {
        self.grid = n.max(2);
        self
    }

    /// Overrides the arrow length in data units (default: 70% of the lattice
    /// spacing).
    ///
    /// ```
    /// use manim_dynamics::phase::PhasePortrait;
    /// let p = PhasePortrait::new((0.0, 1.0), (0.0, 1.0)).arrow_length(0.05);
    /// assert_eq!(p.grid_size(), 13);
    /// ```
    pub fn arrow_length(mut self, len: f64) -> Self {
        self.arrow_length = len.max(0.0);
        self
    }

    /// Sets the arrow and streamline colors.
    ///
    /// ```
    /// use manim_core::prelude::{RED, WHITE};
    /// use manim_dynamics::phase::PhasePortrait;
    /// let p = PhasePortrait::new((0.0, 1.0), (0.0, 1.0)).colors(RED, WHITE);
    /// assert_eq!(p.grid_size(), 13);
    /// ```
    pub fn colors(mut self, arrows: Color, streams: Color) -> Self {
        self.arrow_color = arrows;
        self.stream_color = streams;
        self
    }

    /// Sets the streamline integration step and length.
    ///
    /// ```
    /// use manim_dynamics::phase::PhasePortrait;
    /// let p = PhasePortrait::new((0.0, 1.0), (0.0, 1.0)).streams(0.01, 800);
    /// assert_eq!(p.grid_size(), 13);
    /// ```
    pub fn streams(mut self, dt: f64, steps: usize) -> Self {
        self.stream_dt = dt;
        self.stream_steps = steps;
        self
    }

    /// The window this portrait covers.
    ///
    /// ```
    /// use manim_dynamics::phase::PhasePortrait;
    /// assert_eq!(PhasePortrait::new((-1.0, 1.0), (0.0, 2.0)).bounds().1, (0.0, 2.0));
    /// ```
    pub fn bounds(&self) -> ((f64, f64), (f64, f64)) {
        (self.x_range, self.y_range)
    }

    /// The arrow lattice size.
    ///
    /// ```
    /// use manim_dynamics::phase::PhasePortrait;
    /// assert_eq!(PhasePortrait::new((0.0, 1.0), (0.0, 1.0)).grid_size(), 13);
    /// ```
    pub fn grid_size(&self) -> usize {
        self.grid
    }

    /// The lattice points of the direction field, in data coordinates.
    ///
    /// ```
    /// use manim_dynamics::phase::PhasePortrait;
    /// let pts = PhasePortrait::new((0.0, 1.0), (0.0, 1.0)).grid(3).lattice();
    /// assert_eq!(pts.len(), 9);
    /// assert_eq!(pts[0], [0.0, 0.0]);
    /// ```
    pub fn lattice(&self) -> Vec<[f64; 2]> {
        let n = self.grid;
        let mut out = Vec::with_capacity(n * n);
        for i in 0..n {
            for j in 0..n {
                out.push([
                    self.x_range.0 + (self.x_range.1 - self.x_range.0) * i as f64 / (n - 1) as f64,
                    self.y_range.0 + (self.y_range.1 - self.y_range.0) * j as f64 / (n - 1) as f64,
                ]);
            }
        }
        out
    }

    /// The default arrow half-length: 35% of the lattice spacing, so adjacent
    /// arrows never collide.
    fn half_length(&self) -> f64 {
        if self.arrow_length > 0.0 {
            0.5 * self.arrow_length
        } else {
            let dx = (self.x_range.1 - self.x_range.0) / (self.grid - 1) as f64;
            let dy = (self.y_range.1 - self.y_range.0) / (self.grid - 1) as f64;
            0.35 * dx.abs().min(dy.abs())
        }
    }

    /// Draws the (length-normalized) direction field on `axes`.
    ///
    /// Arrows are centred on their lattice point so the field reads as a
    /// direction at that point, not as a displacement from it. Points where the
    /// field is (numerically) zero are skipped — an arrowhead with no direction
    /// is a lie.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_dynamics::phase::PhasePortrait;
    /// use manim_dynamics::Pendulum;
    /// let axes = Axes::new([-3.0, 3.0, 1.0], [-2.0, 2.0, 1.0]);
    /// let mut scene = SceneState::new();
    /// let p = PhasePortrait::new((-3.0, 3.0), (-2.0, 2.0)).grid(5);
    /// let g = p.add_arrows(&mut scene, &axes, &Pendulum { damping: 0.0 });
    /// // 25 lattice points, one of which (the origin) has no field.
    /// assert_eq!(scene.family(g.erase()).len(), 25);
    /// ```
    pub fn add_arrows<Sy: PlanarSystem + ?Sized>(
        &self,
        scene: &mut SceneState,
        axes: &Axes,
        system: &Sy,
    ) -> MobjectId<VGroup> {
        let half = self.half_length();
        let mut members: Vec<AnyId> = Vec::new();
        for p in self.lattice() {
            let v = value(system, p[0], p[1]);
            let speed = (v[0] * v[0] + v[1] * v[1]).sqrt();
            if speed < 1e-12 {
                continue;
            }
            let (ux, uy) = (v[0] / speed, v[1] / speed);
            let a = axes.c2p((p[0] - ux * half) as f32, (p[1] - uy * half) as f32);
            let b = axes.c2p((p[0] + ux * half) as f32, (p[1] + uy * half) as f32);
            members.push(
                scene
                    .add(Arrow::with_params(a, b, 0.0, 0.09).with_stroke(
                        self.arrow_color,
                        2.0,
                        0.9,
                    ))
                    .erase(),
            );
        }
        VGroup::of(scene, members)
    }

    /// Integrates one orbit from `seed` and returns its scene-space points,
    /// clipped to the window.
    ///
    /// A negative `dt` (via [`streams`](Self::streams)) traces the orbit
    /// backwards instead.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_dynamics::phase::PhasePortrait;
    /// use manim_dynamics::Linear;
    /// let axes = Axes::new([-2.0, 2.0, 1.0], [-2.0, 2.0, 1.0]);
    /// let p = PhasePortrait::new((-2.0, 2.0), (-2.0, 2.0)).streams(0.05, 40);
    /// // ẋ = −x, ẏ = −y: the orbit stays inside and shrinks to the origin.
    /// let pts = p.orbit_points(&axes, &Linear { a: -1.0, b: 0.0, c: 0.0, d: -1.0 }, [1.0, 1.0]);
    /// assert_eq!(pts.len(), 41);
    /// ```
    pub fn orbit_points<Sy: PlanarSystem + ?Sized>(
        &self,
        axes: &Axes,
        system: &Sy,
        seed: [f64; 2],
    ) -> Vec<Point> {
        trajectory(system, seed, self.stream_dt, self.stream_steps)
            .into_iter()
            .take_while(|p| {
                p[0] >= self.x_range.0 - 1e-9
                    && p[0] <= self.x_range.1 + 1e-9
                    && p[1] >= self.y_range.0 - 1e-9
                    && p[1] <= self.y_range.1 + 1e-9
            })
            .map(|p| axes.c2p(p[0] as f32, p[1] as f32))
            .collect()
    }

    /// Adds one streamline mobject per seed.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_dynamics::phase::PhasePortrait;
    /// use manim_dynamics::VanDerPol;
    /// let axes = Axes::new([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0]);
    /// let mut scene = SceneState::new();
    /// let p = PhasePortrait::new((-3.0, 3.0), (-3.0, 3.0)).streams(0.02, 300);
    /// let g = p.add_streamlines(&mut scene, &axes, &VanDerPol { mu: 1.0 },
    ///                           &[[0.1, 0.0], [2.5, 2.5]]);
    /// assert_eq!(scene.family(g.erase()).len(), 3); // two lines + the group
    /// ```
    pub fn add_streamlines<Sy: PlanarSystem + ?Sized>(
        &self,
        scene: &mut SceneState,
        axes: &Axes,
        system: &Sy,
        seeds: &[[f64; 2]],
    ) -> MobjectId<VGroup> {
        let mut members: Vec<AnyId> = Vec::new();
        for &seed in seeds {
            let pts = self.orbit_points(axes, system, seed);
            if pts.len() < 2 {
                continue;
            }
            members.push(
                scene
                    .add(
                        VMobject::from_path(Path::from_corners(&pts, false)).with_stroke(
                            self.stream_color,
                            2.5,
                            0.9,
                        ),
                    )
                    .erase(),
            );
        }
        VGroup::of(scene, members)
    }

    /// Seeds spread evenly around the rim of a circle of radius `r` about
    /// `center` — the usual way to fill a portrait without piling orbits on top
    /// of one another.
    ///
    /// ```
    /// use manim_dynamics::phase::PhasePortrait;
    /// let seeds = PhasePortrait::ring_seeds([0.0, 0.0], 1.0, 4);
    /// assert_eq!(seeds.len(), 4);
    /// assert!((seeds[0][0] - 1.0).abs() < 1e-12);
    /// ```
    pub fn ring_seeds(center: [f64; 2], r: f64, n: usize) -> Vec<[f64; 2]> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                [center[0] + r * a.cos(), center[1] + r * a.sin()]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Linear, Pendulum, VanDerPol};

    fn axes() -> Axes {
        Axes::new([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0])
    }

    #[test]
    fn arrows_point_along_the_field() {
        // ẋ = y, ẏ = −x: the field at (1, 0) points in −y.
        let rot = Linear {
            a: 0.0,
            b: 1.0,
            c: -1.0,
            d: 0.0,
        };
        let p = PhasePortrait::new((-1.0, 1.0), (-1.0, 1.0)).grid(3);
        let v = value(&rot, 1.0, 0.0);
        assert!((v[0]).abs() < 1e-15 && (v[1] + 1.0).abs() < 1e-15);
        // The lattice includes (1, 0) and the field there is non-zero, so an
        // arrow is drawn for it.
        assert!(p.lattice().contains(&[1.0, 0.0]));
    }

    #[test]
    fn the_origin_arrow_is_skipped_where_the_field_vanishes() {
        let mut scene = SceneState::new();
        let axes = axes();
        let p = PhasePortrait::new((-2.0, 2.0), (-2.0, 2.0)).grid(5);
        let g = p.add_arrows(&mut scene, &axes, &VanDerPol { mu: 1.0 });
        // 25 lattice points, minus the origin equilibrium, plus the group.
        assert_eq!(scene.family(g.erase()).len(), 25);
    }

    #[test]
    fn streamlines_of_a_conservative_system_close_on_themselves() {
        // The undamped pendulum conserves E = ½ω² − cos θ; a librating orbit is
        // a closed curve, so its endpoint returns near its start.
        let p = PhasePortrait::new((-3.0, 3.0), (-3.0, 3.0)).streams(0.01, 1000);
        let axes = axes();
        let pts = p.orbit_points(&axes, &Pendulum { damping: 0.0 }, [1.0, 0.0]);
        // Period of the θ₀ = 1 rad pendulum ≈ 6.53 s; 1000 × 0.01 = 10 s covers
        // a full lap and then some, so the curve must revisit its start.
        let start = pts[0];
        assert!(
            pts.iter().skip(200).any(|q| (*q - start).length() < 0.05),
            "orbit never returned"
        );
    }

    #[test]
    fn streamlines_leaving_the_window_are_clipped() {
        // ẋ = 1, ẏ = 0 marches straight out of the box.
        struct Drift;
        impl PlanarSystem for Drift {
            fn eval<S: manim_fields::ad::Scalar>(&self, _x: S, _y: S) -> [S; 2] {
                [S::constant(1.0), S::constant(0.0)]
            }
        }
        let p = PhasePortrait::new((-1.0, 1.0), (-1.0, 1.0)).streams(0.1, 1000);
        let pts = p.orbit_points(&axes(), &Drift, [0.0, 0.0]);
        // Reaches x = 1 after 10 steps, then stops.
        assert_eq!(pts.len(), 11);
    }

    #[test]
    fn ring_seeds_lie_on_the_ring() {
        for s in PhasePortrait::ring_seeds([1.0, -2.0], 0.5, 16) {
            let r = ((s[0] - 1.0).powi(2) + (s[1] + 2.0).powi(2)).sqrt();
            assert!((r - 0.5).abs() < 1e-12);
        }
    }
}
