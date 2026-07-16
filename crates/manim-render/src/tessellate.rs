//! CPU tessellation: [`DisplayList`] → a flat GPU-ready triangle mesh.
//!
//! Each [`DrawItem`]'s bezier [`Path`] is fed to
//! [lyon](https://docs.rs/lyon): its [`FillTessellator`] fills closed regions
//! with the non-zero winding rule and its [`StrokeTessellator`] outlines the
//! path. Both emit [`Vertex`]es carrying **premultiplied linear** color, ready
//! to blend on the GPU without further conversion.
//!
//! Tessellation is the renderer's hot path during animation, so a
//! [`TessellationCache`] memoizes each mobject's mesh keyed on its
//! `(`[`arena`](manim_core::display::DisplayList::arena)`, source, generation)`:
//! a mobject whose geometry has not changed reuses last frame's triangles
//! instead of re-tessellating. The combined per-frame result is a [`FrameMesh`]
//! whose items are concatenated back-to-front by `z_index` (painter's algorithm
//! — there is no depth buffer).
//!
//! ```
//! use manim_core::geometry::Circle;
//! use manim_core::scene_state::SceneState;
//! use manim_render::tessellate::TessellationCache;
//!
//! let mut scene = SceneState::new();
//! scene.add(Circle::new());
//! let mut cache = TessellationCache::new();
//! let mesh = cache.tessellate(&scene.display_list());
//! assert!(!mesh.is_empty());
//! ```

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use lyon::math::point;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use manim_color::Color;
use manim_core::display::{DisplayList, DrawItem, ImagePaint, LinearGradient};
use manim_core::mobject::AnyId;
use manim_math::path::Path;
use manim_math::Point;

use crate::camera::Camera2D;

/// Scene units of stroke width per manim CE "stroke point".
///
/// manim CE measures stroke width in points where the `VMobject` default of
/// `4.0` renders as a hairline; empirically `4` points reads as about `0.04`
/// scene units at the default frame zoom, giving `0.01` scene units per point.
/// [`StrokeStyle`](manim_core::display::Stroke)'s width is multiplied by this
/// before tessellation.
///
/// ```
/// use manim_render::tessellate::STROKE_WIDTH_CONVERSION;
/// // The CE default stroke width of 4.0 becomes 0.04 scene units.
/// assert!((4.0 * STROKE_WIDTH_CONVERSION - 0.04).abs() < 1e-6);
/// ```
pub const STROKE_WIDTH_CONVERSION: f32 = 0.01;

/// Default flattening tolerance in scene units.
///
/// The maximum distance, in scene units, between a tessellated curve and the
/// true bezier. Smaller is smoother but heavier; `0.005` keeps curves visually
/// exact at the default frame size.
pub const DEFAULT_TOLERANCE: f32 = 0.005;

/// The frame height (scene units) the [default tolerance](DEFAULT_TOLERANCE) is
/// calibrated for — manim's default `8.0`. Zoom buckets are measured relative to
/// this (see [`TessellationCache::set_zoom`]).
pub const REFERENCE_FRAME_HEIGHT: f32 = 8.0;

/// The zoom bucket for a visible `frame_height`: `round(log2(height / 8))`.
///
/// Each unit is a 2× zoom step, so tessellation only re-runs when zoom crosses a
/// doubling — cheap and visually sufficient (per `docs/design/05-rendering.md`).
/// Zooming *in* (smaller height) gives negative buckets and a finer tolerance.
///
/// ```
/// use manim_render::tessellate::{zoom_bucket_for, REFERENCE_FRAME_HEIGHT};
/// assert_eq!(zoom_bucket_for(REFERENCE_FRAME_HEIGHT), 0);
/// assert_eq!(zoom_bucket_for(4.0), -1); // 2× zoom in
/// assert_eq!(zoom_bucket_for(16.0), 1); // 2× zoom out
/// ```
pub fn zoom_bucket_for(frame_height: f32) -> i32 {
    if frame_height <= 0.0 || !frame_height.is_finite() {
        return 0;
    }
    (frame_height / REFERENCE_FRAME_HEIGHT).log2().round() as i32
}

/// A single tessellated vertex: 2-D position and premultiplied-linear color.
///
/// The layout is `#[repr(C)]` and [`bytemuck::Pod`] so a slice of vertices
/// uploads to a GPU vertex buffer with `bytemuck::cast_slice` and no copies.
///
/// ```
/// use manim_render::tessellate::Vertex;
/// let v = Vertex { position: [1.0, 2.0, 0.0], color: [0.5, 0.0, 0.0, 0.5] };
/// assert_eq!(v.position, [1.0, 2.0, 0.0]);
/// ```
///
/// The position is 3-D so 3D scenes project correctly; the per-vertex `z` is
/// carried through lyon as an interpolated path attribute. For a 2D scene every
/// `z` is `0.0`, so the orthographic camera produces byte-identical output.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// World-space position `[x, y, z]` in scene units.
    pub position: [f32; 3],
    /// Premultiplied linear RGBA color.
    pub color: [f32; 4],
}

/// A standalone triangle mesh: interleaved [`Vertex`]es plus `u32` indices.
///
/// This is the unit the [`TessellationCache`] stores per mobject and the shape
/// of a whole [`FrameMesh`]. An empty mesh (no fill and no stroke, or a
/// degenerate path) is valid and draws nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    /// The mesh vertices.
    pub vertices: Vec<Vertex>,
    /// Triangle indices into [`vertices`](Self::vertices), three per triangle.
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Whether the mesh has no triangles.
    ///
    /// ```
    /// use manim_render::tessellate::MeshData;
    /// assert!(MeshData::default().is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Appends `other`'s geometry onto `self`, rebasing its indices.
    fn append(&mut self, other: &MeshData) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }
}

/// The combined mesh for one frame, in painter's (back-to-front) order.
///
/// Produced by [`TessellationCache::tessellate`]; items are concatenated by
/// ascending `z_index` so later triangles paint over earlier ones. Feed
/// [`vertices`](Self::vertices) and [`indices`](Self::indices) straight to the
/// GPU.
pub type FrameMesh = MeshData;

/// One cached mobject mesh together with the generation, zoom bucket, and paint
/// hash it was built from.
struct CacheEntry {
    generation: u64,
    bucket: i32,
    paint: u64,
    mesh: MeshData,
}

/// A cheap order-sensitive hash of a [`DrawItem`]'s paint (fill, stroke, and
/// background stroke, colors + gradients + widths).
///
/// Vertex colors are baked at tessellation time, and mobject `generation` bumps
/// only on *geometry* change — so paint edits (a new fill color, a gradient)
/// must invalidate the cached mesh too. Hashing the paint catches every such
/// change without touching core's generation semantics.
fn paint_hash(item: &DrawItem) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();

    fn hash_color(h: &mut impl Hasher, c: &Color) {
        for v in [c.r, c.g, c.b, c.a] {
            h.write_u32(v.to_bits());
        }
    }
    fn hash_gradient(h: &mut impl Hasher, g: &Option<LinearGradient>) {
        match g {
            None => h.write_u8(0),
            Some(g) => {
                h.write_u8(1);
                for (p, c) in &g.stops {
                    h.write_u32(p.to_bits());
                    hash_color(h, c);
                }
                for v in [g.start.x, g.start.y, g.end.x, g.end.y] {
                    h.write_u32(v.to_bits());
                }
            }
        }
    }
    fn hash_stroke(h: &mut impl Hasher, width: f32, color: &Color, g: &Option<LinearGradient>) {
        h.write_u32(width.to_bits());
        hash_color(h, color);
        hash_gradient(h, g);
    }

    match &item.fill {
        None => h.write_u8(0),
        Some(f) => {
            h.write_u8(1);
            hash_color(&mut h, &f.color);
            hash_gradient(&mut h, &f.gradient);
        }
    }
    match &item.stroke {
        None => h.write_u8(0),
        Some(s) => {
            h.write_u8(1);
            hash_stroke(&mut h, s.width, &s.color, &s.gradient);
        }
    }
    match &item.background_stroke {
        None => h.write_u8(0),
        Some(s) => {
            h.write_u8(1);
            hash_stroke(&mut h, s.width, &s.color, &s.gradient);
        }
    }
    h.finish()
}

/// A tessellation cache key: which scene arena, and which mobject within it.
///
/// See [`DisplayList::arena`](manim_core::display::DisplayList::arena) for why
/// the mobject id alone is not enough.
type CacheKey = (u64, AnyId);

/// Memoizes per-mobject tessellation, keyed on `(arena, source, generation, zoom
/// bucket, paint hash)`.
///
/// Re-tessellation happens only when a mobject's `generation` (geometry)
/// changes, the camera zoom crosses a bucket boundary (see
/// [`set_zoom`](Self::set_zoom)), or its paint changes (colors are baked into
/// vertices, and paint edits don't bump `generation`). Entries for mobjects that
/// vanish from the display list are evicted. [`hits`] and [`misses`] count cache
/// outcomes for tests and diagnostics.
///
/// The [`arena`](manim_core::display::DisplayList::arena) half of the key is what
/// keeps two scenes apart: `source` is a slot-map key that restarts per scene and
/// a fresh mobject's `generation` is `0`, so two independently-built scenes hand
/// their first mobject identical identity. Without the arena stamp, rendering one
/// scene and then another through the same cache would silently reuse the first
/// scene's triangles.
///
/// [`hits`]: TessellationCache::hits
/// [`misses`]: TessellationCache::misses
///
/// ```
/// use manim_core::geometry::Square;
/// use manim_core::scene_state::SceneState;
/// use manim_render::tessellate::TessellationCache;
///
/// let mut scene = SceneState::new();
/// scene.add(Square::new());
/// let dl = scene.display_list();
///
/// let mut cache = TessellationCache::new();
/// cache.tessellate(&dl); // cold: a miss
/// cache.tessellate(&dl); // warm: a hit, reusing the mesh
/// assert_eq!(cache.hits(), 1);
/// assert_eq!(cache.misses(), 1);
/// ```
pub struct TessellationCache {
    entries: HashMap<CacheKey, CacheEntry>,
    /// Tolerance at the reference zoom (bucket 0).
    base_tolerance: f32,
    /// Effective tolerance for the current zoom bucket.
    tolerance: f32,
    /// The current zoom bucket (see [`set_zoom`](Self::set_zoom)).
    zoom_bucket: i32,
    hits: u64,
    misses: u64,
}

impl Default for TessellationCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TessellationCache {
    /// A cache with the [default tolerance](DEFAULT_TOLERANCE).
    ///
    /// ```
    /// use manim_render::tessellate::TessellationCache;
    /// let cache = TessellationCache::new();
    /// assert_eq!(cache.len(), 0);
    /// ```
    pub fn new() -> Self {
        Self::with_tolerance(DEFAULT_TOLERANCE)
    }

    /// A cache with an explicit flattening `tolerance` in scene units.
    ///
    /// ```
    /// use manim_render::tessellate::TessellationCache;
    /// let cache = TessellationCache::with_tolerance(0.001);
    /// assert_eq!(cache.len(), 0);
    /// ```
    pub fn with_tolerance(tolerance: f32) -> Self {
        Self {
            entries: HashMap::new(),
            base_tolerance: tolerance,
            tolerance,
            zoom_bucket: 0,
            hits: 0,
            misses: 0,
        }
    }

    /// Adapts the tessellation tolerance to the camera zoom, given the visible
    /// `frame_height` in scene units.
    ///
    /// Tolerance is quantized by [`zoom_bucket_for`]: it changes only when zoom
    /// crosses a 2× boundary, and any bucket change invalidates cached meshes so
    /// they re-tessellate at the new fidelity. Call this once per frame before
    /// [`tessellate`](Self::tessellate) when following an animated camera.
    ///
    /// ```
    /// use manim_render::tessellate::{TessellationCache, REFERENCE_FRAME_HEIGHT};
    /// let mut cache = TessellationCache::new();
    /// cache.set_zoom(REFERENCE_FRAME_HEIGHT / 4.0); // 4× zoom in
    /// assert_eq!(cache.zoom_bucket(), -2);
    /// ```
    pub fn set_zoom(&mut self, frame_height: f32) {
        let bucket = zoom_bucket_for(frame_height);
        self.zoom_bucket = bucket;
        self.tolerance = (self.base_tolerance * 2.0_f32.powi(bucket)).max(f32::MIN_POSITIVE);
    }

    /// The current zoom bucket (0 at the reference height).
    pub fn zoom_bucket(&self) -> i32 {
        self.zoom_bucket
    }

    /// The effective flattening tolerance at the current zoom.
    pub fn tolerance(&self) -> f32 {
        self.tolerance
    }

    /// The number of cached mobject meshes.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds no meshes.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The number of cache hits since construction (reused meshes).
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// The number of cache misses since construction (re-tessellations).
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Tessellates `list` into one [`FrameMesh`], reusing cached meshes where
    /// the `(source, generation)` key is unchanged and evicting entries for
    /// mobjects no longer present.
    ///
    /// Items are emitted back-to-front by `z_index` (ties keep list order), so
    /// the returned mesh can be drawn with a single pass and no depth test.
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::scene_state::SceneState;
    /// use manim_render::tessellate::TessellationCache;
    ///
    /// let mut scene = SceneState::new();
    /// scene.add(Circle::new());
    /// let mut cache = TessellationCache::new();
    /// let mesh = cache.tessellate(&scene.display_list());
    /// assert!(!mesh.indices.is_empty());
    /// ```
    pub fn tessellate(&mut self, list: &DisplayList) -> FrameMesh {
        self.refresh(list);

        // Concatenate in painter's order: ascending z, stable within a tie.
        let mut order: Vec<(usize, &DrawItem)> = list.iter().enumerate().collect();
        order.sort_by_key(|(i, it)| (it.z_index, *i));

        let mut frame = FrameMesh::default();
        for (_, item) in order {
            if let Some(entry) = self.entries.get(&(list.arena(), item.source)) {
                frame.append(&entry.mesh);
            }
        }
        frame
    }

    /// Refreshes the per-mobject mesh cache for `list`: re-tessellates items
    /// whose `(generation, zoom bucket, paint)` changed and evicts vanished
    /// ones. Shared by [`tessellate`](Self::tessellate) and
    /// [`tessellate_ops`](Self::tessellate_ops).
    fn refresh(&mut self, list: &DisplayList) {
        let arena = list.arena();
        let mut present: Vec<CacheKey> = Vec::with_capacity(list.len());
        for item in list {
            present.push((arena, item.source));
            let paint = paint_hash(item);
            let stale = self
                .entries
                .get(&(arena, item.source))
                .map(|e| {
                    e.generation != item.generation
                        || e.bucket != self.zoom_bucket
                        || e.paint != paint
                })
                .unwrap_or(true);
            if stale {
                self.misses += 1;
                let mesh = tessellate_item(item, self.tolerance);
                self.entries.insert(
                    (arena, item.source),
                    CacheEntry {
                        generation: item.generation,
                        bucket: self.zoom_bucket,
                        paint,
                        mesh,
                    },
                );
            } else {
                self.hits += 1;
            }
        }
        self.entries.retain(|key, _| present.contains(key));
    }

    /// Builds an ordered list of [`FrameOp`]s in painter's order, batching
    /// consecutive vector items into one mesh and emitting a separate
    /// [`ImageQuad`] for each image item — so raster images draw interleaved
    /// with vector shapes, respecting `z_index`.
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::scene_state::SceneState;
    /// use manim_render::tessellate::{FrameOp, TessellationCache};
    ///
    /// let mut scene = SceneState::new();
    /// scene.add(Circle::new());
    /// let mut cache = TessellationCache::new();
    /// let ops = cache.tessellate_ops(&scene.display_list());
    /// // No images → a single vector batch.
    /// assert_eq!(ops.len(), 1);
    /// assert!(matches!(ops[0], FrameOp::Vector(_)));
    /// ```
    pub fn tessellate_ops(&mut self, list: &DisplayList) -> Vec<FrameOp> {
        self.refresh(list);

        let mut order: Vec<(usize, &DrawItem)> = list.iter().enumerate().collect();
        order.sort_by_key(|(i, it)| (it.z_index, *i));
        self.build_ops(list.arena(), order.into_iter().map(|(_, it)| it))
    }

    /// Builds ops for a 3-D camera: world content depth-sorted by per-item mean
    /// *camera-space* z (painter's algorithm — farthest first, for correct
    /// translucent blending without a depth buffer), plus a `fixed_in_frame` HUD
    /// overlay to draw last with the orthographic matrix.
    ///
    /// Image quads join the same world depth sort as vector items. HUD items keep
    /// `z_index` order. For a 2-D camera prefer [`tessellate_ops`](Self::tessellate_ops),
    /// which is byte-identical to the pre-3-D renderer.
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::scene_state::SceneState;
    /// use manim_render::camera::Camera2D;
    /// use manim_render::tessellate::TessellationCache;
    /// use manim_core::camera::ThreeDParams;
    ///
    /// let mut scene = SceneState::new();
    /// scene.add(Circle::new());
    /// let mut cam = Camera2D::from(&manim_core::config::Config::default());
    /// cam.three_d = Some(ThreeDParams::default());
    /// let mut cache = TessellationCache::new();
    /// let frame = cache.tessellate_ops_layered(&scene.display_list(), &cam);
    /// assert_eq!(frame.world.len(), 1);
    /// assert!(frame.hud.is_empty());
    /// ```
    pub fn tessellate_ops_layered(
        &mut self,
        list: &DisplayList,
        camera: &Camera2D,
    ) -> LayeredFrame {
        self.refresh(list);
        let view = camera.view_matrix();

        // World content sorts back-to-front by camera-space z; HUD keeps z_index.
        let mut world: Vec<(f32, usize, &DrawItem)> = Vec::new();
        let mut hud: Vec<(i32, usize, &DrawItem)> = Vec::new();
        for (i, item) in list.iter().enumerate() {
            if item.fixed_in_frame {
                hud.push((item.z_index, i, item));
            } else {
                world.push((mean_camera_z(&item.path, &view), i, item));
            }
        }
        // Ascending camera-space z: in right-handed view space the camera looks
        // down -z, so more-negative z is farther and must paint first.
        world.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
        });
        hud.sort_by_key(|(z, i, _)| (*z, *i));

        LayeredFrame {
            world: self.build_ops(list.arena(), world.iter().map(|(_, _, it)| *it)),
            hud: self.build_ops(list.arena(), hud.iter().map(|(_, _, it)| *it)),
        }
    }

    /// Batches an ordered item stream into [`FrameOp`]s: consecutive vector items
    /// coalesce into one mesh; each image item breaks the batch and emits its own
    /// [`ImageQuad`]. Shared by [`tessellate_ops`](Self::tessellate_ops) and
    /// [`tessellate_ops_layered`](Self::tessellate_ops_layered).
    fn build_ops<'a>(&self, arena: u64, items: impl Iterator<Item = &'a DrawItem>) -> Vec<FrameOp> {
        let mut ops: Vec<FrameOp> = Vec::new();
        let mut batch = MeshData::default();
        for item in items {
            if let Some(paint) = &item.image {
                if !batch.is_empty() {
                    ops.push(FrameOp::Vector(std::mem::take(&mut batch)));
                }
                if let Some(quad) = image_quad(item, paint.clone()) {
                    ops.push(FrameOp::Image(quad));
                }
            } else if let Some(entry) = self.entries.get(&(arena, item.source)) {
                batch.append(&entry.mesh);
            }
        }
        if !batch.is_empty() {
            ops.push(FrameOp::Vector(batch));
        }
        ops
    }
}

/// The mean camera-space z of a path's anchor points, used as the painter's
/// depth key. `view` is the camera's world→view matrix
/// ([`Camera2D::view_matrix`](crate::camera::Camera2D::view_matrix)); it is
/// affine, so anchors transform without a perspective divide.
fn mean_camera_z(path: &Path, view: &Mat4) -> f32 {
    let mut sum = 0.0;
    let mut n = 0.0;
    for sub in &path.subpaths {
        for c in &sub.curves {
            sum += view.transform_point3(c.p0).z;
            n += 1.0;
        }
    }
    if n == 0.0 {
        0.0
    } else {
        sum / n
    }
}

/// A frame split for 3-D rendering: depth-sorted [`world`](Self::world) content
/// drawn with the perspective camera, then a [`hud`](Self::hud) overlay
/// (`fixed_in_frame` items) drawn with the orthographic camera.
pub struct LayeredFrame {
    /// World content, batched in painter's (farthest-first) camera-z order.
    pub world: Vec<FrameOp>,
    /// Fixed-in-frame HUD content, in `z_index` order.
    pub hud: Vec<FrameOp>,
}

/// One ordered draw in a frame: a batch of vector triangles or a textured quad.
pub enum FrameOp {
    /// A concatenated mesh of consecutive vector items.
    Vector(MeshData),
    /// A single raster-image quad.
    Image(ImageQuad),
}

/// A world-space textured quad extracted from an image [`DrawItem`].
pub struct ImageQuad {
    /// The four corners `[TL, TR, BR, BL]` in world space (from the item's
    /// quad path), matching the UVs `(0,0),(1,0),(1,1),(0,1)`. 3-D so image
    /// planes project under a 3D camera.
    pub corners: [[f32; 3]; 4],
    /// The image pixels and sampler.
    pub paint: ImagePaint,
    /// The source mobject (texture cache key).
    pub source: AnyId,
    /// The source generation (texture cache key).
    pub generation: u64,
}

/// Extracts the four quad corners from an image item's path (the first four
/// subpath anchors), or `None` if the path is not a quad.
fn image_quad(item: &DrawItem, paint: ImagePaint) -> Option<ImageQuad> {
    let sub = item.path.subpaths.first()?;
    // A closed rect through 4 corners is 3 line segments; the 4th corner is the
    // last segment's endpoint.
    if sub.curves.len() < 3 {
        return None;
    }
    let last = sub.curves.len() - 1;
    let corners = [
        sub.curves[0].p0,
        sub.curves[1].p0,
        sub.curves[2].p0,
        sub.curves[last].p3,
    ];
    Some(ImageQuad {
        corners: [
            [corners[0].x, corners[0].y, corners[0].z],
            [corners[1].x, corners[1].y, corners[1].z],
            [corners[2].x, corners[2].y, corners[2].z],
            [corners[3].x, corners[3].y, corners[3].z],
        ],
        paint,
        source: item.source,
        generation: item.generation,
    })
}

/// Tessellates one [`DrawItem`] into a single [`MeshData`], in paint order:
/// background stroke, then fill, then stroke.
///
/// The background stroke is emitted first so it reads as an outline behind the
/// fill (manim's text outline); the fill precedes the stroke so an opaque stroke
/// paints over the fill edge. Where a fill or stroke carries a
/// [`LinearGradient`], vertex colors are evaluated per vertex from the vertex
/// position; otherwise the solid color is used. Tessellation failures on a
/// degenerate path yield an empty contribution rather than panicking.
///
/// ```
/// use manim_color::BLUE;
/// use manim_core::display::Fill;
/// use manim_core::geometry::Circle;
/// use manim_core::scene_state::SceneState;
/// use manim_render::tessellate::{tessellate_item, DEFAULT_TOLERANCE};
///
/// // Pull a real DrawItem (with a valid source id) out of a scene.
/// let mut scene = SceneState::new();
/// scene.add(Circle::new());
/// let mut item = scene.display_list().0.remove(0);
/// item.fill = Some(Fill { color: BLUE, gradient: None });
/// let mesh = tessellate_item(&item, DEFAULT_TOLERANCE);
/// assert!(!mesh.is_empty());
/// ```
pub fn tessellate_item(item: &DrawItem, tolerance: f32) -> MeshData {
    let mut mesh = MeshData::default();

    // Both fills and strokes tessellate in a path-fitted plane, so a face or edge
    // that runs parallel to world z keeps its extent instead of collapsing. A
    // z-flat (2-D) path fits the (X, Y) plane, in which case the mapping is the
    // identity — 2-D output is byte-identical to the pre-3-D renderer.
    let (e, f) = fit_plane(&item.path);
    let flat = e == Vec3::X && f == Vec3::Y;
    let lyon_path = to_lyon_path_in_plane(&item.path, e, f);

    if let Some(bg) = &item.background_stroke {
        let width = bg.width * STROKE_WIDTH_CONVERSION;
        append_stroke(
            &mut mesh,
            &lyon_path,
            e,
            f,
            flat,
            bg.color,
            bg.gradient.as_ref(),
            width,
            tolerance,
        );
    }
    if let Some(fill) = &item.fill {
        append_fill(
            &mut mesh,
            &lyon_path,
            e,
            f,
            flat,
            fill.color,
            fill.gradient.as_ref(),
            tolerance,
        );
    }
    if let Some(stroke) = &item.stroke {
        let width = stroke.width * STROKE_WIDTH_CONVERSION;
        append_stroke(
            &mut mesh,
            &lyon_path,
            e,
            f,
            flat,
            stroke.color,
            stroke.gradient.as_ref(),
            width,
            tolerance,
        );
    }
    mesh
}

/// The premultiplied-linear vertex color at world position `(x, y)`: the
/// gradient sample if `gradient` is set, else the solid `color`.
#[inline]
fn vertex_color(color: Color, gradient: Option<&LinearGradient>, x: f32, y: f32) -> [f32; 4] {
    match gradient {
        Some(g) => g.color_at(Point::new(x, y, 0.0)).premultiplied(),
        None => color.premultiplied(),
    }
}

/// Picks two orthonormal world-axis basis vectors `(e, f)` spanning the plane a
/// path tessellates in: the two axes of largest coordinate range, dropping the
/// axis with the smallest range (highest index wins ties).
///
/// A 2-D path (all z equal) always drops z and returns `(X, Y)`, so it
/// tessellates exactly as the pre-3-D renderer did — byte-identical goldens. A
/// path extended along z (a vertical cube edge, a `ThreeDAxes` z-axis, a
/// vertical solid face) keeps a z-containing plane, so it retains extent instead
/// of collapsing.
fn fit_plane(path: &Path) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut any = false;
    for sub in &path.subpaths {
        for c in &sub.curves {
            for p in [c.p0, c.p1, c.p2, c.p3] {
                min = min.min(p);
                max = max.max(p);
                any = true;
            }
        }
    }
    if !any {
        return (Vec3::X, Vec3::Y);
    }
    let r = (max - min).to_array();
    // Drop the axis of least extent; `<=` makes the highest index win ties, so a
    // z-flat (2-D) path drops z and keeps (X, Y).
    let mut drop = 0usize;
    for a in 1..3 {
        if r[a] <= r[drop] {
            drop = a;
        }
    }
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    let kept: Vec<usize> = (0..3).filter(|&a| a != drop).collect();
    (axes[kept[0]], axes[kept[1]])
}

/// Converts a manim [`Path`] into a lyon path embedded in the plane spanned by
/// `(e, f)`: each point's 2-D lyon coordinate is `(P·e, P·f)`, and its full 3-D
/// position `[x, y, z]` rides along as three interpolated attributes.
///
/// [`world_pos`] reconstructs each tessellated vertex's world position from the
/// interpolated attributes plus its in-plane offset, so fills and strokes keep
/// their true 3-D shape (stroke width / fill area applied within `(e, f)`).
fn to_lyon_path_in_plane(path: &Path, e: Vec3, f: Vec3) -> LyonPath {
    let mut builder = LyonPath::builder_with_attributes(3);
    let uv = |p: Point| point(p.dot(e), p.dot(f));
    for sub in &path.subpaths {
        let Some(first) = sub.curves.first() else {
            continue;
        };
        let p0 = first.p0;
        builder.begin(uv(p0), &[p0.x, p0.y, p0.z]);
        for c in &sub.curves {
            builder.cubic_bezier_to(uv(c.p1), uv(c.p2), uv(c.p3), &[c.p3.x, c.p3.y, c.p3.z]);
        }
        builder.end(sub.closed);
    }
    builder.build()
}

/// Reconstructs a tessellated vertex's world-space position from its in-plane
/// lyon coordinate `uv` and interpolated 3-D attributes `attrs = [x, y, z]`.
///
/// When `flat` (the `(X, Y)` plane, i.e. a 2-D path), this is the identity map
/// `[uv.x, uv.y, z]` — bit-for-bit the pre-3-D output. Otherwise it adds the
/// in-plane offset of `uv` from the interpolated point onto that point, keeping
/// the vertex on the fitted plane.
#[inline]
fn world_pos(uv: lyon::math::Point, attrs: &[f32], e: Vec3, f: Vec3, flat: bool) -> Vec3 {
    let center = Vec3::new(
        attrs.first().copied().unwrap_or(0.0),
        attrs.get(1).copied().unwrap_or(0.0),
        attrs.get(2).copied().unwrap_or(0.0),
    );
    if flat {
        return Vec3::new(uv.x, uv.y, center.z);
    }
    let offset_u = uv.x - center.dot(e);
    let offset_v = uv.y - center.dot(f);
    center + e * offset_u + f * offset_v
}

/// Appends a non-zero-winding fill of `lyon_path` onto `mesh`, colored by
/// `gradient` per vertex or the solid `color`. `lyon_path` is embedded in the
/// `(e, f)` plane; each vertex is mapped back to world space via [`world_pos`]
/// (`flat` selecting the byte-identical 2-D path).
#[allow(clippy::too_many_arguments)]
fn append_fill(
    mesh: &mut MeshData,
    lyon_path: &LyonPath,
    e: Vec3,
    f: Vec3,
    flat: bool,
    color: Color,
    gradient: Option<&LinearGradient>,
    tolerance: f32,
) {
    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();
    let options = FillOptions::tolerance(tolerance).with_fill_rule(FillRule::NonZero);
    let mut tess = FillTessellator::new();
    let ok = tess.tessellate_path(
        lyon_path,
        &options,
        &mut BuffersBuilder::new(&mut buffers, |mut v: FillVertex| {
            let w = world_pos(v.position(), v.interpolated_attributes(), e, f, flat);
            Vertex {
                position: [w.x, w.y, w.z],
                color: vertex_color(color, gradient, w.x, w.y),
            }
        }),
    );
    if ok.is_ok() {
        mesh.append(&MeshData {
            vertices: buffers.vertices,
            indices: buffers.indices,
        });
    }
}

/// Appends a round-capped, round-joined stroke of `lyon_path` onto `mesh`.
///
/// `lyon_path` is embedded in the `(e, f)` plane (see [`to_lyon_path_in_plane`]);
/// each tessellated vertex is mapped back to its true world-space position via
/// [`world_pos`]. Round joins and caps are chosen (over CE's historical butt
/// caps) so that corners and endpoints stay artifact-free under the affine camera
/// zoom; the difference is sub-pixel for the thin default stroke.
#[allow(clippy::too_many_arguments)]
fn append_stroke(
    mesh: &mut MeshData,
    lyon_path: &LyonPath,
    e: Vec3,
    f: Vec3,
    flat: bool,
    color: Color,
    gradient: Option<&LinearGradient>,
    width: f32,
    tolerance: f32,
) {
    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();
    let options = StrokeOptions::tolerance(tolerance)
        .with_line_width(width)
        .with_line_join(LineJoin::Round)
        .with_line_cap(LineCap::Round);
    let mut tess = StrokeTessellator::new();
    let ok = tess.tessellate_path(
        lyon_path,
        &options,
        &mut BuffersBuilder::new(&mut buffers, |mut v: StrokeVertex| {
            let w = world_pos(v.position(), v.interpolated_attributes(), e, f, flat);
            Vertex {
                position: [w.x, w.y, w.z],
                color: vertex_color(color, gradient, w.x, w.y),
            }
        }),
    );
    if ok.is_ok() {
        mesh.append(&MeshData {
            vertices: buffers.vertices,
            indices: buffers.indices,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_color::{BLUE, RED, WHITE};
    use manim_core::display::{Fill, Stroke};
    use manim_core::geometry::{Circle, Line, Square, Triangle};
    use manim_core::mobject::Buildable;
    use manim_core::scene_state::SceneState;
    use manim_math::RIGHT;

    /// A display list with one filled circle plus its real `source` id.
    fn filled_circle() -> DisplayList {
        let mut scene = SceneState::new();
        let c = scene.add(Circle::new());
        scene.set_style_family(c.erase(), |s| {
            s.set_fill(BLUE, 1.0);
        });
        scene.display_list()
    }

    #[test]
    fn circle_fill_has_geometry() {
        let dl = filled_circle();
        let mut cache = TessellationCache::new();
        let mesh = cache.tessellate(&dl);
        assert!(mesh.vertices.len() > 2);
        assert!(mesh.indices.len() >= 3);
        assert_eq!(mesh.indices.len() % 3, 0);
    }

    /// The tessellation half of the arena footgun: two scenes whose first
    /// mobject shares `(source, generation)` must not share triangles.
    ///
    /// `Square` and `Triangle` specifically: both are unmutated (so their
    /// generations are still `0` — any builder call would bump one and paper over
    /// the collision) *and* they carry the identical default white stroke, so the
    /// paint hash matches too. That leaves the arena stamp as the only thing
    /// telling the two entries apart, which is precisely what this pins. A
    /// `Circle` would not do: its default stroke is red, so the paint hash alone
    /// would separate them and the test would pass even with the stamp defeated.
    #[test]
    fn independent_scenes_do_not_share_cache_entries() {
        let mut a = SceneState::new();
        a.add(Square::new());
        let mut b = SceneState::new();
        b.add(Triangle::new());
        let (da, db) = (a.display_list(), b.display_list());

        // Preconditions: every other part of the key really does collide.
        assert_eq!(da.0[0].source, db.0[0].source);
        assert_eq!(da.0[0].generation, db.0[0].generation);
        assert_eq!(paint_hash(&da.0[0]), paint_hash(&db.0[0]));
        assert_ne!(da.arena(), db.arena());

        let mut cache = TessellationCache::new();
        let square = cache.tessellate(&da);
        let triangle = cache.tessellate(&db);
        // Both cold: the triangle must not have been served the square's mesh.
        assert_eq!(cache.misses(), 2);
        assert_eq!(cache.hits(), 0);
        assert_ne!(
            square.vertices.len(),
            triangle.vertices.len(),
            "the second scene was served the first scene's triangles"
        );
    }

    #[test]
    fn a_snapshot_clone_still_hits_the_cache() {
        // Seeking replays cloned snapshots; they must keep hitting.
        let mut scene = SceneState::new();
        scene.add(Circle::new());
        let mut cache = TessellationCache::new();
        cache.tessellate(&scene.display_list());
        assert_eq!(cache.misses(), 1);

        for _ in 0..4 {
            cache.tessellate(&scene.clone().display_list());
        }
        assert_eq!(
            cache.misses(),
            1,
            "a snapshot replay must not re-tessellate"
        );
        assert_eq!(cache.hits(), 4);
        assert_eq!(cache.len(), 1, "and must not grow the cache");
    }

    #[test]
    fn cache_hit_on_unchanged_generation() {
        let dl = filled_circle();
        let mut cache = TessellationCache::new();
        cache.tessellate(&dl);
        assert_eq!((cache.hits(), cache.misses()), (0, 1));
        // Second identical call reuses the cached mesh.
        cache.tessellate(&dl);
        assert_eq!((cache.hits(), cache.misses()), (1, 1));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn zoom_bucket_change_retessellates() {
        let dl = filled_circle();
        let mut cache = TessellationCache::new();
        cache.tessellate(&dl);
        assert_eq!((cache.hits(), cache.misses()), (0, 1));
        // Same zoom bucket → hit.
        cache.tessellate(&dl);
        assert_eq!(cache.hits(), 1);
        // Zoom in 4× (bucket -2) → the cached mesh is stale, re-tessellate.
        cache.set_zoom(2.0);
        assert_eq!(cache.zoom_bucket(), -2);
        cache.tessellate(&dl);
        assert_eq!((cache.hits(), cache.misses()), (1, 2));
    }

    #[test]
    fn zooming_in_refines_curves() {
        let dl = filled_circle();
        let mut coarse = TessellationCache::new();
        let coarse_mesh = coarse.tessellate(&dl);
        let mut fine = TessellationCache::new();
        fine.set_zoom(REFERENCE_FRAME_HEIGHT / 8.0); // 8× zoom in → finer tolerance
        let fine_mesh = fine.tessellate(&dl);
        assert!(fine.tolerance() < coarse.tolerance());
        assert!(fine_mesh.vertices.len() > coarse_mesh.vertices.len());
    }

    #[test]
    fn vanished_mobject_is_evicted() {
        let dl = filled_circle();
        let mut cache = TessellationCache::new();
        cache.tessellate(&dl);
        assert_eq!(cache.len(), 1);
        // An empty list evicts everything.
        cache.tessellate(&DisplayList::default());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn painter_order_is_by_z_index() {
        // Two overlapping squares with distinct fills and z-indices. The mesh
        // must begin with the lower-z (background) item's vertices.
        let mut scene = SceneState::new();
        let back = scene.add(Square::new());
        scene.set_style_family(back.erase(), |s| {
            s.set_fill(RED, 1.0);
        });
        scene.get_dyn_mut(back.erase()).data_mut().z_index = -1;
        let front = scene.add(Square::new().with_shift(0.5 * RIGHT));
        scene.set_style_family(front.erase(), |s| {
            s.set_fill(BLUE, 1.0);
        });

        let dl = scene.display_list();
        let mut cache = TessellationCache::new();
        let mesh = cache.tessellate(&dl);
        // First vertex is red (background), premultiplied-linear.
        assert_eq!(mesh.vertices[0].color, RED.premultiplied());
    }

    #[test]
    fn stroke_and_fill_both_contribute() {
        let mut scene = SceneState::new();
        let c = scene.add(Circle::new());
        scene.set_style_family(c.erase(), |s| {
            s.set_fill(BLUE, 1.0).set_stroke(WHITE, 4.0, 1.0);
        });
        let mut item = scene.display_list().0.remove(0);

        // Fill only.
        item.stroke = None;
        let fill_only = tessellate_item(&item, DEFAULT_TOLERANCE);
        // Fill + stroke.
        item.fill = Some(Fill {
            color: BLUE,
            gradient: None,
        });
        item.stroke = Some(Stroke {
            color: WHITE,
            width: 4.0,
            gradient: None,
        });
        let both = tessellate_item(&item, DEFAULT_TOLERANCE);
        assert!(both.indices.len() > fill_only.indices.len());
    }

    #[test]
    fn paint_change_invalidates_cache() {
        // Changing fill color (no geometry change → same generation) must
        // re-tessellate, because vertex colors are baked at tessellation time.
        let mut scene = SceneState::new();
        let c = scene.add(Circle::new());
        scene.set_style_family(c.erase(), |s| {
            s.set_fill(BLUE, 1.0);
        });
        let mut cache = TessellationCache::new();
        cache.tessellate(&scene.display_list());
        assert_eq!((cache.hits(), cache.misses()), (0, 1));

        // Recolor the same mobject; generation is unchanged.
        scene.set_style_family(c.erase(), |s| {
            s.set_fill(RED, 1.0);
        });
        let mesh = cache.tessellate(&scene.display_list());
        assert_eq!(cache.misses(), 2, "paint change should be a miss");
        assert_eq!(mesh.vertices[0].color, RED.premultiplied());
    }

    #[test]
    fn stroke_plane_keeps_xy_for_flat_paths() {
        // A z-flat (2-D) path drops z → basis (X, Y), so 2-D strokes tessellate
        // exactly as the pre-3-D renderer did (byte-identical goldens).
        let dl = filled_circle();
        let (e, f) = fit_plane(&dl.0[0].path);
        assert_eq!((e, f), (Vec3::X, Vec3::Y));
    }

    #[test]
    fn vertical_stroke_survives_tessellation() {
        // A line purely along world z (zero x/y extent) vanished under x/y
        // tessellation; the plane-fitted stroke path keeps it.
        let mut scene = SceneState::new();
        let l = scene.add(Line::new(
            Point::new(0.0, 0.0, -2.0),
            Point::new(0.0, 0.0, 2.0),
        ));
        scene.set_style_family(l.erase(), |s| {
            s.set_stroke(WHITE, 4.0, 1.0);
        });
        let (e, f) = fit_plane(&scene.display_list().0[0].path);
        assert!(e == Vec3::Z || f == Vec3::Z, "plane must contain z");

        let item = scene.display_list().0.remove(0);
        let mesh = tessellate_item(&item, DEFAULT_TOLERANCE);
        assert!(!mesh.is_empty(), "z-parallel stroke should tessellate");
        let (zmin, zmax) = mesh
            .vertices
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v.position[2]), hi.max(v.position[2]))
            });
        assert!(
            zmax - zmin > 3.0,
            "stroke should span its z extent: {zmin}..{zmax}"
        );
    }

    /// A default 3-D camera (eye on +z, looking at the origin).
    fn cam_3d() -> Camera2D {
        let mut cam = Camera2D::from(&manim_core::config::Config::default());
        cam.three_d = Some(manim_core::camera::ThreeDParams::default());
        cam
    }

    #[test]
    fn depth_sort_draws_far_items_first() {
        // Add the near (blue, world z=+2) square first, the far (red, z=-2)
        // second. The camera-space depth sort must still paint the far one first,
        // regardless of list order — proving it sorts by camera z, not index.
        let mut scene = SceneState::new();
        let near = scene.add(Square::new().with_shift(Point::new(0.0, 0.0, 2.0)));
        scene.set_style_family(near.erase(), |s| {
            s.set_fill(BLUE, 1.0);
        });
        let far = scene.add(Square::new().with_shift(Point::new(0.0, 0.0, -2.0)));
        scene.set_style_family(far.erase(), |s| {
            s.set_fill(RED, 1.0);
        });

        let mut cache = TessellationCache::new();
        let frame = cache.tessellate_ops_layered(&scene.display_list(), &cam_3d());
        assert!(frame.hud.is_empty());
        // Consecutive vectors merge into one batch; the far item is appended first.
        let FrameOp::Vector(mesh) = &frame.world[0] else {
            panic!("expected a vector op");
        };
        assert_eq!(
            mesh.vertices[0].color,
            RED.premultiplied(),
            "farthest (red) item must paint first"
        );
    }

    #[test]
    fn fixed_in_frame_splits_into_hud() {
        let mut scene = SceneState::new();
        let world = scene.add(Square::new());
        scene.set_style_family(world.erase(), |s| {
            s.set_fill(BLUE, 1.0);
        });
        let hud = scene.add(Circle::new());
        scene.set_style_family(hud.erase(), |s| {
            s.set_fill(RED, 1.0);
        });
        // Mark the circle as a HUD overlay.
        scene.get_dyn_mut(hud.erase()).data_mut().fixed_in_frame = true;

        let mut cache = TessellationCache::new();
        let frame = cache.tessellate_ops_layered(&scene.display_list(), &cam_3d());
        // The square stays in the world layer; the circle moves to the HUD.
        assert_eq!(frame.world.len(), 1);
        assert_eq!(frame.hud.len(), 1);
        let FrameOp::Vector(w) = &frame.world[0] else {
            panic!("expected a world vector op");
        };
        assert_eq!(w.vertices[0].color, BLUE.premultiplied());
        let FrameOp::Vector(h) = &frame.hud[0] else {
            panic!("expected a hud vector op");
        };
        assert_eq!(h.vertices[0].color, RED.premultiplied());
    }

    #[test]
    fn gradient_fill_varies_across_vertices() {
        // A BLUE→RED horizontal gradient fill produces different vertex colors.
        let mut scene = SceneState::new();
        let sq = scene.add(Square::new());
        scene.set_style_family(sq.erase(), |s| {
            s.set_fill_gradient(manim_core::style::Gradient::from_colors(&[BLUE, RED]));
        });
        let mut cache = TessellationCache::new();
        let mesh = cache.tessellate(&scene.display_list());
        let first = mesh.vertices[0].color;
        assert!(
            mesh.vertices.iter().any(|v| v.color != first),
            "gradient should vary vertex colors"
        );
    }
}
