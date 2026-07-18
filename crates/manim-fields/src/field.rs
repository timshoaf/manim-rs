//! Scalar, vector, complex, and tensor fields as composable, differentiable
//! values.
//!
//! A field is a function of space that knows how to differentiate itself: the
//! [`ScalarField`] and [`VectorField3`] carry their closures in a form that
//! forward-mode AD can evaluate, so [`grad`](ScalarField::grad),
//! [`laplacian`](ScalarField::laplacian), [`divergence`](VectorField3::divergence)
//! and [`curl`](VectorField3::curl) are *exact* (to floating-point roundoff),
//! never finite-differenced. Combinators (`add` / `sub` / `mul` / `scale` /
//! `map` / `translate`) build new fields whose derivatives fall out of the dual
//! arithmetic automatically.
//!
//! ```
//! use manim_fields::field::{ScalarField, UnaryOp};
//! use manim_fields::Point;
//! // f(x,y,z) = sin(x) + y·z
//! let f = ScalarField::coordinate(0)
//!     .map(UnaryOp::Sin)
//!     .add(&ScalarField::coordinate(1).mul(&ScalarField::coordinate(2)));
//! let p = Point::new(0.0, 2.0, 3.0);
//! assert!((f.at(p) - 6.0).abs() < 1e-12);           // sin0 + 2·3
//! assert!((f.grad(p) - Point::new(1.0, 3.0, 2.0)).length() < 1e-12); // (cos0, z, y)
//! ```

use std::sync::Arc;

use glam::{DMat2, DVec2, DVec3};

use crate::ad::{Dual2, Dual3, Scalar};
use crate::complex::Complex;
use crate::Point;

// ---------------------------------------------------------------------------
// Sampler plumbing: one object evaluable at f64 / Dual3 / Dual2.
// ---------------------------------------------------------------------------

/// A scalar function of three coordinates written generically over the
/// [`Scalar`] trait, so it can be evaluated for its value or any derivative.
///
/// Implement this to build an arbitrary analytic [`ScalarField`] via
/// [`ScalarField::from_closure`].
///
/// ```
/// use manim_fields::ad::Scalar;
/// use manim_fields::field::{ScalarClosure, ScalarField};
/// use manim_fields::Point;
/// struct Paraboloid;
/// impl ScalarClosure for Paraboloid {
///     fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
///         p[0] * p[0] + p[1] * p[1]
///     }
/// }
/// let f = ScalarField::from_closure(Paraboloid);
/// assert!((f.laplacian(Point::new(1.0, 1.0, 0.0)) - 4.0).abs() < 1e-9); // ∇² = 4
/// ```
pub trait ScalarClosure: Send + Sync {
    /// Evaluates the field at `p` in the chosen scalar type.
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S;
}

/// Object-safe sampler: the three concrete monomorphizations a field needs.
trait Sampler: Send + Sync {
    fn at(&self, p: DVec3) -> f64;
    fn d3(&self, p: [Dual3; 3]) -> Dual3;
    fn d2(&self, p: [Dual2; 3]) -> Dual2;
}

impl<C: ScalarClosure> Sampler for C {
    fn at(&self, p: DVec3) -> f64 {
        self.eval([p.x, p.y, p.z])
    }
    fn d3(&self, p: [Dual3; 3]) -> Dual3 {
        self.eval(p)
    }
    fn d2(&self, p: [Dual2; 3]) -> Dual2 {
        self.eval(p)
    }
}

/// A pointwise transcendental applied to a field (kept as a closed set so the
/// derivative is known exactly).
#[derive(Clone, Copy, Debug)]
pub enum UnaryOp {
    /// `exp`
    Exp,
    /// `ln`
    Ln,
    /// `sin`
    Sin,
    /// `cos`
    Cos,
    /// `tan`
    Tan,
    /// `sqrt`
    Sqrt,
    /// Real power `x^p`
    Powf(f64),
    /// Integer power `x^n`
    Powi(i32),
}

impl UnaryOp {
    /// Applies the operation in any scalar type.
    pub fn apply<S: Scalar>(self, x: S) -> S {
        match self {
            UnaryOp::Exp => x.exp(),
            UnaryOp::Ln => x.ln(),
            UnaryOp::Sin => x.sin(),
            UnaryOp::Cos => x.cos(),
            UnaryOp::Tan => x.tan(),
            UnaryOp::Sqrt => x.sqrt(),
            UnaryOp::Powf(p) => x.powf(p),
            UnaryOp::Powi(n) => x.powi(n),
        }
    }
}

// Internal sampler adapters for the primitives and combinators. -------------

struct Coord(usize);
impl Sampler for Coord {
    fn at(&self, p: DVec3) -> f64 {
        [p.x, p.y, p.z][self.0]
    }
    fn d3(&self, p: [Dual3; 3]) -> Dual3 {
        p[self.0]
    }
    fn d2(&self, p: [Dual2; 3]) -> Dual2 {
        p[self.0]
    }
}

struct Const(f64);
impl Sampler for Const {
    fn at(&self, _: DVec3) -> f64 {
        self.0
    }
    fn d3(&self, _: [Dual3; 3]) -> Dual3 {
        Dual3::constant(self.0)
    }
    fn d2(&self, _: [Dual2; 3]) -> Dual2 {
        Dual2::constant(self.0)
    }
}

/// The four elementwise binary combinations.
enum BinKind {
    Add,
    Sub,
    Mul,
    Div,
}
struct Bin(Arc<dyn Sampler>, Arc<dyn Sampler>, BinKind);
impl Bin {
    fn combine<S: Scalar>(&self, a: S, b: S) -> S {
        match self.2 {
            BinKind::Add => a + b,
            BinKind::Sub => a - b,
            BinKind::Mul => a * b,
            BinKind::Div => a / b,
        }
    }
}
impl Sampler for Bin {
    fn at(&self, p: DVec3) -> f64 {
        self.combine(self.0.at(p), self.1.at(p))
    }
    fn d3(&self, p: [Dual3; 3]) -> Dual3 {
        self.combine(self.0.d3(p), self.1.d3(p))
    }
    fn d2(&self, p: [Dual2; 3]) -> Dual2 {
        self.combine(self.0.d2(p), self.1.d2(p))
    }
}

struct Scaled(Arc<dyn Sampler>, f64);
impl Sampler for Scaled {
    fn at(&self, p: DVec3) -> f64 {
        self.0.at(p) * self.1
    }
    fn d3(&self, p: [Dual3; 3]) -> Dual3 {
        self.0.d3(p).scale(self.1)
    }
    fn d2(&self, p: [Dual2; 3]) -> Dual2 {
        self.0.d2(p).scale(self.1)
    }
}

struct Mapped(Arc<dyn Sampler>, UnaryOp);
impl Sampler for Mapped {
    fn at(&self, p: DVec3) -> f64 {
        self.1.apply(self.0.at(p))
    }
    fn d3(&self, p: [Dual3; 3]) -> Dual3 {
        self.1.apply(self.0.d3(p))
    }
    fn d2(&self, p: [Dual2; 3]) -> Dual2 {
        self.1.apply(self.0.d2(p))
    }
}

struct Translated(Arc<dyn Sampler>, DVec3);
impl Sampler for Translated {
    fn at(&self, p: DVec3) -> f64 {
        self.0.at(p - self.1)
    }
    fn d3(&self, p: [Dual3; 3]) -> Dual3 {
        // Shifting the input by a constant leaves the gradient unchanged.
        let s = self.1;
        self.0.d3([
            p[0] - Dual3::constant(s.x),
            p[1] - Dual3::constant(s.y),
            p[2] - Dual3::constant(s.z),
        ])
    }
    fn d2(&self, p: [Dual2; 3]) -> Dual2 {
        let s = self.1;
        self.0.d2([
            p[0] - Dual2::constant(s.x),
            p[1] - Dual2::constant(s.y),
            p[2] - Dual2::constant(s.z),
        ])
    }
}

// ---------------------------------------------------------------------------
// ScalarField
// ---------------------------------------------------------------------------

/// A differentiable scalar field `ℝ³ → ℝ`.
#[derive(Clone)]
pub struct ScalarField {
    s: Arc<dyn Sampler>,
}

impl ScalarField {
    /// A field from an analytic closure (see [`ScalarClosure`]).
    pub fn from_closure<C: ScalarClosure + 'static>(c: C) -> Self {
        Self { s: Arc::new(c) }
    }
    /// The coordinate field `p ↦ p[axis]` (`axis` is 0, 1, or 2).
    ///
    /// ```
    /// use manim_fields::field::ScalarField;
    /// use manim_fields::Point;
    /// let y = ScalarField::coordinate(1);
    /// assert_eq!(y.at(Point::new(4.0, 5.0, 6.0)), 5.0);
    /// ```
    pub fn coordinate(axis: usize) -> Self {
        assert!(axis < 3, "axis must be 0..3");
        Self {
            s: Arc::new(Coord(axis)),
        }
    }
    /// A constant field.
    pub fn constant(c: f64) -> Self {
        Self {
            s: Arc::new(Const(c)),
        }
    }
    /// The value at `p`.
    pub fn at(&self, p: Point) -> f64 {
        self.s.at(p)
    }
    /// The gradient `∇f` at `p`, from one [`Dual3`] evaluation.
    ///
    /// ```
    /// use manim_fields::field::{ScalarField, UnaryOp};
    /// use manim_fields::Point;
    /// // f = ln(x); ∂f/∂x = 1/x.
    /// let f = ScalarField::coordinate(0).map(UnaryOp::Ln);
    /// assert!((f.grad(Point::new(2.0, 0.0, 0.0)).x - 0.5).abs() < 1e-12);
    /// ```
    pub fn grad(&self, p: Point) -> DVec3 {
        self.s.d3(Dual3::vars(p.x, p.y, p.z)).grad
    }
    /// The Laplacian `∇²f = Σ ∂²f/∂xᵢ²` at `p` (forward-over-forward AD, one
    /// seeded axis at a time).
    ///
    /// ```
    /// use manim_fields::field::{ScalarField, UnaryOp};
    /// use manim_fields::Point;
    /// // f = x² + y² + z² ; ∇²f = 6.
    /// let sq = |a| ScalarField::coordinate(a).map(UnaryOp::Powi(2));
    /// let f = sq(0).add(&sq(1)).add(&sq(2));
    /// assert!((f.laplacian(Point::new(1.0, 2.0, 3.0)) - 6.0).abs() < 1e-9);
    /// ```
    pub fn laplacian(&self, p: Point) -> f64 {
        let c = [p.x, p.y, p.z];
        let mut lap = 0.0;
        for i in 0..3 {
            let mut arr = [
                Dual2::constant(c[0]),
                Dual2::constant(c[1]),
                Dual2::constant(c[2]),
            ];
            arr[i] = Dual2::var(c[i]);
            lap += self.s.d2(arr).d2;
        }
        lap
    }
    /// Sum of two fields.
    pub fn add(&self, other: &Self) -> Self {
        self.bin(other, BinKind::Add)
    }
    /// Difference of two fields.
    pub fn sub(&self, other: &Self) -> Self {
        self.bin(other, BinKind::Sub)
    }
    /// Product of two fields.
    pub fn mul(&self, other: &Self) -> Self {
        self.bin(other, BinKind::Mul)
    }
    /// Quotient of two fields.
    pub fn div(&self, other: &Self) -> Self {
        self.bin(other, BinKind::Div)
    }
    fn bin(&self, other: &Self, k: BinKind) -> Self {
        Self {
            s: Arc::new(Bin(self.s.clone(), other.s.clone(), k)),
        }
    }
    /// Multiplies the field by a constant.
    pub fn scale(&self, k: f64) -> Self {
        Self {
            s: Arc::new(Scaled(self.s.clone(), k)),
        }
    }
    /// Applies a pointwise transcendental (see [`UnaryOp`]).
    pub fn map(&self, op: UnaryOp) -> Self {
        Self {
            s: Arc::new(Mapped(self.s.clone(), op)),
        }
    }
    /// Precomposes with a rigid translation: `(f.translate(v))(p) = f(p − v)`.
    pub fn translate(&self, v: DVec3) -> Self {
        Self {
            s: Arc::new(Translated(self.s.clone(), v)),
        }
    }
}

// ---------------------------------------------------------------------------
// VectorField3
// ---------------------------------------------------------------------------

/// A differentiable vector field `ℝ³ → ℝ³`, stored as its three scalar
/// components so [`divergence`](Self::divergence) and [`curl`](Self::curl) reuse
/// the component gradients.
#[derive(Clone)]
pub struct VectorField3 {
    x: ScalarField,
    y: ScalarField,
    z: ScalarField,
}

impl VectorField3 {
    /// A vector field from three scalar component fields.
    pub fn from_components(x: ScalarField, y: ScalarField, z: ScalarField) -> Self {
        Self { x, y, z }
    }
    /// The value at `p`.
    ///
    /// ```
    /// use manim_fields::field::{ScalarField, VectorField3};
    /// use manim_fields::Point;
    /// // Rigid rotation field v = (−y, x, 0).
    /// let v = VectorField3::from_components(
    ///     ScalarField::coordinate(1).scale(-1.0),
    ///     ScalarField::coordinate(0),
    ///     ScalarField::constant(0.0),
    /// );
    /// assert_eq!(v.at(Point::new(1.0, 2.0, 0.0)), Point::new(-2.0, 1.0, 0.0));
    /// ```
    pub fn at(&self, p: Point) -> DVec3 {
        DVec3::new(self.x.at(p), self.y.at(p), self.z.at(p))
    }
    /// The Jacobian matrix rows (`∇vₓ, ∇v_y, ∇v_z`) at `p`.
    pub fn jacobian_rows(&self, p: Point) -> [DVec3; 3] {
        [self.x.grad(p), self.y.grad(p), self.z.grad(p)]
    }
    /// The divergence `∇·v = ∂vₓ/∂x + ∂v_y/∂y + ∂v_z/∂z`.
    ///
    /// ```
    /// use manim_fields::field::{ScalarField, VectorField3};
    /// use manim_fields::Point;
    /// // v = (x, y, z) has divergence 3.
    /// let v = VectorField3::from_components(
    ///     ScalarField::coordinate(0),
    ///     ScalarField::coordinate(1),
    ///     ScalarField::coordinate(2),
    /// );
    /// assert!((v.divergence(Point::new(1.0, 1.0, 1.0)) - 3.0).abs() < 1e-12);
    /// ```
    pub fn divergence(&self, p: Point) -> f64 {
        self.x.grad(p).x + self.y.grad(p).y + self.z.grad(p).z
    }
    /// The curl `∇×v`.
    ///
    /// ```
    /// use manim_fields::field::{ScalarField, VectorField3};
    /// use manim_fields::Point;
    /// // Rotation field v = (−y, x, 0) has curl (0, 0, 2).
    /// let v = VectorField3::from_components(
    ///     ScalarField::coordinate(1).scale(-1.0),
    ///     ScalarField::coordinate(0),
    ///     ScalarField::constant(0.0),
    /// );
    /// let c = v.curl(Point::new(0.5, 0.5, 0.0));
    /// assert!((c - Point::new(0.0, 0.0, 2.0)).length() < 1e-12);
    /// ```
    pub fn curl(&self, p: Point) -> DVec3 {
        let [gx, gy, gz] = self.jacobian_rows(p);
        DVec3::new(gz.y - gy.z, gx.z - gz.x, gy.x - gx.y)
    }
    /// The time-`dt` flow of `p` along the field, by `steps` RK4 sub-steps
    /// (autonomous field). Traces integral curves for streamlines.
    ///
    /// ```
    /// use manim_fields::field::{ScalarField, VectorField3};
    /// use manim_fields::Point;
    /// // Rotation field: flowing by π/2 rotates (1,0) to ≈(0,1).
    /// let v = VectorField3::from_components(
    ///     ScalarField::coordinate(1).scale(-1.0),
    ///     ScalarField::coordinate(0),
    ///     ScalarField::constant(0.0),
    /// );
    /// let q = v.flow(Point::new(1.0, 0.0, 0.0), std::f64::consts::FRAC_PI_2, 200);
    /// assert!((q - Point::new(0.0, 1.0, 0.0)).length() < 1e-4);
    /// ```
    pub fn flow(&self, p: Point, dt: f64, steps: usize) -> Point {
        let h = dt / steps as f64;
        let mut q = p;
        for _ in 0..steps {
            let k1 = self.at(q);
            let k2 = self.at(q + k1 * (h * 0.5));
            let k3 = self.at(q + k2 * (h * 0.5));
            let k4 = self.at(q + k3 * h);
            q += (k1 + k2 * 2.0 + k3 * 2.0 + k4) * (h / 6.0);
        }
        q
    }
    /// Adds two vector fields componentwise.
    pub fn add(&self, other: &Self) -> Self {
        Self {
            x: self.x.add(&other.x),
            y: self.y.add(&other.y),
            z: self.z.add(&other.z),
        }
    }
    /// Scales the field by a constant.
    pub fn scale(&self, k: f64) -> Self {
        Self {
            x: self.x.scale(k),
            y: self.y.scale(k),
            z: self.z.scale(k),
        }
    }
}

// ---------------------------------------------------------------------------
// ComplexField
// ---------------------------------------------------------------------------

/// A complex field `ℂ → ℂ` (the substrate for domain coloring).
#[derive(Clone)]
pub struct ComplexField {
    f: Arc<dyn Fn(Complex) -> Complex + Send + Sync>,
}

impl ComplexField {
    /// A complex field from a closure.
    ///
    /// ```
    /// use manim_fields::complex::Complex;
    /// use manim_fields::field::ComplexField;
    /// let sq = ComplexField::new(|z| z * z);
    /// assert!((sq.at(Complex::i()) - Complex::real(-1.0)).norm() < 1e-12);
    /// ```
    pub fn new(f: impl Fn(Complex) -> Complex + Send + Sync + 'static) -> Self {
        Self { f: Arc::new(f) }
    }
    /// The value at `z`.
    pub fn at(&self, z: Complex) -> Complex {
        (self.f)(z)
    }
    /// The phase (argument) at `z`, in `(−π, π]` — the hue channel of a domain
    /// coloring.
    pub fn phase(&self, z: Complex) -> f64 {
        self.at(z).arg()
    }
    /// The modulus `|f(z)|` — the brightness channel of a domain coloring.
    pub fn modulus(&self, z: Complex) -> f64 {
        self.at(z).norm()
    }
    /// The composition `self ∘ other` (`z ↦ self(other(z))`).
    pub fn compose(&self, other: &Self) -> Self {
        let a = self.f.clone();
        let b = other.f.clone();
        Self {
            f: Arc::new(move |z| a(b(z))),
        }
    }
    /// The sum of two complex fields.
    pub fn add(&self, other: &Self) -> Self {
        let a = self.f.clone();
        let b = other.f.clone();
        Self {
            f: Arc::new(move |z| a(z) + b(z)),
        }
    }
    /// Scales by a real factor.
    pub fn scale(&self, k: f64) -> Self {
        let a = self.f.clone();
        Self {
            f: Arc::new(move |z| a(z).scale(k)),
        }
    }
}

// ---------------------------------------------------------------------------
// TensorField2
// ---------------------------------------------------------------------------

/// A field of symmetric 2×2 tensors (stress, metric, second fundamental form),
/// stored by its independent entries `(t_xx, t_xy, t_yy)`.
#[derive(Clone)]
pub struct TensorField2 {
    f: Arc<dyn Fn(Point) -> [f64; 3] + Send + Sync>,
}

impl TensorField2 {
    /// A tensor field from a closure returning `(t_xx, t_xy, t_yy)`.
    ///
    /// ```
    /// use manim_fields::field::TensorField2;
    /// use manim_fields::Point;
    /// // Constant identity metric.
    /// let g = TensorField2::new(|_| [1.0, 0.0, 1.0]);
    /// let (l0, l1, _) = g.eigen(Point::ZERO);
    /// assert!((l0 - 1.0).abs() < 1e-12 && (l1 - 1.0).abs() < 1e-12);
    /// ```
    pub fn new(f: impl Fn(Point) -> [f64; 3] + Send + Sync + 'static) -> Self {
        Self { f: Arc::new(f) }
    }
    /// The tensor at `p` as a [`DMat2`].
    pub fn at(&self, p: Point) -> DMat2 {
        let [xx, xy, yy] = (self.f)(p);
        DMat2::from_cols(DVec2::new(xx, xy), DVec2::new(xy, yy))
    }
    /// The eigenvalues `(λ₀ ≥ λ₁)` and the angle (radians) of the λ₀
    /// eigenvector — the data a tensor glyph (ellipse) needs.
    pub fn eigen(&self, p: Point) -> (f64, f64, f64) {
        let [xx, xy, yy] = (self.f)(p);
        let tr = xx + yy;
        let disc = ((xx - yy) * (xx - yy) + 4.0 * xy * xy).sqrt();
        let l0 = 0.5 * (tr + disc);
        let l1 = 0.5 * (tr - disc);
        // Eigenvector of λ₀: direction (λ₀ − yy, xy); arbitrary when isotropic.
        let (ey, ex) = (l0 - yy, xy);
        let angle = if ey == 0.0 && ex == 0.0 {
            0.0
        } else {
            ey.atan2(ex)
        };
        (l0, l1, angle)
    }
    /// Adds two tensor fields.
    pub fn add(&self, other: &Self) -> Self {
        let a = self.f.clone();
        let b = other.f.clone();
        Self {
            f: Arc::new(move |p| {
                let (u, v) = (a(p), b(p));
                [u[0] + v[0], u[1] + v[1], u[2] + v[2]]
            }),
        }
    }
    /// Scales by a constant.
    pub fn scale(&self, k: f64) -> Self {
        let a = self.f.clone();
        Self {
            f: Arc::new(move |p| {
                let u = a(p);
                [u[0] * k, u[1] * k, u[2] * k]
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Time-dependent fields
// ---------------------------------------------------------------------------

/// A time-dependent scalar field `(p, t) ↦ f`.
#[derive(Clone)]
pub struct TimeScalarField {
    f: Arc<dyn Fn(Point, f64) -> f64 + Send + Sync>,
}

impl TimeScalarField {
    /// A time field from a closure.
    ///
    /// ```
    /// use manim_fields::field::TimeScalarField;
    /// use manim_fields::Point;
    /// // A travelling wave sin(x − t).
    /// let w = TimeScalarField::new(|p, t| (p.x - t).sin());
    /// assert!((w.at(Point::new(1.0, 0.0, 0.0), 1.0)).abs() < 1e-12);
    /// ```
    pub fn new(f: impl Fn(Point, f64) -> f64 + Send + Sync + 'static) -> Self {
        Self { f: Arc::new(f) }
    }
    /// The value at `(p, t)`.
    pub fn at(&self, p: Point, t: f64) -> f64 {
        (self.f)(p, t)
    }
}

/// A time-dependent vector field `(p, t) ↦ v`.
#[derive(Clone)]
pub struct TimeVectorField3 {
    f: Arc<dyn Fn(Point, f64) -> DVec3 + Send + Sync>,
}

impl TimeVectorField3 {
    /// A time vector field from a closure.
    ///
    /// ```
    /// use manim_fields::field::TimeVectorField3;
    /// use manim_fields::Point;
    /// let v = TimeVectorField3::new(|p, t| Point::new(-p.y, p.x, 0.0) * t);
    /// assert_eq!(v.at(Point::new(1.0, 0.0, 0.0), 2.0), Point::new(0.0, 2.0, 0.0));
    /// ```
    pub fn new(f: impl Fn(Point, f64) -> DVec3 + Send + Sync + 'static) -> Self {
        Self { f: Arc::new(f) }
    }
    /// The value at `(p, t)`.
    pub fn at(&self, p: Point, t: f64) -> DVec3 {
        (self.f)(p, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    // A test field f = sin(x)·cos(y) + z², with analytic derivatives.
    fn test_field() -> ScalarField {
        let sinx = ScalarField::coordinate(0).map(UnaryOp::Sin);
        let cosy = ScalarField::coordinate(1).map(UnaryOp::Cos);
        let z2 = ScalarField::coordinate(2).map(UnaryOp::Powi(2));
        sinx.mul(&cosy).add(&z2)
    }

    #[test]
    fn gradient_matches_analytic_to_1e5() {
        let f = test_field();
        let p = Point::new(0.6, 1.1, -0.8);
        let g = f.grad(p);
        // ∇f = (cos x cos y, −sin x sin y, 2z).
        let want = DVec3::new(p.x.cos() * p.y.cos(), -p.x.sin() * p.y.sin(), 2.0 * p.z);
        assert!((g - want).length() < 1e-5, "grad {g:?} vs {want:?}");
    }

    #[test]
    fn laplacian_matches_analytic_to_1e5() {
        let f = test_field();
        let p = Point::new(0.6, 1.1, -0.8);
        // ∇²f = −sin x cos y − sin x cos y + 2 = −2 sin x cos y + 2.
        let want = -2.0 * p.x.sin() * p.y.cos() + 2.0;
        let got = f.laplacian(p);
        assert!((got - want).abs() < 1e-5, "lap {got} vs {want}");
    }

    #[test]
    fn divergence_and_curl_of_known_field() {
        // v = (x²y, y z, x z) : div = 2xy + z + x ; curl = (−y, −z, −x²).
        let v = VectorField3::from_components(
            ScalarField::coordinate(0)
                .map(UnaryOp::Powi(2))
                .mul(&ScalarField::coordinate(1)),
            ScalarField::coordinate(1).mul(&ScalarField::coordinate(2)),
            ScalarField::coordinate(0).mul(&ScalarField::coordinate(2)),
        );
        let p = Point::new(1.3, -0.7, 2.1);
        let div = v.divergence(p);
        let want_div = 2.0 * p.x * p.y + p.z + p.x;
        assert!((div - want_div).abs() < 1e-5, "div {div} vs {want_div}");
        let curl = v.curl(p);
        let want_curl = DVec3::new(-p.y, -p.z, -p.x * p.x);
        assert!(
            (curl - want_curl).length() < 1e-5,
            "curl {curl:?} vs {want_curl:?}"
        );
    }

    #[test]
    fn curl_of_gradient_is_zero() {
        // ∇f for f = sin(x)cos(y)+z² is a gradient field ⇒ zero curl.
        let grad_field = VectorField3::from_components(
            ScalarField::coordinate(0)
                .map(UnaryOp::Cos)
                .mul(&ScalarField::coordinate(1).map(UnaryOp::Cos)),
            ScalarField::coordinate(0)
                .map(UnaryOp::Sin)
                .mul(&ScalarField::coordinate(1).map(UnaryOp::Sin))
                .scale(-1.0),
            ScalarField::coordinate(2).scale(2.0),
        );
        for p in [Point::new(0.3, 0.5, 1.0), Point::new(-1.0, 2.0, -0.4)] {
            assert!(grad_field.curl(p).length() < 1e-5, "curl grad ≠ 0 at {p:?}");
        }
    }

    #[test]
    fn divergence_of_curl_is_zero() {
        // curl of v=(x²y, yz, xz) is C=(−y, −z, −x²); ∇·C = 0.
        let c = VectorField3::from_components(
            ScalarField::coordinate(1).scale(-1.0),
            ScalarField::coordinate(2).scale(-1.0),
            ScalarField::coordinate(0).map(UnaryOp::Powi(2)).scale(-1.0),
        );
        for p in [Point::new(0.3, 0.5, 1.0), Point::new(-1.0, 2.0, -0.4)] {
            assert!(c.divergence(p).abs() < 1e-5, "div curl ≠ 0 at {p:?}");
        }
    }

    struct GaussBump;
    impl ScalarClosure for GaussBump {
        fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
            let r2 = p[0] * p[0] + p[1] * p[1] + p[2] * p[2];
            (-r2).exp()
        }
    }

    #[test]
    fn translate_shifts_evaluation_and_gradient() {
        let bump = ScalarField::from_closure(GaussBump);
        let v = DVec3::new(1.0, -2.0, 0.5);
        let g = bump.translate(v);
        // Peak (value 1, zero gradient) sits at v.
        assert!((g.at(v) - 1.0).abs() < 1e-12);
        assert!(g.grad(v).length() < 1e-9);
    }

    #[test]
    fn complex_and_tensor_fields() {
        let f = ComplexField::new(|z| z * z);
        assert!((f.modulus(Complex::new(3.0, 4.0)) - 25.0).abs() < 1e-12); // |z²| = 25
        let t = TensorField2::new(|p| [p.x, 0.0, p.y]);
        let (l0, l1, _) = t.eigen(Point::new(3.0, 1.0, 0.0));
        assert!((l0 - 3.0).abs() < 1e-12 && (l1 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn time_fields_evaluate() {
        let w = TimeScalarField::new(|p, t| (p.x - t).sin());
        assert!((w.at(Point::new(PI, 0.0, 0.0), 0.0) - PI.sin()).abs() < 1e-12);
    }
}
