//! Differential geometry of parametric surfaces and space curves.
//!
//! This module computes the classical local invariants — the first and second
//! fundamental forms, Gaussian / mean / principal curvatures, and the Frenet
//! frame with curvature and torsion — from a user-supplied *sampler* that is
//! written generically over [`Scalar`]. Evaluating the sampler at a seeded
//! higher-order jet yields all the partial derivatives it needs *exactly*, with
//! no finite differencing.
//!
//! Two local jet types drive this:
//!
//! - [`J2`] — a **bivariate 2-jet** carrying `f`, `f_u`, `f_v`, `f_uu`, `f_uv`,
//!   `f_vv`. Seeding a surface sampler with [`J2::u`] / [`J2::v`] gives the two
//!   tangents and three second derivatives that the fundamental forms need.
//! - [`J3`] — a **univariate 3-jet** carrying `γ`, `γ'`, `γ''`, `γ'''`. Torsion
//!   needs the third derivative, which the built-in duals do not provide.
//!
//! Both implement [`Scalar`], so the same generic sampler body can be evaluated
//! as plain `f64` or at a jet.

use glam::DVec3;
use manim_fields::ad::Scalar;
use std::ops::{Add, Div, Mul, Neg, Sub};

// ===========================================================================
// J2 — bivariate second-order jet.
// ===========================================================================

/// A bivariate second-order jet in the two variables `u` and `v`.
///
/// It carries the value and all first and second partial derivatives of a
/// scalar function of `(u, v)`. Seed one coordinate with [`J2::u`] and the other
/// with [`J2::v`]; evaluating a [`SurfaceSampler`] on the pair then propagates
/// every partial derivative through the arithmetic and transcendentals by the
/// chain and product rules.
///
/// ```
/// use manim_sci::diffgeo::J2;
/// use manim_fields::ad::Scalar;
/// // f(u, v) = u² · v : f_uu = 2v, f_uv = 2u, f_vv = 0 at (u, v) = (3, 5).
/// let f = J2::u(3.0) * J2::u(3.0) * J2::v(5.0);
/// assert!((f.duu - 10.0).abs() < 1e-12);
/// assert!((f.duv - 6.0).abs() < 1e-12);
/// assert!((f.dvv - 0.0).abs() < 1e-12);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct J2 {
    /// Value `f`.
    pub re: f64,
    /// First partial `∂f/∂u`.
    pub du: f64,
    /// First partial `∂f/∂v`.
    pub dv: f64,
    /// Second partial `∂²f/∂u²`.
    pub duu: f64,
    /// Mixed partial `∂²f/∂u∂v`.
    pub duv: f64,
    /// Second partial `∂²f/∂v²`.
    pub dvv: f64,
}

impl J2 {
    /// The `u` coordinate seeded at `x` (`∂/∂u = 1`, all other jets zero).
    pub fn u(x: f64) -> Self {
        Self {
            re: x,
            du: 1.0,
            dv: 0.0,
            duu: 0.0,
            duv: 0.0,
            dvv: 0.0,
        }
    }
    /// The `v` coordinate seeded at `x` (`∂/∂v = 1`, all other jets zero).
    pub fn v(x: f64) -> Self {
        Self {
            re: x,
            du: 0.0,
            dv: 1.0,
            duu: 0.0,
            duv: 0.0,
            dvv: 0.0,
        }
    }
    /// Applies a unary function given `h(v)`, `h'(v)`, `h''(v)` (the chain rule).
    fn chain(self, hv: f64, dhv: f64, ddhv: f64) -> Self {
        Self {
            re: hv,
            du: dhv * self.du,
            dv: dhv * self.dv,
            duu: ddhv * self.du * self.du + dhv * self.duu,
            duv: ddhv * self.du * self.dv + dhv * self.duv,
            dvv: ddhv * self.dv * self.dv + dhv * self.dvv,
        }
    }
}

impl Add for J2 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self {
            re: self.re + o.re,
            du: self.du + o.du,
            dv: self.dv + o.dv,
            duu: self.duu + o.duu,
            duv: self.duv + o.duv,
            dvv: self.dvv + o.dvv,
        }
    }
}
impl Sub for J2 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self {
            re: self.re - o.re,
            du: self.du - o.du,
            dv: self.dv - o.dv,
            duu: self.duu - o.duu,
            duv: self.duv - o.duv,
            dvv: self.dvv - o.dvv,
        }
    }
}
impl Mul for J2 {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        let f = self;
        let g = o;
        Self {
            re: f.re * g.re,
            du: f.du * g.re + f.re * g.du,
            dv: f.dv * g.re + f.re * g.dv,
            duu: f.duu * g.re + 2.0 * f.du * g.du + f.re * g.duu,
            duv: f.duv * g.re + f.du * g.dv + f.dv * g.du + f.re * g.duv,
            dvv: f.dvv * g.re + 2.0 * f.dv * g.dv + f.re * g.dvv,
        }
    }
}
impl Div for J2 {
    type Output = Self;
    // f / g = f · (1/g), with `recip` supplying the reciprocal jet.
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn div(self, o: Self) -> Self {
        self * o.recip()
    }
}
impl Neg for J2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            re: -self.re,
            du: -self.du,
            dv: -self.dv,
            duu: -self.duu,
            duv: -self.duv,
            dvv: -self.dvv,
        }
    }
}

impl Scalar for J2 {
    fn constant(c: f64) -> Self {
        Self {
            re: c,
            du: 0.0,
            dv: 0.0,
            duu: 0.0,
            duv: 0.0,
            dvv: 0.0,
        }
    }
    fn value(self) -> f64 {
        self.re
    }
    fn recip(self) -> Self {
        let x = self.re;
        self.chain(1.0 / x, -1.0 / (x * x), 2.0 / (x * x * x))
    }
    fn exp(self) -> Self {
        let e = self.re.exp();
        self.chain(e, e, e)
    }
    fn ln(self) -> Self {
        let x = self.re;
        self.chain(x.ln(), 1.0 / x, -1.0 / (x * x))
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
        let x = self.re;
        let s = x.sqrt();
        self.chain(s, 0.5 / s, -0.25 / (s * x))
    }
    fn powf(self, p: f64) -> Self {
        let x = self.re;
        self.chain(
            x.powf(p),
            p * x.powf(p - 1.0),
            p * (p - 1.0) * x.powf(p - 2.0),
        )
    }
}

// ===========================================================================
// J3 — univariate third-order jet.
// ===========================================================================

/// A univariate third-order jet carrying value and first three derivatives.
///
/// Space-curve torsion needs `γ'''`, one order beyond the built-in duals. Seed
/// the single variable with [`J3::var`]; the value and all three derivatives
/// then propagate exactly.
///
/// ```
/// use manim_sci::diffgeo::J3;
/// use manim_fields::ad::Scalar;
/// // f(t) = t⁴ : f''' = 24 t = 48 at t = 2.
/// let f = J3::var(2.0).powi(4);
/// assert!((f.d3 - 48.0).abs() < 1e-9);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct J3 {
    /// Value.
    pub re: f64,
    /// First derivative.
    pub d1: f64,
    /// Second derivative.
    pub d2: f64,
    /// Third derivative.
    pub d3: f64,
}

impl J3 {
    /// The variable seeded at `x` (`d1 = 1`, higher derivatives zero).
    pub fn var(x: f64) -> Self {
        Self {
            re: x,
            d1: 1.0,
            d2: 0.0,
            d3: 0.0,
        }
    }
    /// Applies a unary function given `h`, `h'`, `h''`, `h'''` (the chain rule).
    fn chain(self, h: f64, h1: f64, h2: f64, h3: f64) -> Self {
        let (a, b, c) = (self.d1, self.d2, self.d3);
        Self {
            re: h,
            d1: h1 * a,
            d2: h2 * a * a + h1 * b,
            d3: h3 * a * a * a + 3.0 * h2 * a * b + h1 * c,
        }
    }
}

impl Add for J3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self {
            re: self.re + o.re,
            d1: self.d1 + o.d1,
            d2: self.d2 + o.d2,
            d3: self.d3 + o.d3,
        }
    }
}
impl Sub for J3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self {
            re: self.re - o.re,
            d1: self.d1 - o.d1,
            d2: self.d2 - o.d2,
            d3: self.d3 - o.d3,
        }
    }
}
impl Mul for J3 {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        let f = self;
        let g = o;
        Self {
            re: f.re * g.re,
            d1: f.d1 * g.re + f.re * g.d1,
            d2: f.d2 * g.re + 2.0 * f.d1 * g.d1 + f.re * g.d2,
            d3: f.d3 * g.re + 3.0 * f.d2 * g.d1 + 3.0 * f.d1 * g.d2 + f.re * g.d3,
        }
    }
}
impl Div for J3 {
    type Output = Self;
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn div(self, o: Self) -> Self {
        self * o.recip()
    }
}
impl Neg for J3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            re: -self.re,
            d1: -self.d1,
            d2: -self.d2,
            d3: -self.d3,
        }
    }
}

impl Scalar for J3 {
    fn constant(c: f64) -> Self {
        Self {
            re: c,
            d1: 0.0,
            d2: 0.0,
            d3: 0.0,
        }
    }
    fn value(self) -> f64 {
        self.re
    }
    fn recip(self) -> Self {
        let x = self.re;
        let x2 = x * x;
        self.chain(1.0 / x, -1.0 / x2, 2.0 / (x2 * x), -6.0 / (x2 * x2))
    }
    fn exp(self) -> Self {
        let e = self.re.exp();
        self.chain(e, e, e, e)
    }
    fn ln(self) -> Self {
        let x = self.re;
        self.chain(x.ln(), 1.0 / x, -1.0 / (x * x), 2.0 / (x * x * x))
    }
    fn sin(self) -> Self {
        let (s, c) = self.re.sin_cos();
        self.chain(s, c, -s, -c)
    }
    fn cos(self) -> Self {
        let (s, c) = self.re.sin_cos();
        self.chain(c, -s, -c, s)
    }
    fn tan(self) -> Self {
        let t = self.re.tan();
        let sec2 = 1.0 + t * t;
        self.chain(t, sec2, 2.0 * t * sec2, 2.0 * sec2 * (1.0 + 3.0 * t * t))
    }
    fn sqrt(self) -> Self {
        let x = self.re;
        self.chain(
            x.sqrt(),
            0.5 * x.powf(-0.5),
            -0.25 * x.powf(-1.5),
            0.375 * x.powf(-2.5),
        )
    }
    fn powf(self, p: f64) -> Self {
        let x = self.re;
        self.chain(
            x.powf(p),
            p * x.powf(p - 1.0),
            p * (p - 1.0) * x.powf(p - 2.0),
            p * (p - 1.0) * (p - 2.0) * x.powf(p - 3.0),
        )
    }
}

// ===========================================================================
// Samplers.
// ===========================================================================

/// A parametric surface `f(u, v) → ℝ³`, written generically over [`Scalar`].
///
/// Because `eval` is generic, evaluating at [`J2`] yields the tangents and
/// second derivatives the fundamental forms need, exactly.
///
/// ```
/// use manim_sci::diffgeo::{SurfaceSampler, gaussian_curvature};
/// use manim_fields::ad::Scalar;
/// struct UnitSphere;
/// impl SurfaceSampler for UnitSphere {
///     fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
///         [u.sin() * v.cos(), u.sin() * v.sin(), u.cos()]
///     }
/// }
/// assert!((gaussian_curvature(&UnitSphere, 1.0, 0.5) - 1.0).abs() < 1e-6);
/// ```
pub trait SurfaceSampler: Send + Sync {
    /// Evaluates the surface point at parameters `(u, v)`.
    fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3];
}

/// A parametric space curve `γ(t) → ℝ³`, written generically over [`Scalar`].
///
/// Evaluating at [`J3`] yields `γ'`, `γ''`, `γ'''` for the Frenet frame,
/// curvature, and torsion.
///
/// ```
/// use manim_sci::diffgeo::{CurveSampler, curvature};
/// use manim_fields::ad::Scalar;
/// struct Helix;
/// impl CurveSampler for Helix {
///     fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
///         [t.cos(), t.sin(), t] // a = b = 1
///     }
/// }
/// // κ = a / (a² + b²) = 1/2.
/// assert!((curvature(&Helix, 0.3) - 0.5).abs() < 1e-9);
/// ```
pub trait CurveSampler: Send + Sync {
    /// Evaluates the curve point at parameter `t`.
    fn eval<S: Scalar>(&self, t: S) -> [S; 3];
}

// ===========================================================================
// Surface derivatives.
// ===========================================================================

/// The point and all first/second parameter derivatives of a surface at `(u, v)`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SurfaceDerivs {
    /// First tangent `f_u`.
    pub fu: DVec3,
    /// Second tangent `f_v`.
    pub fv: DVec3,
    /// Second derivative `f_uu`.
    pub fuu: DVec3,
    /// Mixed derivative `f_uv`.
    pub fuv: DVec3,
    /// Second derivative `f_vv`.
    pub fvv: DVec3,
}

/// Evaluates a surface sampler at a seeded [`J2`] and unpacks the jets into the
/// five parameter-derivative vectors.
pub(crate) fn surface_derivs<Sm: SurfaceSampler + ?Sized>(s: &Sm, u: f64, v: f64) -> SurfaceDerivs {
    let p = s.eval(J2::u(u), J2::v(v));
    SurfaceDerivs {
        fu: DVec3::new(p[0].du, p[1].du, p[2].du),
        fv: DVec3::new(p[0].dv, p[1].dv, p[2].dv),
        fuu: DVec3::new(p[0].duu, p[1].duu, p[2].duu),
        fuv: DVec3::new(p[0].duv, p[1].duv, p[2].duv),
        fvv: DVec3::new(p[0].dvv, p[1].dvv, p[2].dvv),
    }
}

// ===========================================================================
// Fundamental forms and curvatures.
// ===========================================================================

/// The first fundamental form coefficients `(E, F, G)` at `(u, v)`.
///
/// With tangents `f_u`, `f_v`: `E = f_u·f_u`, `F = f_u·f_v`, `G = f_v·f_v`.
///
/// ```
/// use manim_sci::diffgeo::{SurfaceSampler, first_fundamental_form};
/// use manim_fields::ad::Scalar;
/// struct UnitSphere;
/// impl SurfaceSampler for UnitSphere {
///     fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
///         [u.sin() * v.cos(), u.sin() * v.sin(), u.cos()]
///     }
/// }
/// // Sphere metric: E = 1, F = 0, G = sin²u.
/// let (e, f, g) = first_fundamental_form(&UnitSphere, 1.0, 0.4);
/// assert!((e - 1.0).abs() < 1e-9 && f.abs() < 1e-9);
/// assert!((g - 1.0_f64.sin().powi(2)).abs() < 1e-9);
/// ```
pub fn first_fundamental_form<Sm: SurfaceSampler + ?Sized>(
    s: &Sm,
    u: f64,
    v: f64,
) -> (f64, f64, f64) {
    let d = surface_derivs(s, u, v);
    (d.fu.dot(d.fu), d.fu.dot(d.fv), d.fv.dot(d.fv))
}

/// The second fundamental form coefficients `(L, M, N)` at `(u, v)`.
///
/// With unit normal `n = (f_u × f_v).normalize()`: `L = f_uu·n`, `M = f_uv·n`,
/// `N = f_vv·n`.
pub fn second_fundamental_form<Sm: SurfaceSampler + ?Sized>(
    s: &Sm,
    u: f64,
    v: f64,
) -> (f64, f64, f64) {
    let d = surface_derivs(s, u, v);
    let n = d.fu.cross(d.fv).normalize();
    (d.fuu.dot(n), d.fuv.dot(n), d.fvv.dot(n))
}

/// The Gaussian curvature `K = (LN − M²) / (EG − F²)` at `(u, v)`.
///
/// ```
/// use manim_sci::diffgeo::{SurfaceSampler, gaussian_curvature};
/// use manim_fields::ad::Scalar;
/// struct UnitSphere;
/// impl SurfaceSampler for UnitSphere {
///     fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
///         [u.sin() * v.cos(), u.sin() * v.sin(), u.cos()]
///     }
/// }
/// assert!((gaussian_curvature(&UnitSphere, 0.9, 2.1) - 1.0).abs() < 1e-6);
/// ```
pub fn gaussian_curvature<Sm: SurfaceSampler + ?Sized>(s: &Sm, u: f64, v: f64) -> f64 {
    let (e, f, g) = first_fundamental_form(s, u, v);
    let (l, m, n) = second_fundamental_form(s, u, v);
    (l * n - m * m) / (e * g - f * f)
}

/// The mean curvature `H = (EN − 2FM + GL) / (2(EG − F²))` at `(u, v)`.
///
/// The sign depends on the orientation of the unit normal `(f_u × f_v)`.
pub fn mean_curvature<Sm: SurfaceSampler + ?Sized>(s: &Sm, u: f64, v: f64) -> f64 {
    let (e, f, g) = first_fundamental_form(s, u, v);
    let (l, m, n) = second_fundamental_form(s, u, v);
    (e * n - 2.0 * f * m + g * l) / (2.0 * (e * g - f * f))
}

/// The principal curvatures `(k1, k2) = H ± √(max(H² − K, 0))` at `(u, v)`.
pub fn principal_curvatures<Sm: SurfaceSampler + ?Sized>(s: &Sm, u: f64, v: f64) -> (f64, f64) {
    let h = mean_curvature(s, u, v);
    let k = gaussian_curvature(s, u, v);
    let disc = (h * h - k).max(0.0).sqrt();
    (h + disc, h - disc)
}

/// The unit surface normal `n = (f_u × f_v).normalize()` at `(u, v)`.
pub fn normal<Sm: SurfaceSampler + ?Sized>(s: &Sm, u: f64, v: f64) -> DVec3 {
    let d = surface_derivs(s, u, v);
    d.fu.cross(d.fv).normalize()
}

/// The two (unnormalized) tangent vectors `(f_u, f_v)` at `(u, v)`.
pub fn tangents<Sm: SurfaceSampler + ?Sized>(s: &Sm, u: f64, v: f64) -> (DVec3, DVec3) {
    let d = surface_derivs(s, u, v);
    (d.fu, d.fv)
}

// ===========================================================================
// Frenet frame.
// ===========================================================================

/// The orthonormal Frenet frame `(T, N, B)` of a space curve at a point.
///
/// `T = γ'/|γ'|`, `B = (γ' × γ'')/|γ' × γ''|`, `N = B × T`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrenetFrame {
    /// Unit tangent.
    pub t: DVec3,
    /// Unit principal normal.
    pub n: DVec3,
    /// Unit binormal.
    pub b: DVec3,
}

/// Evaluates a curve sampler at a seeded [`J3`], returning `(γ', γ'', γ''')`.
fn curve_derivs<C: CurveSampler + ?Sized>(c: &C, t: f64) -> (DVec3, DVec3, DVec3) {
    let p = c.eval(J3::var(t));
    let d1 = DVec3::new(p[0].d1, p[1].d1, p[2].d1);
    let d2 = DVec3::new(p[0].d2, p[1].d2, p[2].d2);
    let d3 = DVec3::new(p[0].d3, p[1].d3, p[2].d3);
    (d1, d2, d3)
}

/// The Frenet frame of the curve at parameter `t`.
///
/// ```
/// use manim_sci::diffgeo::{CurveSampler, frenet_frame};
/// use manim_fields::ad::Scalar;
/// struct Circle;
/// impl CurveSampler for Circle {
///     fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
///         [t.cos(), t.sin(), S::constant(0.0)]
///     }
/// }
/// let f = frenet_frame(&Circle, 0.0);
/// // At t = 0: T = (0, 1, 0), B = (0, 0, 1).
/// assert!((f.t - glam::DVec3::new(0.0, 1.0, 0.0)).length() < 1e-9);
/// assert!((f.b - glam::DVec3::new(0.0, 0.0, 1.0)).length() < 1e-9);
/// ```
pub fn frenet_frame<C: CurveSampler + ?Sized>(c: &C, t: f64) -> FrenetFrame {
    let (d1, d2, _) = curve_derivs(c, t);
    let tan = d1.normalize();
    let b = d1.cross(d2).normalize();
    let n = b.cross(tan);
    FrenetFrame { t: tan, n, b }
}

/// The curvature `κ = |γ' × γ''| / |γ'|³` at parameter `t`.
///
/// ```
/// use manim_sci::diffgeo::{CurveSampler, curvature};
/// use manim_fields::ad::Scalar;
/// struct Helix;
/// impl CurveSampler for Helix {
///     fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
///         [t.cos().scale(2.0), t.sin().scale(2.0), t] // a = 2, b = 1
///     }
/// }
/// // κ = a / (a² + b²) = 2/5.
/// assert!((curvature(&Helix, 1.1) - 0.4).abs() < 1e-9);
/// ```
pub fn curvature<C: CurveSampler + ?Sized>(c: &C, t: f64) -> f64 {
    let (d1, d2, _) = curve_derivs(c, t);
    d1.cross(d2).length() / d1.length().powi(3)
}

/// The torsion `τ = (γ' × γ'')·γ''' / |γ' × γ''|²` at parameter `t`.
///
/// ```
/// use manim_sci::diffgeo::{CurveSampler, torsion};
/// use manim_fields::ad::Scalar;
/// struct Helix;
/// impl CurveSampler for Helix {
///     fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
///         [t.cos().scale(2.0), t.sin().scale(2.0), t] // a = 2, b = 1
///     }
/// }
/// // τ = b / (a² + b²) = 1/5.
/// assert!((torsion(&Helix, 1.1) - 0.2).abs() < 1e-9);
/// ```
pub fn torsion<C: CurveSampler + ?Sized>(c: &C, t: f64) -> f64 {
    let (d1, d2, d3) = curve_derivs(c, t);
    let cross = d1.cross(d2);
    cross.dot(d3) / cross.length_squared()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    // ---- Jet correctness against analytic derivatives. ----

    #[test]
    fn j2_matches_analytic_second_derivatives() {
        // f(u, v) = sin(u) · exp(v) at (0.7, 0.3).
        let (u0, v0) = (0.7_f64, 0.3_f64);
        let f = J2::u(u0).sin() * J2::v(v0).exp();
        let ev = v0.exp();
        assert!((f.re - u0.sin() * ev).abs() < 1e-12);
        assert!((f.du - u0.cos() * ev).abs() < 1e-12); // f_u
        assert!((f.dv - u0.sin() * ev).abs() < 1e-12); // f_v
        assert!((f.duu + u0.sin() * ev).abs() < 1e-12); // f_uu = -sin·exp
        assert!((f.duv - u0.cos() * ev).abs() < 1e-12); // f_uv = cos·exp
        assert!((f.dvv - u0.sin() * ev).abs() < 1e-12); // f_vv = sin·exp
    }

    #[test]
    fn j2_quotient_matches_analytic() {
        // f(u, v) = u / v ; f_uu = 0, f_uv = -1/v², f_vv = 2u/v³ at (2, 4).
        let f = J2::u(2.0) / J2::v(4.0);
        assert!((f.re - 0.5).abs() < 1e-12);
        assert!((f.du - 0.25).abs() < 1e-12); // 1/v
        assert!((f.dv + 2.0 / 16.0).abs() < 1e-12); // -u/v²
        assert!(f.duu.abs() < 1e-12);
        assert!((f.duv + 1.0 / 16.0).abs() < 1e-12); // -1/v²
        assert!((f.dvv - 2.0 * 2.0 / 64.0).abs() < 1e-12); // 2u/v³
    }

    #[test]
    fn j3_matches_analytic_third_derivatives() {
        // f(t) = sin(t) : f''' = -cos(t) at t = 0.6.
        let f = J3::var(0.6).sin();
        assert!((f.d1 - 0.6_f64.cos()).abs() < 1e-12);
        assert!((f.d2 + 0.6_f64.sin()).abs() < 1e-12);
        assert!((f.d3 + 0.6_f64.cos()).abs() < 1e-12);
        // g(t) = ln(t) : g''' = 2/t³ at t = 1.3.
        let g = J3::var(1.3).ln();
        assert!((g.d3 - 2.0 / 1.3_f64.powi(3)).abs() < 1e-12);
        // h(t) = t·exp(t) : h''' = (t+3)·exp(t) at t = 0.4.
        let h = J3::var(0.4) * J3::var(0.4).exp();
        assert!((h.d3 - (0.4 + 3.0) * 0.4_f64.exp()).abs() < 1e-12);
    }

    // ---- Surface diff-geo: unit sphere. ----

    struct UnitSphere;
    impl SurfaceSampler for UnitSphere {
        fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
            [u.sin() * v.cos(), u.sin() * v.sin(), u.cos()]
        }
    }

    #[test]
    fn sphere_gaussian_curvature_is_one() {
        for &(u, v) in &[(0.5, 0.0), (1.0, 1.3), (2.0, 3.0), (1.57, 4.2)] {
            let k = gaussian_curvature(&UnitSphere, u, v);
            assert!((k - 1.0).abs() < 1e-4, "K at ({u},{v}) = {k}");
        }
    }

    #[test]
    fn sphere_mean_curvature_is_unit() {
        for &(u, v) in &[(0.6, 0.2), (1.2, 2.0), (2.3, 5.0)] {
            let h = mean_curvature(&UnitSphere, u, v);
            assert!(
                (h.abs() - 1.0).abs() < 1e-4,
                "|H| at ({u},{v}) = {}",
                h.abs()
            );
        }
    }

    #[test]
    fn sphere_principal_curvatures_are_one() {
        let (k1, k2) = principal_curvatures(&UnitSphere, 1.0, 0.7);
        assert!((k1.abs() - 1.0).abs() < 1e-4);
        assert!((k2.abs() - 1.0).abs() < 1e-4);
    }

    // ---- Surface diff-geo: torus curvature sign. ----

    struct Torus {
        big_r: f64,
        r: f64,
    }
    impl SurfaceSampler for Torus {
        fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
            let ring = u.cos().scale(self.r) + S::constant(self.big_r);
            [ring * v.cos(), ring * v.sin(), u.sin().scale(self.r)]
        }
    }

    #[test]
    fn torus_curvature_sign_splits_outer_vs_inner() {
        let torus = Torus { big_r: 1.0, r: 0.4 };
        // Outer equator u = 0: positive curvature.
        let k_outer = gaussian_curvature(&torus, 0.0, 1.0);
        // Inner equator u = π: negative curvature.
        let k_inner = gaussian_curvature(&torus, PI, 1.0);
        assert!(k_outer > 0.0, "outer K = {k_outer}");
        assert!(k_inner < 0.0, "inner K = {k_inner}");
    }

    // ---- Frenet: helix exact curvature and torsion. ----

    struct Helix {
        a: f64,
        b: f64,
    }
    impl CurveSampler for Helix {
        fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
            [
                t.cos().scale(self.a),
                t.sin().scale(self.a),
                t.scale(self.b),
            ]
        }
    }

    #[test]
    fn helix_curvature_and_torsion_exact() {
        let helix = Helix { a: 2.0, b: 0.75 };
        let denom = helix.a * helix.a + helix.b * helix.b;
        let want_k = helix.a / denom;
        let want_tau = helix.b / denom;
        for &t in &[0.0, 0.7, 2.4, 5.1] {
            assert!((curvature(&helix, t) - want_k).abs() < 1e-6);
            assert!((torsion(&helix, t) - want_tau).abs() < 1e-6);
        }
    }

    #[test]
    fn helix_frenet_frame_is_orthonormal() {
        let helix = Helix { a: 1.0, b: 0.5 };
        let f = frenet_frame(&helix, 1.3);
        assert!((f.t.length() - 1.0).abs() < 1e-9);
        assert!((f.n.length() - 1.0).abs() < 1e-9);
        assert!((f.b.length() - 1.0).abs() < 1e-9);
        assert!(f.t.dot(f.n).abs() < 1e-9);
        assert!(f.t.dot(f.b).abs() < 1e-9);
        assert!(f.n.dot(f.b).abs() < 1e-9);
        // Right-handed: T × N = B.
        assert!((f.t.cross(f.n) - f.b).length() < 1e-9);
    }
}
