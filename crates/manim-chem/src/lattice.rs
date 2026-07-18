//! Crystal lattices: a unit cell plus an atom basis, replicated into a
//! [`Molecule`], with rock-salt / diamond / graphene presets.
//!
//! A [`Lattice`] stores the six cell parameters (`a`, `b`, `c` in ångström and
//! the angles `alpha`, `beta`, `gamma` in **degrees**) and a **cartesian** atom
//! basis: the [`Atom`] positions are cartesian coordinates *inside the unit
//! cell* (ångström), not fractional coordinates. [`cell_vectors`](Lattice::cell_vectors)
//! builds the three lattice vectors from the cell parameters via the standard
//! crystallographic convention (**a** along *x*, **b** in the *xy* plane), and
//! [`replicate`](Lattice::replicate) tiles the basis over an `n × m × k` block.
//!
//! ```
//! use manim_chem::lattice::nacl;
//! let cell = nacl();
//! let crystal = cell.replicate(2, 2, 2);
//! assert_eq!(crystal.atoms.len(), cell.basis.len() * 8);
//! ```

use glam::Vec3;

use crate::molecule::{Atom, Molecule};

/// A crystal unit cell: cell parameters plus a cartesian-in-cell atom basis.
#[derive(Debug, Clone, PartialEq)]
pub struct Lattice {
    /// Cell edge length **a** (ångström).
    pub a: f32,
    /// Cell edge length **b** (ångström).
    pub b: f32,
    /// Cell edge length **c** (ångström).
    pub c: f32,
    /// Angle between **b** and **c** (degrees).
    pub alpha: f32,
    /// Angle between **a** and **c** (degrees).
    pub beta: f32,
    /// Angle between **a** and **b** (degrees).
    pub gamma: f32,
    /// The atom basis, as cartesian positions inside the unit cell (ångström).
    pub basis: Vec<Atom>,
}

impl Lattice {
    /// The three lattice vectors `[a_vec, b_vec, c_vec]` in cartesian ångström.
    ///
    /// Uses the standard crystallographic setting: **a** points along *x*,
    /// **b** lies in the *xy* plane, and **c** completes a right-handed frame.
    ///
    /// ```
    /// use manim_chem::lattice::Lattice;
    /// use glam::Vec3;
    /// // A simple cubic cell of side 2 gives the cartesian axes scaled by 2.
    /// let cell = Lattice { a: 2.0, b: 2.0, c: 2.0, alpha: 90.0, beta: 90.0, gamma: 90.0, basis: vec![] };
    /// let [av, bv, cv] = cell.cell_vectors();
    /// assert!((av - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-5);
    /// assert!((bv - Vec3::new(0.0, 2.0, 0.0)).length() < 1e-5);
    /// assert!((cv - Vec3::new(0.0, 0.0, 2.0)).length() < 1e-5);
    /// ```
    pub fn cell_vectors(&self) -> [Vec3; 3] {
        let (alpha, beta, gamma) = (
            self.alpha.to_radians(),
            self.beta.to_radians(),
            self.gamma.to_radians(),
        );
        let (ca, cb, cg) = (alpha.cos(), beta.cos(), gamma.cos());
        let sg = gamma.sin();

        let a_vec = Vec3::new(self.a, 0.0, 0.0);
        let b_vec = Vec3::new(self.b * cg, self.b * sg, 0.0);

        let cx = cb;
        let cy = (ca - cb * cg) / sg;
        // Clamp against tiny negative round-off before the square root.
        let cz2 = 1.0 - cx * cx - cy * cy;
        let cz = cz2.max(0.0).sqrt();
        let c_vec = Vec3::new(self.c * cx, self.c * cy, self.c * cz);

        [a_vec, b_vec, c_vec]
    }

    /// Tiles the basis over an `n × m × k` block of cells, returning a
    /// [`Molecule`] (no bonds — connectivity is perceived elsewhere).
    ///
    /// Atom `p` in cell `(i, j, l)` is placed at
    /// `p + i·a_vec + j·b_vec + l·c_vec`.
    ///
    /// ```
    /// use manim_chem::lattice::diamond;
    /// let mol = diamond().replicate(1, 1, 1);
    /// assert_eq!(mol.atoms.len(), 8); // conventional diamond cell
    /// ```
    pub fn replicate(&self, n: usize, m: usize, k: usize) -> Molecule {
        let [av, bv, cv] = self.cell_vectors();
        let mut atoms = Vec::with_capacity(self.basis.len() * n * m * k);
        for i in 0..n {
            for j in 0..m {
                for l in 0..k {
                    let shift = i as f32 * av + j as f32 * bv + l as f32 * cv;
                    for atom in &self.basis {
                        atoms.push(Atom::new(atom.element.clone(), atom.pos + shift));
                    }
                }
            }
        }
        Molecule {
            atoms,
            bonds: Vec::new(),
        }
    }

    /// The 12 edges of the unit-cell parallelepiped, as `(start, end)` pairs,
    /// with one corner at the origin.
    ///
    /// ```
    /// use manim_chem::lattice::nacl;
    /// assert_eq!(nacl().cell_edges().len(), 12);
    /// ```
    pub fn cell_edges(&self) -> Vec<(Vec3, Vec3)> {
        let [av, bv, cv] = self.cell_vectors();
        // Corner c(i,j,k) = i*av + j*bv + k*cv, with i,j,k in {0,1}.
        let corner = |i: u8, j: u8, l: u8| i as f32 * av + j as f32 * bv + l as f32 * cv;
        let mut edges = Vec::with_capacity(12);
        // Edges connect corners differing in exactly one axis.
        for j in 0..2u8 {
            for l in 0..2u8 {
                edges.push((corner(0, j, l), corner(1, j, l))); // along a
            }
        }
        for i in 0..2u8 {
            for l in 0..2u8 {
                edges.push((corner(i, 0, l), corner(i, 1, l))); // along b
            }
        }
        for i in 0..2u8 {
            for j in 0..2u8 {
                edges.push((corner(i, j, 0), corner(i, j, 1))); // along c
            }
        }
        edges
    }
}

/// Sodium chloride (rock salt), a cubic lattice with `a ≈ 5.64 Å`.
///
/// The conventional cell holds four Na and four Cl on interpenetrating FCC
/// sub-lattices, so the basis has 8 atoms and the nearest Na–Cl separation is
/// `a/2 ≈ 2.82 Å`.
///
/// ```
/// use manim_chem::lattice::nacl;
/// assert_eq!(nacl().basis.len(), 8);
/// ```
pub fn nacl() -> Lattice {
    let a = 5.64_f32;
    let fcc = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.5, 0.5, 0.0),
        Vec3::new(0.5, 0.0, 0.5),
        Vec3::new(0.0, 0.5, 0.5),
    ];
    let mut basis = Vec::with_capacity(8);
    for f in fcc {
        basis.push(Atom::new("Na", f * a));
    }
    // Cl shifted by half a cell edge along x.
    for f in fcc {
        basis.push(Atom::new("Cl", (f + Vec3::new(0.5, 0.0, 0.0)) * a));
    }
    Lattice {
        a,
        b: a,
        c: a,
        alpha: 90.0,
        beta: 90.0,
        gamma: 90.0,
        basis,
    }
}

/// Diamond cubic carbon, `a ≈ 3.567 Å`.
///
/// The conventional cell is an FCC lattice of carbon with a two-atom motif,
/// giving 8 atoms and a nearest C–C distance of `a·√3/4 ≈ 1.54 Å`.
///
/// ```
/// use manim_chem::lattice::diamond;
/// assert_eq!(diamond().basis.len(), 8);
/// ```
pub fn diamond() -> Lattice {
    let a = 3.567_f32;
    let fcc = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.5, 0.5, 0.0),
        Vec3::new(0.5, 0.0, 0.5),
        Vec3::new(0.0, 0.5, 0.5),
    ];
    let mut basis = Vec::with_capacity(8);
    for f in fcc {
        basis.push(Atom::new("C", f * a));
    }
    // Second sub-lattice offset by (1/4, 1/4, 1/4).
    let quarter = Vec3::splat(0.25);
    for f in fcc {
        basis.push(Atom::new("C", (f + quarter) * a));
    }
    Lattice {
        a,
        b: a,
        c: a,
        alpha: 90.0,
        beta: 90.0,
        gamma: 90.0,
        basis,
    }
}

/// A single graphene sheet: a 2-D hexagonal cell with `a = b ≈ 2.46 Å`,
/// `gamma = 120°`, and a two-carbon basis.
///
/// The out-of-plane `c` axis is given a large spacing (vacuum) so replicating
/// in the *xy* plane tiles a flat sheet. The nearest C–C distance is
/// `a/√3 ≈ 1.42 Å`.
///
/// ```
/// use manim_chem::lattice::graphene;
/// let g = graphene();
/// assert_eq!(g.basis.len(), 2);
/// assert!((g.gamma - 120.0).abs() < 1e-6);
/// ```
pub fn graphene() -> Lattice {
    let a = 2.46_f32;
    let c = 10.0_f32; // vacuum spacing between sheets
    let mut cell = Lattice {
        a,
        b: a,
        c,
        alpha: 90.0,
        beta: 90.0,
        gamma: 120.0,
        basis: Vec::new(),
    };
    // Basis in fractional coords (0,0) and (1/3, 2/3); convert to cartesian.
    let [av, bv, _cv] = cell.cell_vectors();
    cell.basis = vec![
        Atom::new("C", Vec3::ZERO),
        Atom::new("C", (1.0 / 3.0) * av + (2.0 / 3.0) * bv),
    ];
    cell
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    /// The minimum distance between distinct atoms in a molecule.
    fn min_pair_distance(mol: &Molecule) -> f32 {
        let mut best = f32::INFINITY;
        for i in 0..mol.atoms.len() {
            for j in (i + 1)..mol.atoms.len() {
                let d = (mol.atoms[i].pos - mol.atoms[j].pos).length();
                best = best.min(d);
            }
        }
        best
    }

    #[test]
    fn nacl_replicate_count() {
        let cell = nacl();
        let crystal = cell.replicate(2, 2, 2);
        assert_eq!(crystal.atoms.len(), cell.basis.len() * 8);
        assert!(crystal.bonds.is_empty());
    }

    #[test]
    fn nacl_nearest_neighbour() {
        // Nearest Na-Cl spacing is a/2 ~ 2.82 A.
        let crystal = nacl().replicate(2, 2, 2);
        assert_relative_eq!(min_pair_distance(&crystal), 5.64 / 2.0, epsilon = 1e-3);
    }

    #[test]
    fn diamond_bond_length() {
        // Nearest C-C is a*sqrt(3)/4 ~ 1.544 A.
        let crystal = diamond().replicate(2, 2, 2);
        let expected = 3.567 * 3.0_f32.sqrt() / 4.0;
        assert_relative_eq!(min_pair_distance(&crystal), expected, epsilon = 1e-3);
    }

    #[test]
    fn graphene_cc_distance() {
        // Nearest C-C in graphene ~ 1.42 A.
        let sheet = graphene().replicate(3, 3, 1);
        assert_relative_eq!(min_pair_distance(&sheet), 1.42, epsilon = 1e-2);
    }

    #[test]
    fn cell_edges_count() {
        assert_eq!(nacl().cell_edges().len(), 12);
        assert_eq!(graphene().cell_edges().len(), 12);
    }

    #[test]
    fn cubic_cell_vectors_are_axes() {
        let [av, bv, cv] = nacl().cell_vectors();
        assert_relative_eq!(av.x, 5.64, epsilon = 1e-4);
        assert!(av.y.abs() < 1e-4 && av.z.abs() < 1e-4);
        assert!(bv.x.abs() < 1e-4);
        assert_relative_eq!(bv.y, 5.64, epsilon = 1e-4);
        assert_relative_eq!(cv.z, 5.64, epsilon = 1e-4);
    }
}
