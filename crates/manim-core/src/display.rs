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

use std::sync::Arc;

use manim_color::Color;
use manim_math::path::Path;
use manim_math::Point;

use crate::mobject::AnyId;

/// Raw RGBA8 pixel data for an [`ImagePaint`], row-major, 4 bytes per pixel.
#[derive(Clone, PartialEq)]
pub struct ImageData {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height × 4` RGBA bytes (sRGB, straight alpha).
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for ImageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageData")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.rgba.len())
            .finish()
    }
}

/// Texture sampling mode for an [`ImagePaint`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sampler {
    /// Bilinear filtering (smooth); manim's default.
    Linear,
    /// Nearest-neighbor (crisp pixels).
    Nearest,
}

/// A resolved raster-image paint: the pixels plus the sampler. Carried by an
/// [`ImageMobject`](crate::image_mobject::ImageMobject)'s [`DrawItem`], whose
/// `path` is the world-space quad the texture maps onto.
///
/// Equality is by pixel-buffer identity ([`Arc`] pointer) plus sampler, so a
/// renderer can cache the uploaded texture cheaply.
#[derive(Clone)]
pub struct ImagePaint {
    /// The shared pixel data.
    pub data: Arc<ImageData>,
    /// How the texture is sampled.
    pub sampler: Sampler,
}

impl PartialEq for ImagePaint {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data) && self.sampler == other.sampler
    }
}

impl std::fmt::Debug for ImagePaint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImagePaint")
            .field("data", &self.data)
            .field("sampler", &self.sampler)
            .finish()
    }
}

/// A resolved linear gradient: `(position, color)` stops along a world-space
/// axis, with opacity already folded into each stop's alpha.
///
/// The renderer evaluates it per vertex by projecting the vertex position onto
/// the `start → end` axis. Produced from a [`Gradient`](crate::style::Gradient)
/// when the display list is built (its bounding-box-relative axis resolved to
/// concrete world points).
///
/// ```
/// use manim_core::display::LinearGradient;
/// use manim_color::{BLUE, RED};
/// use manim_math::{Point, RIGHT};
/// let g = LinearGradient {
///     stops: vec![(0.0, BLUE), (1.0, RED)],
///     start: Point::ZERO,
///     end: RIGHT,
/// };
/// // Midpoint is the linear blend of the endpoints.
/// assert_eq!(g.color_at(Point::new(0.5, 0.0, 0.0)), BLUE.interpolate(&RED, 0.5));
/// // Off-axis points project onto the axis.
/// assert_eq!(g.color_at(Point::new(0.5, 9.0, 0.0)), BLUE.interpolate(&RED, 0.5));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradient {
    /// `(position, color)` stops with `position ∈ [0, 1]`, opacity folded in.
    pub stops: Vec<(f32, Color)>,
    /// World-space axis start (gradient position `0`).
    pub start: Point,
    /// World-space axis end (gradient position `1`).
    pub end: Point,
}

impl LinearGradient {
    /// The color at world-space point `p`, projecting it onto the gradient axis
    /// and interpolating the stops in linear space.
    ///
    /// A degenerate axis (`start == end`) or empty stop list falls back to the
    /// first stop (or transparent black if there are none).
    pub fn color_at(&self, p: Point) -> Color {
        let Some(&(_, first)) = self.stops.first() else {
            return Color::from_rgba(0.0, 0.0, 0.0, 0.0);
        };
        let axis = self.end - self.start;
        let len2 = axis.length_squared();
        let t = if len2 <= 1e-12 {
            0.0
        } else {
            ((p - self.start).dot(axis) / len2).clamp(0.0, 1.0)
        };
        // Find the bracketing stops and interpolate.
        let mut lo = (0.0_f32, first);
        let mut hi = *self.stops.last().unwrap();
        for w in self.stops.windows(2) {
            if t >= w[0].0 && t <= w[1].0 {
                lo = w[0];
                hi = w[1];
                break;
            }
        }
        let span = hi.0 - lo.0;
        let local = if span <= 1e-9 {
            0.0
        } else {
            ((t - lo.0) / span).clamp(0.0, 1.0)
        };
        lo.1.interpolate(&hi.1, local)
    }
}

/// A resolved fill for a [`DrawItem`].
///
/// `color`'s alpha already has the style's fill opacity folded in (see
/// [`Style::render_fill`](crate::style::Style::render_fill)). When
/// [`gradient`](Self::gradient) is set it paints the fill per vertex; `color` is
/// then a representative solid used as a fallback.
#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    /// Fill color with opacity folded into its alpha channel.
    pub color: Color,
    /// Optional per-vertex gradient (overrides `color` when present).
    pub gradient: Option<LinearGradient>,
}

/// A resolved stroke for a [`DrawItem`].
///
/// `color`'s alpha already has the style's stroke opacity folded in. When
/// [`gradient`](Self::gradient) is set it paints the stroke per vertex.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    /// Stroke color with opacity folded into its alpha channel.
    pub color: Color,
    /// Stroke width in manim's scene-relative points.
    pub width: f32,
    /// Optional per-vertex gradient (overrides `color` when present).
    pub gradient: Option<LinearGradient>,
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
    /// A stroke drawn *behind* the fill (manim's background stroke), or `None`.
    /// Renderers must draw this before the fill so it reads as an outline.
    pub background_stroke: Option<Stroke>,
    /// A raster image mapped onto the item's quad [`path`](Self::path), or
    /// `None`. When set, the renderer draws the textured quad (respecting
    /// `z_index`) instead of vector fill.
    pub image: Option<ImagePaint>,
    /// Whether this item is fixed in the camera frame (a HUD overlay). Under a
    /// 3-D camera the renderer draws it with the orthographic matrix instead of
    /// the perspective one, so it stays flat and unmoving; ignored in 2-D.
    pub fixed_in_frame: bool,
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
