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

/// A perceptual colormap: a scalar in `[0, 1]` → color, for heatmaps, domain
/// coloring, and mesh `set_fill_by_value`.
///
/// The variants are the four workhorse maps: perceptually-uniform sequential
/// [`Viridis`](Self::Viridis) / [`Magma`](Self::Magma), diverging
/// [`Coolwarm`](Self::Coolwarm), and the rainbow-ish [`Turbo`](Self::Turbo). Each
/// is defined by a small set of evenly-spaced sRGB anchors and interpolated —
/// close to the matplotlib originals, exact enough for figures and stable enough
/// to bless goldens against. Both a CPU sampler ([`sample`](Self::sample), for
/// per-vertex mesh coloring) and a GPU LUT ([`lut_rgba8`](Self::lut_rgba8), a
/// 256×1 sRGB texture) read the same anchors.
///
/// ```
/// use manim_core::display::Colormap;
/// // Viridis runs dark purple → yellow.
/// let lo = Colormap::Viridis.sample(0.0).to_srgb_u8();
/// let hi = Colormap::Viridis.sample(1.0).to_srgb_u8();
/// assert!(lo[2] > lo[0]); // dark end is blue-purple
/// assert!(hi[0] > 200 && hi[1] > 200 && hi[2] < 120); // bright end is yellow
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Colormap {
    /// Perceptually-uniform sequential dark-purple → teal → yellow.
    Viridis,
    /// Perceptually-uniform sequential black → magenta → cream.
    Magma,
    /// Diverging blue → white → red (good for signed data around 0).
    Coolwarm,
    /// High-contrast rainbow (Google's Turbo).
    Turbo,
}

impl Colormap {
    /// The evenly-spaced sRGB anchor colors defining this map.
    fn anchors(&self) -> &'static [[f32; 3]] {
        match self {
            Colormap::Viridis => &VIRIDIS,
            Colormap::Magma => &MAGMA,
            Colormap::Coolwarm => &COOLWARM,
            Colormap::Turbo => &TURBO,
        }
    }

    /// The sRGB components at `t ∈ [0, 1]` (clamped), linearly interpolating the
    /// evenly-spaced anchors.
    fn srgb_at(&self, t: f32) -> [f32; 3] {
        let a = self.anchors();
        let n = a.len();
        let t = t.clamp(0.0, 1.0) * (n - 1) as f32;
        let i = (t.floor() as usize).min(n - 2);
        let f = t - i as f32;
        let (lo, hi) = (a[i], a[i + 1]);
        [
            lo[0] + (hi[0] - lo[0]) * f,
            lo[1] + (hi[1] - lo[1]) * f,
            lo[2] + (hi[2] - lo[2]) * f,
        ]
    }

    /// The (opaque, linear-light) [`Color`] at `t ∈ [0, 1]` (clamped).
    pub fn sample(&self, t: f32) -> Color {
        let s = self.srgb_at(t);
        Color::from_srgb(s[0], s[1], s[2])
    }

    /// A 256-entry **sRGB** RGBA8 lookup table (`256 × 4` bytes) for upload as a
    /// `256×1` sRGB texture. The GPU decodes sRGB→linear on sample, matching
    /// [`sample`](Self::sample).
    ///
    /// ```
    /// use manim_core::display::Colormap;
    /// let lut = Colormap::Turbo.lut_rgba8();
    /// assert_eq!(lut.len(), 256 * 4);
    /// assert_eq!(lut[3], 255); // opaque
    /// ```
    pub fn lut_rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256 * 4);
        for i in 0..256 {
            let s = self.srgb_at(i as f32 / 255.0);
            out.push((s[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            out.push((s[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            out.push((s[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            out.push(255);
        }
        out
    }
}

/// Viridis anchors (sRGB), evenly spaced over `[0, 1]`.
#[rustfmt::skip]
const VIRIDIS: [[f32; 3]; 11] = [
    [0.267, 0.005, 0.329], [0.283, 0.131, 0.449], [0.263, 0.242, 0.521],
    [0.221, 0.343, 0.549], [0.177, 0.438, 0.558], [0.143, 0.523, 0.556],
    [0.120, 0.607, 0.540], [0.166, 0.691, 0.497], [0.320, 0.771, 0.411],
    [0.526, 0.833, 0.288], [0.993, 0.906, 0.144],
];

/// Magma anchors (sRGB), evenly spaced over `[0, 1]`.
#[rustfmt::skip]
const MAGMA: [[f32; 3]; 11] = [
    [0.001, 0.000, 0.014], [0.078, 0.054, 0.211], [0.232, 0.059, 0.437],
    [0.390, 0.100, 0.502], [0.550, 0.161, 0.506], [0.716, 0.215, 0.475],
    [0.868, 0.288, 0.409], [0.967, 0.440, 0.360], [0.994, 0.624, 0.427],
    [0.996, 0.795, 0.573], [0.987, 0.991, 0.750],
];

/// Coolwarm (diverging) anchors (sRGB), evenly spaced over `[0, 1]`.
#[rustfmt::skip]
const COOLWARM: [[f32; 3]; 5] = [
    [0.230, 0.299, 0.754], [0.552, 0.690, 0.996], [0.866, 0.866, 0.866],
    [0.968, 0.657, 0.537], [0.706, 0.016, 0.150],
];

/// Turbo anchors (sRGB), evenly spaced over `[0, 1]`.
#[rustfmt::skip]
const TURBO: [[f32; 3]; 11] = [
    [0.190, 0.072, 0.232], [0.275, 0.408, 0.859], [0.180, 0.702, 0.949],
    [0.146, 0.887, 0.730], [0.353, 0.977, 0.457], [0.628, 0.998, 0.235],
    [0.859, 0.921, 0.183], [0.984, 0.739, 0.222], [0.973, 0.462, 0.106],
    [0.842, 0.208, 0.030], [0.480, 0.016, 0.011],
];

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

/// Channel count of a [`TextureData`] field grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldChannels {
    /// One channel per texel (a scalar field), `R32Float`.
    R = 1,
    /// Two channels per texel (a complex/2-vector field), `Rg32Float`.
    Rg = 2,
}

/// A scalar (`R32F`) or 2-vector (`RG32F`) field sampled on a regular grid and
/// pinned to a scene-space rectangle — the input a [`Material`] shades per pixel.
///
/// Callers (e.g. `manim-sci`, or a golden test) sample a closure into
/// [`data`](Self::data) (row-major, `width × height × channels` floats) over the
/// rectangle centered at [`center`](Self::center) with [`size`](Self::size). The
/// renderer uploads it as a float texture and samples it in the quad's UVs.
#[derive(Clone)]
pub struct TextureData {
    /// Grid columns.
    pub width: u32,
    /// Grid rows.
    pub height: u32,
    /// Samples per texel.
    pub channels: FieldChannels,
    /// Row-major samples: `width × height × channels` floats.
    pub data: Vec<f32>,
    /// Scene-space center of the covered rectangle.
    pub center: Point,
    /// Scene-space `(width, height)` of the covered rectangle.
    pub size: [f32; 2],
}

impl std::fmt::Debug for TextureData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextureData")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("channels", &self.channels)
            .field("floats", &self.data.len())
            .field("center", &self.center)
            .field("size", &self.size)
            .finish()
    }
}

/// Iso-contour line overlay parameters for a field material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContourParams {
    /// Value spacing between successive contour lines, in field-value units.
    pub spacing: f32,
    /// Line half-width in output pixels (screen-space, `fwidth`-antialiased).
    pub width: f32,
    /// Line color.
    pub color: Color,
}

/// What a [`Material`] computes per pixel from its field [`TextureData`].
///
/// - [`FieldTexture`](Self::FieldTexture): scalar (R) → colormap LUT, with
///   optional iso-contour lines.
/// - [`PhaseHue`](Self::PhaseHue): complex (RG = re, im) → domain coloring
///   (hue = `arg / 2π`, brightness from log-modulus), with optional modulus
///   banding.
/// - [`Heatmap`](Self::Heatmap): scalar (R) → colormap LUT (no contours).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MaterialKind {
    /// Scalar field → colormap, optional contour lines.
    FieldTexture {
        /// The colormap LUT applied to the normalized scalar.
        colormap: Colormap,
        /// Optional iso-contour overlay.
        contours: Option<ContourParams>,
    },
    /// Complex field → phase-hue domain coloring.
    PhaseHue {
        /// Draw log-modulus contour banding (the "phase portrait" look).
        modulus_contours: bool,
    },
    /// Scalar field → colormap (plain heatmap).
    Heatmap {
        /// The colormap LUT.
        colormap: Colormap,
    },
}

/// A per-pixel GPU material paint: domain coloring, heatmaps, and scalar-field
/// textures (S1, `docs/design/12-scientific-extensions.md`).
///
/// Additive to [`DrawItem`] and mirroring [`ImagePaint`]: when set, the renderer
/// draws the item's quad [`path`](DrawItem::path) through a material pipeline that
/// samples [`texture`](Self::texture) in the quad's UVs and shades each pixel per
/// [`kind`](Self::kind), instead of vector fill. Equality is by texture [`Arc`]
/// identity plus the (small, `Copy`) parameters, so the renderer caches the
/// uploaded field texture cheaply.
#[derive(Clone)]
pub struct Material {
    /// The per-pixel shading model.
    pub kind: MaterialKind,
    /// The field grid sampled per pixel.
    pub texture: Arc<TextureData>,
    /// `[min, max]` field-value range mapped to the colormap / brightness `[0, 1]`.
    pub value_range: [f32; 2],
    /// Overall opacity multiplier in `[0, 1]`.
    pub opacity: f32,
}

impl PartialEq for Material {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && Arc::ptr_eq(&self.texture, &other.texture)
            && self.value_range == other.value_range
            && self.opacity == other.opacity
    }
}

impl std::fmt::Debug for Material {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Material")
            .field("kind", &self.kind)
            .field("texture", &self.texture)
            .field("value_range", &self.value_range)
            .field("opacity", &self.opacity)
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
    /// A per-pixel GPU [`Material`] (domain coloring / heatmap / field texture)
    /// mapped onto the item's quad [`path`](Self::path), or `None`. When set, the
    /// renderer shades the quad through the material pipeline instead of vector
    /// fill (mirrors [`image`](Self::image)).
    pub material: Option<Material>,
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
