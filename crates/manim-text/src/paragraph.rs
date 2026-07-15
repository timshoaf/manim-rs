//! [`Paragraph`]: a multi-line [`Text`] constructor with alignment.

use crate::text::{Alignment, Text};

/// A factory for multi-line [`Text`]. `Paragraph::new(&["a", "b"])` is
/// `Text::new("a\nb")` with an alignment. Port of manim CE's `Paragraph`.
///
/// ```
/// use manim_text::{Alignment, Paragraph};
/// use manim_core::mobject::MobjectExt;
/// let p = Paragraph::new(&["Hello", "World"]);
/// // Two lines stack vertically → taller than one line.
/// assert!(p.bounding_box().height() > Paragraph::new(&["Hello"]).bounding_box().height());
/// let _ = Alignment::Center;
/// ```
pub struct Paragraph;

impl Paragraph {
    /// A left-aligned paragraph from the given lines.
    ///
    /// `Paragraph` is a factory, so `new` intentionally returns a [`Text`].
    #[allow(clippy::new_ret_no_self)]
    pub fn new(lines: &[&str]) -> Text {
        Self::aligned(lines, Alignment::Left)
    }

    /// A paragraph with an explicit [`Alignment`].
    ///
    /// ```
    /// use manim_text::{Alignment, Paragraph};
    /// let p = Paragraph::aligned(&["short", "much longer line"], Alignment::Center);
    /// assert_eq!(p.glyph_count() > 0, true);
    /// ```
    pub fn aligned(lines: &[&str], alignment: Alignment) -> Text {
        Text::new(lines.join("\n")).alignment(alignment)
    }
}
