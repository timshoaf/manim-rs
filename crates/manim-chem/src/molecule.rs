//! The molecular model: [`Atom`]s and [`Bond`]s making a [`Molecule`].

use glam::Vec3;

/// A single atom: its element symbol and position (ångström).
#[derive(Clone, Debug, PartialEq)]
pub struct Atom {
    /// Element symbol, e.g. `"C"`, `"O"`, `"Na"` (case as written).
    pub element: String,
    /// Position in ångström.
    pub pos: Vec3,
}

impl Atom {
    /// An atom of `element` at `pos`.
    ///
    /// ```
    /// use manim_chem::molecule::Atom;
    /// use glam::Vec3;
    /// let a = Atom::new("C", Vec3::ZERO);
    /// assert_eq!(a.element, "C");
    /// ```
    pub fn new(element: impl Into<String>, pos: Vec3) -> Self {
        Self {
            element: element.into(),
            pos,
        }
    }
}

/// A bond between two atoms (by index) with an integer bond order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bond {
    /// Index of the first atom.
    pub a: usize,
    /// Index of the second atom.
    pub b: usize,
    /// Bond order (1 = single, 2 = double, 3 = triple).
    pub order: u8,
}

impl Bond {
    /// A bond of `order` between atoms `a` and `b`.
    pub fn new(a: usize, b: usize, order: u8) -> Self {
        Self { a, b, order }
    }
}

/// A molecule: a list of atoms and the bonds between them.
///
/// ```
/// use manim_chem::molecule::{Atom, Bond, Molecule};
/// use glam::Vec3;
/// let mol = Molecule {
///     atoms: vec![Atom::new("O", Vec3::ZERO), Atom::new("H", Vec3::X)],
///     bonds: vec![Bond::new(0, 1, 1)],
/// };
/// assert_eq!(mol.atoms.len(), 2);
/// assert_eq!(mol.bonds.len(), 1);
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Molecule {
    /// The atoms, in file order (bond indices reference this order).
    pub atoms: Vec<Atom>,
    /// The bonds between atoms.
    pub bonds: Vec<Bond>,
}

impl Molecule {
    /// An empty molecule.
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of atoms.
    pub fn atom_count(&self) -> usize {
        self.atoms.len()
    }

    /// The number of bonds.
    pub fn bond_count(&self) -> usize {
        self.bonds.len()
    }

    /// The centroid (mean atom position); the origin if there are no atoms.
    ///
    /// ```
    /// use manim_chem::molecule::{Atom, Molecule};
    /// use glam::Vec3;
    /// let mol = Molecule { atoms: vec![Atom::new("H", Vec3::ZERO), Atom::new("H", 2.0 * Vec3::X)], bonds: vec![] };
    /// assert_eq!(mol.centroid(), Vec3::new(1.0, 0.0, 0.0));
    /// ```
    pub fn centroid(&self) -> Vec3 {
        if self.atoms.is_empty() {
            return Vec3::ZERO;
        }
        let sum: Vec3 = self.atoms.iter().map(|a| a.pos).sum();
        sum / self.atoms.len() as f32
    }
}
