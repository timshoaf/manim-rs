//! Colors for `manim_rust`: the [`Color`] type plus the manim CE catalog.
//!
//! A [`Color`] stores linear-light RGBA components as `f32` — the form a GPU
//! wants — while every human-facing edge (hex strings, HSV/HSL, the named
//! catalog) speaks sRGB, matching [manim CE](https://docs.manim.community).
//!
//! ```
//! use manim_color::{Color, BLUE_C};
//!
//! // Parse a sRGB hex string; store it linear-light.
//! let c = Color::from_hex("#58C4DD").unwrap();
//! assert_eq!(c.to_hex(), "#58C4DD");
//! assert_eq!(BLUE_C.to_hex(), "#58C4DD");
//!
//! // Blend toward white and read it back.
//! let pale = c.lighter(0.5);
//! assert!(pale.to_srgb_u8()[0] > c.to_srgb_u8()[0]);
//! ```
//!
//! # Modules
//!
//! - [`catalog`] — the full manim CE named-color catalog (also re-exported at
//!   the crate root, so `use manim_color::BLUE;` works).
//! - [`gradient`] — multi-stop gradients, averaging, and a seedable RNG.
//!
//! # Parity map
//!
//! | manim CE | here |
//! | --- | --- |
//! | `ManimColor("#RRGGBB")` | [`Color::from_hex`] |
//! | `color.to_hex()` | [`Color::to_hex`] |
//! | `ManimColor.from_hsv` | [`Color::from_hsv`] |
//! | `color.lighter` / `.darker` | [`Color::lighter`] / [`Color::darker`] |
//! | `invert_color` | [`Color::invert`] |
//! | `interpolate_color` | [`gradient::interpolate_color`] |
//! | `color_gradient` | [`gradient::color_gradient`] |
//! | `average_color` | [`gradient::average_color`] |

use std::fmt;
use std::str::FromStr;

pub mod catalog;
pub mod gradient;

pub use catalog::*;

/// An RGBA color stored as linear-light components.
///
/// Fields hold **linear** light (not sRGB-encoded) so that blending and
/// GPU upload are correct without further conversion. Values are conventionally
/// in `[0, 1]` but may stray outside it after arithmetic; conversions to
/// 8-bit / hex clamp as needed.
///
/// ```
/// use manim_color::Color;
/// let c = Color::from_rgba(1.0, 0.0, 0.0, 1.0);
/// assert_eq!(c.r, 1.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Red component (linear light).
    pub r: f32,
    /// Green component (linear light).
    pub g: f32,
    /// Blue component (linear light).
    pub b: f32,
    /// Alpha (opacity), where `0.0` is transparent and `1.0` is opaque.
    pub a: f32,
}

/// Converts a single sRGB-encoded component in `[0, 1]` to linear light.
#[inline]
fn srgb_to_linear(s: f32) -> f32 {
    if s <= 0.040_45 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

/// Converts a single linear-light component in `[0, 1]` to sRGB encoding.
///
/// The endpoints are snapped exactly (`1.055 * 1.0 - 0.055` is not quite `1.0`
/// in `f32`), so opaque white round-trips to exactly `1.0` rather than one ULP
/// short — which otherwise shifts sRGB midpoints off by a quantization step.
#[inline]
fn linear_to_srgb(l: f32) -> f32 {
    if l <= 0.0 {
        0.0
    } else if l >= 1.0 {
        1.0
    } else if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

/// Quantizes a component in `[0, 1]` to an 8-bit value, clamping and rounding.
#[inline]
fn to_u8(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl Color {
    /// Creates a color from linear RGBA components.
    ///
    /// ```
    /// use manim_color::Color;
    /// let c = Color::from_rgba(0.0, 1.0, 0.0, 0.5);
    /// assert_eq!(c.a, 0.5);
    /// ```
    pub const fn from_rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Creates an opaque color from linear RGB components (`a = 1.0`).
    ///
    /// ```
    /// use manim_color::Color;
    /// let c = Color::from_rgb(0.25, 0.5, 0.75);
    /// assert_eq!(c.a, 1.0);
    /// ```
    pub const fn from_rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Creates an opaque color from sRGB-encoded components in `[0, 1]`.
    ///
    /// ```
    /// use manim_color::Color;
    /// let white = Color::from_srgb(1.0, 1.0, 1.0);
    /// assert_eq!(white.to_hex(), "#FFFFFF");
    /// ```
    pub fn from_srgb(r: f32, g: f32, b: f32) -> Self {
        Self::from_srgba(r, g, b, 1.0)
    }

    /// Creates a color from sRGB-encoded RGB in `[0, 1]` plus a linear alpha.
    ///
    /// Alpha is *not* gamma-encoded, so it is stored unchanged.
    ///
    /// ```
    /// use manim_color::Color;
    /// let c = Color::from_srgba(1.0, 1.0, 1.0, 0.5);
    /// assert_eq!(c.a, 0.5);
    /// ```
    pub fn from_srgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: srgb_to_linear(r),
            g: srgb_to_linear(g),
            b: srgb_to_linear(b),
            a,
        }
    }

    /// Creates an opaque color from 8-bit sRGB components.
    ///
    /// ```
    /// use manim_color::Color;
    /// let c = Color::from_srgb_u8(0x58, 0xC4, 0xDD);
    /// assert_eq!(c.to_hex(), "#58C4DD");
    /// ```
    pub fn from_srgb_u8(r: u8, g: u8, b: u8) -> Self {
        Self::from_srgb(
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
        )
    }

    /// Parses a hex color string.
    ///
    /// Accepts `#RGB`, `#RRGGBB`, and `#RRGGBBAA` (the leading `#` is optional
    /// and hex digits are case-insensitive). The `RGB` short form expands each
    /// digit (`#F0A` → `#FF00AA`). Components are interpreted as sRGB.
    ///
    /// ```
    /// use manim_color::Color;
    /// assert_eq!(Color::from_hex("#fff").unwrap(), Color::from_hex("FFFFFF").unwrap());
    /// let semi = Color::from_hex("#FF000080").unwrap();
    /// assert!((semi.a - 128.0 / 255.0).abs() < 1e-6);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`ParseColorError`] if the length is not 3, 6, or 8 hex digits
    /// (after any `#`), or if a character is not a hex digit.
    pub fn from_hex(s: &str) -> Result<Self, ParseColorError> {
        let h = s.strip_prefix('#').unwrap_or(s);
        let bytes = h.as_bytes();
        let hx = |c: u8| -> Result<u8, ParseColorError> {
            (c as char)
                .to_digit(16)
                .map(|d| d as u8)
                .ok_or(ParseColorError::InvalidDigit(c as char))
        };
        let pair = |hi: u8, lo: u8| -> Result<u8, ParseColorError> { Ok((hx(hi)? << 4) | hx(lo)?) };
        match bytes.len() {
            3 => {
                let r = hx(bytes[0])?;
                let g = hx(bytes[1])?;
                let b = hx(bytes[2])?;
                Ok(Self::from_srgb_u8(r << 4 | r, g << 4 | g, b << 4 | b))
            }
            6 => Ok(Self::from_srgb_u8(
                pair(bytes[0], bytes[1])?,
                pair(bytes[2], bytes[3])?,
                pair(bytes[4], bytes[5])?,
            )),
            8 => {
                let rgb = Self::from_srgb_u8(
                    pair(bytes[0], bytes[1])?,
                    pair(bytes[2], bytes[3])?,
                    pair(bytes[4], bytes[5])?,
                );
                let a = pair(bytes[6], bytes[7])?;
                Ok(rgb.with_opacity(f32::from(a) / 255.0))
            }
            n => Err(ParseColorError::InvalidLength(n)),
        }
    }

    /// Creates a color from HSV (hue-saturation-value), matching manim.
    ///
    /// All arguments are in `[0, 1]`; `h` wraps like manim's `[0, 1)` hue. The
    /// HSV triple describes an sRGB color, which is stored linear-light.
    ///
    /// ```
    /// use manim_color::Color;
    /// let red = Color::from_hsv(0.0, 1.0, 1.0);
    /// assert_eq!(red.to_hex(), "#FF0000");
    /// ```
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let (r, g, b) = hsv_to_rgb(h, s, v);
        Self::from_srgb(r, g, b)
    }

    /// Creates a color from HSL (hue-saturation-lightness).
    ///
    /// All arguments are in `[0, 1]`; `h` wraps like a `[0, 1)` hue. The HSL
    /// triple describes an sRGB color, which is stored linear-light.
    ///
    /// ```
    /// use manim_color::Color;
    /// let red = Color::from_hsl(0.0, 1.0, 0.5);
    /// assert_eq!(red.to_hex(), "#FF0000");
    /// ```
    pub fn from_hsl(h: f32, s: f32, l: f32) -> Self {
        let (r, g, b) = hsl_to_rgb(h, s, l);
        Self::from_srgb(r, g, b)
    }

    /// Returns the sRGB-encoded RGBA components in `[0, 1]`.
    ///
    /// Alpha is returned unchanged (it is not gamma-encoded).
    ///
    /// ```
    /// use manim_color::Color;
    /// let s = Color::from_srgb(0.5, 0.25, 0.75).to_srgb();
    /// assert!((s[0] - 0.5).abs() < 1e-6);
    /// ```
    pub fn to_srgb(&self) -> [f32; 4] {
        [
            linear_to_srgb(self.r),
            linear_to_srgb(self.g),
            linear_to_srgb(self.b),
            self.a,
        ]
    }

    /// Returns the 8-bit sRGB RGBA components.
    ///
    /// ```
    /// use manim_color::Color;
    /// assert_eq!(Color::from_hex("#58C4DD").unwrap().to_srgb_u8(), [0x58, 0xC4, 0xDD, 0xFF]);
    /// ```
    pub fn to_srgb_u8(&self) -> [u8; 4] {
        let s = self.to_srgb();
        [to_u8(s[0]), to_u8(s[1]), to_u8(s[2]), to_u8(s[3])]
    }

    /// Formats the color as a hex string.
    ///
    /// Returns `#RRGGBB` when fully opaque, or `#RRGGBBAA` otherwise.
    ///
    /// ```
    /// use manim_color::Color;
    /// assert_eq!(Color::from_hex("#58C4DD").unwrap().to_hex(), "#58C4DD");
    /// assert_eq!(Color::from_srgba(1.0, 0.0, 0.0, 0.5).to_hex(), "#FF000080");
    /// ```
    pub fn to_hex(&self) -> String {
        let [r, g, b, a] = self.to_srgb_u8();
        if a == 255 {
            format!("#{r:02X}{g:02X}{b:02X}")
        } else {
            format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
        }
    }

    /// Returns the color as HSV (hue-saturation-value), each in `[0, 1]`.
    ///
    /// The inverse of [`Color::from_hsv`]; computed in sRGB space.
    ///
    /// ```
    /// use manim_color::Color;
    /// let (h, s, v) = Color::from_hex("#FF0000").unwrap().to_hsv();
    /// assert!((h - 0.0).abs() < 1e-6 && (s - 1.0).abs() < 1e-6 && (v - 1.0).abs() < 1e-6);
    /// ```
    pub fn to_hsv(&self) -> (f32, f32, f32) {
        let [r, g, b, _] = self.to_srgb();
        rgb_to_hsv(r, g, b)
    }

    /// Returns the color as HSL (hue-saturation-lightness), each in `[0, 1]`.
    ///
    /// The inverse of [`Color::from_hsl`]; computed in sRGB space.
    ///
    /// ```
    /// use manim_color::Color;
    /// let (h, s, l) = Color::from_hex("#FF0000").unwrap().to_hsl();
    /// assert!((h - 0.0).abs() < 1e-6 && (s - 1.0).abs() < 1e-6 && (l - 0.5).abs() < 1e-6);
    /// ```
    pub fn to_hsl(&self) -> (f32, f32, f32) {
        let [r, g, b, _] = self.to_srgb();
        rgb_to_hsl(r, g, b)
    }

    /// Returns the alpha-premultiplied linear RGBA components.
    ///
    /// ```
    /// use manim_color::Color;
    /// let p = Color::from_rgba(1.0, 0.5, 0.0, 0.5).premultiplied();
    /// assert_eq!(p, [0.5, 0.25, 0.0, 0.5]);
    /// ```
    pub fn premultiplied(&self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
    }

    /// Linearly interpolates toward `other` by `t`, componentwise (including
    /// alpha), in linear-light space.
    ///
    /// `t = 0` returns `self`, `t = 1` returns `other`.
    ///
    /// ```
    /// use manim_color::Color;
    /// let a = Color::from_rgba(0.0, 0.0, 0.0, 0.0);
    /// let b = Color::from_rgba(1.0, 1.0, 1.0, 1.0);
    /// assert_eq!(a.interpolate(&b, 0.5), Color::from_rgba(0.5, 0.5, 0.5, 0.5));
    /// ```
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            r: lerp(self.r, other.r, t),
            g: lerp(self.g, other.g, t),
            b: lerp(self.b, other.b, t),
            a: lerp(self.a, other.a, t),
        }
    }

    /// Interpolates toward `other` by `t` in sRGB space, matching manim's
    /// visual blend. Alpha is interpolated linearly.
    ///
    /// ```
    /// use manim_color::Color;
    /// let a = Color::from_hex("#000000").unwrap();
    /// let b = Color::from_hex("#FFFFFF").unwrap();
    /// // The sRGB midpoint of black and white is mid-gray (#808080), not #BBBBBB.
    /// assert_eq!(a.interpolate_srgb(&b, 0.5).to_hex(), "#808080");
    /// ```
    pub fn interpolate_srgb(&self, other: &Self, t: f32) -> Self {
        let x = self.to_srgb();
        let y = other.to_srgb();
        Self::from_srgba(
            lerp(x[0], y[0], t),
            lerp(x[1], y[1], t),
            lerp(x[2], y[2], t),
            lerp(x[3], y[3], t),
        )
    }

    /// Returns a lighter color by blending toward white by `amount` in sRGB
    /// space (manim's `lighter`). Alpha is preserved.
    ///
    /// ```
    /// use manim_color::Color;
    /// let c = Color::from_hex("#000000").unwrap().lighter(0.5);
    /// assert_eq!(c.to_hex(), "#808080");
    /// ```
    pub fn lighter(&self, amount: f32) -> Self {
        let s = self.to_srgb();
        Self::from_srgb(
            lerp(s[0], 1.0, amount),
            lerp(s[1], 1.0, amount),
            lerp(s[2], 1.0, amount),
        )
        .with_opacity(self.a)
    }

    /// Returns a darker color by blending toward black by `amount` in sRGB
    /// space (manim's `darker`). Alpha is preserved.
    ///
    /// ```
    /// use manim_color::Color;
    /// let c = Color::from_hex("#FFFFFF").unwrap().darker(0.5);
    /// assert_eq!(c.to_hex(), "#808080");
    /// ```
    pub fn darker(&self, amount: f32) -> Self {
        let s = self.to_srgb();
        Self::from_srgb(
            lerp(s[0], 0.0, amount),
            lerp(s[1], 0.0, amount),
            lerp(s[2], 0.0, amount),
        )
        .with_opacity(self.a)
    }

    /// Returns the inverted color (`1 - c` per channel) in sRGB space, matching
    /// manim's `invert_color`. Alpha is preserved.
    ///
    /// ```
    /// use manim_color::Color;
    /// assert_eq!(Color::from_hex("#000000").unwrap().invert().to_hex(), "#FFFFFF");
    /// assert_eq!(Color::from_hex("#FF8800").unwrap().invert().to_hex(), "#0077FF");
    /// ```
    pub fn invert(&self) -> Self {
        let s = self.to_srgb();
        Self::from_srgb(1.0 - s[0], 1.0 - s[1], 1.0 - s[2]).with_opacity(self.a)
    }

    /// Returns a copy of the color with alpha replaced by `a`.
    ///
    /// ```
    /// use manim_color::Color;
    /// let c = Color::from_rgb(1.0, 0.0, 0.0).with_opacity(0.5);
    /// assert_eq!(c.a, 0.5);
    /// ```
    pub fn with_opacity(&self, a: f32) -> Self {
        Self { a, ..*self }
    }

    /// Returns the alpha (opacity) component.
    ///
    /// ```
    /// use manim_color::Color;
    /// assert_eq!(Color::from_rgba(1.0, 0.0, 0.0, 0.25).opacity(), 0.25);
    /// ```
    pub fn opacity(&self) -> f32 {
        self.a
    }
}

/// Linearly interpolates between two scalars.
///
/// Uses the `(1 - t) * a + t * b` form so the endpoints are reproduced exactly:
/// `t = 0` yields `a` and `t = 1` yields `b` with no floating-point drift.
#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (1.0 - t) * a + t * b
}

/// Converts an HSV triple (each in `[0, 1]`, `h` wrapping) to sRGB in `[0, 1]`.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    if s <= 0.0 {
        return (v, v, v);
    }
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// Converts an sRGB triple (each in `[0, 1]`) to an HSV triple in `[0, 1]`.
fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let v = max;
    let s = if max <= 0.0 { 0.0 } else { d / max };
    let h = hue(r, g, b, max, d);
    (h, s, v)
}

/// Converts an sRGB triple (each in `[0, 1]`) to an HSL triple in `[0, 1]`.
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let l = (max + min) / 2.0;
    let s = if d <= 0.0 {
        0.0
    } else if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = hue(r, g, b, max, d);
    (h, s, l)
}

/// Computes the shared hue (in `[0, 1)`) from RGB, the channel max, and delta.
fn hue(r: f32, g: f32, b: f32, max: f32, d: f32) -> f32 {
    if d <= 0.0 {
        return 0.0;
    }
    let mut h = if max == r {
        (g - b) / d
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    if h < 0.0 {
        h += 1.0;
    }
    h
}

/// Converts an HSL triple (each in `[0, 1]`, `h` wrapping) to sRGB in `[0, 1]`.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s <= 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    (
        hue_to_channel(p, q, h + 1.0 / 3.0),
        hue_to_channel(p, q, h),
        hue_to_channel(p, q, h - 1.0 / 3.0),
    )
}

/// Resolves one HSL channel from the `p`/`q` intermediates and a hue offset.
fn hue_to_channel(p: f32, q: f32, t: f32) -> f32 {
    let t = t.rem_euclid(1.0);
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 1.0 / 2.0 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

impl fmt::Display for Color {
    /// Formats the color as its hex string (see [`Color::to_hex`]).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for Color {
    type Err = ParseColorError;

    /// Parses a hex color string; see [`Color::from_hex`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

/// The error returned when a hex color string cannot be parsed.
///
/// ```
/// use manim_color::{Color, ParseColorError};
/// assert_eq!(Color::from_hex("#12"), Err(ParseColorError::InvalidLength(2)));
/// assert_eq!(Color::from_hex("#GG0000"), Err(ParseColorError::InvalidDigit('G')));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseColorError {
    /// The string had `n` hex digits; expected 3, 6, or 8 (after any `#`).
    InvalidLength(usize),
    /// The string contained a character that is not a hex digit.
    InvalidDigit(char),
}

impl fmt::Display for ParseColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(n) => {
                write!(
                    f,
                    "invalid hex color length {n}: expected 3, 6, or 8 digits"
                )
            }
            Self::InvalidDigit(c) => write!(f, "invalid hex digit {c:?}"),
        }
    }
}

impl std::error::Error for ParseColorError {}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn hex_round_trip_opaque() {
        for hex in ["#58C4DD", "#000000", "#FFFFFF", "#FC6255", "#736357"] {
            assert_eq!(Color::from_hex(hex).unwrap().to_hex(), hex);
        }
    }

    #[test]
    fn hex_short_form_expands() {
        assert_eq!(
            Color::from_hex("#f0a").unwrap(),
            Color::from_hex("#FF00AA").unwrap()
        );
        assert_eq!(
            Color::from_hex("fff").unwrap(),
            Color::from_hex("#FFFFFF").unwrap()
        );
    }

    #[test]
    fn hex_with_alpha() {
        let c = Color::from_hex("#FF000080").unwrap();
        assert_eq!(c.to_srgb_u8(), [255, 0, 0, 128]);
        assert_eq!(c.to_hex(), "#FF000080");
    }

    #[test]
    fn hex_case_insensitive_and_optional_hash() {
        assert_eq!(
            Color::from_hex("58c4dd").unwrap(),
            Color::from_hex("#58C4DD").unwrap()
        );
    }

    #[test]
    fn hex_errors() {
        assert_eq!(
            Color::from_hex("#12"),
            Err(ParseColorError::InvalidLength(2))
        );
        assert_eq!(
            Color::from_hex("#12345"),
            Err(ParseColorError::InvalidLength(5))
        );
        assert_eq!(
            Color::from_hex("#GG0000"),
            Err(ParseColorError::InvalidDigit('G'))
        );
    }

    #[test]
    fn srgb_conversions() {
        // Mid-gray sRGB #808080 is ~0.216 linear.
        let c = Color::from_srgb_u8(0x80, 0x80, 0x80);
        assert_relative_eq!(c.r, 0.2158605, epsilon = 1e-5);
        assert_eq!(c.to_srgb_u8(), [0x80, 0x80, 0x80, 0xFF]);
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(Color::from_hsv(0.0, 1.0, 1.0).to_hex(), "#FF0000");
        assert_eq!(Color::from_hsv(1.0 / 3.0, 1.0, 1.0).to_hex(), "#00FF00");
        assert_eq!(Color::from_hsv(2.0 / 3.0, 1.0, 1.0).to_hex(), "#0000FF");
        assert_eq!(
            Color::from_hsv(0.0, 0.0, 0.5).to_srgb_u8(),
            [128, 128, 128, 255]
        );
    }

    #[test]
    fn hsl_primaries() {
        assert_eq!(Color::from_hsl(0.0, 1.0, 0.5).to_hex(), "#FF0000");
        assert_eq!(Color::from_hsl(1.0 / 3.0, 1.0, 0.5).to_hex(), "#00FF00");
        assert_eq!(Color::from_hsl(2.0 / 3.0, 1.0, 0.5).to_hex(), "#0000FF");
    }

    #[test]
    fn hsv_round_trip() {
        let c = Color::from_hex("#58C4DD").unwrap();
        let (h, s, v) = c.to_hsv();
        assert_relative_eq!(Color::from_hsv(h, s, v).r, c.r, epsilon = 1e-4);
    }

    #[test]
    fn hsl_round_trip() {
        let c = Color::from_hex("#C55F73").unwrap();
        let (h, s, l) = c.to_hsl();
        assert_relative_eq!(Color::from_hsl(h, s, l).r, c.r, epsilon = 1e-4);
    }

    #[test]
    fn interpolate_endpoints_and_midpoint() {
        let a = Color::from_rgba(0.0, 0.0, 0.0, 0.0);
        let b = Color::from_rgba(1.0, 1.0, 1.0, 1.0);
        assert_eq!(a.interpolate(&b, 0.0), a);
        assert_eq!(a.interpolate(&b, 1.0), b);
        assert_eq!(a.interpolate(&b, 0.5), Color::from_rgba(0.5, 0.5, 0.5, 0.5));
    }

    #[test]
    fn interpolate_srgb_midpoint() {
        let a = Color::from_hex("#000000").unwrap();
        let b = Color::from_hex("#FFFFFF").unwrap();
        assert_eq!(a.interpolate_srgb(&b, 0.5).to_hex(), "#808080");
    }

    #[test]
    fn lighter_darker_bounds_and_alpha() {
        let c = Color::from_srgb(0.3, 0.4, 0.5);
        assert_eq!(c.lighter(1.0).to_hex(), "#FFFFFF");
        assert_eq!(c.darker(1.0).to_hex(), "#000000");
        assert_relative_eq!(c.lighter(0.0).r, c.r, epsilon = 1e-6);
        // Alpha is preserved by both operations.
        let t = c.with_opacity(0.7);
        assert_eq!(t.lighter(0.5).a, 0.7);
        assert_eq!(t.darker(0.5).a, 0.7);
    }

    #[test]
    fn invert_involution() {
        let c = Color::from_hex("#FF8800").unwrap();
        assert_eq!(c.invert().to_hex(), "#0077FF");
        assert_eq!(c.invert().invert().to_hex(), "#FF8800");
    }

    #[test]
    fn premultiplied_scales_rgb() {
        let c = Color::from_rgba(1.0, 0.5, 0.0, 0.5);
        assert_eq!(c.premultiplied(), [0.5, 0.25, 0.0, 0.5]);
    }

    #[test]
    fn display_and_fromstr() {
        let c: Color = "#58C4DD".parse().unwrap();
        assert_eq!(c.to_string(), "#58C4DD");
        assert_eq!(format!("{c}"), "#58C4DD");
    }

    #[test]
    fn error_is_std_error() {
        let e = Color::from_hex("#zz").unwrap_err();
        let _dyn: &dyn std::error::Error = &e;
        assert!(!e.to_string().is_empty());
    }
}
