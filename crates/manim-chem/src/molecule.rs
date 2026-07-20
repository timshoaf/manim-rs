//! The molecular model: [`Atom`]s and [`Bond`]s making a [`Molecule`].

use glam::Vec3;

/// A single atom: its element symbol, position (ångström), and optional formal
/// charge.
#[derive(Clone, Debug, PartialEq)]
pub struct Atom {
    /// Element symbol, e.g. `"C"`, `"O"`, `"Na"` (case as written).
    pub element: String,
    /// Position in ångström.
    pub pos: Vec3,
    /// Formal charge (oxidation state), when known.
    ///
    /// Only consulted when sizing atoms by
    /// [`RadiusSource::Ionic`](crate::render::RadiusSource::Ionic); `None` falls
    /// back to the element's
    /// [`common_charge`](crate::element::common_charge). Parsers leave this
    /// `None` — set it with [`with_charge`](Self::with_charge) when a structure
    /// has an oxidation state the common one gets wrong.
    pub charge: Option<i8>,
}

impl Atom {
    /// An atom of `element` at `pos`, with no explicit charge.
    ///
    /// ```
    /// use manim_chem::molecule::Atom;
    /// use glam::Vec3;
    /// let a = Atom::new("C", Vec3::ZERO);
    /// assert_eq!(a.element, "C");
    /// assert_eq!(a.charge, None);
    /// ```
    pub fn new(element: impl Into<String>, pos: Vec3) -> Self {
        Self {
            element: element.into(),
            pos,
            charge: None,
        }
    }

    /// Sets this atom's formal charge (builder), overriding the element's
    /// common oxidation state when sizing by
    /// [`RadiusSource::Ionic`](crate::render::RadiusSource::Ionic).
    ///
    /// ```
    /// use manim_chem::molecule::Atom;
    /// use glam::Vec3;
    /// // Iron is Fe(III) by default; say so explicitly when it is Fe(II).
    /// let fe = Atom::new("Fe", Vec3::ZERO).with_charge(2);
    /// assert_eq!(fe.charge, Some(2));
    /// ```
    pub fn with_charge(mut self, charge: i8) -> Self {
        self.charge = Some(charge);
        self
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
