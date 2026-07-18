//! Tiny, dependency-free parsers for two common molecular-geometry formats.
//!
//! - [`from_xyz`] reads the standard XYZ format (atom count, comment, then
//!   `Sym x y z` lines). XYZ carries no connectivity, so the returned
//!   [`Molecule`] has no bonds — bond perception lives in the renderer.
//! - [`from_sdf`] reads the MDL V2000 Molfile / SDF subset: the counts line,
//!   the atom block (`x y z Sym …`) and the bond block (`a b order …`, with
//!   1-based atom indices that are converted to 0-based [`Bond`] indices).
//!   Parsing stops at the first `M  END` or `$$$$` terminator.
//!
//! Both parsers report errors as [`ChemParseError`], whose messages name the
//! offending 1-based line number.
//!
//! ```
//! use manim_chem::parsers::from_xyz;
//! let mol = from_xyz("2\nH2\nH 0 0 0\nH 0 0 0.74\n").unwrap();
//! assert_eq!(mol.atoms.len(), 2);
//! assert!(mol.bonds.is_empty());
//! ```

use std::fmt;

use glam::Vec3;

use crate::molecule::{Atom, Bond, Molecule};

/// An error from the XYZ, SDF, or Gaussian-cube parsers.
///
/// Every variant carries the format name and the 1-based line number, so the
/// [`Display`](fmt::Display) message can point at the offending line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChemParseError {
    /// The input ended before a required section was read.
    UnexpectedEof {
        /// The format being parsed (`"xyz"`, `"sdf"`, `"cube"`).
        format: &'static str,
        /// What the parser was still expecting.
        expected: &'static str,
    },
    /// A line did not have the expected shape.
    Malformed {
        /// The format being parsed.
        format: &'static str,
        /// The 1-based line number.
        line: usize,
        /// A short description of the expected layout.
        expected: &'static str,
        /// The offending line's contents.
        found: String,
    },
    /// A numeric field could not be parsed.
    BadNumber {
        /// The format being parsed.
        format: &'static str,
        /// The 1-based line number.
        line: usize,
        /// The name of the field that failed.
        field: &'static str,
        /// The token that failed to parse.
        found: String,
    },
    /// A bond referenced an atom index outside `1..=count`.
    BadIndex {
        /// The format being parsed.
        format: &'static str,
        /// The 1-based line number.
        line: usize,
        /// The offending 1-based atom index.
        index: usize,
        /// The number of atoms actually present.
        count: usize,
    },
}

impl fmt::Display for ChemParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChemParseError::UnexpectedEof { format, expected } => {
                write!(f, "{format}: unexpected end of input, expected {expected}")
            }
            ChemParseError::Malformed {
                format,
                line,
                expected,
                found,
            } => write!(
                f,
                "{format}: line {line}: expected `{expected}`, got `{found}`"
            ),
            ChemParseError::BadNumber {
                format,
                line,
                field,
                found,
            } => write!(
                f,
                "{format}: line {line}: could not parse {field} from `{found}`"
            ),
            ChemParseError::BadIndex {
                format,
                line,
                index,
                count,
            } => write!(
                f,
                "{format}: line {line}: atom index {index} out of range (1..={count})"
            ),
        }
    }
}

impl std::error::Error for ChemParseError {}

/// Parses a floating-point token, mapping failure to [`ChemParseError::BadNumber`].
fn parse_f32(
    tok: &str,
    format: &'static str,
    line: usize,
    field: &'static str,
) -> Result<f32, ChemParseError> {
    tok.parse::<f32>().map_err(|_| ChemParseError::BadNumber {
        format,
        line,
        field,
        found: tok.to_string(),
    })
}

/// Parses an integer token, mapping failure to [`ChemParseError::BadNumber`].
fn parse_usize(
    tok: &str,
    format: &'static str,
    line: usize,
    field: &'static str,
) -> Result<usize, ChemParseError> {
    tok.parse::<usize>().map_err(|_| ChemParseError::BadNumber {
        format,
        line,
        field,
        found: tok.to_string(),
    })
}

/// Parses a molecule from the standard XYZ format.
///
/// Layout: line 1 is the atom count, line 2 is a free-form comment, then one
/// `Sym x y z` line per atom (extra trailing columns are ignored). The result
/// has no bonds.
///
/// # Errors
///
/// Returns [`ChemParseError`] if the count is missing/unparseable, the input is
/// truncated, or an atom line lacks a symbol and three coordinates.
///
/// ```
/// use manim_chem::parsers::from_xyz;
/// let mol = from_xyz("1\nlone atom\nNe 1.0 2.0 3.0\n").unwrap();
/// assert_eq!(mol.atoms[0].element, "Ne");
/// assert_eq!(mol.atoms[0].pos.z, 3.0);
/// ```
pub fn from_xyz(s: &str) -> Result<Molecule, ChemParseError> {
    const FMT: &str = "xyz";
    let mut lines = s.lines().enumerate();

    // Line 1: atom count.
    let (idx, count_line) = lines.next().ok_or(ChemParseError::UnexpectedEof {
        format: FMT,
        expected: "atom count on line 1",
    })?;
    let count = parse_usize(count_line.trim(), FMT, idx + 1, "atom count")?;

    // Line 2: comment (ignored, but must exist).
    lines.next().ok_or(ChemParseError::UnexpectedEof {
        format: FMT,
        expected: "comment line 2",
    })?;

    let mut atoms = Vec::with_capacity(count);
    for _ in 0..count {
        let (idx, line) = lines.next().ok_or(ChemParseError::UnexpectedEof {
            format: FMT,
            expected: "more atom lines",
        })?;
        let ln = idx + 1;
        let mut toks = line.split_whitespace();
        let sym = toks.next().ok_or_else(|| ChemParseError::Malformed {
            format: FMT,
            line: ln,
            expected: "Sym x y z",
            found: line.to_string(),
        })?;
        let (x, y, z) = read_xyz_coords(&mut toks, FMT, ln, line)?;
        atoms.push(Atom::new(sym, Vec3::new(x, y, z)));
    }

    Ok(Molecule {
        atoms,
        bonds: Vec::new(),
    })
}

/// Reads three coordinate tokens, reporting the whole line on failure.
fn read_xyz_coords<'a>(
    toks: &mut impl Iterator<Item = &'a str>,
    format: &'static str,
    line: usize,
    whole: &str,
) -> Result<(f32, f32, f32), ChemParseError> {
    let mut next = |field: &'static str| -> Result<f32, ChemParseError> {
        let tok = toks.next().ok_or_else(|| ChemParseError::Malformed {
            format,
            line,
            expected: "Sym x y z",
            found: whole.to_string(),
        })?;
        parse_f32(tok, format, line, field)
    };
    let x = next("x")?;
    let y = next("y")?;
    let z = next("z")?;
    Ok((x, y, z))
}

/// Parses a molecule from the MDL V2000 Molfile / SDF subset.
///
/// The first three lines are the title / program / comment header. Line 4 is
/// the counts line, whose first two integer fields are the atom and bond
/// counts. The atom block follows (`x y z Sym …`), then the bond block
/// (`a b order …`, 1-based indices). Parsing stops at `M  END` or `$$$$`.
///
/// # Errors
///
/// Returns [`ChemParseError`] on a truncated file, a malformed counts / atom /
/// bond line, or a bond that references an atom index out of `1..=natoms`.
///
/// ```
/// use manim_chem::parsers::from_sdf;
/// let sdf = "\n  prog\n\n  2  1  0  0  0  0  0  0  0  0999 V2000\n\
///     0.0 0.0 0.0 H 0 0 0 0 0 0 0 0 0 0 0 0\n\
///     0.0 0.0 0.74 H 0 0 0 0 0 0 0 0 0 0 0 0\n\
///   1  2  1  0\nM  END\n";
/// let mol = from_sdf(sdf).unwrap();
/// assert_eq!(mol.atoms.len(), 2);
/// assert_eq!(mol.bonds[0], manim_chem::molecule::Bond::new(0, 1, 1));
/// ```
pub fn from_sdf(s: &str) -> Result<Molecule, ChemParseError> {
    const FMT: &str = "sdf";
    let lines: Vec<&str> = s.lines().collect();

    // Lines 1..=3: header (title, program, comment). Line 4 (index 3): counts.
    if lines.len() < 4 {
        return Err(ChemParseError::UnexpectedEof {
            format: FMT,
            expected: "header and counts line (4 lines)",
        });
    }
    let counts = lines[3];
    let mut ctoks = counts.split_whitespace();
    let natoms = ctoks
        .next()
        .ok_or_else(|| ChemParseError::Malformed {
            format: FMT,
            line: 4,
            expected: "natoms nbonds ...",
            found: counts.to_string(),
        })
        .and_then(|t| parse_usize(t, FMT, 4, "atom count"))?;
    let nbonds = ctoks
        .next()
        .ok_or_else(|| ChemParseError::Malformed {
            format: FMT,
            line: 4,
            expected: "natoms nbonds ...",
            found: counts.to_string(),
        })
        .and_then(|t| parse_usize(t, FMT, 4, "bond count"))?;

    // Atom block: lines with index 4.. (i.e. line number 5..).
    let mut atoms = Vec::with_capacity(natoms);
    let atom_start = 4;
    for a in 0..natoms {
        let li = atom_start + a;
        let ln = li + 1;
        let line = lines.get(li).ok_or(ChemParseError::UnexpectedEof {
            format: FMT,
            expected: "more atom lines",
        })?;
        let mut toks = line.split_whitespace();
        let mut coord = |field: &'static str| -> Result<f32, ChemParseError> {
            let tok = toks.next().ok_or_else(|| ChemParseError::Malformed {
                format: FMT,
                line: ln,
                expected: "x y z Sym",
                found: line.to_string(),
            })?;
            parse_f32(tok, FMT, ln, field)
        };
        let x = coord("x")?;
        let y = coord("y")?;
        let z = coord("z")?;
        let sym = toks.next().ok_or_else(|| ChemParseError::Malformed {
            format: FMT,
            line: ln,
            expected: "x y z Sym",
            found: line.to_string(),
        })?;
        atoms.push(Atom::new(sym, Vec3::new(x, y, z)));
    }

    // Bond block: immediately after the atom block.
    let mut bonds = Vec::with_capacity(nbonds);
    let bond_start = atom_start + natoms;
    for b in 0..nbonds {
        let li = bond_start + b;
        let ln = li + 1;
        let line = lines.get(li).ok_or(ChemParseError::UnexpectedEof {
            format: FMT,
            expected: "more bond lines",
        })?;
        if is_terminator(line) {
            return Err(ChemParseError::Malformed {
                format: FMT,
                line: ln,
                expected: "a b order",
                found: line.to_string(),
            });
        }
        let mut toks = line.split_whitespace();
        let mut idx = |field: &'static str| -> Result<usize, ChemParseError> {
            let tok = toks.next().ok_or_else(|| ChemParseError::Malformed {
                format: FMT,
                line: ln,
                expected: "a b order",
                found: line.to_string(),
            })?;
            parse_usize(tok, FMT, ln, field)
        };
        let a1 = idx("atom a")?;
        let b1 = idx("atom b")?;
        let order = idx("bond order")?;
        for one_based in [a1, b1] {
            if one_based < 1 || one_based > natoms {
                return Err(ChemParseError::BadIndex {
                    format: FMT,
                    line: ln,
                    index: one_based,
                    count: natoms,
                });
            }
        }
        bonds.push(Bond::new(a1 - 1, b1 - 1, order as u8));
    }

    Ok(Molecule { atoms, bonds })
}

/// True if `line` is an SDF record terminator (`M  END` or `$$$$`).
fn is_terminator(line: &str) -> bool {
    let t = line.trim_end();
    t.trim_start().starts_with("M  END") || t.trim() == "$$$$"
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    const WATER_XYZ: &str = "3
water molecule
O  0.00000  0.00000  0.00000
H  0.75800  0.58600  0.00000
H -0.75800  0.58600  0.00000
";

    const WATER_SDF: &str = "
  manim-chem

  3  2  0  0  0  0  0  0  0  0999 V2000
    0.0000    0.0000    0.0000 O   0  0  0  0  0  0  0  0  0  0  0  0
    0.7580    0.5860    0.0000 H   0  0  0  0  0  0  0  0  0  0  0  0
   -0.7580    0.5860    0.0000 H   0  0  0  0  0  0  0  0  0  0  0  0
  1  2  1  0
  1  3  1  0
M  END
";

    // Benzene C6H6: 6 ring carbons (alternating single/double bonds) + 6 H.
    const BENZENE_SDF: &str = "
  manim-chem benzene

 12 12  0  0  0  0  0  0  0  0999 V2000
    1.3970    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.6985    1.2098    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
   -0.6985    1.2098    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
   -1.3970    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
   -0.6985   -1.2098    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.6985   -1.2098    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    2.4810    0.0000    0.0000 H   0  0  0  0  0  0  0  0  0  0  0  0
    1.2405    2.1486    0.0000 H   0  0  0  0  0  0  0  0  0  0  0  0
   -1.2405    2.1486    0.0000 H   0  0  0  0  0  0  0  0  0  0  0  0
   -2.4810    0.0000    0.0000 H   0  0  0  0  0  0  0  0  0  0  0  0
   -1.2405   -2.1486    0.0000 H   0  0  0  0  0  0  0  0  0  0  0  0
    1.2405   -2.1486    0.0000 H   0  0  0  0  0  0  0  0  0  0  0  0
  1  2  2  0
  2  3  1  0
  3  4  2  0
  4  5  1  0
  5  6  2  0
  6  1  1  0
  1  7  1  0
  2  8  1  0
  3  9  1  0
  4 10  1  0
  5 11  1  0
  6 12  1  0
M  END
$$$$
";

    #[test]
    fn water_xyz_roundtrip() {
        let mol = from_xyz(WATER_XYZ).unwrap();
        assert_eq!(mol.atoms.len(), 3);
        assert!(mol.bonds.is_empty());
        assert_eq!(mol.atoms[0].element, "O");
        assert_eq!(mol.atoms[1].element, "H");
        assert_relative_eq!(mol.atoms[1].pos.x, 0.758, epsilon = 1e-5);
        assert_relative_eq!(mol.atoms[2].pos.x, -0.758, epsilon = 1e-5);
    }

    #[test]
    fn water_sdf_roundtrip() {
        let mol = from_sdf(WATER_SDF).unwrap();
        assert_eq!(mol.atoms.len(), 3);
        assert_eq!(mol.bonds.len(), 2);
        assert_eq!(mol.atoms[0].element, "O");
        // 1-based (1,2)/(1,3) -> 0-based (0,1)/(0,2).
        assert_eq!(mol.bonds[0], Bond::new(0, 1, 1));
        assert_eq!(mol.bonds[1], Bond::new(0, 2, 1));
    }

    #[test]
    fn benzene_sdf() {
        let mol = from_sdf(BENZENE_SDF).unwrap();
        assert_eq!(mol.atoms.len(), 12);
        assert_eq!(mol.bonds.len(), 12);
        let carbons = mol.atoms.iter().filter(|a| a.element == "C").count();
        assert_eq!(carbons, 6);
        // Ring bonds alternate double/single.
        assert_eq!(mol.bonds[0].order, 2);
        assert_eq!(mol.bonds[1].order, 1);
        assert_eq!(mol.bonds[2].order, 2);
        // C-H bonds are single.
        assert!(mol.bonds[6..].iter().all(|b| b.order == 1));
    }

    #[test]
    fn xyz_malformed_reports_line() {
        // Atom line 4 (0-based file line 4, 1-based 4) is missing a coordinate.
        let bad = "2\ncomment\nC 0.0 0.0 0.0\nH 1.0 2.0\n";
        let err = from_xyz(bad).unwrap_err();
        match err {
            ChemParseError::Malformed { line, .. } => assert_eq!(line, 4),
            other => panic!("expected Malformed on line 4, got {other:?}"),
        }
        assert!(format!("{err}").contains("line 4"));
    }

    #[test]
    fn xyz_bad_number_names_field() {
        let bad = "1\ncomment\nC zero 0.0 0.0\n";
        let err = from_xyz(bad).unwrap_err();
        assert!(matches!(err, ChemParseError::BadNumber { line: 3, .. }));
    }

    #[test]
    fn sdf_bad_index_out_of_range() {
        let bad = "
  prog

  1  1  0  0  0  0  0  0  0  0999 V2000
    0.0 0.0 0.0 H 0 0 0 0 0 0 0 0 0 0 0 0
  1  5  1  0
M  END
";
        let err = from_sdf(bad).unwrap_err();
        match err {
            ChemParseError::BadIndex { index, count, .. } => {
                assert_eq!(index, 5);
                assert_eq!(count, 1);
            }
            other => panic!("expected BadIndex, got {other:?}"),
        }
    }

    #[test]
    fn error_is_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        let err = from_xyz("").unwrap_err();
        assert_error(&err);
    }
}
