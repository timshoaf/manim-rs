//! Gaussian `.cube` volumetric-data parser, yielding a
//! [`ScalarField`] sampled by trilinear
//! interpolation.
//!
//! A cube file stores a scalar (e.g. an orbital or density) on a regular grid
//! spanned by an origin and three voxel-axis vectors. This module parses the
//! header, the atom records, and the volumetric block (whitespace-separated
//! floats in the standard Gaussian ordering: the first grid axis varies
//! slowest, the third fastest), then exposes the grid as a [`CubeData`].
//!
//! **Units.** Cube coordinates are in bohr unless a voxel-count on an axis line
//! is written negative, which flags ångström (per the Gaussian convention). A
//! negative atom count on the origin line flags a molecular-orbital cube, whose
//! extra "orbital IDs" line is skipped. This parser converts everything to
//! **ångström** so the grid shares units with [`Molecule`](crate::Molecule).
//!
//! [`CubeData::to_scalar_field`] returns a field whose closure interpolates the
//! grid in `f64`; because the interpolation is done on the raw `f64` values
//! (via `Scalar::value` / `Scalar::constant`) it carries **no** autodiff
//! gradient — which is fine, as the field feeds an isosurface level-set, not a
//! derivative.
//!
//! ```
//! use manim_chem::cube::from_cube;
//! // Tiny 2×2×2 grid of ones, in ångström (negative voxel counts).
//! let src = "comment\ncomment\n1 0 0 0\n-2 1 0 0\n-2 0 1 0\n-2 0 0 1\n\
//!            6 0 0 0 0\n1 1 1 1 1 1 1 1\n";
//! let cube = from_cube(src).unwrap();
//! assert_eq!(cube.dims, [2, 2, 2]);
//! assert_eq!(cube.value_range(), (1.0, 1.0));
//! ```

use glam::{DMat3, DVec3, Vec3};

use manim_fields::ad::Scalar;
use manim_fields::field::{ScalarClosure, ScalarField};

use crate::parsers::ChemParseError;

/// Bohr → ångström conversion factor (CODATA).
const BOHR_TO_ANG: f32 = 0.529_177_2;

/// A parsed Gaussian cube grid.
#[derive(Debug, Clone, PartialEq)]
pub struct CubeData {
    /// Grid origin in ångström.
    pub origin: Vec3,
    /// The three voxel-axis vectors in ångström (one grid step along each axis).
    pub axes: [Vec3; 3],
    /// Number of grid points along each axis, `[nx, ny, nz]`.
    pub dims: [usize; 3],
    /// Volumetric values in row-major order with the third axis fastest:
    /// `values[(ix·ny + iy)·nz + iz]`.
    pub values: Vec<f32>,
    /// Atoms carried by the file, as `(atomic number, position in ångström)`.
    pub atoms: Vec<(u8, Vec3)>,
}

impl CubeData {
    /// The `(min, max)` of the volumetric values (`(0, 0)` if empty).
    ///
    /// ```
    /// use manim_chem::cube::from_cube;
    /// let src = "c\nc\n0 0 0 0\n-2 1 0 0\n-2 0 1 0\n-2 0 0 1\n0 1 2 3 4 5 6 7\n";
    /// let cube = from_cube(src).unwrap();
    /// assert_eq!(cube.value_range(), (0.0, 7.0));
    /// ```
    pub fn value_range(&self) -> (f32, f32) {
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for &v in &self.values {
            min = min.min(v);
            max = max.max(v);
        }
        if self.values.is_empty() {
            (0.0, 0.0)
        } else {
            (min, max)
        }
    }

    /// Builds a [`ScalarField`] that trilinearly interpolates this grid,
    /// returning `0.0` for query points outside the grid box.
    ///
    /// The returned field evaluates on the raw `f64` grid and therefore has no
    /// meaningful autodiff gradient (see the module docs).
    ///
    /// ```
    /// use manim_chem::cube::from_cube;
    /// use manim_fields::Point;
    /// // f(x,y,z) = x on a 3×3×3 unit grid (ångström).
    /// let mut src = String::from("c\nc\n1 0 0 0\n-3 1 0 0\n-3 0 1 0\n-3 0 0 1\n6 0 0 0 0\n");
    /// for i in 0..3 { for _ in 0..9 { src.push_str(&format!("{i} ")); } }
    /// let field = from_cube(&src).unwrap().to_scalar_field();
    /// assert!((field.at(Point::new(1.3, 0.7, 0.2)) - 1.3).abs() < 1e-5);
    /// ```
    pub fn to_scalar_field(&self) -> ScalarField {
        let m = DMat3::from_cols(
            self.axes[0].as_dvec3(),
            self.axes[1].as_dvec3(),
            self.axes[2].as_dvec3(),
        );
        let sampler = CubeSampler {
            origin: self.origin.as_dvec3(),
            inv: m.inverse(),
            dims: self.dims,
            values: self.values.iter().map(|&v| v as f64).collect(),
        };
        ScalarField::from_closure(sampler)
    }
}

/// The trilinear-interpolation sampler backing [`CubeData::to_scalar_field`].
struct CubeSampler {
    origin: DVec3,
    /// Maps `p - origin` to fractional grid indices (inverse of the axis matrix).
    inv: DMat3,
    dims: [usize; 3],
    values: Vec<f64>,
}

impl CubeSampler {
    /// Flat index into `values` for grid point `(i, j, k)`.
    #[inline]
    fn index(&self, i: usize, j: usize, k: usize) -> usize {
        let [_nx, ny, nz] = self.dims;
        (i * ny + j) * nz + k
    }

    /// Trilinearly interpolates the grid at world point `p`; `0.0` if outside.
    fn sample(&self, p: DVec3) -> f64 {
        const EPS: f64 = 1e-9;
        let frac = self.inv * (p - self.origin);
        let coords = [frac.x, frac.y, frac.z];

        let mut base = [0usize; 3];
        let mut t = [0f64; 3];
        for a in 0..3 {
            let n = self.dims[a];
            if n == 0 {
                return 0.0;
            }
            let hi = (n - 1) as f64;
            let c = coords[a];
            if c < -EPS || c > hi + EPS {
                return 0.0;
            }
            if n == 1 {
                base[a] = 0;
                t[a] = 0.0;
            } else {
                let cc = c.clamp(0.0, hi);
                let mut i0 = cc.floor() as usize;
                if i0 >= n - 1 {
                    i0 = n - 2;
                }
                base[a] = i0;
                t[a] = cc - i0 as f64;
            }
        }

        let hi = |a: usize| (base[a] + 1).min(self.dims[a] - 1);
        let (i0, j0, k0) = (base[0], base[1], base[2]);
        let (i1, j1, k1) = (hi(0), hi(1), hi(2));
        let v = |i, j, k| self.values[self.index(i, j, k)];

        // Interpolate along x, then y, then z.
        let lerp = |a: f64, b: f64, s: f64| a + (b - a) * s;
        let c00 = lerp(v(i0, j0, k0), v(i1, j0, k0), t[0]);
        let c01 = lerp(v(i0, j0, k1), v(i1, j0, k1), t[0]);
        let c10 = lerp(v(i0, j1, k0), v(i1, j1, k0), t[0]);
        let c11 = lerp(v(i0, j1, k1), v(i1, j1, k1), t[0]);
        let c0 = lerp(c00, c10, t[1]);
        let c1 = lerp(c01, c11, t[1]);
        lerp(c0, c1, t[2])
    }
}

impl ScalarClosure for CubeSampler {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        // Interpolation needs the raw grid in f64; drop the derivative.
        let world = DVec3::new(p[0].value(), p[1].value(), p[2].value());
        S::constant(self.sample(world))
    }
}

/// Parses a Gaussian `.cube` file into a [`CubeData`].
///
/// # Errors
///
/// Returns [`ChemParseError`] if the header is truncated, a header/atom line is
/// malformed or non-numeric, or the volumetric block has fewer values than the
/// grid dimensions require.
///
/// ```
/// use manim_chem::cube::from_cube;
/// use glam::Vec3;
/// let src = "c\nc\n0 0 0 0\n-2 1 0 0\n-2 0 1 0\n-2 0 0 1\n1 1 1 1 1 1 1 1\n";
/// let cube = from_cube(src).unwrap();
/// assert_eq!(cube.origin, Vec3::ZERO);
/// assert_eq!(cube.dims, [2, 2, 2]);
/// ```
pub fn from_cube(s: &str) -> Result<CubeData, ChemParseError> {
    const FMT: &str = "cube";
    let lines: Vec<&str> = s.lines().collect();
    // 2 comment lines + origin line + 3 axis lines = 6 minimum.
    if lines.len() < 6 {
        return Err(ChemParseError::UnexpectedEof {
            format: FMT,
            expected: "cube header (>= 6 lines)",
        });
    }

    // Line 3 (index 2): natoms x0 y0 z0.
    let (natoms_raw, ox, oy, oz) = {
        let ln = 3;
        let mut toks = lines[2].split_whitespace();
        let natoms = read_i32(&mut toks, FMT, ln, "atom count", lines[2])?;
        let x = read_f32(&mut toks, FMT, ln, "origin x", lines[2])?;
        let y = read_f32(&mut toks, FMT, ln, "origin y", lines[2])?;
        let z = read_f32(&mut toks, FMT, ln, "origin z", lines[2])?;
        (natoms, x, y, z)
    };
    let is_mo_cube = natoms_raw < 0;
    let natoms = natoms_raw.unsigned_abs() as usize;

    // Lines 4..6 (index 3..5): axis vectors. Sign of the count flags units.
    let mut dims = [0usize; 3];
    let mut axis_raw = [Vec3::ZERO; 3];
    let mut bohr = true;
    for (a, li) in (3..6).enumerate() {
        let ln = li + 1;
        let mut toks = lines[li].split_whitespace();
        let npts = read_i32(&mut toks, FMT, ln, "voxel count", lines[li])?;
        if a == 0 {
            bohr = npts > 0;
        }
        dims[a] = npts.unsigned_abs() as usize;
        let vx = read_f32(&mut toks, FMT, ln, "voxel vector x", lines[li])?;
        let vy = read_f32(&mut toks, FMT, ln, "voxel vector y", lines[li])?;
        let vz = read_f32(&mut toks, FMT, ln, "voxel vector z", lines[li])?;
        axis_raw[a] = Vec3::new(vx, vy, vz);
    }

    let scale = if bohr { BOHR_TO_ANG } else { 1.0 };
    let origin = Vec3::new(ox, oy, oz) * scale;
    let axes = [
        axis_raw[0] * scale,
        axis_raw[1] * scale,
        axis_raw[2] * scale,
    ];

    // Atom block: `Z charge x y z`, one per atom, starting at line index 6.
    let mut atoms = Vec::with_capacity(natoms);
    let mut cursor = 6;
    for _ in 0..natoms {
        let ln = cursor + 1;
        let line = lines.get(cursor).ok_or(ChemParseError::UnexpectedEof {
            format: FMT,
            expected: "more atom lines",
        })?;
        let mut toks = line.split_whitespace();
        let z = read_i32(&mut toks, FMT, ln, "atomic number", line)?;
        let _charge = read_f32(&mut toks, FMT, ln, "nuclear charge", line)?;
        let x = read_f32(&mut toks, FMT, ln, "atom x", line)?;
        let y = read_f32(&mut toks, FMT, ln, "atom y", line)?;
        let zc = read_f32(&mut toks, FMT, ln, "atom z", line)?;
        atoms.push((z.unsigned_abs() as u8, Vec3::new(x, y, zc) * scale));
        cursor += 1;
    }

    // MO cubes carry one extra line (orbital count + IDs) before the data.
    if is_mo_cube {
        cursor += 1;
    }

    // Volumetric data: read exactly nx·ny·nz floats from the remaining lines.
    let want = dims[0] * dims[1] * dims[2];
    let mut values = Vec::with_capacity(want);
    'outer: for (off, line) in lines[cursor..].iter().enumerate() {
        let ln = cursor + off + 1;
        for tok in line.split_whitespace() {
            let v = tok.parse::<f32>().map_err(|_| ChemParseError::BadNumber {
                format: FMT,
                line: ln,
                field: "grid value",
                found: tok.to_string(),
            })?;
            values.push(v);
            if values.len() == want {
                break 'outer;
            }
        }
    }
    if values.len() < want {
        return Err(ChemParseError::UnexpectedEof {
            format: FMT,
            expected: "more volumetric data",
        });
    }

    Ok(CubeData {
        origin,
        axes,
        dims,
        values,
        atoms,
    })
}

/// Reads the next token as `f32`, mapping failure onto a [`ChemParseError`].
fn read_f32<'a>(
    toks: &mut impl Iterator<Item = &'a str>,
    format: &'static str,
    line: usize,
    field: &'static str,
    whole: &str,
) -> Result<f32, ChemParseError> {
    let tok = toks.next().ok_or_else(|| ChemParseError::Malformed {
        format,
        line,
        expected: "a numeric field",
        found: whole.to_string(),
    })?;
    tok.parse::<f32>().map_err(|_| ChemParseError::BadNumber {
        format,
        line,
        field,
        found: tok.to_string(),
    })
}

/// Reads the next token as `i32`, mapping failure onto a [`ChemParseError`].
fn read_i32<'a>(
    toks: &mut impl Iterator<Item = &'a str>,
    format: &'static str,
    line: usize,
    field: &'static str,
    whole: &str,
) -> Result<i32, ChemParseError> {
    let tok = toks.next().ok_or_else(|| ChemParseError::Malformed {
        format,
        line,
        expected: "a numeric field",
        found: whole.to_string(),
    })?;
    tok.parse::<i32>().map_err(|_| ChemParseError::BadNumber {
        format,
        line,
        field,
        found: tok.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_fields::Point;

    /// A 3×3×3 grid (in ångström) of the analytic field f(x,y,z) = x.
    fn linear_x_cube() -> String {
        let mut src = String::from("linear f=x cube\nsecond comment\n");
        // natoms=1, origin at the origin.
        src.push_str("1 0.0 0.0 0.0\n");
        // Negative voxel counts flag ångström units; unit step along each axis.
        src.push_str("-3 1.0 0.0 0.0\n");
        src.push_str("-3 0.0 1.0 0.0\n");
        src.push_str("-3 0.0 0.0 1.0\n");
        // One carbon atom at the origin.
        src.push_str("6 0.0 0.0 0.0 0.0\n");
        // Values: x-axis slowest, so value == i for grid point (i, j, k).
        for i in 0..3 {
            for _ in 0..9 {
                src.push_str(&format!("{}.0 ", i));
            }
            src.push('\n');
        }
        src
    }

    #[test]
    fn parses_header() {
        let cube = from_cube(&linear_x_cube()).unwrap();
        assert_eq!(cube.dims, [3, 3, 3]);
        assert_eq!(cube.origin, Vec3::ZERO);
        assert_eq!(cube.axes[0], Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(cube.values.len(), 27);
        assert_eq!(cube.atoms.len(), 1);
        assert_eq!(cube.atoms[0].0, 6);
    }

    #[test]
    fn value_range() {
        let cube = from_cube(&linear_x_cube()).unwrap();
        assert_eq!(cube.value_range(), (0.0, 2.0));
    }

    #[test]
    fn trilinear_matches_analytic() {
        let field = from_cube(&linear_x_cube()).unwrap().to_scalar_field();
        // f = x is linear, so trilinear interpolation is exact off the grid.
        for &(px, py, pz) in &[(1.3, 0.7, 0.2), (0.5, 1.9, 1.1), (2.0, 0.0, 0.0)] {
            let got = field.at(Point::new(px, py, pz));
            assert!(
                (got - px).abs() < 1e-5,
                "at ({px},{py},{pz}): got {got}, want {px}"
            );
        }
    }

    #[test]
    fn outside_box_is_zero() {
        let field = from_cube(&linear_x_cube()).unwrap().to_scalar_field();
        assert_eq!(field.at(Point::new(-1.0, 0.0, 0.0)), 0.0);
        assert_eq!(field.at(Point::new(5.0, 0.0, 0.0)), 0.0);
        assert_eq!(field.at(Point::new(1.0, 9.0, 0.0)), 0.0);
    }

    #[test]
    fn bohr_units_are_converted() {
        // Positive voxel counts => bohr; a unit step becomes BOHR_TO_ANG ang.
        let src = "c\nc\n0 0 0 0\n2 1 0 0\n2 0 1 0\n2 0 0 1\n0 0 0 0 0 0 0 0\n";
        let cube = from_cube(src).unwrap();
        assert!((cube.axes[0].x - BOHR_TO_ANG).abs() < 1e-6);
    }

    #[test]
    fn truncated_data_errors() {
        let src = "c\nc\n0 0 0 0\n-2 1 0 0\n-2 0 1 0\n-2 0 0 1\n1 1 1\n";
        assert!(matches!(
            from_cube(src),
            Err(ChemParseError::UnexpectedEof { .. })
        ));
    }
}
