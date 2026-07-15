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
use manim_math::Point;

/// manim CE's default stroke width in scene-relative "points".
pub const DEFAULT_STROKE_WIDTH: f32 = 4.0;

/// The axis a [`Gradient`] runs along.
///
/// [`Horizontal`](Self::Horizontal) and [`Vertical`](Self::Vertical) are
/// resolved against the mobject's bounding box when the display list is built,
/// so the gradient follows the mobject as it moves; [`Points`](Self::Points)
/// pins it to explicit world-space endpoints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientAxis {
    /// Left → right across the bounding box (manim's default gradient).
    Horizontal,
    /// Bottom → top across the bounding box.
    Vertical,
    /// An explicit world-space axis `(start, end)`.
    Points(Point, Point),
}

/// A multi-stop linear gradient paint (port of manim's `set_color_by_gradient`).
///
/// Stops are `(position, color)` with `position` in `[0, 1]` along the
/// [`axis`](Self::axis). Colors are evaluated per vertex at tessellation time.
///
/// ```
/// use manim_core::style::{Gradient, GradientAxis};
/// use manim_color::{BLUE, RED};
/// let g = Gradient::from_colors(&[BLUE, RED]);
/// assert_eq!(g.axis, GradientAxis::Horizontal);
/// assert_eq!(g.stops.len(), 2);
/// assert_eq!(g.stops[0].0, 0.0);
/// assert_eq!(g.stops[1].0, 1.0);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    /// The `(position, color)` stops, `position ∈ [0, 1]`.
    pub stops: Vec<(f32, Color)>,
    /// The axis the gradient runs along.
    pub axis: GradientAxis,
}

impl Gradient {
    /// Builds a horizontal gradient with `colors` spread evenly over `[0, 1]`.
    ///
    /// A single color yields one stop; an empty slice yields no stops.
    ///
    /// ```
    /// use manim_core::style::Gradient;
    /// use manim_color::{BLUE, GREEN, RED};
    /// let g = Gradient::from_colors(&[BLUE, GREEN, RED]);
    /// assert_eq!(g.stops[1].0, 0.5); // middle color at the midpoint
    /// ```
    pub fn from_colors(colors: &[Color]) -> Self {
        let n = colors.len();
        let stops = colors
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let pos = if n <= 1 {
                    0.0
                } else {
                    i as f32 / (n - 1) as f32
                };
                (pos, c)
            })
            .collect();
        Self {
            stops,
            axis: GradientAxis::Horizontal,
        }
    }

    /// Returns a copy with `opacity` folded into every stop's alpha (used when
    /// the display list bakes fill/stroke opacity into the paint).
    pub fn with_opacity(&self, opacity: f32) -> Gradient {
        Gradient {
            stops: self
                .stops
                .iter()
                .map(|(p, c)| (*p, c.with_opacity(c.a * opacity)))
                .collect(),
            axis: self.axis,
        }
    }
}

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
    /// Optional multi-stop fill gradient. When set (and the fill is visible), it
    /// paints the fill instead of the solid [`fill_color`](Self::fill_color).
    pub fill_gradient: Option<Gradient>,
    /// Optional multi-stop stroke gradient, analogous to
    /// [`fill_gradient`](Self::fill_gradient).
    pub stroke_gradient: Option<Gradient>,
    /// Background stroke color — a stroke drawn *behind* the fill (manim's
    /// `background_stroke`, used to outline text). `None` disables it.
    pub background_stroke_color: Option<Color>,
    /// Background stroke width in scene-relative points.
    pub background_stroke_width: f32,
    /// Background stroke opacity in `[0, 1]`.
    pub background_stroke_opacity: f32,
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
            fill_gradient: None,
            stroke_gradient: None,
            background_stroke_color: None,
            background_stroke_width: DEFAULT_STROKE_WIDTH,
            background_stroke_opacity: 1.0,
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
            ..Self::default()
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

    /// Sets a fill gradient, making the fill visible if it was not (port of
    /// manim's `set_fill(gradient)`). Unset fill color/opacity default to the
    /// first stop and fully opaque.
    ///
    /// ```
    /// use manim_core::style::Style;
    /// use manim_color::{BLUE, RED};
    /// let mut s = Style::default();
    /// s.set_fill_gradient(manim_core::style::Gradient::from_colors(&[BLUE, RED]));
    /// assert!(s.render_fill().is_some());
    /// assert!(s.fill_gradient.is_some());
    /// ```
    pub fn set_fill_gradient(&mut self, gradient: Gradient) -> &mut Self {
        if self.fill_color.is_none() {
            self.fill_color = gradient.stops.first().map(|(_, c)| *c);
        }
        if self.fill_opacity <= 0.0 {
            self.fill_opacity = 1.0;
        }
        self.fill_gradient = Some(gradient);
        self
    }

    /// Applies a color ramp as a gradient (port of manim's
    /// `set_color_by_gradient`): gradients whichever of fill/stroke are already
    /// visible; if neither is, colors the fill.
    ///
    /// ```
    /// use manim_core::style::Style;
    /// use manim_color::{BLUE, RED};
    /// // A stroked-only style gradients the stroke.
    /// let mut s = Style::stroked(BLUE);
    /// s.set_color_by_gradient(&[BLUE, RED]);
    /// assert!(s.stroke_gradient.is_some());
    /// assert!(s.fill_gradient.is_none());
    /// ```
    pub fn set_color_by_gradient(&mut self, colors: &[Color]) -> &mut Self {
        if colors.is_empty() {
            return self;
        }
        let g = Gradient::from_colors(colors);
        let mut touched = false;
        if self.fill_color.is_some() {
            self.fill_gradient = Some(g.clone());
            touched = true;
        }
        if self.stroke_color.is_some() {
            self.stroke_gradient = Some(g.clone());
            touched = true;
        }
        if !touched {
            self.fill_color = Some(colors[0]);
            self.fill_opacity = 1.0;
            self.fill_gradient = Some(g);
        }
        self
    }

    /// Sets the background stroke (a stroke drawn behind the fill), port of
    /// manim's `set_background_stroke`.
    ///
    /// ```
    /// use manim_core::style::Style;
    /// use manim_color::BLACK;
    /// let mut s = Style::default();
    /// s.set_background_stroke(BLACK, 6.0, 1.0);
    /// assert_eq!(s.render_background_stroke(), Some((BLACK, 6.0)));
    /// ```
    pub fn set_background_stroke(&mut self, color: Color, width: f32, opacity: f32) -> &mut Self {
        self.background_stroke_color = Some(color);
        self.background_stroke_width = width;
        self.background_stroke_opacity = opacity;
        self
    }

    /// The visible fill gradient with fill opacity folded into its stops, or
    /// `None` if there is no gradient or the fill is not visible.
    pub fn render_fill_gradient(&self) -> Option<Gradient> {
        let g = self.fill_gradient.as_ref()?;
        self.render_fill()?;
        Some(g.with_opacity(self.fill_opacity))
    }

    /// The visible stroke gradient with stroke opacity folded into its stops, or
    /// `None` if there is no gradient or the stroke is not visible.
    pub fn render_stroke_gradient(&self) -> Option<Gradient> {
        let g = self.stroke_gradient.as_ref()?;
        self.render_stroke()?;
        Some(g.with_opacity(self.stroke_opacity))
    }

    /// The background stroke color (opacity folded into alpha) and width, or
    /// `None` if it is unset, transparent, or zero-width.
    ///
    /// ```
    /// use manim_core::style::Style;
    /// let s = Style::default();
    /// assert_eq!(s.render_background_stroke(), None);
    /// ```
    pub fn render_background_stroke(&self) -> Option<(Color, f32)> {
        let color = self.background_stroke_color?;
        let a = color.a * self.background_stroke_opacity;
        if a <= 0.0 || self.background_stroke_width <= 0.0 {
            None
        } else {
            Some((color.with_opacity(a), self.background_stroke_width))
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
