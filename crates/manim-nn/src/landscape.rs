//! [`LossLandscape`]: a differentiable loss over a 2-D slice, rendered as a
//! [`HeightField`] surface, plus SGD / momentum / Adam descent trajectories.
//!
//! The loss is a [`ScalarField`] `ℝ³ → ℝ` evaluated on the `z = 0` plane, so its
//! **exact** gradient (forward-mode AD, no finite differences) drives the
//! optimizers. The surface is a height field `z = loss(x, y)` over a rectangular
//! region; a trajectory is traced as a polyline lying on that surface.
//!
//! ```
//! use manim_fields::ad::Scalar;
//! use manim_fields::field::{ScalarClosure, ScalarField};
//! use manim_nn::landscape::{LossLandscape, Optimizer};
//! use glam::DVec2;
//!
//! struct Bowl;
//! impl ScalarClosure for Bowl {
//!     fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
//!         p[0] * p[0] + p[1] * p[1] // f = x² + y²
//!     }
//! }
//! let land = LossLandscape::new(ScalarField::from_closure(Bowl), [-2.0, 2.0], [-2.0, 2.0]);
//! let path = land.descend(DVec2::new(1.5, -1.0), Optimizer::Sgd { lr: 0.1 }, 200);
//! assert!(path.last().unwrap().length() < 1e-3); // rolled into the minimum
//! ```

use glam::DVec2;
use manim_core::geometry::{Line, VGroup};
use manim_core::mesh::HeightField;
use manim_core::mobject::{AnyId, MobjectId};
use manim_core::scene_state::SceneState;
use manim_fields::field::ScalarField;
use manim_fields::Point as FieldPoint;
use manim_math::Point;

/// A loss surface: a [`ScalarField`] sampled over a rectangular `(x, y)` region
/// of the `z = 0` plane.
pub struct LossLandscape {
    loss: ScalarField,
    x_range: [f64; 2],
    y_range: [f64; 2],
}

/// A first-order optimizer with its hyper-parameters. Each variant implements
/// the textbook update rule exactly (see [`LossLandscape::descend`]).
#[derive(Clone, Copy, Debug)]
pub enum Optimizer {
    /// Vanilla gradient descent: `x ← x − lr · g`.
    Sgd {
        /// Learning rate.
        lr: f64,
    },
    /// Heavy-ball momentum: `v ← β·v + g`, `x ← x − lr · v`.
    Momentum {
        /// Learning rate.
        lr: f64,
        /// Momentum coefficient `β`.
        beta: f64,
    },
    /// Adam: bias-corrected first/second moment estimates with a per-coordinate
    /// normalized step.
    Adam {
        /// Learning rate.
        lr: f64,
        /// First-moment decay `β₁`.
        beta1: f64,
        /// Second-moment decay `β₂`.
        beta2: f64,
        /// Numerical floor `ε` in the denominator.
        eps: f64,
    },
}

impl LossLandscape {
    /// A landscape from a loss field and the `(x, y)` region to view it over.
    pub fn new(loss: ScalarField, x_range: [f64; 2], y_range: [f64; 2]) -> Self {
        Self {
            loss,
            x_range,
            y_range,
        }
    }

    /// The underlying loss field.
    pub fn loss(&self) -> &ScalarField {
        &self.loss
    }

    /// The region centre in loss coordinates.
    fn center(&self) -> DVec2 {
        DVec2::new(
            0.5 * (self.x_range[0] + self.x_range[1]),
            0.5 * (self.y_range[0] + self.y_range[1]),
        )
    }

    /// The region size `(width, depth)` in loss coordinates.
    fn extent(&self) -> (f32, f32) {
        (
            (self.x_range[1] - self.x_range[0]) as f32,
            (self.y_range[1] - self.y_range[0]) as f32,
        )
    }

    /// The loss value at loss-coordinate `(x, y)`.
    fn loss_at(&self, x: f64, y: f64) -> f64 {
        self.loss.at(FieldPoint::new(x, y, 0.0))
    }

    /// Adds the loss surface as a [`HeightField`] over the region, sampled on an
    /// `resolution.0 × resolution.1` vertex grid. The mesh is centred on the
    /// origin in scene space, with height `z = loss(x, y)`.
    ///
    /// ```
    /// use manim_core::scene_state::SceneState;
    /// use manim_fields::field::ScalarField;
    /// use manim_nn::landscape::LossLandscape;
    /// let bowl = ScalarField::coordinate(0); // any field
    /// let land = LossLandscape::new(bowl, [-2.0, 2.0], [-2.0, 2.0]);
    /// let mut scene = SceneState::new();
    /// let surf = land.add_surface(&mut scene, (16, 16));
    /// assert!(scene.contains(surf.erase()));
    /// ```
    pub fn add_surface(
        &self,
        scene: &mut SceneState,
        resolution: (usize, usize),
    ) -> MobjectId<HeightField> {
        let center = self.center();
        let loss = self.loss.clone();
        let field =
            HeightField::from_fn(resolution.0, resolution.1, self.extent(), move |sx, sy| {
                loss.at(FieldPoint::new(
                    center.x + sx as f64,
                    center.y + sy as f64,
                    0.0,
                )) as f32
            });
        scene.add(field)
    }

    /// Runs `opt` on the loss **gradient** for `steps` iterations from `start`,
    /// returning the `(x, y)` trajectory (length `steps + 1`, including `start`).
    ///
    /// The gradient is exact — the `x` and `y` components of
    /// [`ScalarField::grad`]. The three update rules are:
    ///
    /// - `Sgd`:      `x ← x − lr · g`
    /// - `Momentum`: `v ← β·v + g`,  `x ← x − lr · v`   (`v₀ = 0`)
    /// - `Adam`:     `m ← β₁·m + (1−β₁)·g`, `v ← β₂·v + (1−β₂)·g²`,
    ///   `m̂ = m/(1−β₁ᵗ)`, `v̂ = v/(1−β₂ᵗ)`, `x ← x − lr · m̂/(√v̂ + ε)`
    ///
    /// ```
    /// use glam::DVec2;
    /// use manim_fields::field::{ScalarField, UnaryOp};
    /// use manim_nn::landscape::{LossLandscape, Optimizer};
    /// // f = x²; ∂f/∂x = 2x, so SGD contracts x toward 0.
    /// let f = ScalarField::coordinate(0).map(UnaryOp::Powi(2));
    /// let land = LossLandscape::new(f, [-2.0, 2.0], [-2.0, 2.0]);
    /// let t = land.descend(DVec2::new(1.0, 0.0), Optimizer::Sgd { lr: 0.1 }, 50);
    /// assert!(t[0].x == 1.0 && t.last().unwrap().x.abs() < 1e-3);
    /// ```
    pub fn descend(&self, start: DVec2, opt: Optimizer, steps: usize) -> Vec<DVec2> {
        let mut x = start;
        let mut traj = Vec::with_capacity(steps + 1);
        traj.push(x);

        // Optimizer state (unused fields stay zero for the chosen variant).
        let mut vel = DVec2::ZERO; // momentum velocity
        let mut m = DVec2::ZERO; // Adam first moment
        let mut v = DVec2::ZERO; // Adam second moment

        for step in 1..=steps {
            let g3 = self.loss.grad(FieldPoint::new(x.x, x.y, 0.0));
            let g = DVec2::new(g3.x, g3.y);
            match opt {
                Optimizer::Sgd { lr } => {
                    x -= lr * g;
                }
                Optimizer::Momentum { lr, beta } => {
                    vel = beta * vel + g;
                    x -= lr * vel;
                }
                Optimizer::Adam {
                    lr,
                    beta1,
                    beta2,
                    eps,
                } => {
                    m = beta1 * m + (1.0 - beta1) * g;
                    v = beta2 * v + (1.0 - beta2) * (g * g);
                    let bc1 = 1.0 - beta1.powi(step as i32);
                    let bc2 = 1.0 - beta2.powi(step as i32);
                    let mhat = m / bc1;
                    let vhat = v / bc2;
                    let update = DVec2::new(
                        lr * mhat.x / (vhat.x.sqrt() + eps),
                        lr * mhat.y / (vhat.y.sqrt() + eps),
                    );
                    x -= update;
                }
            }
            traj.push(x);
        }
        traj
    }

    /// Lifts a loss-coordinate point onto the surface in scene space
    /// (`x, y` centred on the origin, `z = loss(x, y)`).
    fn on_surface(&self, p: DVec2) -> Point {
        let c = self.center();
        Point::new(
            (p.x - c.x) as f32,
            (p.y - c.y) as f32,
            self.loss_at(p.x, p.y) as f32,
        )
    }

    /// Runs [`descend`](Self::descend) and traces the trajectory as a polyline
    /// **on** the surface (each vertex lifted to `z = loss(x, y)`), grouped into
    /// a [`VGroup`] of segments.
    ///
    /// ```
    /// use glam::DVec2;
    /// use manim_core::scene_state::SceneState;
    /// use manim_fields::field::{ScalarField, UnaryOp};
    /// use manim_nn::landscape::{LossLandscape, Optimizer};
    /// let f = ScalarField::coordinate(0).map(UnaryOp::Powi(2));
    /// let land = LossLandscape::new(f, [-2.0, 2.0], [-2.0, 2.0]);
    /// let mut scene = SceneState::new();
    /// let path =
    ///     land.descend_on_surface(&mut scene, DVec2::new(1.5, 0.0), Optimizer::Sgd { lr: 0.1 }, 10);
    /// assert!(scene.contains(path.erase()));
    /// ```
    pub fn descend_on_surface(
        &self,
        scene: &mut SceneState,
        start: DVec2,
        opt: Optimizer,
        steps: usize,
    ) -> MobjectId<VGroup> {
        let pts: Vec<Point> = self
            .descend(start, opt, steps)
            .into_iter()
            .map(|p| self.on_surface(p))
            .collect();
        let mut children: Vec<AnyId> = Vec::new();
        for seg in pts.windows(2) {
            children.push(scene.add(Line::new(seg[0], seg[1])).erase());
        }
        VGroup::of(scene, children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_fields::ad::Scalar;
    use manim_fields::field::ScalarClosure;

    /// A convex quadratic bowl `f(x, y) = a·x² + b·y²` (minimum at the origin).
    struct Quadratic {
        a: f64,
        b: f64,
    }
    impl ScalarClosure for Quadratic {
        fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
            (p[0] * p[0]).scale(self.a) + (p[1] * p[1]).scale(self.b)
        }
    }

    fn bowl() -> LossLandscape {
        let loss = ScalarField::from_closure(Quadratic { a: 1.0, b: 3.0 });
        LossLandscape::new(loss, [-3.0, 3.0], [-3.0, 3.0])
    }

    #[test]
    fn all_optimizers_converge_to_minimum() {
        let land = bowl();
        let start = DVec2::new(2.5, -2.0);

        let sgd = land.descend(start, Optimizer::Sgd { lr: 0.1 }, 300);
        let mom = land.descend(
            start,
            Optimizer::Momentum {
                lr: 0.02,
                beta: 0.9,
            },
            400,
        );
        let adam = land.descend(
            start,
            Optimizer::Adam {
                lr: 0.1,
                beta1: 0.9,
                beta2: 0.999,
                eps: 1e-8,
            },
            3000,
        );

        println!("SGD      final = {:?}", sgd.last().unwrap());
        println!("Momentum final = {:?}", mom.last().unwrap());
        println!("Adam     final = {:?}", adam.last().unwrap());

        assert!(sgd.last().unwrap().length() < 1e-3, "SGD did not converge");
        assert!(
            mom.last().unwrap().length() < 1e-2,
            "Momentum did not converge"
        );
        assert!(
            adam.last().unwrap().length() < 1e-2,
            "Adam did not converge"
        );
    }

    #[test]
    fn momentum_overshoots_then_settles() {
        let land = bowl();
        let start = DVec2::new(2.5, -2.0);
        let traj = land.descend(
            start,
            Optimizer::Momentum {
                lr: 0.02,
                beta: 0.9,
            },
            400,
        );

        // Start is at x = +2.5; heavy-ball momentum carries it past the x = 0
        // minimum to the opposite (negative-x) side before settling.
        let overshoot = traj.iter().find(|p| p.x < -0.1).copied();
        println!(
            "momentum overshoot point = {:?}, final = {:?}",
            overshoot,
            traj.last().unwrap()
        );
        assert!(
            overshoot.is_some(),
            "momentum never crossed to the far side of the minimum"
        );
        assert!(
            traj.last().unwrap().length() < 1e-2,
            "momentum did not settle at the minimum"
        );
    }

    #[test]
    fn adam_steps_are_bounded() {
        let land = bowl();
        let start = DVec2::new(2.5, -2.0);
        let (lr, beta1, beta2) = (0.1, 0.9, 0.999);
        let traj = land.descend(
            start,
            Optimizer::Adam {
                lr,
                beta1,
                beta2,
                eps: 1e-8,
            },
            500,
        );

        // Adam's normalized update is bounded per coordinate by the effective
        // stepsize bound from the paper: lr · (1−β₁)/√(1−β₂).
        let bound = lr * (1.0 - beta1) / (1.0 - beta2).sqrt();
        let mut max_step = 0.0_f64;
        for seg in traj.windows(2) {
            let d = seg[1] - seg[0];
            max_step = max_step.max(d.x.abs()).max(d.y.abs());
        }
        println!("Adam max per-coordinate step = {max_step}, bound = {bound}");
        assert!(
            max_step <= bound * (1.0 + 1e-6),
            "Adam step {max_step} exceeded bound {bound}"
        );
    }
}
