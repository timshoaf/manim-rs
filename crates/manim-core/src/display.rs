//! The display-list contract between `manim-core` and a renderer.
//!
//! A [`DisplayList`] is a flat, z-ordered list of [`DrawItem`]s — resolved world
//! -space paths with resolved fill/stroke paint. It is the *only* thing a
//! renderer needs from the core, which keeps both sides independently testable:
//! core tests assert on display lists, renderer golden-tests feed hand-built
//! ones. See `docs/design/01-architecture.md`.
//!
//! Build one with
//! [`SceneState::display_list`](crate::scene_state::SceneState::display_list).

use manim_color::Color;
use manim_math::path::Path;

use crate::mobject::AnyId;

/// A resolved fill for a [`DrawItem`].
///
/// `color`'s alpha already has the style's fill opacity folded in (see
/// [`Style::render_fill`](crate::style::Style::render_fill)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fill {
    /// Fill color with opacity folded into its alpha channel.
    pub color: Color,
}

/// A resolved stroke for a [`DrawItem`].
///
/// `color`'s alpha already has the style's stroke opacity folded in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    /// Stroke color with opacity folded into its alpha channel.
    pub color: Color,
    /// Stroke width in manim's scene-relative points.
    pub width: f32,
}

/// One drawable primitive: a world-space path plus resolved paint.
///
/// `source` and `generation` identify the mobject and its geometry revision, so
/// a renderer can cache tessellation keyed on `(source, generation)`.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawItem {
    /// The world-space geometry to draw.
    pub path: Path,
    /// The resolved fill, or `None` for no fill.
    pub fill: Option<Fill>,
    /// The resolved stroke, or `None` for no stroke.
    pub stroke: Option<Stroke>,
    /// Draw order key; higher draws on top.
    pub z_index: i32,
    /// The mobject this item came from.
    pub source: AnyId,
    /// The source mobject's geometry generation (tessellation cache key).
    pub generation: u64,
}

/// A flat, z-ordered list of [`DrawItem`]s: the core→render contract.
///
/// ```
/// use manim_core::geometry::Circle;
/// use manim_core::scene_state::SceneState;
/// let mut scene = SceneState::new();
/// scene.add(Circle::new());
/// let dl = scene.display_list();
/// assert_eq!(dl.len(), 1);
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DisplayList(pub Vec<DrawItem>);

impl DisplayList {
    /// The number of draw items.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no draw items.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates over the draw items in draw order.
    pub fn iter(&self) -> std::slice::Iter<'_, DrawItem> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a DisplayList {
    type Item = &'a DrawItem;
    type IntoIter = std::slice::Iter<'a, DrawItem>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
