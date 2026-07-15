//! Multi-stop gradients, color averaging, and a seedable RNG.
//!
//! Mirrors manim CE's `manim.utils.color` gradient helpers. Interpolation and
//! averaging happen in **linear-light** space (the crate default); for a blend
//! that matches manim's on-screen appearance pixel-for-pixel, use
//! [`Color::interpolate_srgb`](crate::Color::interpolate_srgb) instead.
//!
//! ```
//! use manim_color::{RED, BLUE};
//! use manim_color::gradient::color_gradient;
//!
//! let ramp = color_gradient(&[RED, BLUE], 3);
//! assert_eq!(ramp.len(), 3);
//! assert_eq!(ramp[0], RED);
//! assert_eq!(ramp[2], BLUE);
//! ```

use crate::Color;

/// Interpolates between two colors by `t` in linear-light space.
///
/// `t = 0` returns `a`, `t = 1` returns `b`. This is the free-function form of
/// [`Color::interpolate`](crate::Color::interpolate), matching manim's
/// `interpolate_color`.
///
/// ```
/// use manim_color::{Color, gradient::interpolate_color};
/// let a = Color::from_rgba(0.0, 0.0, 0.0, 0.0);
/// let b = Color::from_rgba(1.0, 1.0, 1.0, 1.0);
/// assert_eq!(interpolate_color(a, b, 0.25), Color::from_rgba(0.25, 0.25, 0.25, 0.25));
/// ```
pub fn interpolate_color(a: Color, b: Color, t: f32) -> Color {
    a.interpolate(&b, t)
}

/// Builds a gradient of `length` colors evenly sampled across `colors`,
/// matching manim's `color_gradient`.
///
/// The first and last output colors are exactly the first and last inputs.
/// Returns an empty vector if `length` is `0` or `colors` is empty; a single
/// input color is repeated `length` times.
///
/// ```
/// use manim_color::{RED, GREEN, BLUE, gradient::color_gradient};
/// let ramp = color_gradient(&[RED, GREEN, BLUE], 5);
/// assert_eq!(ramp.len(), 5);
/// assert_eq!(ramp[0], RED);
/// assert_eq!(ramp[2], GREEN); // exact middle stop
/// assert_eq!(ramp[4], BLUE);
/// ```
pub fn color_gradient(colors: &[Color], length: usize) -> Vec<Color> {
    if length == 0 || colors.is_empty() {
        return Vec::new();
    }
    if colors.len() == 1 {
        return vec![colors[0]; length];
    }
    if length == 1 {
        return vec![colors[0]];
    }
    let n = colors.len();
    let span = (n - 1) as f32;
    let steps = (length - 1) as f32;
    (0..length)
        .map(|k| {
            let alpha = k as f32 * span / steps;
            let mut floor = alpha.floor() as usize;
            let mut frac = alpha - floor as f32;
            if floor >= n - 1 {
                floor = n - 2;
                frac = 1.0;
            }
            interpolate_color(colors[floor], colors[floor + 1], frac)
        })
        .collect()
}

/// Returns the componentwise mean of the given colors (including alpha) in
/// linear-light space, matching manim's `average_color`.
///
/// Returns transparent black for an empty slice.
///
/// ```
/// use manim_color::{Color, gradient::average_color};
/// let a = Color::from_rgba(0.0, 0.0, 0.0, 0.0);
/// let b = Color::from_rgba(1.0, 1.0, 1.0, 1.0);
/// assert_eq!(average_color(&[a, b]), Color::from_rgba(0.5, 0.5, 0.5, 0.5));
/// ```
pub fn average_color(colors: &[Color]) -> Color {
    if colors.is_empty() {
        return Color::from_rgba(0.0, 0.0, 0.0, 0.0);
    }
    let (mut r, mut g, mut b, mut a) = (0.0, 0.0, 0.0, 0.0);
    for c in colors {
        r += c.r;
        g += c.g;
        b += c.b;
        a += c.a;
    }
    let n = colors.len() as f32;
    Color::from_rgba(r / n, g / n, b / n, a / n)
}

/// A tiny deterministic pseudo-random generator (a 64-bit LCG).
///
/// Used by [`random_color`] and [`random_bright_color`] so color choices are
/// reproducible in tests without pulling in an RNG dependency. It is **not**
/// cryptographically secure.
///
/// ```
/// use manim_color::gradient::ColorRng;
/// let mut a = ColorRng::new(42);
/// let mut b = ColorRng::new(42);
/// assert_eq!(a.next_f32(), b.next_f32()); // same seed → same sequence
/// ```
#[derive(Debug, Clone)]
pub struct ColorRng {
    state: u64,
}

impl ColorRng {
    /// Creates a generator seeded with `seed`.
    ///
    /// ```
    /// use manim_color::gradient::ColorRng;
    /// let mut rng = ColorRng::new(7);
    /// let x = rng.next_f32();
    /// assert!((0.0..1.0).contains(&x));
    /// ```
    pub const fn new(seed: u64) -> Self {
        // Offset the seed so that seed 0 does not start from an all-zero state.
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// Advances the generator and returns the next `u32`.
    ///
    /// ```
    /// use manim_color::gradient::ColorRng;
    /// let mut rng = ColorRng::new(1);
    /// assert_ne!(rng.next_u32(), rng.next_u32());
    /// ```
    pub fn next_u32(&mut self) -> u32 {
        // LCG constants from Knuth's MMIX.
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // Return the high bits, which have the best statistical quality.
        (self.state >> 32) as u32
    }

    /// Advances the generator and returns the next `f32` in `[0, 1)`.
    ///
    /// ```
    /// use manim_color::gradient::ColorRng;
    /// let mut rng = ColorRng::new(1);
    /// let x = rng.next_f32();
    /// assert!((0.0..1.0).contains(&x));
    /// ```
    pub fn next_f32(&mut self) -> f32 {
        // Use the top 24 bits for a uniform value with full f32 mantissa range.
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// The manim palette that [`random_color`] draws from (the canonical named
/// colors, excluding grays).
const PALETTE: &[Color] = &[
    crate::BLUE_A,
    crate::BLUE_B,
    crate::BLUE_C,
    crate::BLUE_D,
    crate::BLUE_E,
    crate::TEAL_A,
    crate::TEAL_B,
    crate::TEAL_C,
    crate::TEAL_D,
    crate::TEAL_E,
    crate::GREEN_A,
    crate::GREEN_B,
    crate::GREEN_C,
    crate::GREEN_D,
    crate::GREEN_E,
    crate::YELLOW_A,
    crate::YELLOW_B,
    crate::YELLOW_C,
    crate::YELLOW_D,
    crate::YELLOW_E,
    crate::GOLD_A,
    crate::GOLD_B,
    crate::GOLD_C,
    crate::GOLD_D,
    crate::GOLD_E,
    crate::RED_A,
    crate::RED_B,
    crate::RED_C,
    crate::RED_D,
    crate::RED_E,
    crate::MAROON_A,
    crate::MAROON_B,
    crate::MAROON_C,
    crate::MAROON_D,
    crate::MAROON_E,
    crate::PURPLE_A,
    crate::PURPLE_B,
    crate::PURPLE_C,
    crate::PURPLE_D,
    crate::PURPLE_E,
    crate::PINK,
    crate::LIGHT_PINK,
    crate::ORANGE,
    crate::LIGHT_BROWN,
    crate::DARK_BROWN,
    crate::GRAY_BROWN,
];

/// Picks a color uniformly at random from the manim palette using `rng`.
///
/// Deterministic for a given seed; matches manim's `random_color`.
///
/// ```
/// use manim_color::gradient::{ColorRng, random_color};
/// let mut rng = ColorRng::new(123);
/// let c = random_color(&mut rng);
/// // Reproducible: the same seed yields the same color.
/// assert_eq!(c, random_color(&mut ColorRng::new(123)));
/// ```
pub fn random_color(rng: &mut ColorRng) -> Color {
    let idx = (rng.next_f32() * PALETTE.len() as f32) as usize;
    PALETTE[idx.min(PALETTE.len() - 1)]
}

/// Picks a random palette color and blends it halfway toward white, matching
/// manim's `random_bright_color`.
///
/// ```
/// use manim_color::gradient::{ColorRng, random_bright_color, random_color};
/// let mut rng = ColorRng::new(9);
/// let bright = random_bright_color(&mut rng);
/// // Brightening toward white never decreases any sRGB channel.
/// let base = random_color(&mut ColorRng::new(9)).to_srgb_u8();
/// let got = bright.to_srgb_u8();
/// assert!(got[0] >= base[0] && got[1] >= base[1] && got[2] >= base[2]);
/// ```
pub fn random_bright_color(rng: &mut ColorRng) -> Color {
    random_color(rng).lighter(0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BLUE, GREEN, RED};
    use approx::assert_relative_eq;

    #[test]
    fn gradient_endpoints_exact() {
        let ramp = color_gradient(&[RED, GREEN, BLUE], 7);
        assert_eq!(ramp.len(), 7);
        assert_eq!(ramp[0], RED);
        assert_eq!(*ramp.last().unwrap(), BLUE);
    }

    #[test]
    fn gradient_hits_interior_stops() {
        // With length = 2*n - 1, interior reference colors land exactly.
        let ramp = color_gradient(&[RED, GREEN, BLUE], 5);
        assert_eq!(ramp[2], GREEN);
    }

    #[test]
    fn gradient_edge_cases() {
        assert!(color_gradient(&[], 4).is_empty());
        assert!(color_gradient(&[RED], 0).is_empty());
        assert_eq!(color_gradient(&[RED], 3), vec![RED, RED, RED]);
        assert_eq!(color_gradient(&[RED, BLUE], 1), vec![RED]);
    }

    #[test]
    fn average_of_two() {
        let a = Color::from_rgba(0.0, 0.0, 0.0, 0.0);
        let b = Color::from_rgba(1.0, 0.5, 0.25, 1.0);
        let avg = average_color(&[a, b]);
        assert_relative_eq!(avg.r, 0.5);
        assert_relative_eq!(avg.g, 0.25);
        assert_relative_eq!(avg.b, 0.125);
        assert_relative_eq!(avg.a, 0.5);
    }

    #[test]
    fn average_empty_is_transparent_black() {
        assert_eq!(average_color(&[]), Color::from_rgba(0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn rng_is_deterministic() {
        let mut a = ColorRng::new(2024);
        let mut b = ColorRng::new(2024);
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn rng_covers_unit_interval() {
        let mut rng = ColorRng::new(1);
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for _ in 0..10_000 {
            let x = rng.next_f32();
            assert!((0.0..1.0).contains(&x));
            lo = lo.min(x);
            hi = hi.max(x);
        }
        assert!(lo < 0.05 && hi > 0.95);
    }

    #[test]
    fn random_color_in_palette() {
        let mut rng = ColorRng::new(555);
        for _ in 0..1000 {
            let c = random_color(&mut rng);
            assert!(PALETTE.contains(&c));
        }
    }

    #[test]
    fn bright_color_is_lightened() {
        let mut rng = ColorRng::new(9);
        let bright = random_bright_color(&mut rng);
        let base = random_color(&mut ColorRng::new(9)).to_srgb();
        let got = bright.to_srgb();
        for i in 0..3 {
            assert!(got[i] >= base[i] - 1e-6);
        }
    }
}
