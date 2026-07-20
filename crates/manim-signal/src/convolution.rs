//! Discrete convolution, and the sliding-kernel picture of it.
//!
//! `(x ∗ h)[n] = Σₖ x[k] · h[n−k]`: for each output index the kernel is
//! **flipped** and slid to position `n`, multiplied against the input sample by
//! sample, and the products are summed. [`ConvolutionBuildup`] draws exactly
//! that — input stems, the flipped kernel at the current shift, one product bar
//! per overlapping tap, and the output stems accumulated so far.

use manim_core::geometry::{Dot, Line, VGroup, VMobject};
use manim_core::graphing::Axes;
use manim_core::mobject::{AnyId, Buildable, MobjectId};
use manim_core::prelude::{Color, Point, BLUE, GREEN, YELLOW};
use manim_core::scene_state::SceneState;
use manim_core::style::Style;
use manim_math::path::Path;
use rustfft::num_complex::Complex64;
use rustfft::FftPlanner;

/// Direct discrete convolution, `O(|x|·|h|)`, of two finite sequences.
///
/// The result has length `|x| + |h| − 1` (empty if either input is empty).
///
/// ```
/// use manim_signal::convolution::convolve;
/// // Convolving with a unit impulse is the identity.
/// assert_eq!(convolve(&[1.0, 2.0, 3.0], &[1.0]), vec![1.0, 2.0, 3.0]);
/// // Polynomial multiplication: (1 + x)(1 + x) = 1 + 2x + x².
/// assert_eq!(convolve(&[1.0, 1.0], &[1.0, 1.0]), vec![1.0, 2.0, 1.0]);
/// ```
pub fn convolve(x: &[f64], h: &[f64]) -> Vec<f64> {
    if x.is_empty() || h.is_empty() {
        return Vec::new();
    }
    let n = x.len() + h.len() - 1;
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(h.len() - 1);
            let hi = i.min(x.len() - 1);
            (lo..=hi).map(|k| x[k] * h[i - k]).sum()
        })
        .collect()
}

/// The same convolution by FFT — `O(N log N)`, and the reason the frequency
/// domain is worth the trip for long kernels.
///
/// Agrees with [`convolve`] to floating-point round-off.
///
/// ```
/// use manim_signal::convolution::{convolve, convolve_fft};
/// let x = [1.0, -2.0, 0.5, 3.0];
/// let h = [0.25, 0.5, 0.25];
/// let (a, b) = (convolve(&x, &h), convolve_fft(&x, &h));
/// assert!(a.iter().zip(&b).all(|(p, q)| (p - q).abs() < 1e-12));
/// ```
pub fn convolve_fft(x: &[f64], h: &[f64]) -> Vec<f64> {
    if x.is_empty() || h.is_empty() {
        return Vec::new();
    }
    let n_out = x.len() + h.len() - 1;
    let n = n_out.next_power_of_two();
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);

    let mut xa: Vec<Complex64> = x.iter().map(|&v| Complex64::new(v, 0.0)).collect();
    let mut ha: Vec<Complex64> = h.iter().map(|&v| Complex64::new(v, 0.0)).collect();
    xa.resize(n, Complex64::new(0.0, 0.0));
    ha.resize(n, Complex64::new(0.0, 0.0));
    fft.process(&mut xa);
    fft.process(&mut ha);
    for (a, b) in xa.iter_mut().zip(&ha) {
        *a *= b;
    }
    ifft.process(&mut xa);
    let inv = 1.0 / n as f64;
    xa.into_iter().take(n_out).map(|c| c.re * inv).collect()
}

/// The mobject ids of a [`ConvolutionBuildup`] figure.
///
/// The kernel and product groups are rebuilt on every
/// [`ConvolutionBuildup::set_shift`], so their ids change; the input and output
/// groups persist.
pub struct ConvolutionIds {
    /// Stems of the input sequence `x` (static).
    pub input: MobjectId<VGroup>,
    /// Stems of the flipped, shifted kernel `h[n−k]` at the current shift.
    pub kernel: MobjectId<VGroup>,
    /// Product bars `x[k]·h[n−k]`, one per overlapping tap.
    pub products: MobjectId<VGroup>,
    /// Output stems revealed so far.
    pub output: MobjectId<VGroup>,
    /// The current shift `n`.
    pub shift: usize,
}

/// The sliding-kernel construction of a discrete convolution as mobjects on a
/// pair of [`Axes`].
///
/// ```
/// use manim_core::prelude::*;
/// use manim_signal::convolution::ConvolutionBuildup;
/// let conv = ConvolutionBuildup::new(vec![1.0, 2.0, 1.0], vec![0.5, 0.5]);
/// assert_eq!(conv.output(), vec![0.5, 1.5, 1.5, 0.5]);
/// let axes = Axes::new([-1.0, 6.0, 1.0], [-1.0, 3.0, 1.0]);
/// let mut scene = SceneState::new();
/// let ids = conv.add_to(&mut scene, &axes);
/// assert!(scene.contains(ids.input));
/// ```
#[derive(Clone, Debug)]
pub struct ConvolutionBuildup {
    x: Vec<f64>,
    h: Vec<f64>,
    input_color: Color,
    kernel_color: Color,
    output_color: Color,
}

impl ConvolutionBuildup {
    /// A buildup for input `x` and kernel `h`.
    ///
    /// ```
    /// use manim_signal::convolution::ConvolutionBuildup;
    /// let c = ConvolutionBuildup::new(vec![1.0], vec![1.0, 1.0]);
    /// assert_eq!(c.output_len(), 2);
    /// ```
    pub fn new(x: Vec<f64>, h: Vec<f64>) -> Self {
        Self {
            x,
            h,
            input_color: BLUE,
            kernel_color: GREEN,
            output_color: YELLOW,
        }
    }

    /// Overrides the input / kernel / output colors.
    ///
    /// ```
    /// use manim_core::prelude::{RED, GREEN, WHITE};
    /// use manim_signal::convolution::ConvolutionBuildup;
    /// let c = ConvolutionBuildup::new(vec![1.0], vec![1.0]).colors(RED, GREEN, WHITE);
    /// assert_eq!(c.output_len(), 1);
    /// ```
    pub fn colors(mut self, input: Color, kernel: Color, output: Color) -> Self {
        self.input_color = input;
        self.kernel_color = kernel;
        self.output_color = output;
        self
    }

    /// The input sequence.
    ///
    /// ```
    /// use manim_signal::convolution::ConvolutionBuildup;
    /// assert_eq!(ConvolutionBuildup::new(vec![2.0], vec![1.0]).input(), &[2.0]);
    /// ```
    pub fn input(&self) -> &[f64] {
        &self.x
    }

    /// The kernel.
    ///
    /// ```
    /// use manim_signal::convolution::ConvolutionBuildup;
    /// assert_eq!(ConvolutionBuildup::new(vec![2.0], vec![1.0]).kernel(), &[1.0]);
    /// ```
    pub fn kernel(&self) -> &[f64] {
        &self.h
    }

    /// The full convolution result.
    ///
    /// ```
    /// use manim_signal::convolution::ConvolutionBuildup;
    /// let c = ConvolutionBuildup::new(vec![1.0, 0.0, -1.0], vec![1.0, 1.0]);
    /// assert_eq!(c.output(), vec![1.0, 1.0, -1.0, -1.0]);
    /// ```
    pub fn output(&self) -> Vec<f64> {
        convolve(&self.x, &self.h)
    }

    /// The number of output samples, `|x| + |h| − 1`.
    ///
    /// ```
    /// use manim_signal::convolution::ConvolutionBuildup;
    /// assert_eq!(ConvolutionBuildup::new(vec![0.0; 5], vec![0.0; 3]).output_len(), 7);
    /// ```
    pub fn output_len(&self) -> usize {
        if self.x.is_empty() || self.h.is_empty() {
            0
        } else {
            self.x.len() + self.h.len() - 1
        }
    }

    /// The individual product terms contributing to output `n`, as
    /// `(k, x[k]·h[n−k])` over the overlapping input indices.
    ///
    /// ```
    /// use manim_signal::convolution::ConvolutionBuildup;
    /// let c = ConvolutionBuildup::new(vec![1.0, 2.0, 3.0], vec![1.0, 1.0]);
    /// // Output 1 is x[0]h[1] + x[1]h[0] = 1 + 2.
    /// assert_eq!(c.products_at(1), vec![(0, 1.0), (1, 2.0)]);
    /// ```
    pub fn products_at(&self, n: usize) -> Vec<(usize, f64)> {
        if self.x.is_empty() || self.h.is_empty() {
            return Vec::new();
        }
        let lo = n.saturating_sub(self.h.len() - 1);
        let hi = n.min(self.x.len() - 1);
        if lo > hi {
            return Vec::new();
        }
        (lo..=hi).map(|k| (k, self.x[k] * self.h[n - k])).collect()
    }

    /// The value of output `n`: the sum of [`products_at`](Self::products_at).
    ///
    /// ```
    /// use manim_signal::convolution::ConvolutionBuildup;
    /// let c = ConvolutionBuildup::new(vec![1.0, 2.0, 3.0], vec![1.0, 1.0]);
    /// assert_eq!(c.value_at(1), 3.0);
    /// ```
    pub fn value_at(&self, n: usize) -> f64 {
        self.products_at(n).iter().map(|&(_, v)| v).sum()
    }

    /// Builds the figure at shift `n = 0`.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_signal::convolution::ConvolutionBuildup;
    /// let conv = ConvolutionBuildup::new(vec![1.0, 2.0], vec![1.0, 1.0]);
    /// let axes = Axes::new([-1.0, 5.0, 1.0], [-1.0, 3.0, 1.0]);
    /// let mut scene = SceneState::new();
    /// let ids = conv.add_to(&mut scene, &axes);
    /// assert_eq!(ids.shift, 0);
    /// ```
    pub fn add_to(&self, scene: &mut SceneState, axes: &Axes) -> ConvolutionIds {
        let input = self.stems(scene, axes, &self.x, 0, self.input_color);
        let kernel = self.kernel_group(scene, axes, 0);
        let products = self.product_group(scene, axes, 0);
        let output = self.output_group(scene, axes, 0);
        ConvolutionIds {
            input,
            kernel,
            products,
            output,
            shift: 0,
        }
    }

    /// Moves the figure to shift `n`: re-flips the kernel, redraws the product
    /// bars, and reveals output samples `0..=n`.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_signal::convolution::ConvolutionBuildup;
    /// let conv = ConvolutionBuildup::new(vec![1.0, 2.0], vec![1.0, 1.0]);
    /// let axes = Axes::new([-1.0, 5.0, 1.0], [-1.0, 3.0, 1.0]);
    /// let mut scene = SceneState::new();
    /// let mut ids = conv.add_to(&mut scene, &axes);
    /// conv.set_shift(&mut scene, &axes, &mut ids, 2);
    /// assert_eq!(ids.shift, 2);
    /// ```
    pub fn set_shift(
        &self,
        scene: &mut SceneState,
        axes: &Axes,
        ids: &mut ConvolutionIds,
        n: usize,
    ) {
        scene.remove(ids.kernel);
        scene.remove(ids.products);
        scene.remove(ids.output);
        ids.kernel = self.kernel_group(scene, axes, n);
        ids.products = self.product_group(scene, axes, n);
        ids.output = self.output_group(scene, axes, n);
        ids.shift = n;
    }

    /// Stems for `values`, placed at integer positions offset by `origin`.
    fn stems(
        &self,
        scene: &mut SceneState,
        axes: &Axes,
        values: &[f64],
        origin: i64,
        color: Color,
    ) -> MobjectId<VGroup> {
        let mut members: Vec<AnyId> = Vec::new();
        for (i, &v) in values.iter().enumerate() {
            let x = (origin + i as i64) as f32;
            let base = axes.c2p(x, 0.0);
            let top = axes.c2p(x, v as f32);
            members.push(
                scene
                    .add(Line::new(base, top).with_stroke(color, 3.0, 1.0))
                    .erase(),
            );
            members.push(
                scene
                    .add(Dot::at(top).radius(0.05).with_fill(color, 1.0))
                    .erase(),
            );
        }
        VGroup::of(scene, members)
    }

    /// The flipped kernel `h[n−k]` drawn at input positions `k`.
    fn kernel_group(&self, scene: &mut SceneState, axes: &Axes, n: usize) -> MobjectId<VGroup> {
        let flipped: Vec<f64> = self.h.iter().rev().copied().collect();
        // h[n−k] is non-zero for k in [n − (|h|−1), n]; the leftmost k is the
        // reversed kernel's first entry.
        let origin = n as i64 - (self.h.len() as i64 - 1);
        self.stems(scene, axes, &flipped, origin, self.kernel_color)
    }

    /// Filled bars for the products contributing to output `n`.
    fn product_group(&self, scene: &mut SceneState, axes: &Axes, n: usize) -> MobjectId<VGroup> {
        let mut members: Vec<AnyId> = Vec::new();
        for (k, v) in self.products_at(n) {
            let (x, half) = (k as f32, 0.22_f32);
            let quad = [
                axes.c2p(x - half, 0.0),
                axes.c2p(x + half, 0.0),
                axes.c2p(x + half, v as f32),
                axes.c2p(x - half, v as f32),
            ];
            let bar = VMobject::new(Path::from_corners(&quad, true), {
                let mut st = Style::filled(self.output_color);
                st.fill_opacity = 0.35;
                st
            });
            members.push(scene.add(bar).erase());
        }
        VGroup::of(scene, members)
    }

    /// Output stems for indices `0..=n`.
    fn output_group(&self, scene: &mut SceneState, axes: &Axes, n: usize) -> MobjectId<VGroup> {
        let full = self.output();
        let take = (n + 1).min(full.len());
        self.stems(scene, axes, &full[..take], 0, self.output_color)
    }
}

/// The scene point of an output stem's tip, for callers annotating the figure.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_signal::convolution::{output_tip, ConvolutionBuildup};
/// let axes = Axes::new([-1.0, 5.0, 1.0], [-1.0, 3.0, 1.0]);
/// let c = ConvolutionBuildup::new(vec![1.0, 2.0], vec![1.0, 1.0]);
/// let p = output_tip(&axes, &c, 1);
/// assert!((p - axes.c2p(1.0, 3.0)).length() < 1e-5);
/// ```
pub fn output_tip(axes: &Axes, conv: &ConvolutionBuildup, n: usize) -> Point {
    axes.c2p(n as f32, conv.value_at(n) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convolution_matches_the_direct_sum_definition() {
        let x = [0.3, -1.2, 4.0, 0.0, 2.5, -0.7];
        let h = [1.0, 0.5, -0.25, 0.125];
        let got = convolve(&x, &h);
        assert_eq!(got.len(), x.len() + h.len() - 1);
        for (n, &y) in got.iter().enumerate() {
            // The definition, written out with explicit bounds checks.
            let mut sum = 0.0;
            for (k, &xk) in x.iter().enumerate() {
                if n >= k && n - k < h.len() {
                    sum += xk * h[n - k];
                }
            }
            assert!((y - sum).abs() < 1e-15, "n = {n}: {y} vs {sum}");
        }
    }

    #[test]
    fn fft_convolution_agrees_with_the_direct_one() {
        let x: Vec<f64> = (0..37).map(|i| (i as f64 * 0.7).sin()).collect();
        let h: Vec<f64> = (0..11).map(|i| 1.0 / (1.0 + i as f64)).collect();
        let a = convolve(&x, &h);
        let b = convolve_fft(&x, &h);
        assert_eq!(a.len(), b.len());
        for (i, (p, q)) in a.iter().zip(&b).enumerate() {
            assert!((p - q).abs() < 1e-12, "n = {i}: {p} vs {q}");
        }
    }

    #[test]
    fn convolution_is_commutative_and_impulse_is_the_identity() {
        let x = [1.0, -3.0, 2.5];
        let h = [0.25, 0.5, 0.25, -1.0];
        let a = convolve(&x, &h);
        let b = convolve(&h, &x);
        assert!(a.iter().zip(&b).all(|(p, q)| (p - q).abs() < 1e-15));
        assert_eq!(convolve(&x, &[1.0]), x.to_vec());
    }

    #[test]
    fn convolution_sums_multiply() {
        // Σ(x∗h) = (Σx)(Σh) — the DC gain of a cascade multiplies.
        let x = [1.0, 2.0, 3.0, 4.0];
        let h = [0.5, -0.25, 2.0];
        let y = convolve(&x, &h);
        let expect: f64 = x.iter().sum::<f64>() * h.iter().sum::<f64>();
        assert!((y.iter().sum::<f64>() - expect).abs() < 1e-12);
    }

    #[test]
    fn per_shift_products_reconstruct_the_output() {
        let conv = ConvolutionBuildup::new(vec![0.5, -1.0, 2.0, 3.0], vec![1.0, 0.5, 0.25]);
        let full = conv.output();
        for (n, want) in full.iter().enumerate() {
            assert!((conv.value_at(n) - want).abs() < 1e-15, "n = {n}");
            // The kernel only ever overlaps |h| taps.
            assert!(conv.products_at(n).len() <= conv.kernel().len());
        }
        // Past the end there is nothing left to accumulate.
        assert!(conv.products_at(conv.output_len()).is_empty());
    }

    #[test]
    fn buildup_geometry_grows_with_the_shift() {
        let conv = ConvolutionBuildup::new(vec![1.0, 2.0, 1.0], vec![0.5, 0.5]);
        let axes = Axes::new([-2.0, 6.0, 1.0], [-1.0, 3.0, 1.0]);
        let mut scene = SceneState::new();
        let mut ids = conv.add_to(&mut scene, &axes);
        let n0 = scene.family(ids.output.erase()).len();
        conv.set_shift(&mut scene, &axes, &mut ids, 3);
        let n3 = scene.family(ids.output.erase()).len();
        assert!(n3 > n0, "output stems should accumulate: {n0} → {n3}");
        // Two taps overlap at shift 1, so two product bars are drawn.
        conv.set_shift(&mut scene, &axes, &mut ids, 1);
        assert_eq!(scene.family(ids.products.erase()).len(), 3); // 2 bars + group
    }
}
