//! `manim-fields`: a standalone applied-math substrate for `manim_rust`.
//!
//! This crate has **no manim dependencies** (only [`glam`] for vectors) — it is a
//! self-contained numerical library that mathematical visualizers consume. It
//! provides:
//!
//! - [`ad`] — forward-mode automatic differentiation ([`Dual`], [`Dual2`],
//!   [`Dual3`]) over a [`Scalar`] trait, so gradients/Jacobians/Laplacians of
//!   user closures are *exact*, never finite-differenced.
//! - [`complex`] — a dependency-free [`Complex`] number with transcendentals and
//!   [`Mobius`] transforms.
//! - [`field`] — [`ScalarField`](field::ScalarField),
//!   [`VectorField3`](field::VectorField3), [`ComplexField`](field::ComplexField)
//!   and [`TensorField2`](field::TensorField2) as composable `Fn` wrappers with
//!   differential operators (grad / div / curl / laplacian) via AD.
//! - [`map`] — [`SpaceMap`](map::SpaceMap), the deformation primitive, with
//!   AD Jacobians, composition, homotopies, and flow maps.
//! - [`integrate`] — ODE integrators: [`rk4`](integrate::rk4), adaptive
//!   [`rk45`](integrate::rk45) Dormand–Prince, and symplectic
//!   [`leapfrog`](integrate::leapfrog) / [`yoshida4`](integrate::yoshida4).
//! - [`pde`] — uniform-grid heat / wave steppers and split-step Schrödinger
//!   evolution (1-D and 2-D) via [`rustfft`].
//!
//! Everything is deterministic and I/O-free.

pub mod ad;
pub mod complex;
pub mod field;
pub mod integrate;
pub mod map;
pub mod pde;

pub use ad::{Dual, Dual2, Dual3, Scalar};
pub use complex::{Complex, Mobius};

/// The point type used throughout the crate: a 3-D vector of `f64`.
///
/// `manim-fields` works in `f64` for numerical quality (symplectic energy tests,
/// PDE dispersion, second-derivative AD); visualizers convert to `f32` at the
/// mobject boundary.
///
/// ```
/// use manim_fields::Point;
/// let p = Point::new(1.0, 2.0, 3.0);
/// assert_eq!(p.x, 1.0);
/// ```
pub type Point = glam::DVec3;
