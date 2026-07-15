//! Property tests for [`manim_color`]: round-trips and interpolation contracts.

use manim_color::gradient::{color_gradient, interpolate_color};
use manim_color::Color;
use proptest::prelude::*;

/// A `Color` built from four linear components in `[0, 1]`.
fn any_color() -> impl Strategy<Value = Color> {
    (0.0f32..=1.0, 0.0f32..=1.0, 0.0f32..=1.0, 0.0f32..=1.0)
        .prop_map(|(r, g, b, a)| Color::from_rgba(r, g, b, a))
}

proptest! {
    /// Any 8-bit sRGB triple survives the linear round-trip exactly.
    #[test]
    fn srgb_u8_round_trip(r in 0u8..=255, g in 0u8..=255, b in 0u8..=255) {
        let c = Color::from_srgb_u8(r, g, b);
        prop_assert_eq!(c.to_srgb_u8(), [r, g, b, 255]);
    }

    /// A hex string parses and re-serializes to itself.
    #[test]
    fn hex_string_round_trip(r in 0u8..=255, g in 0u8..=255, b in 0u8..=255) {
        let hex = format!("#{r:02X}{g:02X}{b:02X}");
        let parsed = Color::from_hex(&hex).unwrap();
        prop_assert_eq!(parsed.to_hex(), hex);
    }

    /// An 8-bit alpha survives a hex round-trip through `#RRGGBBAA`.
    #[test]
    fn hex_alpha_round_trip(
        r in 0u8..=255, g in 0u8..=255, b in 0u8..=255, a in 0u8..=254,
    ) {
        let hex = format!("#{r:02X}{g:02X}{b:02X}{a:02X}");
        let parsed = Color::from_hex(&hex).unwrap();
        // a < 255, so to_hex must keep the alpha channel.
        prop_assert_eq!(parsed.to_srgb_u8(), [r, g, b, a]);
        prop_assert_eq!(parsed.to_hex(), hex);
    }

    /// HSV round-trips back to the same sRGB color within tolerance.
    #[test]
    fn hsv_round_trip(r in 0u8..=255, g in 0u8..=255, b in 0u8..=255) {
        let c = Color::from_srgb_u8(r, g, b);
        let (h, s, v) = c.to_hsv();
        let back = Color::from_hsv(h, s, v);
        let (a, e) = (c.to_srgb(), back.to_srgb());
        for i in 0..3 {
            prop_assert!((a[i] - e[i]).abs() < 1e-3, "channel {}: {} vs {}", i, a[i], e[i]);
        }
    }

    /// HSL round-trips back to the same sRGB color within tolerance.
    #[test]
    fn hsl_round_trip(r in 0u8..=255, g in 0u8..=255, b in 0u8..=255) {
        let c = Color::from_srgb_u8(r, g, b);
        let (h, s, l) = c.to_hsl();
        let back = Color::from_hsl(h, s, l);
        let (a, e) = (c.to_srgb(), back.to_srgb());
        for i in 0..3 {
            prop_assert!((a[i] - e[i]).abs() < 1e-3, "channel {}: {} vs {}", i, a[i], e[i]);
        }
    }

    /// Linear interpolation reproduces its endpoints exactly.
    #[test]
    fn interpolate_endpoints_exact(a in any_color(), b in any_color()) {
        prop_assert_eq!(a.interpolate(&b, 0.0), a);
        prop_assert_eq!(a.interpolate(&b, 1.0), b);
        prop_assert_eq!(interpolate_color(a, b, 0.0), a);
        prop_assert_eq!(interpolate_color(a, b, 1.0), b);
    }

    /// The interpolated midpoint is the componentwise average.
    #[test]
    fn interpolate_midpoint_is_average(a in any_color(), b in any_color()) {
        let m = a.interpolate(&b, 0.5);
        prop_assert!((m.r - 0.5 * (a.r + b.r)).abs() < 1e-6);
        prop_assert!((m.g - 0.5 * (a.g + b.g)).abs() < 1e-6);
        prop_assert!((m.b - 0.5 * (a.b + b.b)).abs() < 1e-6);
        prop_assert!((m.a - 0.5 * (a.a + b.a)).abs() < 1e-6);
    }

    /// `lighter`/`darker` stay in gamut and move in the expected direction.
    #[test]
    fn lighter_darker_bounds(c in any_color(), amount in 0.0f32..=1.0) {
        let up = c.lighter(amount).to_srgb();
        let down = c.darker(amount).to_srgb();
        let base = c.to_srgb();
        for i in 0..3 {
            prop_assert!((0.0..=1.0).contains(&up[i]));
            prop_assert!((0.0..=1.0).contains(&down[i]));
            prop_assert!(up[i] >= base[i] - 1e-4, "lighter decreased channel {i}");
            prop_assert!(down[i] <= base[i] + 1e-4, "darker increased channel {i}");
        }
    }

    /// `lighter`/`darker` preserve alpha.
    #[test]
    fn lighter_darker_preserve_alpha(c in any_color(), amount in 0.0f32..=1.0) {
        prop_assert_eq!(c.lighter(amount).a, c.a);
        prop_assert_eq!(c.darker(amount).a, c.a);
    }

    /// `invert` is an involution (in 8-bit sRGB) and preserves alpha.
    #[test]
    fn invert_is_involution(r in 0u8..=255, g in 0u8..=255, b in 0u8..=255) {
        let c = Color::from_srgb_u8(r, g, b);
        prop_assert_eq!(c.invert().invert().to_srgb_u8(), c.to_srgb_u8());
        prop_assert_eq!(c.invert().a, c.a);
    }

    /// A requested gradient has the requested length with exact endpoints.
    #[test]
    fn gradient_length_and_endpoints(
        stops in prop::collection::vec(any_color(), 1..6),
        length in 2usize..40,
    ) {
        let ramp = color_gradient(&stops, length);
        prop_assert_eq!(ramp.len(), length);
        prop_assert_eq!(ramp[0], stops[0]);
        // With length >= 2 the final sample is always the last stop
        // (or the sole stop, when there is only one).
        prop_assert_eq!(*ramp.last().unwrap(), *stops.last().unwrap());
    }
}
