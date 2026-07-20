//! Short-time Fourier transform, and its heatmap.
//!
//! A single FFT of a whole signal tells you *which* frequencies are present but
//! not *when*. The STFT trades some frequency resolution for time resolution:
//! window the signal into overlapping frames, FFT each one, and stack the
//! magnitude spectra into a time–frequency image. [`Spectrogram::material_quad`]
//! hands that image to a
//! [`manim_sci::material_quad::MaterialQuad`], so it renders as a
//! GPU-shaded rectangle rather than thousands of little mobjects.

use manim_core::display::Colormap;
use manim_fields::ad::Scalar;
use manim_fields::field::{ScalarClosure, ScalarField};
use manim_sci::material_quad::MaterialQuad;
use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

use crate::Window;

/// A time–frequency magnitude image: `frames × bins` of `|X(t, f)|`.
///
/// ```
/// use manim_signal::spectrogram::Spectrogram;
/// use manim_signal::Window;
/// // A 100 Hz tone sampled at 1 kHz.
/// let sig: Vec<f64> = (0..1024)
///     .map(|n| (std::f64::consts::TAU * 100.0 * n as f64 / 1000.0).sin())
///     .collect();
/// let s = Spectrogram::stft(&sig, 1000.0, 256, 128, Window::Hann);
/// assert_eq!(s.bins(), 129);
/// // The loudest bin of the middle frame sits at 100 Hz.
/// assert!((s.peak_frequency(3) - 100.0).abs() < 5.0);
/// ```
#[derive(Clone, Debug)]
pub struct Spectrogram {
    /// Row-major magnitudes, `frames × bins`.
    mags: Vec<f64>,
    frames: usize,
    bins: usize,
    fs: f64,
    hop: usize,
    window_len: usize,
}

impl Spectrogram {
    /// Computes the STFT of `signal` sampled at `fs`, with `window_len`-sample
    /// frames advanced by `hop` samples and tapered by `window`.
    ///
    /// Only the non-negative-frequency half of each spectrum is kept, so a
    /// length-`W` window yields `W/2 + 1` bins spaced `fs/W` apart.
    ///
    /// ```
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// let sig = vec![0.0; 600];
    /// let s = Spectrogram::stft(&sig, 100.0, 128, 64, Window::Hann);
    /// // Frames start at 0, 64, 128, … while a full window still fits.
    /// assert_eq!(s.frames(), 8);
    /// assert_eq!(s.bins(), 65);
    /// ```
    pub fn stft(signal: &[f64], fs: f64, window_len: usize, hop: usize, window: Window) -> Self {
        let window_len = window_len.max(2);
        let hop = hop.max(1);
        let bins = window_len / 2 + 1;
        let frames = if signal.len() < window_len {
            0
        } else {
            (signal.len() - window_len) / hop + 1
        };
        let taper = window.samples(window_len);
        let fft = FftPlanner::new().plan_fft_forward(window_len);

        let mut mags = Vec::with_capacity(frames * bins);
        let mut buf = vec![Complex64::new(0.0, 0.0); window_len];
        for frame in 0..frames {
            let start = frame * hop;
            for (i, slot) in buf.iter_mut().enumerate() {
                *slot = Complex64::new(signal[start + i] * taper[i], 0.0);
            }
            fft.process(&mut buf);
            mags.extend(buf.iter().take(bins).map(|c| c.norm() / window_len as f64));
        }

        Self {
            mags,
            frames,
            bins,
            fs,
            hop,
            window_len,
        }
    }

    /// The number of time frames.
    ///
    /// ```
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// assert_eq!(Spectrogram::stft(&[0.0; 10], 1.0, 64, 32, Window::Hann).frames(), 0);
    /// ```
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// The number of frequency bins, `window_len/2 + 1`.
    ///
    /// ```
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// assert_eq!(Spectrogram::stft(&[0.0; 512], 1.0, 64, 32, Window::Hann).bins(), 33);
    /// ```
    pub fn bins(&self) -> usize {
        self.bins
    }

    /// The magnitude at `(frame, bin)` (`0.0` out of range).
    ///
    /// ```
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// let s = Spectrogram::stft(&[0.0; 512], 1.0, 64, 32, Window::Hann);
    /// assert_eq!(s.magnitude(0, 0), 0.0);
    /// assert_eq!(s.magnitude(999, 0), 0.0);
    /// ```
    pub fn magnitude(&self, frame: usize, bin: usize) -> f64 {
        if frame >= self.frames || bin >= self.bins {
            return 0.0;
        }
        self.mags[frame * self.bins + bin]
    }

    /// The magnitude at `(frame, bin)` in decibels, floored at `floor_db`.
    ///
    /// ```
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// let s = Spectrogram::stft(&[0.0; 512], 1.0, 64, 32, Window::Hann);
    /// assert_eq!(s.magnitude_db(0, 0, -80.0), -80.0);
    /// ```
    pub fn magnitude_db(&self, frame: usize, bin: usize, floor_db: f64) -> f64 {
        (20.0 * self.magnitude(frame, bin).max(1e-300).log10()).max(floor_db)
    }

    /// The centre time of `frame`, in seconds.
    ///
    /// ```
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// let s = Spectrogram::stft(&[0.0; 1024], 100.0, 64, 32, Window::Hann);
    /// // Frame 0 is centred half a window in: 32 samples at 100 Hz.
    /// assert!((s.frame_time(0) - 0.32).abs() < 1e-12);
    /// ```
    pub fn frame_time(&self, frame: usize) -> f64 {
        (frame * self.hop) as f64 / self.fs + 0.5 * self.window_len as f64 / self.fs
    }

    /// The centre frequency of `bin`, in hertz.
    ///
    /// ```
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// let s = Spectrogram::stft(&[0.0; 1024], 1000.0, 100, 50, Window::Hann);
    /// assert_eq!(s.bin_frequency(1), 10.0);
    /// ```
    pub fn bin_frequency(&self, bin: usize) -> f64 {
        bin as f64 * self.fs / self.window_len as f64
    }

    /// The highest frequency the transform resolves (Nyquist).
    ///
    /// ```
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// let s = Spectrogram::stft(&[0.0; 1024], 48000.0, 512, 256, Window::Hann);
    /// assert_eq!(s.max_frequency(), 24000.0);
    /// ```
    pub fn max_frequency(&self) -> f64 {
        0.5 * self.fs
    }

    /// The duration spanned by the frames, in seconds.
    ///
    /// ```
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// let s = Spectrogram::stft(&[0.0; 1024], 1000.0, 256, 128, Window::Hann);
    /// assert!(s.duration() > 0.0);
    /// ```
    pub fn duration(&self) -> f64 {
        if self.frames == 0 {
            0.0
        } else {
            self.frame_time(self.frames - 1) + 0.5 * self.window_len as f64 / self.fs
        }
    }

    /// The frequency of the loudest bin in `frame`.
    ///
    /// ```
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// let sig: Vec<f64> = (0..2048)
    ///     .map(|n| (std::f64::consts::TAU * 50.0 * n as f64 / 1000.0).sin())
    ///     .collect();
    /// let s = Spectrogram::stft(&sig, 1000.0, 512, 256, Window::Hann);
    /// assert!((s.peak_frequency(2) - 50.0).abs() < 2.0);
    /// ```
    pub fn peak_frequency(&self, frame: usize) -> f64 {
        let best = (0..self.bins)
            .max_by(|&a, &b| {
                self.magnitude(frame, a)
                    .partial_cmp(&self.magnitude(frame, b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0);
        self.bin_frequency(best)
    }

    /// The magnitude at an arbitrary `(time, frequency)`, by bilinear
    /// interpolation of the frame/bin grid (clamped at the edges).
    ///
    /// ```
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// let sig: Vec<f64> = (0..2048).map(|n| (n as f64 * 0.3).sin()).collect();
    /// let s = Spectrogram::stft(&sig, 1000.0, 256, 128, Window::Hann);
    /// // Sampling on a grid node returns that node's magnitude.
    /// let exact = s.magnitude(2, 10);
    /// let got = s.sample(s.frame_time(2), s.bin_frequency(10));
    /// assert!((got - exact).abs() < 1e-9);
    /// ```
    pub fn sample(&self, time: f64, frequency: f64) -> f64 {
        if self.frames == 0 || self.bins == 0 {
            return 0.0;
        }
        let fr = ((time - 0.5 * self.window_len as f64 / self.fs) * self.fs / self.hop as f64)
            .clamp(0.0, (self.frames - 1) as f64);
        let bn = (frequency * self.window_len as f64 / self.fs).clamp(0.0, (self.bins - 1) as f64);
        let (f0, b0) = (fr.floor() as usize, bn.floor() as usize);
        let (f1, b1) = ((f0 + 1).min(self.frames - 1), (b0 + 1).min(self.bins - 1));
        let (tf, tb) = (fr - f0 as f64, bn - b0 as f64);
        let m = |f: usize, b: usize| self.magnitude(f, b);
        let lo = m(f0, b0) * (1.0 - tb) + m(f0, b1) * tb;
        let hi = m(f1, b0) * (1.0 - tb) + m(f1, b1) * tb;
        lo * (1.0 - tf) + hi * tf
    }

    /// A [`ScalarField`] over `(x, y) = (time, frequency)` reading this
    /// spectrogram in decibels — the sampling function behind
    /// [`material_quad`](Self::material_quad).
    ///
    /// The field is a table lookup, so it carries no derivative information:
    /// gradients through it are zero by construction.
    ///
    /// ```
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// let sig: Vec<f64> = (0..2048).map(|n| (n as f64 * 0.3).sin()).collect();
    /// let s = Spectrogram::stft(&sig, 1000.0, 256, 128, Window::Hann);
    /// let field = s.db_field(-80.0);
    /// let v = field.at(manim_fields::Point::new(s.frame_time(1), 50.0, 0.0));
    /// assert!(v >= -80.0 && v <= 0.0);
    /// ```
    pub fn db_field(&self, floor_db: f64) -> ScalarField {
        ScalarField::from_closure(SpectrogramLookup {
            spec: self.clone(),
            floor_db,
        })
    }

    /// A heatmap [`MaterialQuad`] of this spectrogram over the given scene
    /// rectangle: `x` spans `0..duration`, `y` spans `0..Nyquist`.
    ///
    /// ```
    /// use manim_core::display::Colormap;
    /// use manim_core::prelude::*;
    /// use manim_signal::spectrogram::Spectrogram;
    /// use manim_signal::Window;
    /// let sig: Vec<f64> = (0..4096)
    ///     .map(|n| (std::f64::consts::TAU * 120.0 * n as f64 / 1000.0).sin())
    ///     .collect();
    /// let s = Spectrogram::stft(&sig, 1000.0, 256, 64, Window::Hann);
    /// let mut scene = SceneState::new();
    /// let quad = s.material_quad([-5.0, 5.0], [-2.5, 2.5], (64, 48), Colormap::Viridis, -70.0);
    /// let id = scene.add(quad);
    /// assert!(scene.contains(id));
    /// ```
    pub fn material_quad(
        &self,
        x_range: [f64; 2],
        y_range: [f64; 2],
        resolution: (usize, usize),
        colormap: Colormap,
        floor_db: f64,
    ) -> MaterialQuad {
        // The lookup field lives in (time, frequency); remap it onto the scene
        // rectangle so the quad can be placed anywhere.
        let field = ScalarField::from_closure(RemappedLookup {
            spec: self.clone(),
            floor_db,
            x_range,
            y_range,
        });
        MaterialQuad::heatmap(x_range, y_range, resolution, &field, colormap)
    }
}

/// A [`ScalarClosure`] reading a spectrogram in `(time, frequency)` coordinates.
struct SpectrogramLookup {
    spec: Spectrogram,
    floor_db: f64,
}

impl ScalarClosure for SpectrogramLookup {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        let m = self.spec.sample(p[0].value(), p[1].value());
        S::constant((20.0 * m.max(1e-300).log10()).max(self.floor_db))
    }
}

/// The same lookup, with the scene rectangle mapped onto time × frequency.
struct RemappedLookup {
    spec: Spectrogram,
    floor_db: f64,
    x_range: [f64; 2],
    y_range: [f64; 2],
}

impl ScalarClosure for RemappedLookup {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        let u = ((p[0].value() - self.x_range[0]) / (self.x_range[1] - self.x_range[0]))
            .clamp(0.0, 1.0);
        let v = ((p[1].value() - self.y_range[0]) / (self.y_range[1] - self.y_range[0]))
            .clamp(0.0, 1.0);
        let m = self
            .spec
            .sample(u * self.spec.duration(), v * self.spec.max_frequency());
        S::constant((20.0 * m.max(1e-300).log10()).max(self.floor_db))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(f: f64, fs: f64, n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| (std::f64::consts::TAU * f * i as f64 / fs).sin())
            .collect()
    }

    #[test]
    fn frame_and_bin_counts_follow_the_window_geometry() {
        let s = Spectrogram::stft(&vec![0.0; 1000], 1000.0, 256, 128, Window::Hann);
        assert_eq!(s.bins(), 129);
        // Frames start at 0,128,…,768 (768+256 = 1024 > 1000 ⇒ stop at 744? no:
        // (1000−256)/128 + 1 = 6 frames, last starting at 640).
        assert_eq!(s.frames(), (1000 - 256) / 128 + 1);
        assert_eq!(s.bin_frequency(0), 0.0);
        assert_eq!(s.bin_frequency(s.bins() - 1), s.max_frequency());
    }

    #[test]
    fn a_pure_tone_lands_in_its_own_bin() {
        let fs = 1000.0;
        // 125 Hz is bin 32 exactly for a 256-sample window (fs/W = 3.90625 Hz).
        let f = 125.0;
        let s = Spectrogram::stft(&tone(f, fs, 4096), fs, 256, 128, Window::Hann);
        for frame in 1..s.frames() - 1 {
            let peak = s.peak_frequency(frame);
            assert!((peak - f).abs() < 4.0, "frame {frame}: peak {peak}");
        }
        // Amplitude: a Hann-windowed unit sine puts ≈ 1/4 of its amplitude in the
        // peak bin (half for the one-sided split, half again for the window's
        // coherent gain).
        let bin = (f * 256.0 / fs).round() as usize;
        let m = s.magnitude(2, bin);
        assert!((m - 0.25).abs() < 0.02, "peak magnitude {m}");
    }

    #[test]
    fn a_chirp_walks_up_the_image() {
        let fs = 1000.0;
        let n = 8192;
        // Linear chirp 50 → 400 Hz: instantaneous f = 50 + 350·t/T.
        let t_total = n as f64 / fs;
        let sig: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64 / fs;
                let phase = std::f64::consts::TAU * (50.0 * t + 0.5 * (350.0 / t_total) * t * t);
                phase.sin()
            })
            .collect();
        let s = Spectrogram::stft(&sig, fs, 512, 128, Window::Hann);
        let first = s.peak_frequency(1);
        let last = s.peak_frequency(s.frames() - 2);
        assert!(first < 120.0, "first peak {first}");
        assert!(last > 330.0, "last peak {last}");
        // Monotone climb, allowing bin-quantisation wobble.
        let peaks: Vec<f64> = (1..s.frames() - 1).map(|i| s.peak_frequency(i)).collect();
        assert!(peaks.windows(2).all(|w| w[1] >= w[0] - 4.0));
    }

    #[test]
    fn silence_is_floored_not_infinite() {
        let s = Spectrogram::stft(&vec![0.0; 2048], 1000.0, 256, 128, Window::Hann);
        assert_eq!(s.magnitude_db(1, 5, -80.0), -80.0);
        let field = s.db_field(-80.0);
        assert_eq!(field.at(manim_fields::Point::new(0.4, 100.0, 0.0)), -80.0);
    }

    #[test]
    fn bilinear_sampling_agrees_on_grid_nodes() {
        let s = Spectrogram::stft(&tone(150.0, 1000.0, 4096), 1000.0, 256, 128, Window::Hann);
        for frame in 0..s.frames().min(5) {
            for bin in [0usize, 7, 40, 128] {
                let want = s.magnitude(frame, bin);
                let got = s.sample(s.frame_time(frame), s.bin_frequency(bin));
                assert!(
                    (got - want).abs() < 1e-9,
                    "({frame},{bin}): {got} vs {want}"
                );
            }
        }
    }
}
