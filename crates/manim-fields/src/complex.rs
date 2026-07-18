//! A dependency-free complex number and Möbius (fractional-linear) transform.
//!
//! We roll our own rather than depend on `num-complex` so the crate stays tiny
//! and the exact semantics (principal branches) are ours to document.
//!
//! ```
//! use manim_fields::complex::Complex;
//! let z = Complex::new(0.0, 1.0); // i
//! // i² = −1.
//! assert!((z * z - Complex::new(-1.0, 0.0)).norm() < 1e-12);
//! ```

use std::ops::{Add, Div, Mul, Neg, Sub};

/// A complex number `re + im·i`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Complex {
    /// Real part.
    pub re: f64,
    /// Imaginary part.
    pub im: f64,
}

impl Complex {
    /// The complex number `re + im·i`.
    pub fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }
    /// `0 + 0i`.
    pub fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    /// `1 + 0i`.
    pub fn one() -> Self {
        Self::new(1.0, 0.0)
    }
    /// The imaginary unit `i`.
    pub fn i() -> Self {
        Self::new(0.0, 1.0)
    }
    /// A real number as a complex.
    pub fn real(re: f64) -> Self {
        Self::new(re, 0.0)
    }
    /// From polar form `r·e^{iθ}`.
    ///
    /// ```
    /// use manim_fields::complex::Complex;
    /// use std::f64::consts::PI;
    /// let z = Complex::from_polar(2.0, PI / 2.0); // 2i
    /// assert!((z - Complex::new(0.0, 2.0)).norm() < 1e-12);
    /// ```
    pub fn from_polar(r: f64, theta: f64) -> Self {
        Self::new(r * theta.cos(), r * theta.sin())
    }
    /// The complex conjugate `re − im·i`.
    pub fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }
    /// The squared modulus `|z|² = re² + im²` (cheaper than [`norm`](Self::norm)).
    pub fn norm_sqr(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
    /// The modulus `|z|`.
    pub fn norm(self) -> f64 {
        self.norm_sqr().sqrt()
    }
    /// The principal argument `arg(z) ∈ (−π, π]`.
    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }
    /// The reciprocal `1 / z`.
    pub fn recip(self) -> Self {
        let d = self.norm_sqr();
        Self::new(self.re / d, -self.im / d)
    }
    /// Scales by a real factor.
    pub fn scale(self, k: f64) -> Self {
        Self::new(self.re * k, self.im * k)
    }
    /// The exponential `e^z`.
    ///
    /// ```
    /// use manim_fields::complex::Complex;
    /// use std::f64::consts::PI;
    /// // Euler: e^{iπ} = −1.
    /// let z = Complex::new(0.0, PI).exp();
    /// assert!((z - Complex::new(-1.0, 0.0)).norm() < 1e-12);
    /// ```
    pub fn exp(self) -> Self {
        Self::from_polar(self.re.exp(), self.im)
    }
    /// The principal natural logarithm `ln|z| + i·arg(z)`.
    ///
    /// ```
    /// use manim_fields::complex::Complex;
    /// let z = Complex::new(3.0, -4.0);
    /// // exp(ln z) = z.
    /// assert!((z.ln().exp() - z).norm() < 1e-12);
    /// ```
    pub fn ln(self) -> Self {
        Self::new(self.norm().ln(), self.arg())
    }
    /// The principal square root.
    ///
    /// ```
    /// use manim_fields::complex::Complex;
    /// let s = Complex::new(-1.0, 0.0).sqrt(); // i
    /// assert!((s - Complex::new(0.0, 1.0)).norm() < 1e-12);
    /// ```
    pub fn sqrt(self) -> Self {
        if self.re == 0.0 && self.im == 0.0 {
            return Self::zero();
        }
        Self::from_polar(self.norm().sqrt(), self.arg() * 0.5)
    }
    /// A real power `z^p` via the principal branch.
    pub fn powf(self, p: f64) -> Self {
        Self::from_polar(self.norm().powf(p), self.arg() * p)
    }
    /// A complex power `z^w = exp(w · ln z)` via the principal branch.
    ///
    /// ```
    /// use manim_fields::complex::Complex;
    /// // i^i = e^{−π/2} (real).
    /// let z = Complex::i().powc(Complex::i());
    /// assert!((z.im).abs() < 1e-12);
    /// assert!((z.re - (-std::f64::consts::FRAC_PI_2).exp()).abs() < 1e-12);
    /// ```
    pub fn powc(self, w: Self) -> Self {
        if self.re == 0.0 && self.im == 0.0 {
            return Self::zero();
        }
        (w * self.ln()).exp()
    }
}

impl Add for Complex {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.im + o.im)
    }
}
impl Sub for Complex {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.im - o.im)
    }
}
impl Mul for Complex {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}
impl Div for Complex {
    type Output = Self;
    // `z / w = z · (1/w)` — the `*` is the mathematically correct body.
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn div(self, o: Self) -> Self {
        self * o.recip()
    }
}
impl Neg for Complex {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.re, -self.im)
    }
}

/// A Möbius (fractional-linear) transform `z ↦ (a·z + b) / (c·z + d)`.
///
/// Represented by its 2×2 coefficient matrix; composition is matrix product and
/// the inverse is the adjugate.
///
/// ```
/// use manim_fields::complex::{Complex, Mobius};
/// // The map z ↦ 1/z is its own inverse.
/// let m = Mobius::new(Complex::zero(), Complex::one(), Complex::one(), Complex::zero());
/// let z = Complex::new(2.0, 1.0);
/// assert!((m.apply(m.apply(z)) - z).norm() < 1e-12);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mobius {
    /// Coefficient `a`.
    pub a: Complex,
    /// Coefficient `b`.
    pub b: Complex,
    /// Coefficient `c`.
    pub c: Complex,
    /// Coefficient `d`.
    pub d: Complex,
}

impl Mobius {
    /// A Möbius transform from its four coefficients.
    pub fn new(a: Complex, b: Complex, c: Complex, d: Complex) -> Self {
        Self { a, b, c, d }
    }
    /// The identity transform `z ↦ z`.
    pub fn identity() -> Self {
        Self::new(
            Complex::one(),
            Complex::zero(),
            Complex::zero(),
            Complex::one(),
        )
    }
    /// The determinant `a·d − b·c`.
    pub fn det(self) -> Complex {
        self.a * self.d - self.b * self.c
    }
    /// Applies the transform to `z`.
    pub fn apply(self, z: Complex) -> Complex {
        (self.a * z + self.b) / (self.c * z + self.d)
    }
    /// The composition `self ∘ other` (apply `other`, then `self`) — the product
    /// of their coefficient matrices.
    ///
    /// ```
    /// use manim_fields::complex::{Complex, Mobius};
    /// let f = Mobius::new(Complex::real(2.0), Complex::one(), Complex::zero(), Complex::one());
    /// let g = Mobius::new(Complex::one(), Complex::real(3.0), Complex::zero(), Complex::one());
    /// let fg = f.compose(&g);
    /// let z = Complex::new(1.0, -1.0);
    /// assert!((fg.apply(z) - f.apply(g.apply(z))).norm() < 1e-12);
    /// ```
    pub fn compose(&self, other: &Self) -> Self {
        Self::new(
            self.a * other.a + self.b * other.c,
            self.a * other.b + self.b * other.d,
            self.c * other.a + self.d * other.c,
            self.c * other.b + self.d * other.d,
        )
    }
    /// The inverse transform (adjugate matrix); requires nonzero determinant.
    ///
    /// ```
    /// use manim_fields::complex::{Complex, Mobius};
    /// let m = Mobius::new(Complex::real(2.0), Complex::one(), Complex::real(1.0), Complex::real(3.0));
    /// let z = Complex::new(0.5, 0.25);
    /// assert!((m.inverse().apply(m.apply(z)) - z).norm() < 1e-12);
    /// ```
    pub fn inverse(self) -> Self {
        Self::new(self.d, -self.b, -self.c, self.a)
    }
    /// Rescales the coefficients so the determinant is `1` (the `SL(2,ℂ)`
    /// normalization); the transform itself is unchanged.
    ///
    /// ```
    /// use manim_fields::complex::{Complex, Mobius};
    /// let m = Mobius::new(Complex::real(2.0), Complex::one(), Complex::real(1.0), Complex::real(3.0));
    /// let n = m.normalize();
    /// assert!((n.det() - Complex::one()).norm() < 1e-12);
    /// // Same map, both act identically.
    /// let z = Complex::new(1.0, 1.0);
    /// assert!((n.apply(z) - m.apply(z)).norm() < 1e-12);
    /// ```
    pub fn normalize(self) -> Self {
        let s = self.det().sqrt();
        Self::new(self.a / s, self.b / s, self.c / s, self.d / s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn arithmetic_and_conjugate() {
        let z = Complex::new(3.0, 4.0);
        assert!((z.norm() - 5.0).abs() < 1e-12);
        assert!((z * z.conj() - Complex::real(25.0)).norm() < 1e-12);
        assert!((z / z - Complex::one()).norm() < 1e-12);
    }

    #[test]
    fn log_exp_roundtrip_and_powers() {
        let z = Complex::new(-2.0, 0.5);
        assert!((z.ln().exp() - z).norm() < 1e-12);
        // z^2 == z*z.
        assert!((z.powf(2.0) - z * z).norm() < 1e-10);
        // z^0.5 squared == z.
        let r = z.sqrt();
        assert!((r * r - z).norm() < 1e-12);
    }

    #[test]
    fn arg_of_i_is_half_pi() {
        assert!((Complex::i().arg() - PI / 2.0).abs() < 1e-12);
    }

    #[test]
    fn mobius_compose_matches_sequential_apply() {
        let f = Mobius::new(
            Complex::new(1.0, 1.0),
            Complex::new(0.0, 2.0),
            Complex::new(1.0, 0.0),
            Complex::new(0.0, -1.0),
        );
        let g = Mobius::new(
            Complex::new(2.0, 0.0),
            Complex::new(-1.0, 1.0),
            Complex::new(0.0, 1.0),
            Complex::new(1.0, 1.0),
        );
        let z = Complex::new(0.3, -0.7);
        let composed = f.compose(&g).apply(z);
        let sequential = f.apply(g.apply(z));
        assert!((composed - sequential).norm() < 1e-12);
    }

    #[test]
    fn mobius_inverse_is_left_and_right_inverse() {
        let m = Mobius::new(
            Complex::new(2.0, 1.0),
            Complex::new(1.0, 0.0),
            Complex::new(0.0, 1.0),
            Complex::new(3.0, -1.0),
        );
        let z = Complex::new(0.4, 0.9);
        assert!((m.inverse().apply(m.apply(z)) - z).norm() < 1e-10);
        assert!((m.apply(m.inverse().apply(z)) - z).norm() < 1e-10);
    }
}
