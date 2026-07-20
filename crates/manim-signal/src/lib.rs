//! `manim-signal`: a signal-processing visualization kit built on
//! [`manim_fields`] (numerics) and [`manim_core`] (mobjects).
//!
//! - [`fourier`] — complex Fourier coefficients of a closed path or closure via
//!   FFT, and the [`EpicycleChain`](fourier::EpicycleChain) builder that turns
//!   them into a rotating circle chain with a traced reconstruction.
//! - [`sampling`] — continuous curve vs. sample stems vs. sinc / zero-order-hold
//!   reconstruction on [`Axes`](manim_core::graphing::Axes), plus the exact
//!   [`alias_frequency`](sampling::alias_frequency) helper.
//! - [`convolution`] — direct discrete convolution and a sliding-kernel buildup
//!   visual whose product areas accumulate into the output.
//! - [`filters`] — windowed-sinc [`Fir`](filters::Fir) design and RBJ
//!   [`Biquad`](filters::Biquad) IIR sections, with magnitude/phase response
//!   curves on linear or log-frequency axes.
//! - [`spectrogram`] — short-time Fourier transform rendered as a
//!   [`MaterialQuad`](manim_sci::material_quad::MaterialQuad) heatmap.
//!
//! Everything numeric is `f64`; mobject geometry is `f32`, as elsewhere in the
//! kit crates. The library never links the GPU renderer — only the examples do,
//! behind the `render-examples` feature.
//!
//! ```
//! use manim_signal::sampling::alias_frequency;
//! // A 90 Hz tone sampled at 100 Hz folds down to 10 Hz, exactly.
//! assert_eq!(alias_frequency(90.0, 100.0), 10.0);
//! ```

pub mod convolution;
pub mod filters;
pub mod fourier;
pub mod sampling;
pub mod spectrogram;

/// The window functions shared by FIR design and the STFT.
///
/// All are defined on `n = 0..len-1` and symmetric about the midpoint.
///
/// ```
/// use manim_signal::Window;
/// assert_eq!(Window::Rect.weight(0, 9), 1.0);
/// // The Hann window vanishes at both endpoints and peaks at the centre.
/// assert!(Window::Hann.weight(0, 9).abs() < 1e-12);
/// assert!((Window::Hann.weight(4, 9) - 1.0).abs() < 1e-12);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// No taper (a boxcar): widest transition band, worst sidelobes.
    Rect,
    /// Hann (raised cosine): zero endpoints, ≈ −31 dB first sidelobe.
    Hann,
    /// Hamming: non-zero endpoints, ≈ −41 dB first sidelobe.
    Hamming,
    /// Blackman: ≈ −57 dB sidelobes at the cost of a wider main lobe.
    Blackman,
}

impl Window {
    /// The window weight at sample `n` of a length-`len` window.
    ///
    /// Out-of-range `n` and `len < 2` both return `1.0` (an untapered sample).
    ///
    /// ```
    /// use manim_signal::Window;
    /// assert_eq!(Window::Blackman.weight(0, 1), 1.0);
    /// ```
    pub fn weight(self, n: usize, len: usize) -> f64 {
        if len < 2 || n >= len {
            return 1.0;
        }
        let x = n as f64 / (len - 1) as f64;
        let tau = std::f64::consts::TAU;
        match self {
            Window::Rect => 1.0,
            Window::Hann => 0.5 - 0.5 * (tau * x).cos(),
            Window::Hamming => 0.54 - 0.46 * (tau * x).cos(),
            Window::Blackman => 0.42 - 0.5 * (tau * x).cos() + 0.08 * (2.0 * tau * x).cos(),
        }
    }

    /// The whole length-`len` window as a vector.
    ///
    /// ```
    /// use manim_signal::Window;
    /// let w = Window::Hann.samples(8);
    /// assert_eq!(w.len(), 8);
    /// assert!(w[0].abs() < 1e-12 && w[7].abs() < 1e-12);
    /// ```
    pub fn samples(self, len: usize) -> Vec<f64> {
        (0..len).map(|n| self.weight(n, len)).collect()
    }
}

/// Normalized sinc, `sin(πx) / (πx)`, with the removable singularity filled in.
///
/// ```
/// use manim_signal::sinc;
/// assert_eq!(sinc(0.0), 1.0);
/// assert!(sinc(1.0).abs() < 1e-12);
/// ```
pub fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}
