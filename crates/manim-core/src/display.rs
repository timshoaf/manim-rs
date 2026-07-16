//! The display-list contract between `manim-core` and a renderer.
//!
//! A [`DisplayList`] is a flat, z-ordered list of [`DrawItem`]s — resolved world
//! -space paths with resolved fill/stroke paint — plus a parallel channel of
//! [`MeshItem`]s for the depth-tested mesh pass. It is the *only* thing a
//! renderer needs from the core, which keeps both sides independently testable:
//! core tests assert on display lists, renderer golden-tests feed hand-built
//! ones. See `docs/design/01-architecture.md` and
//! `docs/design/12-mesh-pipeline.md`.
//!
//! Build one with
//! [`SceneState::display_list`](crate::scene_state::SceneState::display_list).

use std::sync::Arc;

use glam::Mat4;
use manim_color::Color;
use manim_math::path::Path;
use manim_math::Point;

use crate::mesh::{HeightPayload, Instance, MeshMaterial, TriMesh};
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
/// `source` and `generation` identify the mobject and its geometry revision
/// *within one scene*, so a renderer caches tessellation keyed on
/// `(`[`DisplayList::arena`]`, source, generation)` — the arena stamp is what
/// keeps two independently-created scenes apart.
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
    /// Whether this item is depth-tested (read-only) against the mesh pass's
    /// depth buffer, so meshes in front of it occlude it — for 2-D content that
    /// lives *inside* the 3-D scene (contour curves under a surface, wireframe
    /// parameter curves, world-pinned labels). `false` (the default) keeps
    /// today's behavior: vector content draws unconditionally over meshes.
    /// Ignored for image items and `fixed_in_frame` HUD content.
    pub z_test: bool,
    /// Draw order key; higher draws on top.
    pub z_index: i32,
    /// The mobject this item came from.
    pub source: AnyId,
    /// The source mobject's geometry generation (tessellation cache key).
    pub generation: u64,
}

/// One depth-tested triangle mesh to draw: geometry, placement, and appearance.
///
/// This is the mesh-pass counterpart of [`DrawItem`], carried on
/// [`DisplayList`]'s separate [`meshes`](DisplayList::meshes) channel and drawn
/// *before* it — see `docs/design/12-mesh-pipeline.md`. `source` and
/// `generation` identify the mobject and its geometry revision within its scene,
/// so a renderer caches GPU buffers keyed on
/// `(`[`DisplayList::arena`]`, source, generation)` exactly as it caches
/// tessellation for a `DrawItem`.
///
/// The geometry sits behind an [`Arc`], so cloning a display list — which the
/// timeline does per frame — never clones vertex data.
///
/// ```
/// use manim_core::mesh::Mesh;
/// use manim_core::scene_state::SceneState;
/// let mut scene = SceneState::new();
/// let ball = scene.add(Mesh::sphere());
/// let dl = scene.display_list();
/// let item = &dl.meshes()[0];
/// assert_eq!(item.source, ball.erase());
/// assert!(!item.is_translucent());
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct MeshItem {
    /// The shared geometry, in mobject-local space.
    pub mesh: Arc<TriMesh>,
    /// The local → world model matrix.
    pub transform: Mat4,
    /// The resolved surface appearance.
    pub material: MeshMaterial,
    /// Per-instance transforms/colors, drawn in one instanced call; `None` for
    /// a single draw.
    pub instances: Option<Arc<[Instance]>>,
    /// Grid dimensions plus height data for vertex-shader displacement, or
    /// `None` for undisplaced geometry.
    pub height: Option<HeightPayload>,
    /// The mobject this item came from.
    pub source: AnyId,
    /// The source mobject's geometry generation (GPU buffer cache key).
    pub generation: u64,
}

impl MeshItem {
    /// Whether this item belongs in the renderer's translucent queue: its
    /// material is translucent, or any per-vertex color is.
    ///
    /// Opaque items depth-write and need no sorting; translucent ones draw after
    /// them, depth-test read-only, sorted back-to-front.
    ///
    /// ```
    /// use manim_core::mesh::{Mesh, MeshMaterial};
    /// use manim_core::scene_state::SceneState;
    /// use manim_color::BLUE;
    /// let mut scene = SceneState::new();
    /// scene.add(Mesh::sphere().with_material(MeshMaterial::new(BLUE).with_opacity(0.5)));
    /// assert!(scene.display_list().meshes()[0].is_translucent());
    /// ```
    pub fn is_translucent(&self) -> bool {
        self.material.is_translucent()
            || self
                .mesh
                .colors
                .as_ref()
                .is_some_and(|cs| cs.iter().any(|c| c.opacity() < 1.0))
            || self
                .instances
                .as_ref()
                .is_some_and(|xs| xs.iter().any(|i| i.color.opacity() < 1.0))
    }
}

/// The [`arena`](DisplayList::arena) stamp of a list that did not come from a
/// [`SceneState`](crate::scene_state::SceneState) — one built by hand, as
/// renderer tests do.
///
/// Every real scene's stamp is non-zero, so an anonymous list can never be
/// mistaken for a scene's. Anonymous lists do all share this one stamp, which is
/// only sound because their [`source`](DrawItem::source) ids come from somewhere
/// else to begin with; build lists through a `SceneState` if you need two of them
/// to be cached independently.
pub const ANONYMOUS_ARENA: u64 = 0;

/// The core→render contract: a flat, z-ordered list of [`DrawItem`]s plus a
/// parallel channel of [`MeshItem`]s.
///
/// The two channels are separate render paths, not one sorted list: a renderer
/// draws the meshes first (depth-tested, per-pixel shaded), then the draw items
/// over them (painter's algorithm, no depth). 2D vector content is annotation and
/// belongs on top — CE's `add_fixed_in_frame_mobjects` semantics. See
/// `docs/design/12-mesh-pipeline.md` §2.
///
/// The mesh channel is additive: a scene with no meshes produces exactly the
/// display list it did before meshes existed. [`len`](Self::len) and
/// [`is_empty`](Self::is_empty) count **draw items only**, for that reason; use
/// [`meshes`](Self::meshes) for the other channel.
///
/// ```
/// use manim_core::geometry::Circle;
/// use manim_core::mesh::Mesh;
/// use manim_core::scene_state::SceneState;
/// let mut scene = SceneState::new();
/// scene.add(Circle::new());
/// scene.add(Mesh::sphere());
/// let dl = scene.display_list();
/// assert_eq!(dl.len(), 1); // the circle
/// assert_eq!(dl.meshes().len(), 1); // the sphere
/// ```
///
/// # Cache identity
///
/// The third field is the [`arena`](Self::arena) stamp of the
/// [`SceneState`](crate::scene_state::SceneState) that produced the list. An
/// item's `(source, generation)` is only unique *within* one scene — a fresh
/// scene's first mobject always lands at the same arena key with generation `0`
/// — so a renderer must key its caches on `(arena, source, generation)` to avoid
/// serving one scene's buffers to another. See
/// [`SceneState::arena`](crate::scene_state::SceneState::arena).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DisplayList(pub Vec<DrawItem>, pub Vec<MeshItem>, pub u64);

impl DisplayList {
    /// A display list of draw items only, not attributed to any scene
    /// ([`ANONYMOUS_ARENA`]).
    ///
    /// ```
    /// use manim_core::display::{DisplayList, ANONYMOUS_ARENA};
    /// let dl = DisplayList::new(Vec::new());
    /// assert!(dl.is_empty());
    /// assert_eq!(dl.arena(), ANONYMOUS_ARENA);
    /// ```
    pub fn new(items: Vec<DrawItem>) -> Self {
        Self(items, Vec::new(), ANONYMOUS_ARENA)
    }

    /// A display list of both channels, not attributed to any scene
    /// ([`ANONYMOUS_ARENA`]).
    pub fn with_meshes(items: Vec<DrawItem>, meshes: Vec<MeshItem>) -> Self {
        Self(items, meshes, ANONYMOUS_ARENA)
    }

    /// Attributes this list to the scene arena `arena` (builder).
    ///
    /// [`SceneState::display_list`](crate::scene_state::SceneState::display_list)
    /// applies this; call it yourself only when hand-building a list that must
    /// cache as if it came from a particular scene.
    pub fn in_arena(mut self, arena: u64) -> Self {
        self.2 = arena;
        self
    }

    /// The stamp of the scene arena this list's items belong to, or
    /// [`ANONYMOUS_ARENA`] for a hand-built list.
    ///
    /// A renderer cache key must include this: `(source, generation)` alone
    /// collides across independently-created scenes.
    ///
    /// ```
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::geometry::Circle;
    /// let (mut a, mut b) = (SceneState::new(), SceneState::new());
    /// a.add(Circle::new());
    /// b.add(Circle::new());
    /// // Same arena key, same generation — told apart only by the arena stamp.
    /// assert_eq!(a.display_list().0[0].source, b.display_list().0[0].source);
    /// assert_eq!(a.display_list().0[0].generation, b.display_list().0[0].generation);
    /// assert_ne!(a.display_list().arena(), b.display_list().arena());
    /// ```
    pub fn arena(&self) -> u64 {
        self.2
    }

    /// The number of draw items (the mesh channel is counted by
    /// [`meshes`](Self::meshes)).
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

    /// The mesh items, drawn before the draw items.
    pub fn meshes(&self) -> &[MeshItem] {
        &self.1
    }

    /// Mutable access to the mesh items.
    pub fn meshes_mut(&mut self) -> &mut Vec<MeshItem> {
        &mut self.1
    }
}

impl<'a> IntoIterator for &'a DisplayList {
    type Item = &'a DrawItem;
    type IntoIter = std::slice::Iter<'a, DrawItem>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
