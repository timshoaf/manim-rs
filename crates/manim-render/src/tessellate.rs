//! CPU tessellation: [`DisplayList`] → a flat GPU-ready triangle mesh.
//!
//! Each [`DrawItem`](manim_core::display::DrawItem)'s bezier [`Path`] is fed to
//! [lyon](https://docs.rs/lyon): its [`FillTessellator`] fills closed regions
//! with the non-zero winding rule and its [`StrokeTessellator`] outlines the
//! path. Both emit [`Vertex`]es carrying **premultiplied linear** color, ready
//! to blend on the GPU without further conversion.
//!
//! Tessellation is the renderer's hot path during animation, so a
//! [`TessellationCache`] memoizes each mobject's mesh keyed on its
//! `(source, generation)`: a mobject whose geometry has not changed reuses last
//! frame's triangles instead of re-tessellating. The combined per-frame result
//! is a [`FrameMesh`] whose items are concatenated back-to-front by `z_index`
//! (painter's algorithm — there is no depth buffer).
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

use lyon::math::point;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use manim_color::Color;
use manim_core::display::{DisplayList, DrawItem};
use manim_core::mobject::AnyId;
use manim_math::path::Path;

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

/// A single tessellated vertex: 2-D position and premultiplied-linear color.
///
/// The layout is `#[repr(C)]` and [`bytemuck::Pod`] so a slice of vertices
/// uploads to a GPU vertex buffer with `bytemuck::cast_slice` and no copies.
///
/// ```
/// use manim_render::tessellate::Vertex;
/// let v = Vertex { position: [1.0, 2.0], color: [0.5, 0.0, 0.0, 0.5] };
/// assert_eq!(v.position, [1.0, 2.0]);
/// ```
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// World-space position `[x, y]` in scene units.
    pub position: [f32; 2],
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

/// One cached mobject mesh together with the generation it was built from.
struct CacheEntry {
    generation: u64,
    mesh: MeshData,
}

/// Memoizes per-mobject tessellation, keyed on `(source, generation)`.
///
/// Re-tessellation happens only when a mobject's `generation` changes; entries
/// for mobjects that vanish from the display list are evicted. [`hits`] and
/// [`misses`] count cache outcomes for tests and diagnostics.
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
    entries: HashMap<AnyId, CacheEntry>,
    tolerance: f32,
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
            tolerance,
            hits: 0,
            misses: 0,
        }
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
        // Refresh the cache: (re)tessellate changed items, note which survive.
        let mut present: Vec<AnyId> = Vec::with_capacity(list.len());
        for item in list {
            present.push(item.source);
            let stale = self
                .entries
                .get(&item.source)
                .map(|e| e.generation != item.generation)
                .unwrap_or(true);
            if stale {
                self.misses += 1;
                let mesh = tessellate_item(item, self.tolerance);
                self.entries.insert(
                    item.source,
                    CacheEntry {
                        generation: item.generation,
                        mesh,
                    },
                );
            } else {
                self.hits += 1;
            }
        }
        // Evict meshes whose mobject vanished from the list.
        self.entries.retain(|id, _| present.contains(id));

        // Concatenate in painter's order: ascending z, stable within a tie.
        let mut order: Vec<(usize, &DrawItem)> = list.iter().enumerate().collect();
        order.sort_by_key(|(i, it)| (it.z_index, *i));

        let mut frame = FrameMesh::default();
        for (_, item) in order {
            if let Some(entry) = self.entries.get(&item.source) {
                frame.append(&entry.mesh);
            }
        }
        frame
    }
}

/// Tessellates one [`DrawItem`]: its fill (if any) then its stroke (if any),
/// concatenated into a single [`MeshData`].
///
/// The fill is emitted first so an opaque stroke of the same item paints over
/// the fill edge, matching manim CE's draw order. Tessellation failures on a
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
/// item.fill = Some(Fill { color: BLUE });
/// let mesh = tessellate_item(&item, DEFAULT_TOLERANCE);
/// assert!(!mesh.is_empty());
/// ```
pub fn tessellate_item(item: &DrawItem, tolerance: f32) -> MeshData {
    let mut mesh = MeshData::default();
    let lyon_path = to_lyon_path(&item.path);

    if let Some(fill) = item.fill {
        append_fill(&mut mesh, &lyon_path, fill.color, tolerance);
    }
    if let Some(stroke) = item.stroke {
        let width = stroke.width * STROKE_WIDTH_CONVERSION;
        append_stroke(&mut mesh, &lyon_path, stroke.color, width, tolerance);
    }
    mesh
}

/// Converts a manim [`Path`] into a lyon path, emitting cubic-bezier segments
/// and honoring each subpath's `closed` flag.
fn to_lyon_path(path: &Path) -> LyonPath {
    let mut builder = LyonPath::builder();
    for sub in &path.subpaths {
        let Some(first) = sub.curves.first() else {
            continue;
        };
        builder.begin(point(first.p0.x, first.p0.y));
        for c in &sub.curves {
            builder.cubic_bezier_to(
                point(c.p1.x, c.p1.y),
                point(c.p2.x, c.p2.y),
                point(c.p3.x, c.p3.y),
            );
        }
        builder.end(sub.closed);
    }
    builder.build()
}

/// Appends a non-zero-winding fill of `lyon_path` in `color` onto `mesh`.
fn append_fill(mesh: &mut MeshData, lyon_path: &LyonPath, color: Color, tolerance: f32) {
    let premul = color.premultiplied();
    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();
    let options = FillOptions::tolerance(tolerance).with_fill_rule(FillRule::NonZero);
    let mut tess = FillTessellator::new();
    let ok = tess.tessellate_path(
        lyon_path,
        &options,
        &mut BuffersBuilder::new(&mut buffers, move |v: FillVertex| Vertex {
            position: v.position().to_array(),
            color: premul,
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
/// Round joins and caps are chosen (over CE's historical butt caps) so that
/// corners and endpoints stay artifact-free under the affine camera zoom; the
/// difference is sub-pixel for the thin default stroke.
fn append_stroke(
    mesh: &mut MeshData,
    lyon_path: &LyonPath,
    color: Color,
    width: f32,
    tolerance: f32,
) {
    let premul = color.premultiplied();
    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();
    let options = StrokeOptions::tolerance(tolerance)
        .with_line_width(width)
        .with_line_join(LineJoin::Round)
        .with_line_cap(LineCap::Round);
    let mut tess = StrokeTessellator::new();
    let ok = tess.tessellate_path(
        lyon_path,
        &options,
        &mut BuffersBuilder::new(&mut buffers, move |v: StrokeVertex| Vertex {
            position: v.position().to_array(),
            color: premul,
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
    use manim_core::geometry::{Circle, Square};
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
        item.fill = Some(Fill { color: BLUE });
        item.stroke = Some(Stroke {
            color: WHITE,
            width: 4.0,
        });
        let both = tessellate_item(&item, DEFAULT_TOLERANCE);
        assert!(both.indices.len() > fill_only.indices.len());
    }
}
