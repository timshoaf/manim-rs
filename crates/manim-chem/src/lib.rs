//! `manim-chem`: chemistry visualizers for `manim_rust`.
//!
//! - [`molecule`] — the [`Molecule`] / [`Atom`] / [`Bond`] model.
//! - [`parsers`] — tiny dependency-free XYZ and SDF (V2000) parsers.
//! - [`element`] — the CPK element data table (Z, color, covalent / van-der-Waals
//!   radii).
//! - [`render`] — ball-and-stick / space-filling / wireframe builders (GPU
//!   instancing) and orbital isosurfaces.
//! - [`lattice`] — crystal lattices (unit cell + replication) with presets.
//! - [`cube`] — the Gaussian `.cube` volumetric parser → a scalar field.

pub mod cube;
pub mod element;
pub mod lattice;
pub mod molecule;
pub mod parsers;
pub mod render;

pub use molecule::{Atom, Bond, Molecule};
