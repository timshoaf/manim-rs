//! Digital filter design and frequency-response curves.
//!
//! Two families, both parameterised by *normalized* frequency `f` in cycles per
//! sample (so `f = 0` is DC and `f = 0.5` is Nyquist):
//!
//! - [`Fir`] — a windowed-sinc finite impulse response. Exactly linear phase
//!   (symmetric taps), no feedback, no stability question; the price is taps.
//! - [`Biquad`] — one RBJ-cookbook second-order IIR section. Two poles buy a
//!   steeper skirt than any short FIR, at the cost of nonlinear phase.
//!
//! Both implement [`FrequencyResponse`], which is what the plotting helpers
//! ([`magnitude_graph`], [`phase_graph`]) consume, on linear or log-frequency
//! axes.

use manim_core::graphing::{Axes, FunctionGraph};
use manim_fields::Complex;

use crate::{sinc, Window};

/// Anything with a discrete-time frequency response `H(f)`.
///
/// `f` is in cycles per sample: the response is periodic with period 1 and
/// conjugate-symmetric, so only `[0, 0.5]` is ever interesting.
pub trait FrequencyResponse {
    /// The complex response at normalized frequency `f`.
    ///
    /// ```
    /// use manim_signal::filters::{Biquad, FrequencyResponse};
    /// let h = Biquad::lowpass(0.1, 0.707);
    /// assert!((h.response(0.0).norm() - 1.0).abs() < 1e-12);
    /// ```
    fn response(&self, f: f64) -> Complex;

    /// The magnitude `|H(f)|`.
    ///
    /// ```
    /// use manim_signal::filters::{Biquad, FrequencyResponse};
    /// assert!((Biquad::lowpass(0.1, 0.707).magnitude(0.0) - 1.0).abs() < 1e-12);
    /// ```
    fn magnitude(&self, f: f64) -> f64 {
        self.response(f).norm()
    }

    /// The magnitude in decibels, floored at `-120 dB`.
    ///
    /// ```
    /// use manim_signal::filters::{Biquad, FrequencyResponse};
    /// assert!(Biquad::lowpass(0.1, 0.707).magnitude_db(0.0).abs() < 1e-9);
    /// ```
    fn magnitude_db(&self, f: f64) -> f64 {
        (20.0 * self.magnitude(f).log10()).max(-120.0)
    }

    /// The phase `arg H(f)` in radians, in `(−π, π]`.
    ///
    /// ```
    /// use manim_signal::filters::{Biquad, FrequencyResponse};
    /// assert!(Biquad::lowpass(0.1, 0.707).phase(0.0).abs() < 1e-12);
    /// ```
    fn phase(&self, f: f64) -> f64 {
        self.response(f).arg()
    }
}

/// A finite impulse response filter: `y[n] = Σ h[k] x[n−k]`.
///
/// ```
/// use manim_signal::filters::{Fir, FrequencyResponse};
/// use manim_signal::Window;
/// let lp = Fir::lowpass(41, 0.1, Window::Hamming);
/// // Unity gain at DC by construction, and deep stopband attenuation.
/// assert!((lp.magnitude(0.0) - 1.0).abs() < 1e-12);
/// assert!(lp.magnitude(0.3) < 1e-3);
/// ```
#[derive(Clone, Debug, Default)]
pub struct Fir {
    taps: Vec<f64>,
}

impl Fir {
    /// A filter from explicit taps.
    ///
    /// ```
    /// use manim_signal::filters::Fir;
    /// assert_eq!(Fir::new(vec![0.5, 0.5]).taps(), &[0.5, 0.5]);
    /// ```
    pub fn new(taps: Vec<f64>) -> Self {
        Self { taps }
    }

    /// A windowed-sinc lowpass of `len` taps with cutoff `fc` (cycles/sample),
    /// normalized to unity DC gain.
    ///
    /// An odd `len` keeps the sinc centred on a tap, which is what makes the
    /// taps exactly symmetric (and the phase exactly linear).
    ///
    /// ```
    /// use manim_signal::filters::{Fir, FrequencyResponse};
    /// let lp = Fir::lowpass(31, 0.2, Window::Blackman);
    /// use manim_signal::Window;
    /// assert_eq!(lp.taps().len(), 31);
    /// assert!(lp.magnitude(0.45) < 1e-3);
    /// ```
    pub fn lowpass(len: usize, fc: f64, window: Window) -> Self {
        let len = len.max(1);
        let mid = (len - 1) as f64 / 2.0;
        let mut taps: Vec<f64> = (0..len)
            .map(|n| {
                let x = n as f64 - mid;
                2.0 * fc * sinc(2.0 * fc * x) * window.weight(n, len)
            })
            .collect();
        let sum: f64 = taps.iter().sum();
        if sum.abs() > 1e-15 {
            for t in &mut taps {
                *t /= sum;
            }
        }
        Self { taps }
    }

    /// A windowed-sinc highpass, by spectral inversion of the matching lowpass
    /// (`len` must be odd for the inversion to be exact).
    ///
    /// ```
    /// use manim_signal::filters::{Fir, FrequencyResponse};
    /// use manim_signal::Window;
    /// let hp = Fir::highpass(41, 0.2, Window::Hamming);
    /// assert!(hp.magnitude(0.0) < 1e-12);
    /// assert!((hp.magnitude(0.5) - 1.0).abs() < 1e-3);
    /// ```
    pub fn highpass(len: usize, fc: f64, window: Window) -> Self {
        let len = if len % 2 == 0 { len + 1 } else { len };
        let mut lp = Self::lowpass(len, fc, window);
        for t in &mut lp.taps {
            *t = -*t;
        }
        lp.taps[(len - 1) / 2] += 1.0;
        lp
    }

    /// A windowed-sinc bandpass: the difference of two lowpasses.
    ///
    /// ```
    /// use manim_signal::filters::{Fir, FrequencyResponse};
    /// use manim_signal::Window;
    /// let bp = Fir::bandpass(61, 0.1, 0.2, Window::Hamming);
    /// assert!(bp.magnitude(0.0) < 1e-3 && bp.magnitude(0.5) < 1e-3);
    /// assert!(bp.magnitude(0.15) > 0.9);
    /// ```
    pub fn bandpass(len: usize, f_low: f64, f_high: f64, window: Window) -> Self {
        let lo = Self::lowpass(len, f_low, window);
        let hi = Self::lowpass(len, f_high, window);
        Self {
            taps: hi.taps.iter().zip(&lo.taps).map(|(h, l)| h - l).collect(),
        }
    }

    /// The impulse response.
    ///
    /// ```
    /// use manim_signal::filters::Fir;
    /// assert_eq!(Fir::new(vec![1.0]).taps(), &[1.0]);
    /// ```
    pub fn taps(&self) -> &[f64] {
        &self.taps
    }

    /// Filters a signal (direct-form convolution, zero initial state).
    ///
    /// ```
    /// use manim_signal::filters::Fir;
    /// let ma = Fir::new(vec![0.5, 0.5]);
    /// assert_eq!(ma.process(&[1.0, 1.0, 1.0]), vec![0.5, 1.0, 1.0]);
    /// ```
    pub fn process(&self, x: &[f64]) -> Vec<f64> {
        x.iter()
            .enumerate()
            .map(|(n, _)| {
                self.taps
                    .iter()
                    .enumerate()
                    .filter_map(|(k, h)| n.checked_sub(k).map(|i| h * x[i]))
                    .sum()
            })
            .collect()
    }

    /// Whether the taps are symmetric (equivalently: whether the phase is
    /// exactly linear).
    ///
    /// ```
    /// use manim_signal::filters::Fir;
    /// use manim_signal::Window;
    /// assert!(Fir::lowpass(31, 0.2, Window::Hann).is_linear_phase());
    /// assert!(!Fir::new(vec![1.0, 0.5, 0.0]).is_linear_phase());
    /// ```
    pub fn is_linear_phase(&self) -> bool {
        let n = self.taps.len();
        (0..n / 2).all(|i| (self.taps[i] - self.taps[n - 1 - i]).abs() < 1e-12)
    }
}

impl FrequencyResponse for Fir {
    fn response(&self, f: f64) -> Complex {
        self.taps
            .iter()
            .enumerate()
            .fold(Complex::zero(), |acc, (n, &h)| {
                acc + Complex::from_polar(h, -std::f64::consts::TAU * f * n as f64)
            })
    }
}

/// A second-order IIR section in direct form,
/// `y[n] = b₀x[n] + b₁x[n−1] + b₂x[n−2] − a₁y[n−1] − a₂y[n−2]`,
/// with coefficients from the RBJ audio-EQ cookbook (already normalized by
/// `a₀`).
///
/// ```
/// use manim_signal::filters::{Biquad, FrequencyResponse};
/// let lp = Biquad::lowpass(0.1, 0.707);
/// // Analytic endpoints: unity at DC, a double zero at Nyquist.
/// assert!((lp.magnitude(0.0) - 1.0).abs() < 1e-12);
/// assert!(lp.magnitude(0.5) < 1e-15);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    /// Feed-forward coefficient on `x[n]`.
    pub b0: f64,
    /// Feed-forward coefficient on `x[n−1]`.
    pub b1: f64,
    /// Feed-forward coefficient on `x[n−2]`.
    pub b2: f64,
    /// Feedback coefficient on `y[n−1]`.
    pub a1: f64,
    /// Feedback coefficient on `y[n−2]`.
    pub a2: f64,
}

/// The shared RBJ intermediate quantities for centre frequency `f0`
/// (cycles/sample) and quality factor `q`.
fn rbj(f0: f64, q: f64) -> (f64, f64, f64) {
    let w0 = std::f64::consts::TAU * f0;
    let (sin_w0, cos_w0) = w0.sin_cos();
    let alpha = sin_w0 / (2.0 * q.max(1e-9));
    (cos_w0, alpha, 1.0 + alpha)
}

impl Biquad {
    /// A section from explicit (already `a₀`-normalized) coefficients.
    ///
    /// ```
    /// use manim_signal::filters::{Biquad, FrequencyResponse};
    /// // A pure delay: H(f) = e^{-i2πf}.
    /// let d = Biquad::new(0.0, 1.0, 0.0, 0.0, 0.0);
    /// assert!((d.magnitude(0.3) - 1.0).abs() < 1e-12);
    /// ```
    pub fn new(b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) -> Self {
        Self { b0, b1, b2, a1, a2 }
    }

    /// RBJ lowpass at `f0` with quality `q`.
    ///
    /// ```
    /// use manim_signal::filters::{Biquad, FrequencyResponse};
    /// // Butterworth Q: −3 dB at the corner.
    /// let lp = Biquad::lowpass(0.1, std::f64::consts::FRAC_1_SQRT_2);
    /// assert!((lp.magnitude_db(0.1) + 3.0).abs() < 0.35);
    /// ```
    pub fn lowpass(f0: f64, q: f64) -> Self {
        let (cos_w0, alpha, a0) = rbj(f0, q);
        Self {
            b0: (1.0 - cos_w0) / 2.0 / a0,
            b1: (1.0 - cos_w0) / a0,
            b2: (1.0 - cos_w0) / 2.0 / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// RBJ highpass at `f0` with quality `q`.
    ///
    /// ```
    /// use manim_signal::filters::{Biquad, FrequencyResponse};
    /// let hp = Biquad::highpass(0.1, 0.707);
    /// assert!(hp.magnitude(0.0) < 1e-15);
    /// assert!((hp.magnitude(0.5) - 1.0).abs() < 1e-12);
    /// ```
    pub fn highpass(f0: f64, q: f64) -> Self {
        let (cos_w0, alpha, a0) = rbj(f0, q);
        Self {
            b0: (1.0 + cos_w0) / 2.0 / a0,
            b1: -(1.0 + cos_w0) / a0,
            b2: (1.0 + cos_w0) / 2.0 / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// RBJ bandpass with unity peak gain at `f0`.
    ///
    /// ```
    /// use manim_signal::filters::{Biquad, FrequencyResponse};
    /// let bp = Biquad::bandpass(0.15, 4.0);
    /// assert!((bp.magnitude(0.15) - 1.0).abs() < 1e-12);
    /// assert!(bp.magnitude(0.0) < 1e-15 && bp.magnitude(0.5) < 1e-15);
    /// ```
    pub fn bandpass(f0: f64, q: f64) -> Self {
        let (cos_w0, alpha, a0) = rbj(f0, q);
        Self {
            b0: alpha / a0,
            b1: 0.0,
            b2: -alpha / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// RBJ notch: unity everywhere but a null at `f0`.
    ///
    /// ```
    /// use manim_signal::filters::{Biquad, FrequencyResponse};
    /// let n = Biquad::notch(0.2, 6.0);
    /// assert!(n.magnitude(0.2) < 1e-12);
    /// assert!((n.magnitude(0.0) - 1.0).abs() < 1e-12);
    /// ```
    pub fn notch(f0: f64, q: f64) -> Self {
        let (cos_w0, alpha, a0) = rbj(f0, q);
        Self {
            b0: 1.0 / a0,
            b1: -2.0 * cos_w0 / a0,
            b2: 1.0 / a0,
            a1: -2.0 * cos_w0 / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// Filters a signal through the section (zero initial state).
    ///
    /// ```
    /// use manim_signal::filters::Biquad;
    /// let lp = Biquad::lowpass(0.05, 0.707);
    /// let step = vec![1.0; 200];
    /// let y = lp.process(&step);
    /// // A unity-DC-gain lowpass settles at the step height.
    /// assert!((y[199] - 1.0).abs() < 1e-3);
    /// ```
    pub fn process(&self, x: &[f64]) -> Vec<f64> {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        x.iter()
            .map(|&xn| {
                let yn = self.b0 * xn + self.b1 * x1 + self.b2 * x2 - self.a1 * y1 - self.a2 * y2;
                x2 = x1;
                x1 = xn;
                y2 = y1;
                y1 = yn;
                yn
            })
            .collect()
    }

    /// Whether both poles lie strictly inside the unit circle (Jury's test for a
    /// second-order section).
    ///
    /// ```
    /// use manim_signal::filters::Biquad;
    /// assert!(Biquad::lowpass(0.1, 0.707).is_stable());
    /// assert!(!Biquad::new(1.0, 0.0, 0.0, 0.0, 1.5).is_stable());
    /// ```
    pub fn is_stable(&self) -> bool {
        self.a2.abs() < 1.0 && self.a1.abs() < 1.0 + self.a2
    }
}

impl FrequencyResponse for Biquad {
    fn response(&self, f: f64) -> Complex {
        let z1 = Complex::from_polar(1.0, -std::f64::consts::TAU * f);
        let z2 = z1 * z1;
        let num = Complex::real(self.b0) + z1.scale(self.b1) + z2.scale(self.b2);
        let den = Complex::one() + z1.scale(self.a1) + z2.scale(self.a2);
        num / den
    }
}

/// How the frequency axis of a response plot is scaled.
///
/// ```
/// use manim_signal::filters::FreqScale;
/// // On a log axis, x = log₁₀ f.
/// assert!((FreqScale::Log10.to_freq(-2.0) - 0.01).abs() < 1e-12);
/// assert_eq!(FreqScale::Linear.to_freq(0.25), 0.25);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreqScale {
    /// The x-coordinate *is* the normalized frequency.
    Linear,
    /// The x-coordinate is `log₁₀` of the normalized frequency (decade axes).
    Log10,
}

impl FreqScale {
    /// Maps an axis x-coordinate to a normalized frequency.
    ///
    /// ```
    /// use manim_signal::filters::FreqScale;
    /// assert_eq!(FreqScale::Log10.to_freq(-1.0), 0.1);
    /// ```
    pub fn to_freq(self, x: f64) -> f64 {
        match self {
            FreqScale::Linear => x,
            FreqScale::Log10 => 10f64.powf(x),
        }
    }
}

/// A magnitude-response graph in decibels over the axes' x-range.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_signal::filters::{magnitude_graph, Biquad, FreqScale};
/// // Three decades of normalized frequency, −60…+6 dB.
/// let axes = Axes::new([-3.0, -0.3, 1.0], [-60.0, 6.0, 12.0]);
/// let g = magnitude_graph(&axes, Biquad::lowpass(0.05, 0.707), FreqScale::Log10);
/// assert!(!g.data().path.subpaths.is_empty());
/// ```
pub fn magnitude_graph<H: FrequencyResponse + Send + Sync + 'static>(
    axes: &Axes,
    filter: H,
    scale: FreqScale,
) -> FunctionGraph {
    axes.plot(
        move |x| filter.magnitude_db(scale.to_freq(x as f64)) as f32,
        None,
    )
}

/// A phase-response graph in radians over the axes' x-range.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_signal::filters::{phase_graph, Biquad, FreqScale};
/// let axes = Axes::new([0.0, 0.5, 0.1], [-3.2, 3.2, 1.0]);
/// let g = phase_graph(&axes, Biquad::highpass(0.1, 0.707), FreqScale::Linear);
/// assert!(!g.data().path.subpaths.is_empty());
/// ```
pub fn phase_graph<H: FrequencyResponse + Send + Sync + 'static>(
    axes: &Axes,
    filter: H,
    scale: FreqScale,
) -> FunctionGraph {
    axes.plot(move |x| filter.phase(scale.to_freq(x as f64)) as f32, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Brute-force DTFT of an impulse response, for cross-checking.
    fn dtft(h: &[f64], f: f64) -> Complex {
        h.iter().enumerate().fold(Complex::zero(), |acc, (n, &v)| {
            acc + Complex::from_polar(v, -std::f64::consts::TAU * f * n as f64)
        })
    }

    #[test]
    fn biquad_lowpass_endpoints_are_analytic() {
        for &(f0, q) in &[(0.05, 0.707), (0.1, 1.0), (0.25, 4.0)] {
            let lp = Biquad::lowpass(f0, q);
            // DC gain is exactly 1: Σb = Σa = 2(1 − cos ω₀).
            assert!(
                (lp.magnitude(0.0) - 1.0).abs() < 1e-12,
                "DC gain {}",
                lp.magnitude(0.0)
            );
            // Double zero at z = −1 ⇒ |H(Nyquist)| = 0 exactly.
            assert!(lp.magnitude(0.5) < 1e-15, "Nyquist {}", lp.magnitude(0.5));
            assert!(lp.is_stable());
        }
    }

    #[test]
    fn biquad_highpass_endpoints_are_analytic() {
        for &(f0, q) in &[(0.05, 0.707), (0.2, 2.0)] {
            let hp = Biquad::highpass(f0, q);
            assert!(hp.magnitude(0.0) < 1e-15);
            assert!((hp.magnitude(0.5) - 1.0).abs() < 1e-12);
            assert!(hp.is_stable());
        }
    }

    #[test]
    fn biquad_bandpass_peaks_at_unity_on_centre() {
        for &(f0, q) in &[(0.1, 2.0), (0.2, 8.0)] {
            let bp = Biquad::bandpass(f0, q);
            assert!((bp.magnitude(f0) - 1.0).abs() < 1e-12);
            assert!(bp.magnitude(0.0) < 1e-15 && bp.magnitude(0.5) < 1e-15);
            // The centre really is the maximum.
            for i in 0..=100 {
                let f = i as f64 * 0.005;
                assert!(bp.magnitude(f) <= bp.magnitude(f0) + 1e-12);
            }
        }
    }

    #[test]
    fn biquad_notch_nulls_its_centre_and_passes_the_rest() {
        let n = Biquad::notch(0.2, 6.0);
        assert!(n.magnitude(0.2) < 1e-12);
        assert!((n.magnitude(0.0) - 1.0).abs() < 1e-12);
        assert!((n.magnitude(0.5) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn biquad_minus_three_db_at_the_butterworth_corner() {
        // Q = 1/√2 is the maximally flat case: |H(f0)| = 1/√2 in the analog
        // prototype; the bilinear-warped digital section matches it closely.
        let f0 = 0.05;
        let lp = Biquad::lowpass(f0, std::f64::consts::FRAC_1_SQRT_2);
        assert!(
            (lp.magnitude_db(f0) + 3.0103).abs() < 0.1,
            "corner {} dB",
            lp.magnitude_db(f0)
        );
    }

    #[test]
    fn biquad_impulse_response_matches_the_frequency_response() {
        let lp = Biquad::lowpass(0.08, 0.9);
        let mut imp = vec![0.0; 4096];
        imp[0] = 1.0;
        let h = lp.process(&imp);
        for f in [0.0, 0.03, 0.08, 0.2, 0.4, 0.5] {
            let a = dtft(&h, f);
            let b = lp.response(f);
            assert!((a - b).norm() < 1e-6, "f = {f}: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn fir_lowpass_has_unity_dc_gain_and_linear_phase() {
        for w in [
            Window::Rect,
            Window::Hann,
            Window::Hamming,
            Window::Blackman,
        ] {
            let lp = Fir::lowpass(41, 0.15, w);
            assert!((lp.magnitude(0.0) - 1.0).abs() < 1e-12, "{w:?}");
            assert!(lp.is_linear_phase(), "{w:?}");
            // Group delay is (N−1)/2 = 20 samples: phase is −2πf·20, modulo the
            // 2π wrap `arg` folds it into.
            for f in [0.02, 0.05, 0.11] {
                let expect = -std::f64::consts::TAU * f * 20.0;
                let err = (lp.phase(f) - expect).rem_euclid(std::f64::consts::TAU);
                let err = err.min(std::f64::consts::TAU - err);
                assert!(err < 1e-9, "{w:?} at f = {f}: wrapped error {err}");
            }
        }
    }

    #[test]
    fn fir_window_choice_trades_sidelobes_for_transition_width() {
        let rect = Fir::lowpass(41, 0.15, Window::Rect);
        let black = Fir::lowpass(41, 0.15, Window::Blackman);
        // Blackman's stopband is far cleaner than the boxcar's.
        assert!(black.magnitude_db(0.35) < rect.magnitude_db(0.35) - 20.0);
    }

    #[test]
    fn fir_highpass_is_the_spectral_inverse_of_its_lowpass() {
        let (len, fc, w) = (41, 0.2, Window::Hamming);
        let lp = Fir::lowpass(len, fc, w);
        let hp = Fir::highpass(len, fc, w);
        assert!(hp.magnitude(0.0) < 1e-12);
        // lp + hp is an allpass delay of (N−1)/2 samples: |H| = 1 everywhere.
        for i in 0..=50 {
            let f = i as f64 * 0.01;
            let sum = lp.response(f) + hp.response(f);
            assert!((sum.norm() - 1.0).abs() < 1e-9, "f = {f}: {}", sum.norm());
        }
    }

    #[test]
    fn fir_response_matches_direct_convolution() {
        let lp = Fir::lowpass(21, 0.1, Window::Hann);
        let mut imp = vec![0.0; 21];
        imp[0] = 1.0;
        assert_eq!(lp.process(&imp), lp.taps().to_vec());
        for f in [0.0, 0.1, 0.25, 0.5] {
            assert!((lp.response(f) - dtft(lp.taps(), f)).norm() < 1e-15);
        }
    }

    #[test]
    fn responses_are_periodic_and_conjugate_symmetric() {
        let lp = Biquad::lowpass(0.1, 0.8);
        for f in [0.03, 0.17, 0.42] {
            assert!((lp.response(f) - lp.response(f + 1.0)).norm() < 1e-9);
            assert!((lp.response(-f) - lp.response(f).conj()).norm() < 1e-12);
        }
    }
}
