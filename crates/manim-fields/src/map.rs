//! [`SpaceMap`] — the deformation primitive: a map `ℝ³ → ℝ³` that knows its own
//! Jacobian (exactly, via AD, when built from an analytic closure).
//!
//! Space maps compose, translate/scale, interpolate between one another
//! ([`Homotopy`]), embed complex maps of the plane, and realize the time-`t`
//! flow of a vector field.
//!
//! ```
//! use manim_fields::map::SpaceMap;
//! use manim_fields::Point;
//! // The complex squaring map z ↦ z² doubles angles; its Jacobian at z=1 is 2·I₂.
//! let sq = SpaceMap::complex_power(2);
//! let j = sq.jacobian(Point::new(1.0, 0.0, 0.0));
//! assert!((j.x_axis - Point::new(2.0, 0.0, 0.0)).length() < 1e-9);
//! ```

use std::sync::Arc;

use glam::{DMat3, DVec3};

use crate::ad::{Dual3, Scalar};
use crate::field::VectorField3;
use crate::Point;

/// A map `p ↦ f(p)` written generically over [`Scalar`] so its Jacobian is exact
/// (implement this for [`SpaceMap::from_closure`]).
///
/// ```
/// use manim_fields::ad::Scalar;
/// use manim_fields::map::{MapClosure, SpaceMap};
/// use manim_fields::Point;
/// struct Shear;
/// impl MapClosure for Shear {
///     fn eval<S: Scalar>(&self, p: [S; 3]) -> [S; 3] {
///         [p[0] + p[1], p[1], p[2]] // x ↦ x + y
///     }
/// }
/// let m = SpaceMap::from_closure(Shear);
/// let j = m.jacobian(Point::new(3.0, 4.0, 0.0));
/// assert!((j.x_axis - Point::new(1.0, 0.0, 0.0)).length() < 1e-12); // ∂/∂x column
/// assert!((j.y_axis - Point::new(1.0, 1.0, 0.0)).length() < 1e-12); // ∂/∂y column
/// ```
pub trait MapClosure: Send + Sync {
    /// Evaluates the map at `p` in the chosen scalar type.
    fn eval<S: Scalar>(&self, p: [S; 3]) -> [S; 3];
}

type PtFn = Arc<dyn Fn(Point) -> Point + Send + Sync>;
type JacFn = Arc<dyn Fn(Point) -> DMat3 + Send + Sync>;

/// A deformation of space with its Jacobian and an optional inverse.
#[derive(Clone)]
pub struct SpaceMap {
    f: PtFn,
    jac: JacFn,
    inverse: Option<PtFn>,
}

/// Builds a `DMat3` from three output gradients (Jacobian *rows* `∇o_i`).
fn jac_from_rows(rows: [DVec3; 3]) -> DMat3 {
    // Column j is (∂o₀/∂xⱼ, ∂o₁/∂xⱼ, ∂o₂/∂xⱼ).
    DMat3::from_cols(
        DVec3::new(rows[0].x, rows[1].x, rows[2].x),
        DVec3::new(rows[0].y, rows[1].y, rows[2].y),
        DVec3::new(rows[0].z, rows[1].z, rows[2].z),
    )
}

impl SpaceMap {
    /// A map from an analytic closure; the Jacobian is exact (AD via [`Dual3`]).
    pub fn from_closure<M: MapClosure + 'static>(m: M) -> Self {
        let m = Arc::new(m);
        let mf = m.clone();
        let f: PtFn = Arc::new(move |p| {
            let o = mf.eval([p.x, p.y, p.z]);
            DVec3::new(o[0], o[1], o[2])
        });
        let mj = m;
        let jac: JacFn = Arc::new(move |p| {
            let o = mj.eval(Dual3::vars(p.x, p.y, p.z));
            jac_from_rows([o[0].grad, o[1].grad, o[2].grad])
        });
        Self {
            f,
            jac,
            inverse: None,
        }
    }

    /// A map from explicit value and Jacobian closures (escape hatch for maps
    /// whose Jacobian you supply directly, e.g. numerically integrated flows).
    pub fn from_parts(
        f: impl Fn(Point) -> Point + Send + Sync + 'static,
        jac: impl Fn(Point) -> DMat3 + Send + Sync + 'static,
    ) -> Self {
        Self {
            f: Arc::new(f),
            jac: Arc::new(jac),
            inverse: None,
        }
    }

    /// Attaches an inverse map.
    pub fn with_inverse(mut self, inv: impl Fn(Point) -> Point + Send + Sync + 'static) -> Self {
        self.inverse = Some(Arc::new(inv));
        self
    }

    /// The identity map.
    pub fn identity() -> Self {
        Self::from_parts(|p| p, |_| DMat3::IDENTITY).with_inverse(|p| p)
    }

    /// A translation `p ↦ p + v`.
    pub fn translation(v: DVec3) -> Self {
        Self::from_parts(move |p| p + v, |_| DMat3::IDENTITY).with_inverse(move |p| p - v)
    }

    /// A uniform scaling about the origin `p ↦ k·p`.
    pub fn scaling(k: f64) -> Self {
        Self::from_parts(
            move |p| p * k,
            move |_| DMat3::from_diagonal(DVec3::splat(k)),
        )
        .with_inverse(move |p| p / k)
    }

    /// A linear map `p ↦ A·p`.
    pub fn linear(a: DMat3) -> Self {
        Self::from_parts(move |p| a * p, move |_| a)
    }

    /// The complex power map `z ↦ zⁿ` of the `xy`-plane (`z`-coordinate passes
    /// through), with an exact conformal Jacobian.
    pub fn complex_power(n: i32) -> Self {
        assert!(n >= 0, "complex_power expects a non-negative exponent");
        Self::from_closure(ComplexPow(n))
    }

    /// The value `f(p)`.
    pub fn apply(&self, p: Point) -> Point {
        (self.f)(p)
    }

    /// The inverse value `f⁻¹(p)`, if an inverse was supplied.
    pub fn apply_inverse(&self, p: Point) -> Option<Point> {
        self.inverse.as_ref().map(|inv| inv(p))
    }

    /// The Jacobian matrix `∂f/∂p` at `p`.
    pub fn jacobian(&self, p: Point) -> DMat3 {
        (self.jac)(p)
    }

    /// The composition `self ∘ other` (apply `other`, then `self`); its Jacobian
    /// is the matrix product by the chain rule.
    ///
    /// ```
    /// use manim_fields::map::SpaceMap;
    /// use manim_fields::Point;
    /// let f = SpaceMap::scaling(2.0);
    /// let g = SpaceMap::translation(Point::new(1.0, 0.0, 0.0));
    /// let fg = f.compose(&g); // p ↦ 2·(p + (1,0,0))
    /// assert_eq!(fg.apply(Point::ZERO), Point::new(2.0, 0.0, 0.0));
    /// ```
    pub fn compose(&self, other: &Self) -> Self {
        let f1 = self.f.clone();
        let f2 = other.f.clone();
        let j1 = self.jac.clone();
        let j2 = other.jac.clone();
        let f2j = other.f.clone();
        Self {
            f: Arc::new(move |p| f1(f2(p))),
            jac: Arc::new(move |p| j1(f2j(p)) * j2(p)),
            inverse: None,
        }
    }

    /// Post-composes a translation: `p ↦ self(p) + v`.
    pub fn then_translate(&self, v: DVec3) -> Self {
        SpaceMap::translation(v).compose(self)
    }

    /// Post-composes a scaling: `p ↦ k·self(p)`.
    pub fn then_scale(&self, k: f64) -> Self {
        SpaceMap::scaling(k).compose(self)
    }

    /// A straight-line [`Homotopy`] from this map to `other`:
    /// `H(p, t) = (1−t)·self(p) + t·other(p)`.
    pub fn homotopy_to(&self, other: &Self) -> Homotopy {
        Homotopy::straight(self.clone(), other.clone())
    }

    /// The time-`t` flow map of a vector field: `p ↦ φ_t(p)`, following the
    /// field's integral curves. The Jacobian is central-differenced (the flow is
    /// itself a numerical integral, so exact AD does not apply).
    ///
    /// ```
    /// use manim_fields::field::{ScalarField, VectorField3};
    /// use manim_fields::map::SpaceMap;
    /// use manim_fields::Point;
    /// // Flowing the rotation field v=(−y,x,0) for π/2 is a 90° rotation.
    /// let v = VectorField3::from_components(
    ///     ScalarField::coordinate(1).scale(-1.0),
    ///     ScalarField::coordinate(0),
    ///     ScalarField::constant(0.0),
    /// );
    /// let m = SpaceMap::from_flow(&v, std::f64::consts::FRAC_PI_2);
    /// assert!((m.apply(Point::new(1.0, 0.0, 0.0)) - Point::new(0.0, 1.0, 0.0)).length() < 1e-4);
    /// ```
    pub fn from_flow(field: &VectorField3, t: f64) -> Self {
        let steps = (t.abs() * 200.0).ceil().max(1.0) as usize;
        let fa = field.clone();
        let fj = field.clone();
        Self::from_parts(
            move |p| fa.flow(p, t, steps),
            move |p| {
                let h = 1e-6;
                let mut cols = [DVec3::ZERO; 3];
                for (j, axis) in [DVec3::X, DVec3::Y, DVec3::Z].into_iter().enumerate() {
                    let fwd = fj.flow(p + axis * h, t, steps);
                    let bwd = fj.flow(p - axis * h, t, steps);
                    cols[j] = (fwd - bwd) / (2.0 * h);
                }
                DMat3::from_cols(cols[0], cols[1], cols[2])
            },
        )
    }
}

/// The complex power map's generic closure.
struct ComplexPow(i32);
impl MapClosure for ComplexPow {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> [S; 3] {
        // (x + iy)ⁿ by repeated complex multiplication in the scalar type.
        let (x, y) = (p[0], p[1]);
        let mut re = S::constant(1.0);
        let mut im = S::constant(0.0);
        for _ in 0..self.0 {
            let nr = re * x - im * y;
            let ni = re * y + im * x;
            re = nr;
            im = ni;
        }
        [re, im, p[2]]
    }
}

/// A time-parameterized family of positions `H(p, t)`, `t ∈ [0, 1]`, connecting
/// two [`SpaceMap`]s.
#[derive(Clone)]
pub struct Homotopy {
    a: SpaceMap,
    b: SpaceMap,
    #[allow(clippy::type_complexity)]
    path: Arc<dyn Fn(Point, Point, f64) -> Point + Send + Sync>,
}

impl Homotopy {
    /// A straight-line homotopy `H(p, t) = (1−t)·a(p) + t·b(p)`.
    pub fn straight(a: SpaceMap, b: SpaceMap) -> Self {
        Self {
            a,
            b,
            path: Arc::new(|pa, pb, t| pa * (1.0 - t) + pb * t),
        }
    }

    /// A homotopy with a custom interpolation `path(a(p), b(p), t)` (e.g. an arc
    /// instead of a straight line). `path` must satisfy `path(x, y, 0) = x` and
    /// `path(x, y, 1) = y`.
    pub fn with_path(
        a: SpaceMap,
        b: SpaceMap,
        path: impl Fn(Point, Point, f64) -> Point + Send + Sync + 'static,
    ) -> Self {
        Self {
            a,
            b,
            path: Arc::new(path),
        }
    }

    /// Evaluates `H(p, t)`.
    ///
    /// ```
    /// use manim_fields::map::SpaceMap;
    /// use manim_fields::Point;
    /// let h = SpaceMap::identity().homotopy_to(&SpaceMap::scaling(3.0));
    /// let p = Point::new(2.0, 0.0, 0.0);
    /// assert_eq!(h.at(p, 0.0), p);                    // starts at identity
    /// assert_eq!(h.at(p, 1.0), Point::new(6.0, 0.0, 0.0)); // ends at 3×
    /// assert_eq!(h.at(p, 0.5), Point::new(4.0, 0.0, 0.0)); // half-way
    /// ```
    pub fn at(&self, p: Point, t: f64) -> Point {
        (self.path)(self.a.apply(p), self.b.apply(p), t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{ScalarField, VectorField3};
    use std::f64::consts::FRAC_PI_2;

    #[test]
    fn jacobian_of_linear_map_is_the_matrix() {
        let a = DMat3::from_cols(
            DVec3::new(1.0, 2.0, 0.0),
            DVec3::new(-1.0, 3.0, 0.0),
            DVec3::new(0.0, 0.0, 1.0),
        );
        let m = SpaceMap::linear(a);
        let j = m.jacobian(Point::new(5.0, -2.0, 1.0));
        assert!((j - a).abs_diff_eq(DMat3::ZERO, 1e-12) || (j.x_axis - a.x_axis).length() < 1e-12);
        assert!((j.y_axis - a.y_axis).length() < 1e-12);
    }

    #[test]
    fn complex_square_is_conformal() {
        // A holomorphic map has a conformal Jacobian: Jᵀ·J = s²·I (on the plane).
        let sq = SpaceMap::complex_power(2);
        for p in [Point::new(1.3, 0.7, 0.0), Point::new(-0.5, 2.0, 0.0)] {
            let j = sq.jacobian(p);
            // Restrict to the xy 2×2 block.
            let (a, b) = (j.x_axis.x, j.y_axis.x);
            let (c, d) = (j.x_axis.y, j.y_axis.y);
            // Columns orthogonal and equal length ⇒ conformal (J = s·R).
            let dot = a * b + c * d;
            let len0 = (a * a + c * c).sqrt();
            let len1 = (b * b + d * d).sqrt();
            assert!(dot.abs() < 1e-9, "columns not orthogonal at {p:?}: {dot}");
            assert!((len0 - len1).abs() < 1e-9, "unequal scale at {p:?}");
            // Analytic derivative of z² is 2z, so the scale is 2|z|.
            let want = 2.0 * (p.x * p.x + p.y * p.y).sqrt();
            assert!((len0 - want).abs() < 1e-9);
        }
    }

    #[test]
    fn flow_of_rotation_field_is_a_rotation() {
        // v = (−y, x, 0); flowing for angle θ rotates by θ.
        let v = VectorField3::from_components(
            ScalarField::coordinate(1).scale(-1.0),
            ScalarField::coordinate(0),
            ScalarField::constant(0.0),
        );
        let m = SpaceMap::from_flow(&v, FRAC_PI_2);
        // Positions: (1,0)→(0,1), (0,1)→(−1,0).
        assert!((m.apply(Point::new(1.0, 0.0, 0.0)) - Point::new(0.0, 1.0, 0.0)).length() < 1e-4);
        assert!((m.apply(Point::new(0.0, 1.0, 0.0)) - Point::new(-1.0, 0.0, 0.0)).length() < 1e-4);
        // Jacobian ≈ the 90° rotation matrix.
        let j = m.jacobian(Point::new(0.4, -0.3, 0.0));
        let rot = DMat3::from_cols(
            DVec3::new(0.0, 1.0, 0.0),
            DVec3::new(-1.0, 0.0, 0.0),
            DVec3::new(0.0, 0.0, 1.0),
        );
        assert!(
            (j.x_axis - rot.x_axis).length() < 1e-4,
            "jac col0 {:?}",
            j.x_axis
        );
        assert!(
            (j.y_axis - rot.y_axis).length() < 1e-4,
            "jac col1 {:?}",
            j.y_axis
        );
    }

    #[test]
    fn compose_chains_maps_and_jacobians() {
        let f = SpaceMap::scaling(2.0);
        let g = SpaceMap::translation(Point::new(1.0, -1.0, 0.0));
        let fg = f.compose(&g);
        let p = Point::new(3.0, 4.0, 0.0);
        assert_eq!(fg.apply(p), (p + Point::new(1.0, -1.0, 0.0)) * 2.0);
        // d/dp [2(p+v)] = 2I.
        assert!(
            (fg.jacobian(p) - DMat3::from_diagonal(DVec3::splat(2.0)))
                .x_axis
                .length()
                < 1e-12
        );
    }

    #[test]
    fn inverse_roundtrips() {
        let m = SpaceMap::scaling(3.0).with_inverse(|p| p / 3.0);
        let p = Point::new(2.0, -5.0, 1.0);
        assert!((m.apply_inverse(m.apply(p)).unwrap() - p).length() < 1e-12);
    }
}
