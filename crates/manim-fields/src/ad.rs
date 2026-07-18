//! Forward-mode automatic differentiation.
//!
//! Three dual-number types share one [`Scalar`] trait, so a single generic
//! closure can be evaluated for its value or any of its derivatives:
//!
//! - [`Dual`] — one variable, first derivative (`re + du·ε`, `ε² = 0`).
//! - [`Dual2`] — one variable, first *and* second derivative (for curvature and
//!   the field Laplacian).
//! - [`Dual3`] — three variables, full gradient in one evaluation (for
//!   Jacobians, divergence, and curl).
//!
//! Because these are dual numbers, the derivatives are exact to floating-point
//! roundoff — there is no finite-difference step size to tune.
//!
//! ```
//! use manim_fields::ad::{Dual, Scalar};
//! // d/dx [ x^3 ] at x = 2 is 3·2² = 12.
//! let x = Dual::var(2.0);
//! assert!((x.powi(3).du - 12.0).abs() < 1e-12);
//! ```

use glam::DVec3;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// A real-or-dual scalar: the arithmetic and transcendental interface shared by
/// [`f64`] and the dual-number types, so one generic closure differentiates by
/// evaluating at a different `Scalar`.
///
/// ```
/// use manim_fields::ad::{Dual3, Scalar};
/// fn f<S: Scalar>(p: [S; 3]) -> S {
///     p[0].sin() * p[1] + p[2] * p[2]
/// }
/// // Evaluate as plain f64:
/// assert!((f([0.0_f64, 2.0, 3.0]) - 9.0).abs() < 1e-12);
/// // …or with a gradient seed:
/// let g = f(Dual3::vars(0.0, 2.0, 3.0));
/// assert!((g.re - 9.0).abs() < 1e-12);
/// ```
pub trait Scalar:
    Copy
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
{
    /// A constant carrying no derivative.
    fn constant(c: f64) -> Self;
    /// The real (value) part.
    fn value(self) -> f64;
    /// Exponential.
    fn exp(self) -> Self;
    /// Natural logarithm.
    fn ln(self) -> Self;
    /// Sine.
    fn sin(self) -> Self;
    /// Cosine.
    fn cos(self) -> Self;
    /// Tangent.
    fn tan(self) -> Self;
    /// Square root.
    fn sqrt(self) -> Self;
    /// Raise to a real power.
    fn powf(self, p: f64) -> Self;
    /// Raise to an integer power.
    fn powi(self, n: i32) -> Self {
        self.powf(n as f64)
    }
    /// Reciprocal `1 / self`.
    fn recip(self) -> Self {
        Self::constant(1.0) / self
    }
    /// Multiply by an `f64` constant.
    fn scale(self, k: f64) -> Self {
        self * Self::constant(k)
    }
}

impl Scalar for f64 {
    fn constant(c: f64) -> Self {
        c
    }
    fn value(self) -> f64 {
        self
    }
    fn exp(self) -> Self {
        f64::exp(self)
    }
    fn ln(self) -> Self {
        f64::ln(self)
    }
    fn sin(self) -> Self {
        f64::sin(self)
    }
    fn cos(self) -> Self {
        f64::cos(self)
    }
    fn tan(self) -> Self {
        f64::tan(self)
    }
    fn sqrt(self) -> Self {
        f64::sqrt(self)
    }
    fn powf(self, p: f64) -> Self {
        f64::powf(self, p)
    }
    fn powi(self, n: i32) -> Self {
        f64::powi(self, n)
    }
}

// ---------------------------------------------------------------------------
// Dual: one variable, first derivative.
// ---------------------------------------------------------------------------

/// A first-order univariate dual number `re + du·ε` with `ε² = 0`.
///
/// ```
/// use manim_fields::ad::{Dual, Scalar};
/// // d/dx [ sin x ] at 0 is cos 0 = 1.
/// let x = Dual::var(0.0);
/// assert!((x.sin().du - 1.0).abs() < 1e-12);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dual {
    /// Value.
    pub re: f64,
    /// First derivative (coefficient of `ε`).
    pub du: f64,
}

impl Dual {
    /// A dual with the given value and derivative.
    pub fn new(re: f64, du: f64) -> Self {
        Self { re, du }
    }
    /// A seeded variable `x` (derivative 1).
    pub fn var(x: f64) -> Self {
        Self { re: x, du: 1.0 }
    }
    /// Applies a unary function given `h(v)` and `h'(v)` (the chain rule).
    fn chain(self, hv: f64, dhv: f64) -> Self {
        Self {
            re: hv,
            du: dhv * self.du,
        }
    }
}

impl Add for Dual {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.du + o.du)
    }
}
impl Sub for Dual {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.du - o.du)
    }
}
impl Mul for Dual {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self::new(self.re * o.re, self.re * o.du + self.du * o.re)
    }
}
impl Div for Dual {
    type Output = Self;
    fn div(self, o: Self) -> Self {
        Self::new(
            self.re / o.re,
            (self.du * o.re - self.re * o.du) / (o.re * o.re),
        )
    }
}
impl Neg for Dual {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.re, -self.du)
    }
}

impl Scalar for Dual {
    fn constant(c: f64) -> Self {
        Self { re: c, du: 0.0 }
    }
    fn value(self) -> f64 {
        self.re
    }
    fn exp(self) -> Self {
        let e = self.re.exp();
        self.chain(e, e)
    }
    fn ln(self) -> Self {
        self.chain(self.re.ln(), 1.0 / self.re)
    }
    fn sin(self) -> Self {
        self.chain(self.re.sin(), self.re.cos())
    }
    fn cos(self) -> Self {
        self.chain(self.re.cos(), -self.re.sin())
    }
    fn tan(self) -> Self {
        let t = self.re.tan();
        self.chain(t, 1.0 + t * t)
    }
    fn sqrt(self) -> Self {
        let s = self.re.sqrt();
        self.chain(s, 0.5 / s)
    }
    fn powf(self, p: f64) -> Self {
        self.chain(self.re.powf(p), p * self.re.powf(p - 1.0))
    }
}

// ---------------------------------------------------------------------------
// Dual2: one variable, first and second derivative.
// ---------------------------------------------------------------------------

/// A second-order univariate dual carrying value, first, and second derivative.
///
/// Composition uses the chain rule for both orders, so nesting transcendentals
/// stays exact. Used for curvature and the field Laplacian (one seeded axis at a
/// time).
///
/// ```
/// use manim_fields::ad::{Dual2, Scalar};
/// // d²/dx² [ x⁴ ] at x = 1 is 12.
/// let x = Dual2::var(1.0);
/// assert!((x.powi(4).d2 - 12.0).abs() < 1e-10);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dual2 {
    /// Value.
    pub re: f64,
    /// First derivative.
    pub d1: f64,
    /// Second derivative.
    pub d2: f64,
}

impl Dual2 {
    /// A `Dual2` with the given value and derivatives.
    pub fn new(re: f64, d1: f64, d2: f64) -> Self {
        Self { re, d1, d2 }
    }
    /// A seeded variable `x` (first derivative 1, second 0).
    pub fn var(x: f64) -> Self {
        Self {
            re: x,
            d1: 1.0,
            d2: 0.0,
        }
    }
    /// Applies a unary function given `h(v)`, `h'(v)`, `h''(v)`.
    fn chain(self, hv: f64, dhv: f64, ddhv: f64) -> Self {
        Self {
            re: hv,
            d1: dhv * self.d1,
            d2: ddhv * self.d1 * self.d1 + dhv * self.d2,
        }
    }
}

impl Add for Dual2 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.d1 + o.d1, self.d2 + o.d2)
    }
}
impl Sub for Dual2 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.d1 - o.d1, self.d2 - o.d2)
    }
}
impl Mul for Dual2 {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self::new(
            self.re * o.re,
            self.d1 * o.re + self.re * o.d1,
            self.d2 * o.re + 2.0 * self.d1 * o.d1 + self.re * o.d2,
        )
    }
}
impl Div for Dual2 {
    type Output = Self;
    // f/g via f · (1/g), propagating both derivatives through the product.
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn div(self, o: Self) -> Self {
        // 1/g second derivative = (2 g'² − g g'')/g³.
        let inv = o.re.recip();
        let inv_d1 = -o.d1 * inv * inv;
        let inv_d2 = (2.0 * o.d1 * o.d1 - o.re * o.d2) * inv * inv * inv;
        self * Dual2::new(inv, inv_d1, inv_d2)
    }
}
impl Neg for Dual2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.re, -self.d1, -self.d2)
    }
}

impl Scalar for Dual2 {
    fn constant(c: f64) -> Self {
        Self {
            re: c,
            d1: 0.0,
            d2: 0.0,
        }
    }
    fn value(self) -> f64 {
        self.re
    }
    fn exp(self) -> Self {
        let e = self.re.exp();
        self.chain(e, e, e)
    }
    fn ln(self) -> Self {
        let v = self.re;
        self.chain(v.ln(), 1.0 / v, -1.0 / (v * v))
    }
    fn sin(self) -> Self {
        let (s, c) = self.re.sin_cos();
        self.chain(s, c, -s)
    }
    fn cos(self) -> Self {
        let (s, c) = self.re.sin_cos();
        self.chain(c, -s, -c)
    }
    fn tan(self) -> Self {
        let t = self.re.tan();
        let sec2 = 1.0 + t * t;
        self.chain(t, sec2, 2.0 * t * sec2)
    }
    fn sqrt(self) -> Self {
        let v = self.re;
        let s = v.sqrt();
        self.chain(s, 0.5 / s, -0.25 / (s * v))
    }
    fn powf(self, p: f64) -> Self {
        let v = self.re;
        self.chain(
            v.powf(p),
            p * v.powf(p - 1.0),
            p * (p - 1.0) * v.powf(p - 2.0),
        )
    }
}

// ---------------------------------------------------------------------------
// Dual3: three variables, full gradient in one pass.
// ---------------------------------------------------------------------------

/// A first-order multivariate dual: value plus a 3-component gradient, computed
/// in a single evaluation.
///
/// ```
/// use manim_fields::ad::{Dual3, Scalar};
/// // ∇(x·y·z) at (1,2,3) = (yz, xz, xy) = (6, 3, 2).
/// let [x, y, z] = Dual3::vars(1.0, 2.0, 3.0);
/// let g = (x * y * z).grad;
/// assert!((g.x - 6.0).abs() < 1e-12 && (g.y - 3.0).abs() < 1e-12 && (g.z - 2.0).abs() < 1e-12);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dual3 {
    /// Value.
    pub re: f64,
    /// Gradient (`∂/∂x, ∂/∂y, ∂/∂z`).
    pub grad: DVec3,
}

impl Dual3 {
    /// A `Dual3` with the given value and gradient.
    pub fn new(re: f64, grad: DVec3) -> Self {
        Self { re, grad }
    }
    /// The three coordinate variables seeded at `(x, y, z)` — each carries the
    /// corresponding unit gradient, so one evaluation yields the full gradient.
    pub fn vars(x: f64, y: f64, z: f64) -> [Self; 3] {
        [
            Self::new(x, DVec3::X),
            Self::new(y, DVec3::Y),
            Self::new(z, DVec3::Z),
        ]
    }
    /// Applies a unary function given `h(v)` and `h'(v)`.
    fn chain(self, hv: f64, dhv: f64) -> Self {
        Self {
            re: hv,
            grad: self.grad * dhv,
        }
    }
}

impl Add for Dual3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.grad + o.grad)
    }
}
impl Sub for Dual3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.grad - o.grad)
    }
}
impl Mul for Dual3 {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self::new(self.re * o.re, self.grad * o.re + o.grad * self.re)
    }
}
impl Div for Dual3 {
    type Output = Self;
    fn div(self, o: Self) -> Self {
        Self::new(
            self.re / o.re,
            (self.grad * o.re - o.grad * self.re) / (o.re * o.re),
        )
    }
}
impl Neg for Dual3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.re, -self.grad)
    }
}

impl Scalar for Dual3 {
    fn constant(c: f64) -> Self {
        Self {
            re: c,
            grad: DVec3::ZERO,
        }
    }
    fn value(self) -> f64 {
        self.re
    }
    fn exp(self) -> Self {
        let e = self.re.exp();
        self.chain(e, e)
    }
    fn ln(self) -> Self {
        self.chain(self.re.ln(), 1.0 / self.re)
    }
    fn sin(self) -> Self {
        self.chain(self.re.sin(), self.re.cos())
    }
    fn cos(self) -> Self {
        self.chain(self.re.cos(), -self.re.sin())
    }
    fn tan(self) -> Self {
        let t = self.re.tan();
        self.chain(t, 1.0 + t * t)
    }
    fn sqrt(self) -> Self {
        let s = self.re.sqrt();
        self.chain(s, 0.5 / s)
    }
    fn powf(self, p: f64) -> Self {
        self.chain(self.re.powf(p), p * self.re.powf(p - 1.0))
    }
}

// ---------------------------------------------------------------------------
// Convenience differentiation helpers.
// ---------------------------------------------------------------------------

/// The derivative of `f` at `x`, via a [`Dual`].
///
/// ```
/// use manim_fields::ad::derivative;
/// use manim_fields::ad::Scalar;
/// // d/dx [ e^x ] at 1 is e.
/// let d = derivative(|x| x.exp(), 1.0);
/// assert!((d - std::f64::consts::E).abs() < 1e-12);
/// ```
pub fn derivative<F: Fn(Dual) -> Dual>(f: F, x: f64) -> f64 {
    f(Dual::var(x)).du
}

/// The second derivative of `f` at `x`, via a [`Dual2`].
///
/// ```
/// use manim_fields::ad::second_derivative;
/// use manim_fields::ad::Scalar;
/// // d²/dx² [ sin x ] at 0 is 0.
/// let d = second_derivative(|x| x.sin(), 0.0);
/// assert!(d.abs() < 1e-12);
/// ```
pub fn second_derivative<F: Fn(Dual2) -> Dual2>(f: F, x: f64) -> f64 {
    f(Dual2::var(x)).d2
}

/// The gradient of `f` at `p`, via a single [`Dual3`] evaluation.
///
/// ```
/// use manim_fields::ad::gradient;
/// use manim_fields::ad::Scalar;
/// use glam::DVec3;
/// // ∇(x² + y² + z²) at (1,2,3) = (2,4,6).
/// let g = gradient(|p| p[0] * p[0] + p[1] * p[1] + p[2] * p[2], DVec3::new(1.0, 2.0, 3.0));
/// assert!((g - DVec3::new(2.0, 4.0, 6.0)).length() < 1e-12);
/// ```
pub fn gradient<F: Fn([Dual3; 3]) -> Dual3>(f: F, p: DVec3) -> DVec3 {
    f(Dual3::vars(p.x, p.y, p.z)).grad
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn dual_product_and_quotient_rules() {
        // d/dx [ x² / (x+1) ] at x=2 = (2x(x+1) - x²)/(x+1)² = (2·2·3 - 4)/9 = 8/9.
        let x = Dual::var(2.0);
        let f = (x * x) / (x + Dual::constant(1.0));
        assert!((f.re - 4.0 / 3.0).abs() < 1e-12);
        assert!((f.du - 8.0 / 9.0).abs() < 1e-12);
    }

    #[test]
    fn dual_transcendentals_match_analytic() {
        let x = Dual::var(0.7);
        // d/dx tan = sec² ; d/dx sqrt = 0.5/sqrt ; d/dx ln = 1/x.
        assert!((x.tan().du - 1.0 / (0.7_f64.cos().powi(2))).abs() < 1e-12);
        assert!((x.sqrt().du - 0.5 / 0.7_f64.sqrt()).abs() < 1e-12);
        assert!((x.ln().du - 1.0 / 0.7).abs() < 1e-12);
    }

    #[test]
    fn dual2_second_derivatives() {
        // d²/dx² [ exp(x) ] = exp(x); at x=0.3.
        let d = second_derivative(|x| x.exp(), 0.3);
        assert!((d - 0.3_f64.exp()).abs() < 1e-12);
        // d²/dx² [ x·sin x ] = 2 cos x − x sin x ; at x=1.
        let d = second_derivative(|x| x * x.sin(), 1.0);
        let want = 2.0 * 1.0_f64.cos() - 1.0 * 1.0_f64.sin();
        assert!((d - want).abs() < 1e-10);
    }

    #[test]
    fn dual3_gradient_of_mixed_expression() {
        // f = sin(x)·y + z² ; ∇f = (cos x · y, sin x, 2z).
        let g = gradient(
            |p| p[0].sin() * p[1] + p[2] * p[2],
            DVec3::new(PI / 3.0, 2.0, 5.0),
        );
        let want = DVec3::new((PI / 3.0).cos() * 2.0, (PI / 3.0).sin(), 10.0);
        assert!((g - want).length() < 1e-12, "got {g:?} want {want:?}");
    }

    #[test]
    fn f64_is_scalar_identity() {
        // The same generic body evaluates as plain f64 (no derivative tracking).
        fn quad<S: Scalar>(x: S) -> S {
            x * x - x.scale(3.0) + S::constant(2.0)
        }
        assert!((quad(5.0_f64) - 12.0).abs() < 1e-12);
    }
}
