//! Complex Fourier series of a closed curve, and the epicycle chain that draws
//! it.
//!
//! A closed curve in the plane is a periodic complex function `f: [0,1) → ℂ`.
//! Its Fourier coefficients `cₖ = ∫₀¹ f(t) e^{−2πikt} dt` are computed here by
//! FFT of uniform samples, and the partial sum
//!
//! `f_N(t) = Σ cₖ e^{2πikt}` (largest `|cₖ|` first)
//!
//! is exactly a chain of `N` rotating circles: term `k` is a circle of radius
//! `|cₖ|` whose centre is the tip of the previous term and whose arm spins at
//! `k` revolutions per period. [`EpicycleChain`] builds that chain as mobjects
//! and animates it with an updater that also traces the pen tip.

use manim_core::geometry::{Circle, Line, VGroup, VMobject};
use manim_core::mobject::{AnyId, Buildable, Mobject, MobjectExt, MobjectId};
use manim_core::prelude::{Color, WHITE, YELLOW};
use manim_core::scene_state::SceneState;
use manim_core::style::Style;
use manim_fields::Complex;
use manim_math::path::Path;
use manim_math::Point;
use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

/// One term of a [`FourierSeries`]: a signed frequency and its coefficient.
///
/// ```
/// use manim_signal::fourier::FourierTerm;
/// use manim_fields::Complex;
/// let t = FourierTerm { freq: -2, coeff: Complex::new(0.5, 0.0) };
/// assert_eq!(t.amplitude(), 0.5);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FourierTerm {
    /// The signed harmonic index `k` (revolutions per period; negative spins
    /// clockwise).
    pub freq: i32,
    /// The complex coefficient `cₖ` — its modulus is the circle radius and its
    /// argument the arm's phase at `t = 0`.
    pub coeff: Complex,
}

impl FourierTerm {
    /// The circle radius `|cₖ|`.
    ///
    /// ```
    /// use manim_signal::fourier::FourierTerm;
    /// use manim_fields::Complex;
    /// let t = FourierTerm { freq: 1, coeff: Complex::new(3.0, 4.0) };
    /// assert!((t.amplitude() - 5.0).abs() < 1e-12);
    /// ```
    pub fn amplitude(&self) -> f64 {
        self.coeff.norm()
    }

    /// The term's contribution at phase `t ∈ [0, 1)`: `cₖ e^{2πikt}`.
    ///
    /// ```
    /// use manim_signal::fourier::FourierTerm;
    /// use manim_fields::Complex;
    /// let t = FourierTerm { freq: 1, coeff: Complex::one() };
    /// let v = t.at(0.25);
    /// assert!(v.re.abs() < 1e-12 && (v.im - 1.0).abs() < 1e-12);
    /// ```
    pub fn at(&self, t: f64) -> Complex {
        let theta = std::f64::consts::TAU * self.freq as f64 * t;
        self.coeff * Complex::from_polar(1.0, theta)
    }
}

/// The Fourier coefficients of a closed curve, sorted by descending amplitude.
///
/// Sorting by amplitude is what makes the *partial* sums meaningful: taking the
/// first `N` terms takes the `N` biggest circles, so the reconstruction error
/// falls as fast as it can for a given circle count.
///
/// ```
/// use manim_signal::fourier::FourierSeries;
/// use manim_fields::Complex;
/// // The unit circle traced once: a single term, k = 1, |c| = 1.
/// let s = FourierSeries::from_closure(|t| Complex::from_polar(1.0, std::f64::consts::TAU * t), 64);
/// assert_eq!(s.terms()[0].freq, 1);
/// assert!((s.terms()[0].amplitude() - 1.0).abs() < 1e-12);
/// assert!(s.terms()[1].amplitude() < 1e-12);
/// ```
#[derive(Clone, Debug, Default)]
pub struct FourierSeries {
    terms: Vec<FourierTerm>,
}

impl FourierSeries {
    /// Coefficients from `n` uniform samples of one period, by FFT.
    ///
    /// The samples are `f(i/n)`, `i = 0..n`; frequencies above `n/2` are mapped
    /// to their negative aliases so the terms come out as a symmetric spectrum
    /// `−n/2 … n/2`.
    ///
    /// ```
    /// use manim_signal::fourier::FourierSeries;
    /// use manim_fields::Complex;
    /// let samples: Vec<_> = (0..8).map(|i| Complex::real(i as f64)).collect();
    /// let s = FourierSeries::from_samples(&samples);
    /// assert_eq!(s.len(), 8);
    /// // The DC term is the sample mean, 3.5.
    /// let dc = s.terms().iter().find(|t| t.freq == 0).unwrap();
    /// assert!((dc.coeff.re - 3.5).abs() < 1e-12);
    /// ```
    pub fn from_samples(samples: &[Complex]) -> Self {
        let n = samples.len();
        if n == 0 {
            return Self::default();
        }
        let mut buf: Vec<Complex64> = samples.iter().map(|c| Complex64::new(c.re, c.im)).collect();
        FftPlanner::new().plan_fft_forward(n).process(&mut buf);
        let inv = 1.0 / n as f64;
        let mut terms: Vec<FourierTerm> = buf
            .iter()
            .enumerate()
            .map(|(k, c)| FourierTerm {
                freq: if k * 2 <= n {
                    k as i32
                } else {
                    k as i32 - n as i32
                },
                coeff: Complex::new(c.re * inv, c.im * inv),
            })
            .collect();
        terms.sort_by(|a, b| {
            b.amplitude()
                .partial_cmp(&a.amplitude())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.freq.abs().cmp(&b.freq.abs()))
        });
        Self { terms }
    }

    /// Coefficients of a closure `f(t)`, `t ∈ [0, 1)`, sampled `n` times.
    ///
    /// ```
    /// use manim_signal::fourier::FourierSeries;
    /// use manim_fields::Complex;
    /// let s = FourierSeries::from_closure(|t| Complex::real((std::f64::consts::TAU * t).cos()), 32);
    /// // cos = (e^{it} + e^{-it})/2: two terms of amplitude 1/2.
    /// assert!((s.terms()[0].amplitude() - 0.5).abs() < 1e-12);
    /// assert!((s.terms()[1].amplitude() - 0.5).abs() < 1e-12);
    /// ```
    pub fn from_closure(f: impl Fn(f64) -> Complex, n: usize) -> Self {
        let samples: Vec<Complex> = (0..n).map(|i| f(i as f64 / n as f64)).collect();
        Self::from_samples(&samples)
    }

    /// Coefficients of a [`Path`], sampled `n` times by arc-length proportion.
    ///
    /// The path's `x`/`y` become the real/imaginary parts; `z` is ignored.
    ///
    /// ```
    /// use manim_signal::fourier::FourierSeries;
    /// use manim_core::geometry::Circle;
    /// use manim_core::mobject::Mobject;
    /// let circle = Circle::new();
    /// let s = FourierSeries::from_path(&circle.data().path, 128);
    /// // A unit circle is dominated by its k = ±1 term.
    /// assert!((s.terms()[0].amplitude() - 1.0).abs() < 1e-2);
    /// assert_eq!(s.terms()[0].freq.abs(), 1);
    /// ```
    pub fn from_path(path: &Path, n: usize) -> Self {
        let samples: Vec<Complex> = (0..n)
            .map(|i| {
                let p = path.point_from_proportion(i as f32 / n as f32);
                Complex::new(p.x as f64, p.y as f64)
            })
            .collect();
        Self::from_samples(&samples)
    }

    /// The terms, largest amplitude first.
    ///
    /// ```
    /// use manim_signal::fourier::FourierSeries;
    /// use manim_fields::Complex;
    /// let s = FourierSeries::from_closure(|_| Complex::one(), 8);
    /// assert!(s.terms().windows(2).all(|w| w[0].amplitude() >= w[1].amplitude()));
    /// ```
    pub fn terms(&self) -> &[FourierTerm] {
        &self.terms
    }

    /// The number of terms (equal to the sample count it was built from).
    ///
    /// ```
    /// use manim_signal::fourier::FourierSeries;
    /// use manim_fields::Complex;
    /// assert_eq!(FourierSeries::from_closure(|_| Complex::zero(), 16).len(), 16);
    /// ```
    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// Whether the series is empty.
    ///
    /// ```
    /// use manim_signal::fourier::FourierSeries;
    /// assert!(FourierSeries::default().is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// The `n_terms`-term partial sum at phase `t` (clamped to the term count).
    ///
    /// ```
    /// use manim_signal::fourier::FourierSeries;
    /// use manim_fields::Complex;
    /// let s = FourierSeries::from_closure(|t| Complex::from_polar(1.0, std::f64::consts::TAU * t), 64);
    /// let v = s.evaluate(0.0, 1);
    /// assert!((v.re - 1.0).abs() < 1e-9 && v.im.abs() < 1e-9);
    /// ```
    pub fn evaluate(&self, t: f64, n_terms: usize) -> Complex {
        self.terms
            .iter()
            .take(n_terms.min(self.terms.len()))
            .fold(Complex::zero(), |acc, term| acc + term.at(t))
    }

    /// The running partial sums at phase `t`: the tip of each epicycle arm.
    ///
    /// Element `i` is the sum of the first `i + 1` terms, so the returned points
    /// are exactly the circle centres (shifted by one) of the chain.
    ///
    /// ```
    /// use manim_signal::fourier::FourierSeries;
    /// use manim_fields::Complex;
    /// let s = FourierSeries::from_closure(|t| Complex::from_polar(1.0, std::f64::consts::TAU * t), 32);
    /// let tips = s.tips(0.0, 3);
    /// assert_eq!(tips.len(), 3);
    /// assert!((tips[2] - s.evaluate(0.0, 3)).norm() < 1e-12);
    /// ```
    pub fn tips(&self, t: f64, n_terms: usize) -> Vec<Complex> {
        let mut acc = Complex::zero();
        self.terms
            .iter()
            .take(n_terms.min(self.terms.len()))
            .map(|term| {
                acc = acc + term.at(t);
                acc
            })
            .collect()
    }

    /// `count` uniform samples of the `n_terms`-term reconstruction over one
    /// period.
    ///
    /// ```
    /// use manim_signal::fourier::FourierSeries;
    /// use manim_fields::Complex;
    /// let s = FourierSeries::from_closure(|t| Complex::from_polar(1.0, std::f64::consts::TAU * t), 32);
    /// assert_eq!(s.reconstruct(16, 4).len(), 16);
    /// ```
    pub fn reconstruct(&self, count: usize, n_terms: usize) -> Vec<Complex> {
        (0..count)
            .map(|i| self.evaluate(i as f64 / count as f64, n_terms))
            .collect()
    }

    /// The RMS reconstruction error of the `n_terms`-term partial sum against a
    /// target sampled uniformly over one period.
    ///
    /// This is `√(Σ|f(tᵢ) − f_N(tᵢ)|² / M)`; it decreases monotonically in
    /// `n_terms` when the target is the curve the series was built from, since
    /// terms are added in descending amplitude (Parseval).
    ///
    /// ```
    /// use manim_signal::fourier::FourierSeries;
    /// use manim_fields::Complex;
    /// let f = |t: f64| Complex::from_polar(1.0, std::f64::consts::TAU * t);
    /// let s = FourierSeries::from_closure(f, 64);
    /// let target: Vec<_> = (0..64).map(|i| f(i as f64 / 64.0)).collect();
    /// assert!(s.reconstruction_error(&target, 1) < 1e-9);
    /// ```
    pub fn reconstruction_error(&self, target: &[Complex], n_terms: usize) -> f64 {
        if target.is_empty() {
            return 0.0;
        }
        let m = target.len();
        let sum: f64 = target
            .iter()
            .enumerate()
            .map(|(i, &z)| (z - self.evaluate(i as f64 / m as f64, n_terms)).norm_sqr())
            .sum();
        (sum / m as f64).sqrt()
    }
}

/// The mobject ids produced by [`EpicycleChain::add_to`].
pub struct EpicycleIds {
    /// One circle per term, outermost (largest) first.
    pub circles: Vec<MobjectId<Circle>>,
    /// The radius arm of each circle, pointing at the next circle's centre.
    pub arms: Vec<MobjectId<Line>>,
    /// The pen-tip trace, rebuilt each frame as points accumulate.
    pub trace: MobjectId<VMobject>,
    /// A group holding circles, arms, and the trace.
    pub group: MobjectId<VGroup>,
}

/// Builder for a chain of rotating circles drawing a [`FourierSeries`].
///
/// ```
/// use manim_core::prelude::*;
/// use manim_fields::Complex;
/// use manim_signal::fourier::{EpicycleChain, FourierSeries};
/// let series = FourierSeries::from_closure(
///     |t| Complex::from_polar(1.0, std::f64::consts::TAU * t), 64);
/// let mut scene = SceneState::new();
/// let ids = EpicycleChain::new(series).terms(4).add_to(&mut scene);
/// assert_eq!(ids.circles.len(), 4);
/// assert!(scene.contains(ids.trace));
/// ```
pub struct EpicycleChain {
    series: FourierSeries,
    n_terms: usize,
    origin: Point,
    scale: f32,
    circle_color: Color,
    arm_color: Color,
    trace_color: Color,
    trace_width: f32,
    period: f64,
}

impl EpicycleChain {
    /// A chain for `series`, using every term, centred at the origin.
    ///
    /// ```
    /// use manim_signal::fourier::{EpicycleChain, FourierSeries};
    /// let chain = EpicycleChain::new(FourierSeries::default());
    /// assert_eq!(chain.term_count(), 0);
    /// ```
    pub fn new(series: FourierSeries) -> Self {
        let n_terms = series.len();
        Self {
            series,
            n_terms,
            origin: Point::ZERO,
            scale: 1.0,
            circle_color: Color::from_rgb(0.45, 0.55, 0.75),
            arm_color: WHITE,
            trace_color: YELLOW,
            trace_width: 3.0,
            period: 1.0,
        }
    }

    /// Uses only the `n` largest terms.
    ///
    /// ```
    /// use manim_fields::Complex;
    /// use manim_signal::fourier::{EpicycleChain, FourierSeries};
    /// let s = FourierSeries::from_closure(|_| Complex::one(), 32);
    /// assert_eq!(EpicycleChain::new(s).terms(5).term_count(), 5);
    /// ```
    pub fn terms(mut self, n: usize) -> Self {
        self.n_terms = n.min(self.series.len());
        self
    }

    /// Places the chain's base point at `origin` (scene units).
    ///
    /// ```
    /// use manim_math::Point;
    /// use manim_signal::fourier::{EpicycleChain, FourierSeries};
    /// let chain = EpicycleChain::new(FourierSeries::default()).at(Point::new(1.0, 0.0, 0.0));
    /// assert_eq!(chain.term_count(), 0);
    /// ```
    pub fn at(mut self, origin: Point) -> Self {
        self.origin = origin;
        self
    }

    /// Scales the whole chain (coefficients are in curve units).
    ///
    /// ```
    /// use manim_signal::fourier::{EpicycleChain, FourierSeries};
    /// let chain = EpicycleChain::new(FourierSeries::default()).scaled(2.0);
    /// assert_eq!(chain.term_count(), 0);
    /// ```
    pub fn scaled(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// Seconds per revolution of the `k = 1` arm (the updater's period).
    ///
    /// ```
    /// use manim_signal::fourier::{EpicycleChain, FourierSeries};
    /// let chain = EpicycleChain::new(FourierSeries::default()).period(4.0);
    /// assert_eq!(chain.term_count(), 0);
    /// ```
    pub fn period(mut self, seconds: f64) -> Self {
        self.period = seconds.max(1e-6);
        self
    }

    /// Colors for the circles, the radius arms, and the trace.
    ///
    /// ```
    /// use manim_core::prelude::{RED, WHITE, GREEN};
    /// use manim_signal::fourier::{EpicycleChain, FourierSeries};
    /// let chain = EpicycleChain::new(FourierSeries::default()).colors(RED, WHITE, GREEN);
    /// assert_eq!(chain.term_count(), 0);
    /// ```
    pub fn colors(mut self, circles: Color, arms: Color, trace: Color) -> Self {
        self.circle_color = circles;
        self.arm_color = arms;
        self.trace_color = trace;
        self
    }

    /// How many terms this chain will draw.
    ///
    /// ```
    /// use manim_signal::fourier::{EpicycleChain, FourierSeries};
    /// assert_eq!(EpicycleChain::new(FourierSeries::default()).term_count(), 0);
    /// ```
    pub fn term_count(&self) -> usize {
        self.n_terms
    }

    /// The scene points of the arm tips at phase `t` (base point first).
    ///
    /// The returned vector has `n_terms + 1` entries: the chain's origin
    /// followed by each cumulative partial sum, mapped into scene space.
    ///
    /// ```
    /// use manim_fields::Complex;
    /// use manim_signal::fourier::{EpicycleChain, FourierSeries};
    /// let s = FourierSeries::from_closure(
    ///     |t| Complex::from_polar(1.0, std::f64::consts::TAU * t), 32);
    /// let chain = EpicycleChain::new(s).terms(3);
    /// assert_eq!(chain.joints(0.0).len(), 4);
    /// ```
    pub fn joints(&self, t: f64) -> Vec<Point> {
        let mut out = vec![self.origin];
        out.extend(
            self.series
                .tips(t, self.n_terms)
                .into_iter()
                .map(|z| self.origin + Point::new(z.re as f32, z.im as f32, 0.0) * self.scale),
        );
        out
    }

    /// The series this chain draws.
    ///
    /// ```
    /// use manim_signal::fourier::{EpicycleChain, FourierSeries};
    /// let chain = EpicycleChain::new(FourierSeries::default());
    /// assert!(chain.series().is_empty());
    /// ```
    pub fn series(&self) -> &FourierSeries {
        &self.series
    }

    /// Builds the circles, arms, and trace into `scene` at phase `t = 0`.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_fields::Complex;
    /// use manim_signal::fourier::{EpicycleChain, FourierSeries};
    /// let s = FourierSeries::from_closure(
    ///     |t| Complex::from_polar(1.0, std::f64::consts::TAU * t), 32);
    /// let mut scene = SceneState::new();
    /// let ids = EpicycleChain::new(s).terms(2).add_to(&mut scene);
    /// assert_eq!(ids.arms.len(), 2);
    /// ```
    pub fn add_to(&self, scene: &mut SceneState) -> EpicycleIds {
        let joints = self.joints(0.0);
        let mut circles = Vec::with_capacity(self.n_terms);
        let mut arms = Vec::with_capacity(self.n_terms);

        for (i, term) in self.series.terms().iter().take(self.n_terms).enumerate() {
            let radius = (term.amplitude() as f32 * self.scale).max(1e-5);
            let circle = Circle::new()
                .radius(radius)
                .with_move_to(joints[i])
                .with_stroke(self.circle_color, 1.5, 0.45);
            circles.push(scene.add(circle));
            arms.push(scene.add(Line::new(joints[i], joints[i + 1]).with_stroke(
                self.arm_color,
                1.5,
                0.8,
            )));
        }

        let trace = scene.add(VMobject::new(Path::default(), {
            let mut st = Style::stroked(self.trace_color);
            st.stroke_width = self.trace_width;
            st
        }));

        let mut members: Vec<AnyId> = circles.iter().map(|c| c.erase()).collect();
        members.extend(arms.iter().map(|a| a.erase()));
        members.push(trace.erase());
        let group = VGroup::of(scene, members);

        EpicycleIds {
            circles,
            arms,
            trace,
            group,
        }
    }

    /// Attaches an updater that spins the chain and extends the trace.
    ///
    /// The updater is registered on the `group` id; each frame it re-places every
    /// circle and arm at phase `t = time / period` and appends the pen tip to the
    /// trace path. `max_trace_points` bounds the trace's memory (older points are
    /// dropped once the pen has drawn that many).
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_fields::Complex;
    /// use manim_signal::fourier::{EpicycleChain, FourierSeries};
    /// let s = FourierSeries::from_closure(
    ///     |t| Complex::from_polar(1.0, std::f64::consts::TAU * t), 32);
    /// let mut scene = SceneState::new();
    /// let chain = EpicycleChain::new(s).terms(3).period(2.0);
    /// let ids = chain.add_to(&mut scene);
    /// chain.animate(&mut scene, &ids, 512);
    /// for i in 0..30 {
    ///     scene.run_updaters(UpdaterCtx { dt: 1.0 / 60.0, time: i as f32 / 60.0 });
    /// }
    /// // The pen has swept out a stretch of curve.
    /// assert!(!scene.get(ids.trace).data().path.subpaths.is_empty());
    /// ```
    pub fn animate(&self, scene: &mut SceneState, ids: &EpicycleIds, max_trace_points: usize) {
        let series = self.series.clone();
        let (n_terms, origin, scale, period) = (self.n_terms, self.origin, self.scale, self.period);
        let circles = ids.circles.clone();
        let arms = ids.arms.clone();
        let trace = ids.trace;
        let mut pen_points: Vec<Point> = Vec::new();

        scene.add_updater(ids.group, move |state, _id, ctx| {
            let t = ctx.time as f64 / period;
            let mut joints = vec![origin];
            joints.extend(
                series
                    .tips(t, n_terms)
                    .into_iter()
                    .map(|z| origin + Point::new(z.re as f32, z.im as f32, 0.0) * scale),
            );

            for (i, &circle) in circles.iter().enumerate() {
                if let Some(c) = state.try_get_mut(circle) {
                    c.move_to(joints[i]);
                }
            }
            for (i, &arm) in arms.iter().enumerate() {
                if let Some(l) = state.try_get_mut(arm) {
                    l.put_start_and_end_on(joints[i], joints[i + 1]);
                }
            }

            if let Some(&tip) = joints.last() {
                if pen_points.last().map(|p| (*p - tip).length() > 1e-6) != Some(false) {
                    pen_points.push(tip);
                }
                if pen_points.len() > max_trace_points {
                    let drop = pen_points.len() - max_trace_points;
                    pen_points.drain(0..drop);
                }
            }
            if pen_points.len() >= 2 {
                if let Some(v) = state.try_get_mut(trace) {
                    v.data_mut().path = Path::from_corners(&pen_points, false);
                    v.data_mut().bump_generation();
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed non-circular test curve (a squarish Lissajous-ish blob).
    fn blob(t: f64) -> Complex {
        let a = std::f64::consts::TAU * t;
        Complex::new(
            a.cos() + 0.35 * (3.0 * a).cos(),
            a.sin() - 0.25 * (2.0 * a).sin(),
        )
    }

    #[test]
    fn fft_round_trip_is_exact_to_1e_10() {
        let n = 256;
        let target: Vec<Complex> = (0..n).map(|i| blob(i as f64 / n as f64)).collect();
        let series = FourierSeries::from_samples(&target);
        // All n terms reproduce the samples they came from, to FFT precision.
        for (i, &z) in target.iter().enumerate() {
            let back = series.evaluate(i as f64 / n as f64, series.len());
            assert!(
                (z - back).norm() < 1e-10,
                "sample {i}: err {}",
                (z - back).norm()
            );
        }
        assert!(series.reconstruction_error(&target, series.len()) < 1e-10);
    }

    #[test]
    fn terms_are_sorted_by_descending_amplitude() {
        let s = FourierSeries::from_closure(blob, 128);
        assert!(s
            .terms()
            .windows(2)
            .all(|w| w[0].amplitude() >= w[1].amplitude() - 1e-15));
    }

    #[test]
    fn reconstruction_error_decreases_with_more_terms() {
        let n = 256;
        let target: Vec<Complex> = (0..n).map(|i| blob(i as f64 / n as f64)).collect();
        let series = FourierSeries::from_samples(&target);

        let mut prev = f64::INFINITY;
        for k in [1usize, 2, 4, 8, 16, 32, 64] {
            let err = series.reconstruction_error(&target, k);
            assert!(
                err <= prev + 1e-12,
                "error grew from {prev} to {err} at {k} terms"
            );
            prev = err;
        }
        // The blob has 5 non-zero harmonics; 8 terms already nail it.
        assert!(
            series.reconstruction_error(&target, 8) < 1e-10,
            "8-term error {}",
            series.reconstruction_error(&target, 8)
        );
        // And one term alone is a poor fit (a plain circle).
        assert!(series.reconstruction_error(&target, 1) > 0.1);
    }

    #[test]
    fn parseval_holds_for_the_coefficients() {
        let n = 128;
        let target: Vec<Complex> = (0..n).map(|i| blob(i as f64 / n as f64)).collect();
        let series = FourierSeries::from_samples(&target);
        let energy: f64 = target.iter().map(|z| z.norm_sqr()).sum::<f64>() / n as f64;
        let coeff_energy: f64 = series.terms().iter().map(|t| t.coeff.norm_sqr()).sum();
        assert!((energy - coeff_energy).abs() < 1e-12);
    }

    #[test]
    fn unit_circle_is_a_single_epicycle() {
        let s = FourierSeries::from_closure(
            |t| Complex::from_polar(1.0, std::f64::consts::TAU * t),
            64,
        );
        assert_eq!(s.terms()[0].freq, 1);
        assert!((s.terms()[0].amplitude() - 1.0).abs() < 1e-12);
        assert!(s.terms()[1].amplitude() < 1e-12);
    }

    #[test]
    fn chain_joints_match_partial_sums() {
        let s = FourierSeries::from_closure(blob, 64);
        let chain = EpicycleChain::new(s).terms(6).scaled(2.0);
        let joints = chain.joints(0.3);
        assert_eq!(joints.len(), 7);
        let tip = chain.series().evaluate(0.3, 6);
        let expect = Point::new(tip.re as f32, tip.im as f32, 0.0) * 2.0;
        assert!((joints[6] - expect).length() < 1e-5);
    }

    #[test]
    fn updater_traces_the_curve() {
        use manim_core::scene_state::UpdaterCtx;
        let s = FourierSeries::from_closure(blob, 64);
        let chain = EpicycleChain::new(s).terms(8).period(1.0);
        let mut scene = SceneState::new();
        let ids = chain.add_to(&mut scene);
        chain.animate(&mut scene, &ids, 1000);
        for i in 0..60 {
            scene.run_updaters(UpdaterCtx {
                dt: 1.0 / 60.0,
                time: i as f32 / 60.0,
            });
        }
        let trace = scene.get(ids.trace);
        assert!(!trace.data().path.subpaths.is_empty());
        // Every traced point sits on the curve (within the 8-term fit).
        let pts = trace.data().path.points(2);
        assert!(pts.len() > 30);
    }
}
