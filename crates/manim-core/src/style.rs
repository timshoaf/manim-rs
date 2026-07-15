//! Fill and stroke styling for vectorized mobjects.
//!
//! [`Style`] is the port of the paint-related attributes on manim CE's
//! `VMobject`: a fill color + opacity, a stroke color + opacity + width, and an
//! optional dash pattern. Colors and opacity are kept as separate fields (as in
//! manim) so that setting one does not disturb the other; the renderer folds the
//! opacity into the alpha channel when it builds the display list.
//!
//! | manim CE | here |
//! | --- | --- |
//! | `set_fill(color, opacity)` | [`Style::set_fill`] |
//! | `set_stroke(color, width, opacity)` | [`Style::set_stroke`] |
//! | `set_color(color)` | [`Style::set_color`] |
//! | `set_opacity(opacity)` | [`Style::set_opacity`] |

use manim_color::{Color, WHITE};

/// manim CE's default stroke width in scene-relative "points".
pub const DEFAULT_STROKE_WIDTH: f32 = 4.0;

/// The paint (fill + stroke) of a vectorized mobject.
///
/// Fill and stroke each carry a color *and* an independent opacity, matching
/// manim CE. A `None` color means "unset": the fill or stroke is only drawn when
/// its color is set and its opacity is positive.
///
/// ```
/// use manim_core::style::Style;
/// use manim_color::{BLUE, RED};
/// let mut s = Style::default();
/// s.set_fill(BLUE, 0.5).set_stroke(RED, 2.0, 1.0);
/// assert_eq!(s.fill_color, Some(BLUE));
/// assert_eq!(s.stroke_width, 2.0);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    /// Fill color, or `None` for no fill.
    pub fill_color: Option<Color>,
    /// Fill opacity in `[0, 1]`.
    pub fill_opacity: f32,
    /// Stroke color, or `None` for no stroke.
    pub stroke_color: Option<Color>,
    /// Stroke opacity in `[0, 1]`.
    pub stroke_opacity: f32,
    /// Stroke width in manim's scene-relative "points" (CE default `4.0`).
    pub stroke_width: f32,
    /// Optional dash pattern (on/off run lengths); `None` draws a solid stroke.
    pub dash_pattern: Option<Vec<f32>>,
}

impl Default for Style {
    /// The manim CE `VMobject` default: no fill, opaque white stroke of width
    /// `4.0`, solid.
    ///
    /// ```
    /// use manim_core::style::Style;
    /// let s = Style::default();
    /// assert_eq!(s.fill_color, None);
    /// assert_eq!(s.stroke_width, 4.0);
    /// ```
    fn default() -> Self {
        Self {
            fill_color: None,
            fill_opacity: 0.0,
            stroke_color: Some(WHITE),
            stroke_opacity: 1.0,
            stroke_width: DEFAULT_STROKE_WIDTH,
            dash_pattern: None,
        }
    }
}

impl Style {
    /// A style with a solid fill of `color` and no stroke (manim's "filled"
    /// look, used by `Dot`, `Sector`, and friends).
    ///
    /// ```
    /// use manim_core::style::Style;
    /// use manim_color::WHITE;
    /// let s = Style::filled(WHITE);
    /// assert_eq!(s.fill_opacity, 1.0);
    /// assert_eq!(s.stroke_color, None);
    /// ```
    pub fn filled(color: Color) -> Self {
        Self {
            fill_color: Some(color),
            fill_opacity: 1.0,
            stroke_color: None,
            stroke_opacity: 0.0,
            stroke_width: DEFAULT_STROKE_WIDTH,
            dash_pattern: None,
        }
    }

    /// A style with an opaque stroke of `color` and no fill (the default look of
    /// most manim outlines, e.g. `Circle`, `Square`).
    ///
    /// ```
    /// use manim_core::style::Style;
    /// use manim_color::RED;
    /// let s = Style::stroked(RED);
    /// assert_eq!(s.stroke_color, Some(RED));
    /// assert_eq!(s.fill_color, None);
    /// ```
    pub fn stroked(color: Color) -> Self {
        Self {
            stroke_color: Some(color),
            ..Self::default()
        }
    }

    /// Sets the fill color and opacity (port of manim's `set_fill`).
    ///
    /// ```
    /// use manim_core::style::Style;
    /// use manim_color::GREEN;
    /// let mut s = Style::default();
    /// s.set_fill(GREEN, 0.75);
    /// assert_eq!(s.fill_color, Some(GREEN));
    /// assert_eq!(s.fill_opacity, 0.75);
    /// ```
    pub fn set_fill(&mut self, color: Color, opacity: f32) -> &mut Self {
        self.fill_color = Some(color);
        self.fill_opacity = opacity;
        self
    }

    /// Sets the stroke color, width, and opacity (port of manim's `set_stroke`).
    ///
    /// ```
    /// use manim_core::style::Style;
    /// use manim_color::YELLOW;
    /// let mut s = Style::default();
    /// s.set_stroke(YELLOW, 6.0, 0.9);
    /// assert_eq!(s.stroke_color, Some(YELLOW));
    /// assert_eq!(s.stroke_width, 6.0);
    /// assert_eq!(s.stroke_opacity, 0.9);
    /// ```
    pub fn set_stroke(&mut self, color: Color, width: f32, opacity: f32) -> &mut Self {
        self.stroke_color = Some(color);
        self.stroke_width = width;
        self.stroke_opacity = opacity;
        self
    }

    /// Sets just the stroke width, leaving color and opacity untouched.
    ///
    /// ```
    /// use manim_core::style::Style;
    /// let mut s = Style::default();
    /// s.set_stroke_width(10.0);
    /// assert_eq!(s.stroke_width, 10.0);
    /// ```
    pub fn set_stroke_width(&mut self, width: f32) -> &mut Self {
        self.stroke_width = width;
        self
    }

    /// Sets both the fill and stroke color to `color` (port of manim's
    /// `set_color`), leaving opacities and width untouched.
    ///
    /// ```
    /// use manim_core::style::Style;
    /// use manim_color::RED;
    /// let mut s = Style::default();
    /// s.set_color(RED);
    /// assert_eq!(s.fill_color, Some(RED));
    /// assert_eq!(s.stroke_color, Some(RED));
    /// ```
    pub fn set_color(&mut self, color: Color) -> &mut Self {
        self.fill_color = Some(color);
        self.stroke_color = Some(color);
        self
    }

    /// Sets both the fill and stroke opacity to `opacity` (port of manim's
    /// `set_opacity`).
    ///
    /// ```
    /// use manim_core::style::Style;
    /// let mut s = Style::default();
    /// s.set_opacity(0.3);
    /// assert_eq!(s.fill_opacity, 0.3);
    /// assert_eq!(s.stroke_opacity, 0.3);
    /// ```
    pub fn set_opacity(&mut self, opacity: f32) -> &mut Self {
        self.fill_opacity = opacity;
        self.stroke_opacity = opacity;
        self
    }

    /// Sets the dash pattern (on/off run lengths); pass an empty slice or use
    /// [`Style::clear_dash`] for a solid stroke.
    ///
    /// ```
    /// use manim_core::style::Style;
    /// let mut s = Style::default();
    /// s.set_dash(&[0.1, 0.05]);
    /// assert_eq!(s.dash_pattern, Some(vec![0.1, 0.05]));
    /// ```
    pub fn set_dash(&mut self, pattern: &[f32]) -> &mut Self {
        self.dash_pattern = if pattern.is_empty() {
            None
        } else {
            Some(pattern.to_vec())
        };
        self
    }

    /// Clears any dash pattern, restoring a solid stroke.
    ///
    /// ```
    /// use manim_core::style::Style;
    /// let mut s = Style::default();
    /// s.set_dash(&[0.1, 0.1]).clear_dash();
    /// assert_eq!(s.dash_pattern, None);
    /// ```
    pub fn clear_dash(&mut self) -> &mut Self {
        self.dash_pattern = None;
        self
    }

    /// The fill color to render, with the fill opacity folded into its alpha, or
    /// `None` if the fill is unset or fully transparent.
    ///
    /// ```
    /// use manim_core::style::Style;
    /// use manim_color::RED;
    /// let mut s = Style::default();
    /// assert_eq!(s.render_fill(), None);
    /// s.set_fill(RED, 0.5);
    /// assert!((s.render_fill().unwrap().a - 0.5).abs() < 1e-6);
    /// ```
    pub fn render_fill(&self) -> Option<Color> {
        let color = self.fill_color?;
        let a = color.a * self.fill_opacity;
        if a <= 0.0 {
            None
        } else {
            Some(color.with_opacity(a))
        }
    }

    /// The stroke color (opacity folded into alpha) and width to render, or
    /// `None` if the stroke is unset, transparent, or zero-width.
    ///
    /// ```
    /// use manim_core::style::Style;
    /// use manim_color::WHITE;
    /// let s = Style::default();
    /// let (color, width) = s.render_stroke().unwrap();
    /// assert_eq!(color, WHITE);
    /// assert_eq!(width, 4.0);
    /// ```
    pub fn render_stroke(&self) -> Option<(Color, f32)> {
        let color = self.stroke_color?;
        let a = color.a * self.stroke_opacity;
        if a <= 0.0 || self.stroke_width <= 0.0 {
            None
        } else {
            Some((color.with_opacity(a), self.stroke_width))
        }
    }

    /// Whether this style would draw nothing (no visible fill and no visible
    /// stroke).
    ///
    /// ```
    /// use manim_core::style::Style;
    /// let mut s = Style::default();
    /// assert!(!s.is_invisible());
    /// s.set_stroke_width(0.0);
    /// assert!(s.is_invisible());
    /// ```
    pub fn is_invisible(&self) -> bool {
        self.render_fill().is_none() && self.render_stroke().is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_color::{BLUE, RED};

    #[test]
    fn set_color_touches_both_channels() {
        let mut s = Style::default();
        s.set_color(RED);
        assert_eq!(s.fill_color, Some(RED));
        assert_eq!(s.stroke_color, Some(RED));
    }

    #[test]
    fn opacity_folds_into_alpha() {
        let mut s = Style::default();
        s.set_fill(BLUE, 0.25);
        let f = s.render_fill().unwrap();
        assert!((f.a - 0.25).abs() < 1e-6);
    }

    #[test]
    fn zero_opacity_hides_fill() {
        let mut s = Style::default();
        s.set_fill(RED, 0.0);
        assert_eq!(s.render_fill(), None);
    }

    #[test]
    fn filled_has_no_stroke() {
        let s = Style::filled(RED);
        assert_eq!(s.render_stroke(), None);
        assert!(s.render_fill().is_some());
    }
}
