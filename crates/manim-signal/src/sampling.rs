//! Sampling, aliasing, and reconstruction.
//!
//! The picture this module builds is the standard three-layer one: the
//! continuous signal, the sample stems that survive an ADC, and the signal a
//! reconstruction filter puts back — either the ideal Whittaker–Shannon sinc
//! interpolation or the staircase a zero-order-hold DAC actually produces.
//!
//! [`alias_frequency`] is the arithmetic behind the punchline: a tone at `f`
//! sampled at `fs` is indistinguishable from one at `|f − k·fs|`, and the
//! reconstruction picks the representative in `[0, fs/2]`.

use manim_core::geometry::{Dot, Line, VGroup};
use manim_core::graphing::{Axes, FunctionGraph};
use manim_core::mobject::{AnyId, Buildable, MobjectId};
use manim_core::prelude::Color;
use manim_core::scene_state::SceneState;

use crate::sinc;

/// The frequency a tone at `f` masquerades as when sampled at `fs`: the unique
/// representative of `f` modulo `fs` that lies in `[0, fs/2]`.
///
/// This is exact arithmetic on the fold, not an approximation: for any integer
/// `k`, `alias_frequency(f + k·fs, fs) == alias_frequency(f, fs)`.
///
/// ```
/// use manim_signal::sampling::alias_frequency;
/// // Below Nyquist, nothing happens.
/// assert_eq!(alias_frequency(30.0, 100.0), 30.0);
/// // Above it, the tone folds back down.
/// assert_eq!(alias_frequency(70.0, 100.0), 30.0);
/// assert_eq!(alias_frequency(130.0, 100.0), 30.0);
/// assert_eq!(alias_frequency(170.0, 100.0), 30.0);
/// ```
pub fn alias_frequency(f: f64, fs: f64) -> f64 {
    if fs <= 0.0 {
        return f.abs();
    }
    let r = f.abs().rem_euclid(fs);
    if r > 0.5 * fs {
        fs - r
    } else {
        r
    }
}

/// Whether `f` is representable at sample rate `fs` (strictly below Nyquist).
///
/// ```
/// use manim_signal::sampling::is_below_nyquist;
/// assert!(is_below_nyquist(49.0, 100.0));
/// assert!(!is_below_nyquist(51.0, 100.0));
/// ```
pub fn is_below_nyquist(f: f64, fs: f64) -> bool {
    f.abs() < 0.5 * fs
}

/// A uniformly sampled signal: values `y[n]` taken at `t0 + n/fs`.
///
/// ```
/// use manim_signal::sampling::Samples;
/// let s = Samples::of(|t| t, 0.0, 1.0, 4.0);
/// assert_eq!(s.len(), 4);
/// assert_eq!(s.time(2), 0.5);
/// assert_eq!(s.value(2), 0.5);
/// ```
#[derive(Clone, Debug)]
pub struct Samples {
    /// The sample values, in order.
    values: Vec<f64>,
    /// The time of `values[0]`.
    t0: f64,
    /// The sample rate (samples per unit time).
    fs: f64,
}

impl Samples {
    /// Samples `f` on `[t0, t1)` at rate `fs`.
    ///
    /// ```
    /// use manim_signal::sampling::Samples;
    /// let s = Samples::of(|t: f64| (std::f64::consts::TAU * t).sin(), 0.0, 2.0, 8.0);
    /// assert_eq!(s.len(), 16);
    /// ```
    pub fn of(f: impl Fn(f64) -> f64, t0: f64, t1: f64, fs: f64) -> Self {
        let n = (((t1 - t0) * fs).ceil().max(0.0)) as usize;
        let values = (0..n).map(|i| f(t0 + i as f64 / fs)).collect();
        Self { values, t0, fs }
    }

    /// Wraps an existing value sequence taken at `fs` starting at `t0`.
    ///
    /// ```
    /// use manim_signal::sampling::Samples;
    /// let s = Samples::new(vec![1.0, 0.0, -1.0], 0.0, 3.0);
    /// assert_eq!(s.value(1), 0.0);
    /// ```
    pub fn new(values: Vec<f64>, t0: f64, fs: f64) -> Self {
        Self { values, t0, fs }
    }

    /// The sample values.
    ///
    /// ```
    /// use manim_signal::sampling::Samples;
    /// assert_eq!(Samples::new(vec![2.0], 0.0, 1.0).values(), &[2.0]);
    /// ```
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// The sample count.
    ///
    /// ```
    /// use manim_signal::sampling::Samples;
    /// assert_eq!(Samples::new(vec![1.0, 2.0], 0.0, 1.0).len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether there are no samples.
    ///
    /// ```
    /// use manim_signal::sampling::Samples;
    /// assert!(Samples::new(vec![], 0.0, 1.0).is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The sample rate.
    ///
    /// ```
    /// use manim_signal::sampling::Samples;
    /// assert_eq!(Samples::new(vec![], 0.0, 44100.0).rate(), 44100.0);
    /// ```
    pub fn rate(&self) -> f64 {
        self.fs
    }

    /// The time of sample `n`.
    ///
    /// ```
    /// use manim_signal::sampling::Samples;
    /// assert_eq!(Samples::new(vec![0.0; 4], 1.0, 2.0).time(3), 2.5);
    /// ```
    pub fn time(&self, n: usize) -> f64 {
        self.t0 + n as f64 / self.fs
    }

    /// The value of sample `n` (`0.0` outside the record).
    ///
    /// ```
    /// use manim_signal::sampling::Samples;
    /// let s = Samples::new(vec![5.0], 0.0, 1.0);
    /// assert_eq!(s.value(0), 5.0);
    /// assert_eq!(s.value(9), 0.0);
    /// ```
    pub fn value(&self, n: usize) -> f64 {
        self.values.get(n).copied().unwrap_or(0.0)
    }

    /// Ideal (Whittaker–Shannon) reconstruction at time `t`:
    /// `Σ y[n] · sinc(fs·(t − tₙ))`.
    ///
    /// For a signal strictly below Nyquist and a long enough record this
    /// reproduces the original between the samples, not just at them.
    ///
    /// ```
    /// use manim_signal::sampling::Samples;
    /// let s = Samples::new(vec![0.0, 1.0, 0.0], 0.0, 1.0);
    /// // Interpolation is exact at the sample instants.
    /// assert!((s.sinc_interpolate(1.0) - 1.0).abs() < 1e-12);
    /// ```
    pub fn sinc_interpolate(&self, t: f64) -> f64 {
        self.values
            .iter()
            .enumerate()
            .map(|(n, &y)| y * sinc(self.fs * (t - self.time(n))))
            .sum()
    }

    /// Zero-order-hold reconstruction at time `t`: the most recent sample.
    ///
    /// ```
    /// use manim_signal::sampling::Samples;
    /// let s = Samples::new(vec![1.0, -1.0], 0.0, 1.0);
    /// assert_eq!(s.zero_order_hold(0.75), 1.0);
    /// assert_eq!(s.zero_order_hold(1.25), -1.0);
    /// ```
    pub fn zero_order_hold(&self, t: f64) -> f64 {
        if self.values.is_empty() || t < self.t0 {
            return 0.0;
        }
        let n = ((t - self.t0) * self.fs).floor() as usize;
        self.value(n.min(self.values.len().saturating_sub(1)))
    }

    /// Adds one stem (a line to the axis plus a dot) per sample to `scene`,
    /// returning the group holding them.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_signal::sampling::Samples;
    /// let axes = Axes::new([0.0, 2.0, 0.5], [-1.5, 1.5, 0.5]);
    /// let mut scene = SceneState::new();
    /// let s = Samples::of(|t: f64| (std::f64::consts::TAU * t).sin(), 0.0, 2.0, 8.0);
    /// let stems = s.add_stems(&mut scene, &axes, YELLOW);
    /// // One line + one dot per sample, plus the group itself.
    /// assert_eq!(scene.family(stems.erase()).len(), 2 * s.len() + 1);
    /// ```
    pub fn add_stems(
        &self,
        scene: &mut SceneState,
        axes: &Axes,
        color: Color,
    ) -> MobjectId<VGroup> {
        let mut members: Vec<AnyId> = Vec::with_capacity(2 * self.values.len());
        for (n, &y) in self.values.iter().enumerate() {
            let t = self.time(n) as f32;
            let base = axes.c2p(t, 0.0);
            let top = axes.c2p(t, y as f32);
            members.push(
                scene
                    .add(Line::new(base, top).with_stroke(color, 2.5, 1.0))
                    .erase(),
            );
            members.push(
                scene
                    .add(Dot::at(top).radius(0.045).with_fill(color, 1.0))
                    .erase(),
            );
        }
        VGroup::of(scene, members)
    }

    /// A graph of the ideal sinc reconstruction over the axes' x-range.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_signal::sampling::Samples;
    /// let axes = Axes::new([0.0, 2.0, 0.5], [-1.5, 1.5, 0.5]);
    /// let s = Samples::of(|t: f64| (std::f64::consts::TAU * t).sin(), 0.0, 2.0, 16.0);
    /// let g = s.sinc_graph(&axes);
    /// assert!(!g.data().path.subpaths.is_empty());
    /// ```
    pub fn sinc_graph(&self, axes: &Axes) -> FunctionGraph {
        let me = self.clone();
        axes.plot(move |x| me.sinc_interpolate(x as f64) as f32, None)
    }

    /// A graph of the zero-order-hold staircase over the axes' x-range.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_signal::sampling::Samples;
    /// let axes = Axes::new([0.0, 2.0, 0.5], [-1.5, 1.5, 0.5]);
    /// let s = Samples::of(|t: f64| (std::f64::consts::TAU * t).sin(), 0.0, 2.0, 16.0);
    /// let g = s.hold_graph(&axes);
    /// assert!(!g.data().path.subpaths.is_empty());
    /// ```
    pub fn hold_graph(&self, axes: &Axes) -> FunctionGraph {
        let me = self.clone();
        axes.plot(move |x| me.zero_order_hold(x as f64) as f32, None)
    }
}

/// A graph of the aliased tone a sampler *appears* to see: the sinusoid at
/// [`alias_frequency`] with the phase that matches the true tone at every sample
/// instant.
///
/// Drawn on top of the true high-frequency tone and its samples, this is the
/// curve the eye connects the dots into.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_signal::sampling::alias_graph;
/// let axes = Axes::new([0.0, 1.0, 0.25], [-1.5, 1.5, 0.5]);
/// // 9 Hz sampled at 10 Hz looks like 1 Hz.
/// let g = alias_graph(&axes, 9.0, 10.0, 1.0);
/// assert!(!g.data().path.subpaths.is_empty());
/// ```
pub fn alias_graph(axes: &Axes, f: f64, fs: f64, amplitude: f64) -> FunctionGraph {
    let fa = alias_frequency(f, fs);
    // Folding an odd number of times flips the apparent phase direction.
    let folds = (f.abs() / (0.5 * fs)).floor() as i64;
    let sign = if folds % 2 == 0 { 1.0 } else { -1.0 };
    axes.plot(
        move |x| (amplitude * (sign * std::f64::consts::TAU * fa * x as f64).sin()) as f32,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_frequency_is_exact_on_the_fold() {
        let fs = 100.0;
        // f_alias = |f - k*fs| for the nearest multiple k, exactly.
        for &(f, expect) in &[
            (0.0, 0.0),
            (10.0, 10.0),
            (50.0, 50.0),
            (60.0, 40.0),
            (90.0, 10.0),
            (100.0, 0.0),
            (110.0, 10.0),
            (190.0, 10.0),
            (210.0, 10.0),
        ] {
            assert_eq!(alias_frequency(f, fs), expect, "f = {f}");
        }
    }

    #[test]
    fn alias_frequency_is_periodic_in_fs() {
        let fs = 44100.0;
        for f in [0.0, 1234.5, 20000.0, 22049.9] {
            for k in 1..5 {
                assert!(
                    (alias_frequency(f + k as f64 * fs, fs) - alias_frequency(f, fs)).abs() < 1e-9,
                    "f = {f}, k = {k}"
                );
            }
        }
    }

    #[test]
    fn aliased_tone_agrees_with_the_true_tone_at_every_sample() {
        // 9 Hz sampled at 10 Hz is indistinguishable from 1 Hz.
        let (f, fs) = (9.0, 10.0);
        let fa = alias_frequency(f, fs);
        assert_eq!(fa, 1.0);
        let folds = (f / (0.5 * fs)).floor() as i64;
        let sign = if folds % 2 == 0 { 1.0 } else { -1.0 };
        for n in 0..40 {
            let t = n as f64 / fs;
            let true_v = (std::f64::consts::TAU * f * t).sin();
            let alias_v = (sign * std::f64::consts::TAU * fa * t).sin();
            assert!(
                (true_v - alias_v).abs() < 1e-9,
                "n = {n}: {true_v} vs {alias_v}"
            );
        }
    }

    #[test]
    fn sinc_reconstruction_is_exact_at_sample_instants() {
        let fs = 20.0;
        let f = 3.0;
        let s = Samples::of(|t| (std::f64::consts::TAU * f * t).sin(), 0.0, 4.0, fs);
        for n in 0..s.len() {
            let got = s.sinc_interpolate(s.time(n));
            assert!((got - s.value(n)).abs() < 1e-9, "n = {n}");
        }
    }

    #[test]
    fn sinc_reconstruction_recovers_a_band_limited_tone_between_samples() {
        // 1 Hz well under Nyquist (fs = 20), long record so truncation is mild.
        let fs = 20.0;
        let sig = |t: f64| (std::f64::consts::TAU * t).sin();
        let s = Samples::of(sig, -20.0, 20.0, fs);
        let mut worst: f64 = 0.0;
        for i in 0..200 {
            let t = i as f64 / 200.0; // one period, mid-record
            worst = worst.max((s.sinc_interpolate(t) - sig(t)).abs());
        }
        assert!(worst < 2e-3, "worst inter-sample error {worst}");
    }

    #[test]
    fn zero_order_hold_is_piecewise_constant() {
        let s = Samples::new(vec![1.0, 2.0, 3.0], 0.0, 4.0);
        for (t, expect) in [
            (0.0, 1.0),
            (0.2, 1.0),
            (0.25, 2.0),
            (0.49, 2.0),
            (0.5, 3.0),
            (0.9, 3.0),
        ] {
            assert_eq!(s.zero_order_hold(t), expect, "t = {t}");
        }
    }

    #[test]
    fn nyquist_predicate_matches_the_alias_fold() {
        // Strictly below Nyquist the fold is the identity; above it, it is not.
        // (Exactly at Nyquist, `f` is its own alias but is not representable.)
        for f in [1.0, 24.0, 24.999] {
            assert!(is_below_nyquist(f, 50.0) && alias_frequency(f, 50.0) == f);
        }
        for f in [30.0, 49.0, 51.0] {
            assert!(!is_below_nyquist(f, 50.0) && alias_frequency(f, 50.0) != f);
        }
    }
}
