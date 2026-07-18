//! `manim-quantum`: quantum-mechanics visualizers built on [`manim_fields`]
//! (complex fields, the split-step Schrödinger stepper), [`manim_sci`]
//! (isosurfaces), and [`manim_core`] mobjects.
//!
//! - [`wavefunction`] — [`Wavefunction1D`](wavefunction::Wavefunction1D) styles
//!   (probability density, re/im pair, phase-hue) and
//!   [`Wavefunction2D`](wavefunction::Wavefunction2D) (a phase-hue texture).
//! - [`eigenstates`] — analytic eigenstates: particle-in-a-box, the harmonic
//!   oscillator (Hermite), and hydrogen (associated Laguerre × real spherical
//!   harmonics), plus [`hydrogen_orbital`](eigenstates::hydrogen_orbital) as a
//!   field and [`orbital_isosurface`](eigenstates::orbital_isosurface).
//! - [`superposition`] — time-evolving superpositions and coherent states.
//! - [`bloch`] — the [`BlochSphere`](bloch::BlochSphere) with animated gates.
//! - [`wells`] — potential-well diagrams and the tunneling scene.

pub mod bloch;
pub mod eigenstates;
pub mod superposition;
pub mod wavefunction;
pub mod wells;
