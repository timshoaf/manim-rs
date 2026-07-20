//! CPK element data: atomic number, colour, and radii for H..Xe (Z = 1..54).
//!
//! Per-element data used by the ball-and-stick / space-filling builders:
//!
//! - **Colours** are the Jmol CPK palette (Corey–Pauling–Koltun colouring as
//!   popularised by Jmol / RasMol): H white, C dark grey, N blue, O red,
//!   F/Cl green, P orange, S yellow, Br dark red, Na purple, Fe orange-brown,
//!   etc. Stored as 8-bit sRGB via [`Color::from_srgb_u8`].
//! - **Covalent radii** (ångström) are the single-bond values of
//!   Cordero *et al.*, "Covalent radii revisited", *Dalton Trans.* (2008)
//!   2832–2838.
//! - **Van-der-Waals radii** (ångström) are Bondi, *J. Phys. Chem.* **68**
//!   (1964) 441 for the main-group elements Bondi tabulated, filled in for the
//!   remaining elements (Be, B, the transition metals, …) from Alvarez,
//!   "A cartography of the van der Waals territories", *Dalton Trans.* **42**
//!   (2013) 8617.
//!
//! - **Ionic radii** (ångström) are Shannon effective ionic radii for
//!   **coordination number 6**: R. D. Shannon, "Revised effective ionic radii
//!   and systematic studies of interatomic distances in halides and
//!   chalcogenides", *Acta Cryst.* **A32** (1976) 751–767. See
//!   [`ionic_radius`] for the caveats that come with using one CN.
//! - **Electronegativities** are Pauling-scale values (CRC Handbook), used only
//!   by the [`BondRule`](crate::render::BondRule) ionic heuristic.
//!
//! ```
//! use manim_chem::element::data;
//! let o = data("O").unwrap();
//! assert_eq!(o.z, 8);
//! assert!(o.cpk_color.r > o.cpk_color.b); // oxygen is red
//! assert!(data("Xx").is_none());
//! ```

use manim_core::prelude::Color;

/// Per-element data for rendering: atomic number, CPK colour, and radii.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementData {
    /// Atomic number (proton count).
    pub z: u8,
    /// Jmol/CPK colour for this element.
    pub cpk_color: Color,
    /// Single-bond covalent radius in ångström (Cordero *et al.* 2008).
    pub covalent_radius: f32,
    /// Van-der-Waals radius in ångström (Bondi 1964 / Alvarez 2013).
    pub vdw_radius: f32,
}

/// One row of the raw element table: symbol, Z, sRGB colour, covalent, vdW.
struct Row {
    symbol: &'static str,
    z: u8,
    rgb: (u8, u8, u8),
    covalent: f32,
    vdw: f32,
}

/// The element table for H..Xe (Z = 1..54).
///
/// Colours are Jmol CPK sRGB triples; radii are in ångström (see module docs).
#[rustfmt::skip]
static TABLE: &[Row] = &[
    Row { symbol: "H",  z: 1,  rgb: (0xFF, 0xFF, 0xFF), covalent: 0.31, vdw: 1.20 },
    Row { symbol: "He", z: 2,  rgb: (0xD9, 0xFF, 0xFF), covalent: 0.28, vdw: 1.40 },
    Row { symbol: "Li", z: 3,  rgb: (0xCC, 0x80, 0xFF), covalent: 1.28, vdw: 1.82 },
    Row { symbol: "Be", z: 4,  rgb: (0xC2, 0xFF, 0x00), covalent: 0.96, vdw: 1.53 },
    Row { symbol: "B",  z: 5,  rgb: (0xFF, 0xB5, 0xB5), covalent: 0.84, vdw: 1.92 },
    Row { symbol: "C",  z: 6,  rgb: (0x90, 0x90, 0x90), covalent: 0.76, vdw: 1.70 },
    Row { symbol: "N",  z: 7,  rgb: (0x30, 0x50, 0xF8), covalent: 0.71, vdw: 1.55 },
    Row { symbol: "O",  z: 8,  rgb: (0xFF, 0x0D, 0x0D), covalent: 0.66, vdw: 1.52 },
    Row { symbol: "F",  z: 9,  rgb: (0x90, 0xE0, 0x50), covalent: 0.57, vdw: 1.47 },
    Row { symbol: "Ne", z: 10, rgb: (0xB3, 0xE3, 0xF5), covalent: 0.58, vdw: 1.54 },
    Row { symbol: "Na", z: 11, rgb: (0xAB, 0x5C, 0xF2), covalent: 1.66, vdw: 2.27 },
    Row { symbol: "Mg", z: 12, rgb: (0x8A, 0xFF, 0x00), covalent: 1.41, vdw: 1.73 },
    Row { symbol: "Al", z: 13, rgb: (0xBF, 0xA6, 0xA6), covalent: 1.21, vdw: 1.84 },
    Row { symbol: "Si", z: 14, rgb: (0xF0, 0xC8, 0xA0), covalent: 1.11, vdw: 2.10 },
    Row { symbol: "P",  z: 15, rgb: (0xFF, 0x80, 0x00), covalent: 1.07, vdw: 1.80 },
    Row { symbol: "S",  z: 16, rgb: (0xFF, 0xFF, 0x30), covalent: 1.05, vdw: 1.80 },
    Row { symbol: "Cl", z: 17, rgb: (0x1F, 0xF0, 0x1F), covalent: 1.02, vdw: 1.75 },
    Row { symbol: "Ar", z: 18, rgb: (0x80, 0xD1, 0xE3), covalent: 1.06, vdw: 1.88 },
    Row { symbol: "K",  z: 19, rgb: (0x8F, 0x40, 0xD4), covalent: 2.03, vdw: 2.75 },
    Row { symbol: "Ca", z: 20, rgb: (0x3D, 0xFF, 0x00), covalent: 1.76, vdw: 2.31 },
    Row { symbol: "Sc", z: 21, rgb: (0xE6, 0xE6, 0xE6), covalent: 1.70, vdw: 2.11 },
    Row { symbol: "Ti", z: 22, rgb: (0xBF, 0xC2, 0xC7), covalent: 1.60, vdw: 2.15 },
    Row { symbol: "V",  z: 23, rgb: (0xA6, 0xA6, 0xAB), covalent: 1.53, vdw: 2.07 },
    Row { symbol: "Cr", z: 24, rgb: (0x8A, 0x99, 0xC7), covalent: 1.39, vdw: 2.06 },
    Row { symbol: "Mn", z: 25, rgb: (0x9C, 0x7A, 0xC7), covalent: 1.39, vdw: 2.05 },
    Row { symbol: "Fe", z: 26, rgb: (0xE0, 0x66, 0x33), covalent: 1.32, vdw: 2.04 },
    Row { symbol: "Co", z: 27, rgb: (0xF0, 0x90, 0xA0), covalent: 1.26, vdw: 2.00 },
    Row { symbol: "Ni", z: 28, rgb: (0x50, 0xD0, 0x50), covalent: 1.24, vdw: 1.63 },
    Row { symbol: "Cu", z: 29, rgb: (0xC8, 0x80, 0x33), covalent: 1.32, vdw: 1.40 },
    Row { symbol: "Zn", z: 30, rgb: (0x7D, 0x80, 0xB0), covalent: 1.22, vdw: 1.39 },
    Row { symbol: "Ga", z: 31, rgb: (0xC2, 0x8F, 0x8F), covalent: 1.22, vdw: 1.87 },
    Row { symbol: "Ge", z: 32, rgb: (0x66, 0x8F, 0x8F), covalent: 1.20, vdw: 2.11 },
    Row { symbol: "As", z: 33, rgb: (0xBD, 0x80, 0xE3), covalent: 1.19, vdw: 1.85 },
    Row { symbol: "Se", z: 34, rgb: (0xFF, 0xA1, 0x00), covalent: 1.20, vdw: 1.90 },
    Row { symbol: "Br", z: 35, rgb: (0xA6, 0x29, 0x29), covalent: 1.20, vdw: 1.85 },
    Row { symbol: "Kr", z: 36, rgb: (0x5C, 0xB8, 0xD1), covalent: 1.16, vdw: 2.02 },
    Row { symbol: "Rb", z: 37, rgb: (0x70, 0x2E, 0xB0), covalent: 2.20, vdw: 3.03 },
    Row { symbol: "Sr", z: 38, rgb: (0x00, 0xFF, 0x00), covalent: 1.95, vdw: 2.49 },
    Row { symbol: "Y",  z: 39, rgb: (0x94, 0xFF, 0xFF), covalent: 1.90, vdw: 2.19 },
    Row { symbol: "Zr", z: 40, rgb: (0x94, 0xE0, 0xE0), covalent: 1.75, vdw: 2.15 },
    Row { symbol: "Nb", z: 41, rgb: (0x73, 0xC2, 0xC9), covalent: 1.64, vdw: 2.07 },
    Row { symbol: "Mo", z: 42, rgb: (0x54, 0xB5, 0xB5), covalent: 1.54, vdw: 2.11 },
    Row { symbol: "Tc", z: 43, rgb: (0x3B, 0x9E, 0x9E), covalent: 1.47, vdw: 2.20 },
    Row { symbol: "Ru", z: 44, rgb: (0x24, 0x8F, 0x8F), covalent: 1.46, vdw: 2.13 },
    Row { symbol: "Rh", z: 45, rgb: (0x0A, 0x7D, 0x8C), covalent: 1.42, vdw: 2.10 },
    Row { symbol: "Pd", z: 46, rgb: (0x00, 0x69, 0x85), covalent: 1.39, vdw: 1.63 },
    Row { symbol: "Ag", z: 47, rgb: (0xC0, 0xC0, 0xC0), covalent: 1.45, vdw: 1.72 },
    Row { symbol: "Cd", z: 48, rgb: (0xFF, 0xD9, 0x8F), covalent: 1.44, vdw: 1.58 },
    Row { symbol: "In", z: 49, rgb: (0xA6, 0x75, 0x73), covalent: 1.42, vdw: 1.93 },
    Row { symbol: "Sn", z: 50, rgb: (0x66, 0x80, 0x80), covalent: 1.39, vdw: 2.17 },
    Row { symbol: "Sb", z: 51, rgb: (0x9E, 0x63, 0xB5), covalent: 1.39, vdw: 2.06 },
    Row { symbol: "Te", z: 52, rgb: (0xD4, 0x7A, 0x00), covalent: 1.38, vdw: 2.06 },
    Row { symbol: "I",  z: 53, rgb: (0x94, 0x00, 0x94), covalent: 1.39, vdw: 1.98 },
    Row { symbol: "Xe", z: 54, rgb: (0x42, 0x9E, 0xB0), covalent: 1.40, vdw: 2.16 },
];

/// Normalises an element symbol to canonical casing (`"NA"`, `"na"` → `"Na"`).
///
/// Returns at most a two-byte buffer: first character upper-cased, any second
/// character lower-cased, the rest dropped. A `\0` second byte marks a
/// single-letter symbol.
fn canonical(symbol: &str) -> Option<[u8; 2]> {
    let mut chars = symbol.trim().chars();
    let first = chars.next()?.to_ascii_uppercase();
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let second = match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => c.to_ascii_lowercase(),
        Some(_) => return None,
        None => '\0',
    };
    Some([first as u8, second as u8])
}

/// Looks up per-element data by symbol (case-insensitive: `"fe"`, `"FE"`,
/// `"Fe"` all resolve to iron). Returns `None` for unknown symbols.
///
/// ```
/// use manim_chem::element::data;
/// assert_eq!(data("fe").unwrap().z, 26);
/// assert_eq!(data("H").unwrap().z, 1);
/// assert!(data("unobtanium").is_none());
/// ```
pub fn data(symbol: &str) -> Option<ElementData> {
    let key = canonical(symbol)?;
    let want: &[u8] = if key[1] == b'\0' {
        &key[0..1]
    } else {
        &key[0..2]
    };
    let want = std::str::from_utf8(want).ok()?;
    TABLE
        .iter()
        .find(|r| r.symbol == want)
        .map(|r| ElementData {
            z: r.z,
            cpk_color: Color::from_srgb_u8(r.rgb.0, r.rgb.1, r.rgb.2),
            covalent_radius: r.covalent,
            vdw_radius: r.vdw,
        })
}

/// One row of the ionic table: symbol, charge, Shannon CN-6 radius (ångström).
struct IonRow {
    symbol: &'static str,
    charge: i8,
    radius: f32,
}

/// Shannon effective ionic radii at **coordination number 6**, in ångström.
///
/// The *first* row for an element is its common/default oxidation state, which
/// is what [`common_charge`] returns and what [`ionic_radius`] uses when no
/// charge is supplied. Elements with no common monatomic ion (the noble gases,
/// and H, whose cation is a bare proton with no meaningful radius) are absent.
///
/// Transition-metal values are high-spin where the distinction applies.
#[rustfmt::skip]
static IONIC: &[IonRow] = &[
    IonRow { symbol: "Li", charge:  1, radius: 0.76 },
    IonRow { symbol: "Be", charge:  2, radius: 0.45 },
    IonRow { symbol: "B",  charge:  3, radius: 0.27 },
    IonRow { symbol: "C",  charge:  4, radius: 0.16 },
    IonRow { symbol: "N",  charge: -3, radius: 1.46 },
    IonRow { symbol: "O",  charge: -2, radius: 1.40 },
    IonRow { symbol: "F",  charge: -1, radius: 1.33 },
    IonRow { symbol: "Na", charge:  1, radius: 1.02 },
    IonRow { symbol: "Mg", charge:  2, radius: 0.72 },
    IonRow { symbol: "Al", charge:  3, radius: 0.535 },
    IonRow { symbol: "Si", charge:  4, radius: 0.40 },
    IonRow { symbol: "P",  charge:  5, radius: 0.38 },
    IonRow { symbol: "S",  charge: -2, radius: 1.84 },
    IonRow { symbol: "Cl", charge: -1, radius: 1.81 },
    IonRow { symbol: "K",  charge:  1, radius: 1.38 },
    IonRow { symbol: "Ca", charge:  2, radius: 1.00 },
    IonRow { symbol: "Sc", charge:  3, radius: 0.745 },
    IonRow { symbol: "Ti", charge:  4, radius: 0.605 },
    IonRow { symbol: "Ti", charge:  3, radius: 0.67 },
    IonRow { symbol: "V",  charge:  5, radius: 0.54 },
    IonRow { symbol: "V",  charge:  3, radius: 0.64 },
    IonRow { symbol: "Cr", charge:  3, radius: 0.615 },
    IonRow { symbol: "Cr", charge:  6, radius: 0.44 },
    IonRow { symbol: "Mn", charge:  2, radius: 0.83 },
    IonRow { symbol: "Mn", charge:  4, radius: 0.53 },
    IonRow { symbol: "Fe", charge:  3, radius: 0.645 },
    IonRow { symbol: "Fe", charge:  2, radius: 0.78 },
    IonRow { symbol: "Co", charge:  2, radius: 0.745 },
    IonRow { symbol: "Co", charge:  3, radius: 0.61 },
    IonRow { symbol: "Ni", charge:  2, radius: 0.69 },
    IonRow { symbol: "Cu", charge:  2, radius: 0.73 },
    IonRow { symbol: "Cu", charge:  1, radius: 0.77 },
    IonRow { symbol: "Zn", charge:  2, radius: 0.74 },
    IonRow { symbol: "Ga", charge:  3, radius: 0.62 },
    IonRow { symbol: "Ge", charge:  4, radius: 0.53 },
    IonRow { symbol: "As", charge:  3, radius: 0.58 },
    IonRow { symbol: "Se", charge: -2, radius: 1.98 },
    IonRow { symbol: "Br", charge: -1, radius: 1.96 },
    IonRow { symbol: "Rb", charge:  1, radius: 1.52 },
    IonRow { symbol: "Sr", charge:  2, radius: 1.18 },
    IonRow { symbol: "Y",  charge:  3, radius: 0.90 },
    IonRow { symbol: "Zr", charge:  4, radius: 0.72 },
    IonRow { symbol: "Nb", charge:  5, radius: 0.64 },
    IonRow { symbol: "Mo", charge:  6, radius: 0.59 },
    IonRow { symbol: "Mo", charge:  4, radius: 0.65 },
    IonRow { symbol: "Tc", charge:  4, radius: 0.645 },
    IonRow { symbol: "Ru", charge:  3, radius: 0.68 },
    IonRow { symbol: "Rh", charge:  3, radius: 0.665 },
    IonRow { symbol: "Pd", charge:  2, radius: 0.86 },
    IonRow { symbol: "Ag", charge:  1, radius: 1.15 },
    IonRow { symbol: "Cd", charge:  2, radius: 0.95 },
    IonRow { symbol: "In", charge:  3, radius: 0.80 },
    IonRow { symbol: "Sn", charge:  4, radius: 0.69 },
    IonRow { symbol: "Sb", charge:  3, radius: 0.76 },
    IonRow { symbol: "Te", charge: -2, radius: 2.21 },
    IonRow { symbol: "I",  charge: -1, radius: 2.20 },
];

/// Pauling-scale electronegativities (CRC Handbook), by symbol.
///
/// Elements with no accepted Pauling value (He, Ne, Ar) are absent.
#[rustfmt::skip]
static ELECTRONEGATIVITY: &[(&str, f32)] = &[
    ("H", 2.20),
    ("Li", 0.98), ("Be", 1.57), ("B", 2.04), ("C", 2.55), ("N", 3.04),
    ("O", 3.44), ("F", 3.98),
    ("Na", 0.93), ("Mg", 1.31), ("Al", 1.61), ("Si", 1.90), ("P", 2.19),
    ("S", 2.58), ("Cl", 3.16),
    ("K", 0.82), ("Ca", 1.00), ("Sc", 1.36), ("Ti", 1.54), ("V", 1.63),
    ("Cr", 1.66), ("Mn", 1.55), ("Fe", 1.83), ("Co", 1.88), ("Ni", 1.91),
    ("Cu", 1.90), ("Zn", 1.65), ("Ga", 1.81), ("Ge", 2.01), ("As", 2.18),
    ("Se", 2.55), ("Br", 2.96), ("Kr", 3.00),
    ("Rb", 0.82), ("Sr", 0.95), ("Y", 1.22), ("Zr", 1.33), ("Nb", 1.60),
    ("Mo", 2.16), ("Tc", 1.90), ("Ru", 2.20), ("Rh", 2.28), ("Pd", 2.20),
    ("Ag", 1.93), ("Cd", 1.69), ("In", 1.78), ("Sn", 1.96), ("Sb", 2.05),
    ("Te", 2.10), ("I", 2.66), ("Xe", 2.60),
];

/// Normalizes `symbol` to the exact spelling used as a table key, or `None` if
/// it is not a plausible element symbol.
fn table_key(symbol: &str) -> Option<String> {
    let key = canonical(symbol)?;
    let want: &[u8] = if key[1] == b'\0' {
        &key[0..1]
    } else {
        &key[0..2]
    };
    let mut s = std::str::from_utf8(want).ok()?.to_string();
    // `canonical` lowercases; the tables are keyed with a capital first letter.
    s[0..1].make_ascii_uppercase();
    Some(s)
}

/// The common (default) oxidation state of `symbol`, or `None` when the element
/// has no common monatomic ion in the table (noble gases, hydrogen).
///
/// ```
/// use manim_chem::element::common_charge;
/// assert_eq!(common_charge("Na"), Some(1));
/// assert_eq!(common_charge("Cl"), Some(-1));
/// assert_eq!(common_charge("Ar"), None);
/// ```
pub fn common_charge(symbol: &str) -> Option<i8> {
    let key = table_key(symbol)?;
    IONIC.iter().find(|r| r.symbol == key).map(|r| r.charge)
}

/// The Shannon effective ionic radius of `symbol` in ångström, for `charge` —
/// or for the element's [`common_charge`] when `charge` is `None`.
///
/// Returns `None` when the element has no ion in the table, or when it has ions
/// but not the requested `charge`.
///
/// **Caveats.** Every value is the **CN = 6** (octahedral) radius. Real ionic
/// radii grow with coordination number — Na⁺ is 1.02 Å at CN 6 but 1.18 Å at
/// CN 8 — so a structure with a different coordination will be drawn slightly
/// off. Values for transition metals are high-spin where the spin state
/// matters. These are *effective* radii from crystal-structure fits, not
/// physical boundaries: they are the right choice for showing the relative size
/// of ions in a lattice, and the wrong one for a covalent molecule (use
/// [`RadiusSource::Covalent`](crate::render::RadiusSource::Covalent) there).
///
/// ```
/// use manim_chem::element::ionic_radius;
/// // Rock salt: the anion is much the larger of the pair.
/// assert!(ionic_radius("Cl", None).unwrap() > ionic_radius("Na", None).unwrap());
/// // An explicit charge selects a different oxidation state.
/// assert!(ionic_radius("Fe", Some(2)).unwrap() > ionic_radius("Fe", Some(3)).unwrap());
/// assert!(ionic_radius("Ar", None).is_none());
/// assert!(ionic_radius("Na", Some(7)).is_none());
/// ```
pub fn ionic_radius(symbol: &str, charge: Option<i8>) -> Option<f32> {
    let key = table_key(symbol)?;
    IONIC
        .iter()
        .find(|r| r.symbol == key && charge.is_none_or(|c| r.charge == c))
        .map(|r| r.radius)
}

/// The Pauling-scale electronegativity of `symbol`, or `None` for the elements
/// with no accepted value (He, Ne, Ar).
///
/// ```
/// use manim_chem::element::electronegativity;
/// assert!(electronegativity("F").unwrap() > electronegativity("Na").unwrap());
/// assert!(electronegativity("Ne").is_none());
/// ```
pub fn electronegativity(symbol: &str) -> Option<f32> {
    let key = table_key(symbol)?;
    ELECTRONEGATIVITY
        .iter()
        .find(|(s, _)| *s == key)
        .map(|(_, v)| *v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_check_atomic_numbers() {
        assert_eq!(data("H").unwrap().z, 1);
        assert_eq!(data("C").unwrap().z, 6);
        assert_eq!(data("O").unwrap().z, 8);
        assert_eq!(data("Fe").unwrap().z, 26);
        assert_eq!(data("Xe").unwrap().z, 54);
    }

    #[test]
    fn hydrogen_is_whiteish() {
        let h = data("H").unwrap();
        assert!(h.cpk_color.r > 0.9 && h.cpk_color.g > 0.9 && h.cpk_color.b > 0.9);
    }

    #[test]
    fn oxygen_is_red() {
        let o = data("O").unwrap();
        assert!(o.cpk_color.r > 0.5);
        assert!(o.cpk_color.g < 0.1 && o.cpk_color.b < 0.1);
    }

    #[test]
    fn nitrogen_is_blue() {
        let n = data("N").unwrap();
        assert!(n.cpk_color.b > n.cpk_color.r);
        assert!(n.cpk_color.b > n.cpk_color.g);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(data("fe"), data("Fe"));
        assert_eq!(data("FE"), data("Fe"));
        assert_eq!(data("nA"), data("Na"));
    }

    #[test]
    fn unknown_is_none() {
        assert!(data("xx").is_none());
        assert!(data("").is_none());
        assert!(data("Zz").is_none());
    }

    #[test]
    fn radii_are_positive() {
        for r in TABLE {
            assert!(r.covalent > 0.0, "{} covalent", r.symbol);
            assert!(r.vdw > 0.0, "{} vdw", r.symbol);
        }
    }

    #[test]
    fn ionic_radii_are_positive_and_in_the_element_table() {
        for r in IONIC {
            assert!(r.radius > 0.0, "{} ionic", r.symbol);
            assert!(
                data(r.symbol).is_some(),
                "{} not in element table",
                r.symbol
            );
        }
    }

    /// The whole point of the ionic source: anions are big, cations are small,
    /// which is the opposite of what the *covalent* radii say for Na/Cl.
    #[test]
    fn anions_outsize_cations_unlike_covalent_radii() {
        let na = ionic_radius("Na", None).unwrap();
        let cl = ionic_radius("Cl", None).unwrap();
        assert!(cl > na, "Cl- {cl} should exceed Na+ {na}");
        // Covalent radii have it the other way round — this is the bug FE-142a fixes.
        let na_cov = data("Na").unwrap().covalent_radius;
        let cl_cov = data("Cl").unwrap().covalent_radius;
        assert!(na_cov > cl_cov);
    }

    #[test]
    fn explicit_charge_overrides_the_common_one() {
        assert_eq!(common_charge("Fe"), Some(3));
        assert_eq!(ionic_radius("Fe", None), ionic_radius("Fe", Some(3)));
        assert_ne!(ionic_radius("Fe", Some(2)), ionic_radius("Fe", Some(3)));
        assert!(ionic_radius("Fe", Some(9)).is_none());
    }

    #[test]
    fn ionic_lookup_is_case_insensitive_like_data() {
        assert_eq!(ionic_radius("cl", None), ionic_radius("Cl", None));
        assert_eq!(ionic_radius("NA", None), ionic_radius("Na", None));
        assert_eq!(electronegativity("fe"), electronegativity("Fe"));
    }

    #[test]
    fn elements_without_common_ions_are_absent() {
        for noble in ["He", "Ne", "Ar", "Kr", "Xe"] {
            assert!(ionic_radius(noble, None).is_none(), "{noble}");
            assert!(common_charge(noble).is_none(), "{noble}");
        }
        assert!(ionic_radius("H", None).is_none());
    }

    #[test]
    fn electronegativity_is_in_pauling_range() {
        for (s, v) in ELECTRONEGATIVITY {
            assert!((0.7..=4.0).contains(v), "{s} = {v}");
            assert!(data(s).is_some(), "{s} not in element table");
        }
        // The canonical extremes among the elements we carry.
        assert_eq!(electronegativity("F"), Some(3.98));
        assert_eq!(electronegativity("Cs"), None); // beyond Xe
    }
}
