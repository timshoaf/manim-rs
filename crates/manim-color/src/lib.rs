//! Colors for `manim_rust`: the [`Color`] type plus the manim CE catalog.
//!
//! This is a placeholder module scaffold; the full implementation is
//! tracked in Linear issue FE-80.

/// An RGBA color stored as linear-light components in `[0, 1]`.
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
    /// Alpha (opacity).
    pub a: f32,
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
}
