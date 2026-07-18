//! Time-evolving superpositions of harmonic-oscillator eigenstates.
//!
//! A [`Superposition`] is a fixed linear combination `Σ cₙ|n⟩` over the
//! oscillator basis (`ħ = m = ω = 1`). Each stationary state carries the phase
//! `e^{-iEₙt}` with `Eₙ = n + 1/2`, so the packet evolves as
//! `ψ(x, t) = Σ cₙ ψₙ(x) e^{-iEₙt}` ([`amplitude_at`](Superposition::amplitude_at)).
//!
//! The headline construction is the [`coherent_state`](Superposition::coherent_state):
//! its density's center follows the *classical* trajectory
//! `⟨x⟩(t) = √2·Re(α e^{-it})` with no spreading.
//!
//! ```
//! use manim_fields::complex::Complex;
//! use manim_quantum::superposition::Superposition;
//! // A coherent state's initial center is √2·Re(α).
//! let psi = Superposition::coherent_state(Complex::new(1.0, 0.0));
//! let x0 = psi.expectation_x(0.0);
//! assert!((x0 - 2.0_f64.sqrt()).abs() < 1e-2);
//! ```

use manim_fields::complex::Complex;

use crate::eigenstates::{harmonic_energy, harmonic_oscillator};

/// A superposition `Σ cₙ|n⟩` of harmonic-oscillator eigenstates.
///
/// Terms are `(amplitude cₙ, level n)` pairs. Nothing is normalized for you;
/// [`coherent_state`](Self::coherent_state) produces normalized amplitudes.
pub struct Superposition {
    terms: Vec<(Complex, usize)>,
}

impl Superposition {
    /// Builds a superposition from explicit `(cₙ, n)` terms.
    ///
    /// ```
    /// use manim_fields::complex::Complex;
    /// use manim_quantum::superposition::Superposition;
    /// // An equal mix of the ground and first excited states.
    /// let s = 1.0 / 2.0_f64.sqrt();
    /// let psi = Superposition::new(vec![
    ///     (Complex::real(s), 0),
    ///     (Complex::real(s), 1),
    /// ]);
    /// assert!((psi.probability_density(0.0, 0.0) >= 0.0));
    /// ```
    pub fn new(terms: Vec<(Complex, usize)>) -> Self {
        Self { terms }
    }

    /// The amplitudes, as `(cₙ, n)` pairs.
    pub fn terms(&self) -> &[(Complex, usize)] {
        &self.terms
    }

    /// The complex amplitude `ψ(x, t) = Σ cₙ ψₙ(x) e^{-iEₙt}`.
    ///
    /// ```
    /// use manim_fields::complex::Complex;
    /// use manim_quantum::superposition::Superposition;
    /// let psi = Superposition::new(vec![(Complex::one(), 0)]);
    /// // A single stationary state only picks up a phase, keeping |ψ| fixed.
    /// let a0 = psi.amplitude_at(0.3, 0.0).norm();
    /// let a1 = psi.amplitude_at(0.3, 1.7).norm();
    /// assert!((a0 - a1).abs() < 1e-12);
    /// ```
    pub fn amplitude_at(&self, x: f64, t: f64) -> Complex {
        let mut acc = Complex::zero();
        for &(c, n) in &self.terms {
            let psi = harmonic_oscillator(n, x);
            let phase = Complex::from_polar(1.0, -harmonic_energy(n) * t);
            acc = acc + c.scale(psi) * phase;
        }
        acc
    }

    /// The probability density `|ψ(x, t)|²`.
    pub fn probability_density(&self, x: f64, t: f64) -> f64 {
        self.amplitude_at(x, t).norm_sqr()
    }

    /// A **coherent state** of the oscillator: `cₙ = e^{-|α|²/2}·αⁿ/√(n!)`,
    /// truncated at enough terms that the tail is negligible for the given
    /// `|α|`.
    ///
    /// ```
    /// use manim_fields::complex::Complex;
    /// use manim_quantum::superposition::Superposition;
    /// let psi = Superposition::coherent_state(Complex::new(0.8, 0.4));
    /// // The amplitudes are (very nearly) normalized: Σ|cₙ|² ≈ 1.
    /// let total: f64 = psi.terms().iter().map(|(c, _)| c.norm_sqr()).sum();
    /// assert!((total - 1.0).abs() < 1e-6);
    /// ```
    pub fn coherent_state(alpha: Complex) -> Self {
        // Enough terms for the Poissonian weight to die off past ⟨n⟩ = |α|².
        let nmax = (alpha.norm_sqr().ceil() as usize) * 4 + 24;
        let prefactor = (-0.5 * alpha.norm_sqr()).exp();
        let mut terms = Vec::with_capacity(nmax);
        let mut alpha_pow = Complex::one(); // αⁿ
        let mut sqrt_fact = 1.0; // √(n!)
        for n in 0..nmax {
            if n > 0 {
                alpha_pow = alpha_pow * alpha;
                sqrt_fact *= (n as f64).sqrt();
            }
            let c = alpha_pow.scale(prefactor / sqrt_fact);
            terms.push((c, n));
        }
        Self { terms }
    }

    /// The density-weighted position expectation `⟨x⟩(t)` by grid quadrature
    /// over a window wide enough to contain the packet.
    ///
    /// ```
    /// use manim_fields::complex::Complex;
    /// use manim_quantum::superposition::Superposition;
    /// use std::f64::consts::PI;
    /// let psi = Superposition::coherent_state(Complex::new(1.0, 0.0));
    /// // Half a period later the center has swung to the opposite side.
    /// let x_half = psi.expectation_x(PI);
    /// assert!((x_half + 2.0_f64.sqrt()).abs() < 1e-2);
    /// ```
    pub fn expectation_x(&self, t: f64) -> f64 {
        let (num, den) = self.moments(t);
        num[1] / den
    }

    /// The spatial variance `⟨x²⟩ − ⟨x⟩²` of the packet at time `t` (a measure
    /// of its width).
    pub fn variance_x(&self, t: f64) -> f64 {
        let (num, den) = self.moments(t);
        let mean = num[1] / den;
        num[2] / den - mean * mean
    }

    /// Grid quadrature of `∫|ψ|² dx`, `∫x|ψ|² dx`, `∫x²|ψ|² dx` over a fixed
    /// window. Returns `([m0, m1, m2], m0)` (the zeroth moment repeated as the
    /// normalization denominator).
    fn moments(&self, t: f64) -> ([f64; 3], f64) {
        let (a, b, n) = (-12.0_f64, 12.0_f64, 4000usize);
        let h = (b - a) / n as f64;
        let mut m = [0.0; 3];
        for i in 0..=n {
            let x = a + i as f64 * h;
            let w = if i == 0 || i == n { 0.5 } else { 1.0 };
            let rho = self.probability_density(x, t) * w;
            m[0] += rho;
            m[1] += rho * x;
            m[2] += rho * x * x;
        }
        for mk in &mut m {
            *mk *= h;
        }
        (m, m[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn coherent_state_follows_classical_trajectory() {
        // α with both real and imaginary parts exercises Re(α e^{-it}) fully.
        let alpha = Complex::new(1.0, 0.8);
        let psi = Superposition::coherent_state(alpha);

        let period = 2.0 * PI;
        let mut max_err: f64 = 0.0;
        for k in 0..=8 {
            let t = k as f64 * period / 8.0;
            // Classical oscillator (ħ=m=ω=1): x(t) = √2·Re(α e^{-it}).
            let classical = 2.0_f64.sqrt() * (alpha * Complex::from_polar(1.0, -t)).re;
            let measured = psi.expectation_x(t);
            println!("t={t:.4}  ⟨x⟩={measured:+.5}  classical={classical:+.5}");
            max_err = max_err.max((measured - classical).abs());
        }
        assert!(
            max_err < 1e-2,
            "⟨x⟩(t) deviates from classical by {max_err}"
        );
    }

    #[test]
    fn coherent_state_does_not_disperse() {
        // A coherent state is a minimum-uncertainty packet with ⟨(Δx)²⟩ = 1/2
        // for all t (no spreading).
        let psi = Superposition::coherent_state(Complex::new(1.0, 0.8));
        let period = 2.0 * PI;
        for k in 0..=8 {
            let t = k as f64 * period / 8.0;
            let var = psi.variance_x(t);
            println!("t={t:.4}  var={var:.5}");
            assert!((var - 0.5).abs() < 2e-2, "packet width drifted: var={var}");
        }
    }
}
