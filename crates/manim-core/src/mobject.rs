//! The mobject model: [`MobjectData`], the [`Mobject`] trait, typed handles, and
//! the shared transform/positioning API ([`MobjectExt`]).
//!
//! A concrete mobject (`Circle`, `Square`, `Line`, …) is a plain struct that
//! embeds a [`MobjectData`] plus its own semantic parameters, and implements
//! [`Mobject`] (usually via the `impl_mobject!` macro). All behavior shared by
//! every mobject — transforms, positioning, size queries, styling — lives once
//! on the blanket-implemented [`MobjectExt`] extension trait, mirroring manim
//! CE's `Mobject` base class.
//!
//! # Own-path vs. family transforms
//!
//! The methods on [`MobjectExt`] operate on a mobject's **own path only**. This
//! is exactly what you want *before* adding to a scene (builder style) — a free
//! mobject has no children. Once a mobject is in the arena, hierarchy lives
//! there, so family-aware transforms (self + descendants) are the
//! `SceneState::shift`/`rotate_about`/… methods. See
//! [`crate::scene_state::SceneState`].
//!
//! # Builder and mutate styles
//!
//! ```
//! use manim_core::geometry::Square;
//! use manim_core::mobject::{Buildable, MobjectExt};
//! use manim_math::RIGHT;
//! use manim_color::BLUE;
//!
//! // Declarative construction: consuming semantic builders, then `.with(..)`
//! // for the shared transform/style API.
//! let sq = Square::new()
//!     .side_length(2.0)
//!     .with(|s| {
//!         s.set_fill(BLUE, 0.5).shift(2.0 * RIGHT);
//!     });
//! assert!((sq.get_center().x - 2.0).abs() < 1e-6);
//! ```

use std::any::Any;
use std::marker::PhantomData;

use manim_math::path::Path;
use manim_math::space_ops::rotation_matrix;
use manim_math::{Point, ORIGIN, OUT};
use slotmap::DefaultKey;

use crate::style::Style;

/// Process-wide source of geometry revision stamps.
///
/// Stamps must be unique across *clones* of a scene state, not merely
/// monotonic per mobject: timeline playback restores a snapshot and re-applies
/// an animation every frame, so a per-mobject counter would assign the same
/// number to different geometry and stale tessellations would be served from
/// the renderer cache. Starts at 1 so `0` always means "never mutated".
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The data every mobject carries, matching manim CE's `Mobject` attributes.
///
/// Concrete mobjects embed one of these. [`generation`](Self::generation) is a
/// process-globally unique stamp refreshed on every geometry mutation; the
/// renderer uses `(id, generation)` as a tessellation cache key. Unchanged
/// mobjects keep their stamp (including across clones), so caching static
/// geometry stays effective.
///
/// ```
/// use manim_core::mobject::MobjectData;
/// let data = MobjectData::default();
/// assert_eq!(data.z_index, 0);
/// assert_eq!(data.generation, 0);
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MobjectData {
    /// The vectorized geometry, in the mobject's current (world) coordinates.
    pub path: Path,
    /// Fill and stroke paint.
    pub style: Style,
    /// Draw order key; higher is drawn on top.
    pub z_index: i32,
    /// Optional human-readable name (manim's `Mobject.name`).
    pub name: Option<String>,
    /// Child handles, in draw order (manim's `submobjects`).
    pub children: Vec<AnyId>,
    /// Parent handle, if this mobject is a submobject of another.
    pub parent: Option<AnyId>,
    /// Geometry revision counter; bumped on every geometry mutation.
    pub generation: u64,
    /// A raster image paint for image mobjects (the `path` is its quad); `None`
    /// for ordinary vector mobjects.
    pub image: Option<crate::display::ImagePaint>,
    /// Whether this mobject is fixed in the camera frame (a HUD overlay drawn
    /// orthographically under a 3-D camera). manim's `add_fixed_in_frame_mobjects`.
    pub fixed_in_frame: bool,
}

impl MobjectData {
    /// Builds data from a path and style, with default z-index and no hierarchy.
    ///
    /// ```
    /// use manim_core::mobject::MobjectData;
    /// use manim_core::style::Style;
    /// use manim_math::path::Path;
    /// use manim_math::{Point, RIGHT};
    /// let path = Path::from_corners(&[Point::ZERO, RIGHT], false);
    /// let data = MobjectData::new(path, Style::default());
    /// assert_eq!(data.path.n_curves(), 1);
    /// ```
    pub fn new(path: Path, style: Style) -> Self {
        Self {
            path,
            style,
            ..Self::default()
        }
    }

    /// Refreshes the geometry [`generation`](Self::generation) with a fresh
    /// process-globally unique stamp, invalidating any cached tessellation.
    ///
    /// ```
    /// use manim_core::mobject::MobjectData;
    /// let mut data = MobjectData::default();
    /// data.bump_generation();
    /// let first = data.generation;
    /// data.bump_generation();
    /// assert!(data.generation > first);
    /// ```
    pub fn bump_generation(&mut self) {
        self.generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Anything that can be placed in a scene.
///
/// This is deliberately tiny: it exposes the shared [`MobjectData`], supports
/// boxed cloning (for scene snapshots that the animation phase relies on), and
/// downcasting to the concrete type. All the rich shared behavior lives on
/// [`MobjectExt`], which is blanket-implemented for every `Mobject`.
///
/// Implement it with the `impl_mobject!` macro rather than by hand.
pub trait Mobject: 'static {
    /// Shared mobject data (geometry, style, hierarchy).
    fn data(&self) -> &MobjectData;
    /// Mutable access to the shared mobject data.
    fn data_mut(&mut self) -> &mut MobjectData;
    /// Clones this mobject into a fresh box (enables `SceneState: Clone`).
    fn clone_box(&self) -> Box<dyn Mobject>;
    /// Upcasts to `&dyn Any` for downcasting to the concrete type.
    fn as_any(&self) -> &dyn Any;
    /// Upcasts to `&mut dyn Any` for downcasting to the concrete type.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// This mobject's triangle-mesh geometry, if it draws through the mesh pass
    /// rather than the 2D vector pass — `None` for every ordinary mobject.
    ///
    /// [`SceneState::display_list`](crate::scene_state::SceneState::display_list)
    /// turns a returned payload into a [`MeshItem`](crate::display::MeshItem) and
    /// emits **no** [`DrawItem`](crate::display::DrawItem) for the mobject.
    /// Implement it by pairing [`MeshMobject`](crate::mesh::MeshMobject) with
    /// `impl_mobject!($t, mesh)` rather than by hand.
    fn mesh_payload(&self) -> Option<crate::mesh::MeshPayload> {
        None
    }
}

impl Clone for Box<dyn Mobject> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Style setters callable directly on a `&mut dyn Mobject`.
///
/// The fluent [`MobjectExt`] setters are `Self: Sized`, so they cannot be called
/// on a trait object (e.g. a child fetched via
/// [`SceneState::get_dyn_mut`](crate::scene_state::SceneState::get_dyn_mut)).
/// These inherent methods on `dyn Mobject` fill that gap with the same behavior,
/// so updater and child-manipulation code can style without downcasting.
///
/// ```
/// use manim_core::geometry::Circle;
/// use manim_core::mobject::Mobject;
/// use manim_color::RED;
/// let mut circle = Circle::new();
/// let m: &mut dyn Mobject = &mut circle;
/// m.set_fill(RED, 1.0).set_stroke(RED, 2.0, 1.0);
/// assert_eq!(m.data().style.fill_color, Some(RED));
/// ```
impl dyn Mobject {
    /// Sets the fill color and opacity (manim's `set_fill`).
    pub fn set_fill(&mut self, color: manim_color::Color, opacity: f32) -> &mut Self {
        self.data_mut().style.set_fill(color, opacity);
        self
    }

    /// Sets the stroke color, width, and opacity (manim's `set_stroke`).
    pub fn set_stroke(&mut self, color: manim_color::Color, width: f32, opacity: f32) -> &mut Self {
        self.data_mut().style.set_stroke(color, width, opacity);
        self
    }

    /// Sets both fill and stroke color (manim's `set_color`).
    pub fn set_color(&mut self, color: manim_color::Color) -> &mut Self {
        self.data_mut().style.set_color(color);
        self
    }

    /// Sets both fill and stroke opacity (manim's `set_opacity`).
    pub fn set_opacity(&mut self, opacity: f32) -> &mut Self {
        self.data_mut().style.set_opacity(opacity);
        self
    }
}

/// Implements [`Mobject`] for a struct that has a `data: MobjectData` field and
/// derives `Clone`.
///
/// ```
/// use manim_core::impl_mobject;
/// use manim_core::mobject::{MobjectData, Mobject};
///
/// #[derive(Clone)]
/// struct MyShape {
///     data: MobjectData,
/// }
/// impl_mobject!(MyShape);
///
/// let s = MyShape { data: MobjectData::default() };
/// assert_eq!(s.data().z_index, 0);
/// ```
///
/// # Mesh mobjects
///
/// The `mesh` arm additionally wires [`Mobject::mesh_payload`] up to the type's
/// [`MeshMobject`](crate::mesh::MeshMobject) impl, putting it on the display
/// list's mesh channel instead of the 2D vector pass:
///
/// ```
/// use manim_core::impl_mobject;
/// use manim_core::mesh::{MeshMaterial, MeshMobject, MeshPayload, TriMesh};
/// use manim_core::mobject::{Mobject, MobjectData};
/// use glam::Mat4;
/// use std::sync::Arc;
///
/// #[derive(Clone)]
/// struct MyMesh {
///     data: MobjectData,
///     geometry: Arc<TriMesh>,
/// }
/// impl_mobject!(MyMesh, mesh);
///
/// impl MeshMobject for MyMesh {
///     fn payload(&self) -> MeshPayload {
///         MeshPayload::new(Arc::clone(&self.geometry), Mat4::IDENTITY, MeshMaterial::default())
///     }
/// }
///
/// let m = MyMesh { data: MobjectData::default(), geometry: Arc::new(TriMesh::grid(2, 2)) };
/// assert!(m.mesh_payload().is_some());
/// ```
#[macro_export]
macro_rules! impl_mobject {
    ($t:ty) => {
        $crate::impl_mobject!(@base $t);
    };
    ($t:ty, mesh) => {
        $crate::impl_mobject!(@base $t,
            fn mesh_payload(&self) -> ::std::option::Option<$crate::mesh::MeshPayload> {
                ::std::option::Option::Some(
                    <Self as $crate::mesh::MeshMobject>::payload(self),
                )
            }
        );
    };
    (@base $t:ty $(, $extra:item)*) => {
        impl $crate::mobject::Mobject for $t {
            fn data(&self) -> &$crate::mobject::MobjectData {
                &self.data
            }
            fn data_mut(&mut self) -> &mut $crate::mobject::MobjectData {
                &mut self.data
            }
            fn clone_box(&self) -> ::std::boxed::Box<dyn $crate::mobject::Mobject> {
                ::std::boxed::Box::new(::std::clone::Clone::clone(self))
            }
            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
                self
            }
            $($extra)*
        }
    };
}

/// A type-erased handle to a mobject in a [`SceneState`](crate::scene_state::SceneState).
///
/// This is the untyped form used for heterogeneous collections (children of a
/// `VGroup`, family traversals). Get one from [`MobjectId::erase`].
///
/// ```
/// use manim_core::geometry::Circle;
/// use manim_core::scene_state::SceneState;
/// let mut scene = SceneState::new();
/// let c = scene.add(Circle::new());
/// let any = c.erase();
/// assert!(scene.contains(any));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnyId(pub(crate) DefaultKey);

/// A typed, `Copy` handle to a mobject of concrete type `M` in a
/// [`SceneState`](crate::scene_state::SceneState).
///
/// Handles are cheap to copy into closures and animations. The type parameter
/// gives ergonomic typed access (`scene[id].radius_value()`) and is erased with
/// [`MobjectId::erase`] for heterogeneous storage.
///
/// ```
/// use manim_core::geometry::Circle;
/// use manim_core::scene_state::SceneState;
/// let mut scene = SceneState::new();
/// let c = scene.add(Circle::new());
/// // Copyable and comparable.
/// let c2 = c;
/// assert_eq!(c, c2);
/// ```
pub struct MobjectId<M> {
    pub(crate) key: DefaultKey,
    _marker: PhantomData<fn() -> M>,
}

impl<M> MobjectId<M> {
    /// Wraps a raw slotmap key as a typed handle.
    pub(crate) fn new(key: DefaultKey) -> Self {
        Self {
            key,
            _marker: PhantomData,
        }
    }

    /// Erases the type parameter, yielding an [`AnyId`].
    ///
    /// ```
    /// use manim_core::geometry::Square;
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let s = scene.add(Square::new());
    /// let any = s.erase();
    /// assert_eq!(scene.get_dyn(any).data().z_index, 0);
    /// ```
    pub fn erase(self) -> AnyId {
        AnyId(self.key)
    }
}

// Manual impls so that `M` need not be `Clone`/`Copy` itself.
impl<M> Clone for MobjectId<M> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<M> Copy for MobjectId<M> {}
impl<M> PartialEq for MobjectId<M> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl<M> Eq for MobjectId<M> {}
impl<M> std::hash::Hash for MobjectId<M> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}
impl<M> std::fmt::Debug for MobjectId<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MobjectId({:?})", self.key)
    }
}

impl<M> From<MobjectId<M>> for AnyId {
    fn from(id: MobjectId<M>) -> Self {
        id.erase()
    }
}

/// An axis-aligned bounding box in scene space.
///
/// Empty geometry yields a degenerate box at the origin. Ports the role of
/// manim CE's bounding-box / `get_critical_point` machinery.
///
/// ```
/// use manim_core::mobject::BoundingBox;
/// use manim_math::{Point, RIGHT, UP};
/// let bb = BoundingBox::new(Point::ZERO, 2.0 * RIGHT + 2.0 * UP);
/// assert_eq!(bb.center(), RIGHT + UP);
/// assert_eq!(bb.width(), 2.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    /// Minimum corner (per-axis minima).
    pub min: Point,
    /// Maximum corner (per-axis maxima).
    pub max: Point,
}

impl BoundingBox {
    /// Constructs a box from its two corners.
    pub fn new(min: Point, max: Point) -> Self {
        Self { min, max }
    }

    /// A degenerate box at the origin (used for empty geometry).
    ///
    /// ```
    /// use manim_core::mobject::BoundingBox;
    /// use manim_math::Point;
    /// assert_eq!(BoundingBox::empty().center(), Point::ZERO);
    /// ```
    pub fn empty() -> Self {
        Self {
            min: ORIGIN,
            max: ORIGIN,
        }
    }

    /// The center point.
    pub fn center(&self) -> Point {
        (self.min + self.max) * 0.5
    }

    /// The extent along x.
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    /// The extent along y.
    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    /// The extent along z.
    pub fn depth(&self) -> f32 {
        self.max.z - self.min.z
    }

    /// The point on the box in direction `dir` (manim's `get_critical_point`).
    ///
    /// For each axis, picks the max side when `dir` is positive, the min side
    /// when negative, and the center when zero.
    ///
    /// ```
    /// use manim_core::mobject::BoundingBox;
    /// use manim_math::{Point, RIGHT, UP};
    /// let bb = BoundingBox::new(-RIGHT - UP, RIGHT + UP);
    /// assert_eq!(bb.point_in_direction(UP), UP);
    /// assert_eq!(bb.point_in_direction(RIGHT + UP), RIGHT + UP);
    /// ```
    pub fn point_in_direction(&self, dir: Point) -> Point {
        let center = self.center();
        let pick = |axis: usize| {
            if dir[axis] > 0.0 {
                self.max[axis]
            } else if dir[axis] < 0.0 {
                self.min[axis]
            } else {
                center[axis]
            }
        };
        Point::new(pick(0), pick(1), pick(2))
    }

    /// The union of two boxes.
    pub fn union(&self, other: &BoundingBox) -> BoundingBox {
        BoundingBox {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}

/// The bounding box of a path, or [`BoundingBox::empty`] when it has no curves.
///
/// ```
/// use manim_core::mobject::bbox_of;
/// use manim_math::path::Path;
/// use manim_math::{Point, RIGHT, UP};
/// let p = Path::from_corners(&[Point::ZERO, RIGHT + UP], false);
/// let bb = bbox_of(&p);
/// assert_eq!(bb.max, RIGHT + UP);
/// ```
pub fn bbox_of(path: &Path) -> BoundingBox {
    match path.bounding_box() {
        Some((min, max)) => BoundingBox { min, max },
        None => BoundingBox::empty(),
    }
}

// ---------------------------------------------------------------------------
// Free geometry-mutation helpers. These operate directly on `MobjectData` (so
// they work through `&mut dyn Mobject` in scene family ops) and bump the
// generation counter. `MobjectExt` wraps them for the fluent builder API.
// ---------------------------------------------------------------------------

/// Translates the path by `delta`.
pub fn apply_shift(data: &mut MobjectData, delta: Point) {
    data.path.apply(|p| p + delta);
    data.bump_generation();
}

/// Scales the path by `factor` about `center`.
pub fn apply_scale_about(data: &mut MobjectData, factor: f32, center: Point) {
    data.path.apply(|p| center + (p - center) * factor);
    data.bump_generation();
}

/// Rotates the path by `angle` radians about `center` around `axis`.
pub fn apply_rotate_about(data: &mut MobjectData, angle: f32, center: Point, axis: Point) {
    let m = rotation_matrix(angle, axis);
    data.path.apply(|p| center + m * (p - center));
    data.bump_generation();
}

/// Stretches the path along a single axis (`dim`) by `factor` about `center`.
pub fn apply_stretch_about(data: &mut MobjectData, factor: f32, dim: usize, center: Point) {
    data.path.apply(|p| {
        let mut q = p;
        q[dim] = center[dim] + (p[dim] - center[dim]) * factor;
        q
    });
    data.bump_generation();
}

/// Applies an arbitrary point function to every control point (manim's
/// `apply_function`).
pub fn apply_point_function<F: Fn(Point) -> Point>(data: &mut MobjectData, f: F) {
    data.path.apply(f);
    data.bump_generation();
}

/// A positioning reference: either a bare point or a bounding box.
///
/// This lets [`MobjectExt::next_to`] and [`MobjectExt::align_to`] target either
/// a coordinate or another mobject's box, mirroring manim's
/// `mobject_or_point` arguments.
///
/// ```
/// use manim_core::mobject::{BoundingBox, RefTarget};
/// use manim_math::{Point, RIGHT};
/// let a: RefTarget = RIGHT.into();
/// assert_eq!(a.critical_point(RIGHT), RIGHT);
/// let b: RefTarget = BoundingBox::new(Point::ZERO, RIGHT).into();
/// assert_eq!(b.critical_point(RIGHT), RIGHT);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RefTarget {
    /// A single reference point.
    Point(Point),
    /// A reference bounding box.
    Bounds(BoundingBox),
}

impl RefTarget {
    /// The reference point in direction `dir`: the point itself for
    /// [`RefTarget::Point`], or the box's critical point for
    /// [`RefTarget::Bounds`].
    pub fn critical_point(&self, dir: Point) -> Point {
        match self {
            RefTarget::Point(p) => *p,
            RefTarget::Bounds(b) => b.point_in_direction(dir),
        }
    }
}

impl From<Point> for RefTarget {
    fn from(p: Point) -> Self {
        RefTarget::Point(p)
    }
}
impl From<BoundingBox> for RefTarget {
    fn from(b: BoundingBox) -> Self {
        RefTarget::Bounds(b)
    }
}

/// The shared transform / positioning / styling API, blanket-implemented for
/// every [`Mobject`].
///
/// Every method operates on the mobject's **own path** (see the module docs for
/// the own-path vs. family distinction). Mutating methods return `&mut Self` so
/// they chain, both on a mutable scene entry (`scene[id].rotate(..)`) and inside
/// a [`Buildable::with`] closure at construction time.
///
/// | manim CE | here |
/// | --- | --- |
/// | `shift` | [`shift`](MobjectExt::shift) |
/// | `scale` | [`scale`](MobjectExt::scale) / [`scale_about`](MobjectExt::scale_about) |
/// | `rotate` | [`rotate`](MobjectExt::rotate) / [`rotate_about`](MobjectExt::rotate_about) |
/// | `flip` | [`flip`](MobjectExt::flip) |
/// | `stretch` | [`stretch`](MobjectExt::stretch) |
/// | `move_to` | [`move_to`](MobjectExt::move_to) |
/// | `next_to` | [`next_to`](MobjectExt::next_to) |
/// | `align_to` | [`align_to`](MobjectExt::align_to) |
/// | `to_edge` / `to_corner` | [`to_edge`](MobjectExt::to_edge) / [`to_corner`](MobjectExt::to_corner) |
/// | `center` | [`center`](MobjectExt::center) |
/// | `get_center` / `get_top` / … | [`get_center`](MobjectExt::get_center) / [`get_top`](MobjectExt::get_top) / … |
/// | `width` / `set_width` | [`width`](MobjectExt::width) / [`set_width`](MobjectExt::set_width) |
pub trait MobjectExt: Mobject {
    /// This mobject's own-path bounding box.
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// let c = Circle::new();
    /// assert!((c.bounding_box().width() - 2.0).abs() < 1e-4);
    /// ```
    fn bounding_box(&self) -> BoundingBox {
        bbox_of(&self.data().path)
    }

    /// The center of the bounding box (manim's `get_center`).
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::Point;
    /// assert_eq!(Circle::new().get_center(), Point::ZERO);
    /// ```
    fn get_center(&self) -> Point {
        self.bounding_box().center()
    }

    /// The top-edge midpoint (manim's `get_top`).
    fn get_top(&self) -> Point {
        self.bounding_box().point_in_direction(manim_math::UP)
    }

    /// The bottom-edge midpoint (manim's `get_bottom`).
    fn get_bottom(&self) -> Point {
        self.bounding_box().point_in_direction(manim_math::DOWN)
    }

    /// The left-edge midpoint (manim's `get_left`).
    fn get_left(&self) -> Point {
        self.bounding_box().point_in_direction(manim_math::LEFT)
    }

    /// The right-edge midpoint (manim's `get_right`).
    fn get_right(&self) -> Point {
        self.bounding_box().point_in_direction(manim_math::RIGHT)
    }

    /// The bounding-box point in direction `dir` (manim's `get_corner`).
    ///
    /// ```
    /// use manim_core::geometry::Square;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::{UR, Point};
    /// // A default (side 2) square spans [-1, 1]²; its upper-right corner is (1, 1).
    /// assert_eq!(Square::new().get_corner(UR), Point::new(1.0, 1.0, 0.0));
    /// ```
    fn get_corner(&self, dir: Point) -> Point {
        self.bounding_box().point_in_direction(dir)
    }

    /// The x-coordinate of the center.
    fn get_x(&self) -> f32 {
        self.get_center().x
    }

    /// The y-coordinate of the center.
    fn get_y(&self) -> f32 {
        self.get_center().y
    }

    /// The bounding-box width (manim's `width`).
    fn width(&self) -> f32 {
        self.bounding_box().width()
    }

    /// The bounding-box height (manim's `height`).
    fn height(&self) -> f32 {
        self.bounding_box().height()
    }

    /// Translates by `delta` (manim's `shift`).
    ///
    /// ```
    /// use manim_core::geometry::Dot;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::{Point, RIGHT};
    /// let mut d = Dot::new();
    /// d.shift(3.0 * RIGHT);
    /// assert!((d.get_center() - 3.0 * RIGHT).length() < 1e-6);
    /// ```
    fn shift(&mut self, delta: Point) -> &mut Self
    where
        Self: Sized,
    {
        apply_shift(self.data_mut(), delta);
        self
    }

    /// Scales by `factor` about the center (manim's `scale`).
    ///
    /// ```
    /// use manim_core::geometry::Square;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// let mut s = Square::new(); // side 2
    /// s.scale(2.0);
    /// assert!((s.width() - 4.0).abs() < 1e-4);
    /// ```
    fn scale(&mut self, factor: f32) -> &mut Self
    where
        Self: Sized,
    {
        let c = self.get_center();
        apply_scale_about(self.data_mut(), factor, c);
        self
    }

    /// Scales by `factor` about an explicit `point`.
    ///
    /// ```
    /// use manim_core::geometry::Dot;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::{Point, RIGHT};
    /// let mut d = Dot::new();
    /// d.shift(RIGHT).scale_about(2.0, Point::ZERO);
    /// // Doubling about the origin moves the center from (1,0) to (2,0).
    /// assert!((d.get_center() - 2.0 * RIGHT).length() < 1e-6);
    /// ```
    fn scale_about(&mut self, factor: f32, point: Point) -> &mut Self
    where
        Self: Sized,
    {
        apply_scale_about(self.data_mut(), factor, point);
        self
    }

    /// Rotates by `angle` radians about the center around the `OUT` axis
    /// (manim's `rotate`).
    ///
    /// ```
    /// use manim_core::geometry::Square;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::TAU;
    /// let mut s = Square::new();
    /// let before = s.get_corner(manim_math::UR);
    /// s.rotate(TAU); // full turn returns to start
    /// assert!((s.get_corner(manim_math::UR) - before).length() < 1e-4);
    /// ```
    fn rotate(&mut self, angle: f32) -> &mut Self
    where
        Self: Sized,
    {
        let c = self.get_center();
        apply_rotate_about(self.data_mut(), angle, c, OUT);
        self
    }

    /// Rotates by `angle` radians about `point` around `axis`.
    fn rotate_about(&mut self, angle: f32, point: Point, axis: Point) -> &mut Self
    where
        Self: Sized,
    {
        apply_rotate_about(self.data_mut(), angle, point, axis);
        self
    }

    /// Flips (mirrors) about the line through the center along `axis` (manim's
    /// `flip`); implemented as a half-turn about `axis`.
    ///
    /// ```
    /// use manim_core::geometry::Dot;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::{UP, RIGHT};
    /// let mut d = Dot::new();
    /// d.shift(RIGHT).flip_about(UP, manim_math::Point::ZERO);
    /// // Flipping horizontally about the origin sends (1,0) to (-1,0).
    /// assert!((d.get_center() + RIGHT).length() < 1e-6);
    /// ```
    fn flip(&mut self, axis: Point) -> &mut Self
    where
        Self: Sized,
    {
        let c = self.get_center();
        apply_rotate_about(self.data_mut(), manim_math::PI, c, axis);
        self
    }

    /// Flips (mirrors) about the line through `point` along `axis`.
    fn flip_about(&mut self, axis: Point, point: Point) -> &mut Self
    where
        Self: Sized,
    {
        apply_rotate_about(self.data_mut(), manim_math::PI, point, axis);
        self
    }

    /// Stretches by `factor` along axis `dim` (0 = x, 1 = y, 2 = z) about the
    /// center (manim's `stretch`).
    ///
    /// ```
    /// use manim_core::geometry::Square;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// let mut s = Square::new(); // 2 × 2
    /// s.stretch(3.0, 0); // widen only
    /// assert!((s.width() - 6.0).abs() < 1e-4);
    /// assert!((s.height() - 2.0).abs() < 1e-4);
    /// ```
    fn stretch(&mut self, factor: f32, dim: usize) -> &mut Self
    where
        Self: Sized,
    {
        let c = self.get_center();
        apply_stretch_about(self.data_mut(), factor, dim, c);
        self
    }

    /// Applies an arbitrary point function to the geometry (manim's
    /// `apply_function`).
    fn apply_function<F: Fn(Point) -> Point>(&mut self, f: F) -> &mut Self
    where
        Self: Sized,
    {
        apply_point_function(self.data_mut(), f);
        self
    }

    /// Moves the center to `point` (manim's `move_to`).
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::{Point, UP};
    /// let mut c = Circle::new();
    /// c.move_to(2.0 * UP);
    /// assert!((c.get_center() - 2.0 * UP).length() < 1e-6);
    /// ```
    fn move_to(&mut self, point: Point) -> &mut Self
    where
        Self: Sized,
    {
        let delta = point - self.get_center();
        apply_shift(self.data_mut(), delta);
        self
    }

    /// Sets the center x-coordinate (manim's `set_x`).
    fn set_x(&mut self, x: f32) -> &mut Self
    where
        Self: Sized,
    {
        let delta = Point::new(x - self.get_x(), 0.0, 0.0);
        apply_shift(self.data_mut(), delta);
        self
    }

    /// Sets the center y-coordinate (manim's `set_y`).
    fn set_y(&mut self, y: f32) -> &mut Self
    where
        Self: Sized,
    {
        let delta = Point::new(0.0, y - self.get_y(), 0.0);
        apply_shift(self.data_mut(), delta);
        self
    }

    /// Centers the mobject at the origin (manim's `center`).
    fn center(&mut self) -> &mut Self
    where
        Self: Sized,
    {
        self.move_to(ORIGIN)
    }

    /// Aligns this mobject's edge in direction `dir` to `target`'s edge in the
    /// same direction, moving only along the non-zero axes of `dir` (manim's
    /// `align_to`).
    ///
    /// ```
    /// use manim_core::geometry::Square;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::{UP, Point};
    /// let mut a = Square::new();
    /// // Align the top of `a` to y = 3.
    /// a.align_to(3.0 * UP, UP);
    /// assert!((a.get_top().y - 3.0).abs() < 1e-5);
    /// ```
    fn align_to(&mut self, target: impl Into<RefTarget>, dir: Point) -> &mut Self
    where
        Self: Sized,
    {
        let target_point = target.into().critical_point(dir);
        let self_point = self.bounding_box().point_in_direction(dir);
        let mut delta = target_point - self_point;
        for axis in 0..3 {
            if dir[axis] == 0.0 {
                delta[axis] = 0.0;
            }
        }
        apply_shift(self.data_mut(), delta);
        self
    }

    /// Positions this mobject next to `target` in direction `dir`, separated by
    /// `buff` (manim's `next_to`).
    ///
    /// ```
    /// use manim_core::geometry::Square;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::{RIGHT, MED_SMALL_BUFF};
    /// let a = Square::new(); // spans x ∈ [-1, 1]
    /// let mut b = Square::new();
    /// b.next_to(a.bounding_box(), RIGHT, MED_SMALL_BUFF);
    /// // b's left edge sits `buff` to the right of a's right edge.
    /// assert!((b.get_left().x - (1.0 + MED_SMALL_BUFF)).abs() < 1e-5);
    /// ```
    fn next_to(&mut self, target: impl Into<RefTarget>, dir: Point, buff: f32) -> &mut Self
    where
        Self: Sized,
    {
        let target_point = target.into().critical_point(dir);
        let point_to_align = self.bounding_box().point_in_direction(-dir);
        let delta = target_point - point_to_align + buff * dir;
        apply_shift(self.data_mut(), delta);
        self
    }

    /// Moves the mobject to a frame edge in direction `dir`, `buff` from the
    /// border, assuming manim's default frame (manim's `to_edge`).
    ///
    /// ```
    /// use manim_core::geometry::Dot;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::{LEFT, LARGE_BUFF, FRAME_WIDTH};
    /// let mut d = Dot::new();
    /// d.to_edge(LEFT, LARGE_BUFF);
    /// assert!((d.get_left().x - (-FRAME_WIDTH / 2.0 + LARGE_BUFF)).abs() < 1e-4);
    /// ```
    fn to_edge(&mut self, dir: Point, buff: f32) -> &mut Self
    where
        Self: Sized,
    {
        let bbox = self.bounding_box();
        align_on_frame(self.data_mut(), &bbox, dir, buff);
        self
    }

    /// Moves the mobject to a frame corner in direction `dir`, `buff` from the
    /// border (manim's `to_corner`). Same rule as [`to_edge`](Self::to_edge) but
    /// typically called with a diagonal direction.
    fn to_corner(&mut self, dir: Point, buff: f32) -> &mut Self
    where
        Self: Sized,
    {
        let bbox = self.bounding_box();
        align_on_frame(self.data_mut(), &bbox, dir, buff);
        self
    }

    /// Rescales uniformly so the bounding-box width becomes `width` (manim's
    /// `set_width`); `stretch` widens only the x-axis instead.
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// let mut c = Circle::new(); // width 2
    /// c.set_width(4.0, false);
    /// assert!((c.width() - 4.0).abs() < 1e-4);
    /// assert!((c.height() - 4.0).abs() < 1e-4); // uniform: height grew too
    /// ```
    fn set_width(&mut self, width: f32, stretch: bool) -> &mut Self
    where
        Self: Sized,
    {
        let cur = self.width();
        if cur.abs() < 1e-9 {
            return self;
        }
        let factor = width / cur;
        let c = self.get_center();
        if stretch {
            apply_stretch_about(self.data_mut(), factor, 0, c);
        } else {
            apply_scale_about(self.data_mut(), factor, c);
        }
        self
    }

    /// Rescales uniformly so the bounding-box height becomes `height` (manim's
    /// `set_height`); `stretch` scales only the y-axis instead.
    fn set_height(&mut self, height: f32, stretch: bool) -> &mut Self
    where
        Self: Sized,
    {
        let cur = self.height();
        if cur.abs() < 1e-9 {
            return self;
        }
        let factor = height / cur;
        let c = self.get_center();
        if stretch {
            apply_stretch_about(self.data_mut(), factor, 1, c);
        } else {
            apply_scale_about(self.data_mut(), factor, c);
        }
        self
    }

    // --- Styling (does not bump generation; geometry is unchanged) ---

    /// Sets the fill color and opacity (manim's `set_fill`).
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_color::BLUE;
    /// let mut c = Circle::new();
    /// c.set_fill(BLUE, 0.5);
    /// assert_eq!(c.data().style.fill_color, Some(BLUE));
    /// ```
    fn set_fill(&mut self, color: manim_color::Color, opacity: f32) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut().style.set_fill(color, opacity);
        self
    }

    /// Sets the stroke color, width, and opacity (manim's `set_stroke`).
    fn set_stroke(&mut self, color: manim_color::Color, width: f32, opacity: f32) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut().style.set_stroke(color, width, opacity);
        self
    }

    /// Sets both fill and stroke color (manim's `set_color`).
    fn set_color(&mut self, color: manim_color::Color) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut().style.set_color(color);
        self
    }

    /// Sets both fill and stroke opacity (manim's `set_opacity`).
    fn set_opacity(&mut self, opacity: f32) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut().style.set_opacity(opacity);
        self
    }

    /// Colors this mobject with a gradient ramp (manim's `set_color_by_gradient`).
    ///
    /// Gradients whichever of fill/stroke is already visible; the display list
    /// then paints it per vertex along the bounding-box axis.
    ///
    /// ```
    /// use manim_core::geometry::Square;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_color::{BLUE, RED};
    /// let mut sq = Square::new();
    /// sq.set_fill(BLUE, 1.0).set_color_by_gradient(&[BLUE, RED]);
    /// assert!(sq.data().style.fill_gradient.is_some());
    /// ```
    fn set_color_by_gradient(&mut self, colors: &[manim_color::Color]) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut().style.set_color_by_gradient(colors);
        self
    }

    /// Sets a fill gradient (manim's `set_fill` with a gradient), making the
    /// fill visible if it was not.
    ///
    /// ```
    /// use manim_core::geometry::Square;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_core::style::Gradient;
    /// use manim_color::{BLUE, RED};
    /// let mut sq = Square::new();
    /// sq.set_fill_gradient(Gradient::from_colors(&[BLUE, RED]));
    /// assert!(sq.data().style.render_fill().is_some());
    /// ```
    fn set_fill_gradient(&mut self, gradient: crate::style::Gradient) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut().style.set_fill_gradient(gradient);
        self
    }

    /// Sets a background stroke drawn behind the fill (manim's
    /// `set_background_stroke`), used to outline text.
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_color::BLACK;
    /// let mut c = Circle::new();
    /// c.set_background_stroke(BLACK, 6.0, 1.0);
    /// assert_eq!(c.data().style.background_stroke_color, Some(BLACK));
    /// ```
    fn set_background_stroke(
        &mut self,
        color: manim_color::Color,
        width: f32,
        opacity: f32,
    ) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut()
            .style
            .set_background_stroke(color, width, opacity);
        self
    }

    /// Sets the z-index (draw order) of this mobject.
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// let mut c = Circle::new();
    /// c.set_z_index(5);
    /// assert_eq!(c.data().z_index, 5);
    /// ```
    fn set_z_index(&mut self, z: i32) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut().z_index = z;
        self
    }

    /// Fixes (or unfixes) this mobject in the camera frame — a HUD overlay drawn
    /// orthographically under a 3-D camera (manim's `add_fixed_in_frame_mobjects`).
    ///
    /// ```
    /// use manim_core::geometry::Square;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// let mut s = Square::new();
    /// s.set_fixed_in_frame(true);
    /// assert!(s.data().fixed_in_frame);
    /// ```
    fn set_fixed_in_frame(&mut self, fixed: bool) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut().fixed_in_frame = fixed;
        self
    }

    /// Sets the human-readable name of this mobject.
    fn set_name(&mut self, name: impl Into<String>) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut().name = Some(name.into());
        self
    }

    // --- Point-setting and adoption (manim's VMobject point API) ---

    /// Replaces the geometry with straight segments through `corners` (manim's
    /// `set_points_as_corners`).
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::{Point, RIGHT, UP};
    /// let mut c = Circle::new();
    /// c.set_points_as_corners(&[Point::ZERO, RIGHT, RIGHT + UP]);
    /// assert_eq!(c.data().path.n_curves(), 2);
    /// ```
    fn set_points_as_corners(&mut self, corners: &[Point]) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut().path = Path::from_corners(corners, false);
        self.data_mut().bump_generation();
        self
    }

    /// Replaces the geometry with a smooth spline through `anchors` (manim's
    /// `set_points_smoothly`).
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_math::Point;
    /// let mut c = Circle::new();
    /// c.set_points_smoothly(&[
    ///     Point::new(-1.0, 0.0, 0.0),
    ///     Point::ZERO,
    ///     Point::new(1.0, 1.0, 0.0),
    /// ]);
    /// assert_eq!(c.data().path.n_curves(), 2);
    /// ```
    fn set_points_smoothly(&mut self, anchors: &[Point]) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut().path = Path::from_smooth_anchors(anchors, false);
        self.data_mut().bump_generation();
        self
    }

    /// Adopts another mobject's path and style, becoming a copy of its
    /// appearance (manim's `become`). Named with a raw identifier because
    /// `become` is a reserved keyword.
    ///
    /// ```
    /// use manim_core::geometry::{Circle, Square};
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// let mut sq = Square::new();
    /// let circle = Circle::new();
    /// sq.r#become(&circle);
    /// // The square now has the circle's (rounded) outline: width 2.
    /// assert!((sq.bounding_box().width() - 2.0).abs() < 1e-4);
    /// ```
    fn r#become(&mut self, other: &dyn Mobject) -> &mut Self
    where
        Self: Sized,
    {
        let src = other.data();
        let data = self.data_mut();
        data.path = src.path.clone();
        data.style = src.style.clone();
        data.z_index = src.z_index;
        data.bump_generation();
        self
    }

    /// Copies another mobject's style (fill/stroke) without touching geometry
    /// (manim's `match_style`).
    ///
    /// ```
    /// use manim_core::geometry::{Circle, Square};
    /// use manim_core::mobject::{Mobject, MobjectExt};
    /// use manim_color::RED;
    /// let mut sq = Square::new();
    /// let circle = Circle::new(); // default stroke RED
    /// sq.match_style(&circle);
    /// assert_eq!(sq.data().style.stroke_color, Some(RED));
    /// ```
    fn match_style(&mut self, other: &dyn Mobject) -> &mut Self
    where
        Self: Sized,
    {
        self.data_mut().style = other.data().style.clone();
        self
    }
}

impl<T: Mobject + ?Sized> MobjectExt for T {}

/// Aligns a mobject's box against the default frame border in direction `dir`.
///
/// Port of manim's `align_on_border`: the shift is computed against
/// `sign(dir) * (FRAME_WIDTH/2, FRAME_HEIGHT/2, 0)` and masked to the axes where
/// `dir` is non-zero.
fn align_on_frame(data: &mut MobjectData, bbox: &BoundingBox, dir: Point, buff: f32) {
    let radius = Point::new(
        manim_math::FRAME_WIDTH / 2.0,
        manim_math::FRAME_HEIGHT / 2.0,
        0.0,
    );
    // sign(0) must be 0 here (f32::signum yields 1.0 for 0.0), so guard it.
    let sign = |v: f32| {
        if v > 0.0 {
            1.0
        } else if v < 0.0 {
            -1.0
        } else {
            0.0
        }
    };
    let target_point = Point::new(sign(dir.x), sign(dir.y), sign(dir.z)) * radius;
    let point_to_align = bbox.point_in_direction(dir);
    let mut delta = target_point - point_to_align - buff * dir;
    for axis in 0..3 {
        if dir[axis] == 0.0 {
            delta[axis] = 0.0;
        }
    }
    apply_shift(data, delta);
}

/// Construction-time sugar for applying the [`MobjectExt`] mutators to a freshly
/// built, owned mobject and getting it back by value.
///
/// Blanket-implemented for every sized [`Mobject`]. This is the bridge that lets
/// the fluent `&mut self` API be used in a declarative one-liner:
///
/// ```
/// use manim_core::geometry::Circle;
/// use manim_core::mobject::{Buildable, MobjectExt};
/// use manim_math::UP;
/// use manim_color::RED;
///
/// let c = Circle::new().with(|c| {
///     c.set_fill(RED, 1.0).shift(UP);
/// });
/// assert!((c.get_center() - UP).length() < 1e-6);
/// ```
pub trait Buildable: Mobject + Sized {
    /// Runs `f` against a mutable borrow of `self`, then returns `self`.
    fn with(mut self, f: impl FnOnce(&mut Self)) -> Self {
        f(&mut self);
        self
    }

    /// Consuming builder for [`MobjectExt::shift`].
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::mobject::{Buildable, MobjectExt};
    /// use manim_math::RIGHT;
    /// let c = Circle::new().with_shift(2.0 * RIGHT);
    /// assert!((c.get_center() - 2.0 * RIGHT).length() < 1e-6);
    /// ```
    fn with_shift(mut self, delta: Point) -> Self {
        self.shift(delta);
        self
    }

    /// Consuming builder for [`MobjectExt::move_to`].
    fn with_move_to(mut self, point: Point) -> Self {
        self.move_to(point);
        self
    }

    /// Consuming builder for [`MobjectExt::scale`].
    fn with_scale(mut self, factor: f32) -> Self {
        self.scale(factor);
        self
    }

    /// Consuming builder for [`MobjectExt::rotate`].
    fn with_rotate(mut self, angle: f32) -> Self {
        self.rotate(angle);
        self
    }

    /// Consuming builder for [`MobjectExt::set_fill`].
    fn with_fill(mut self, color: manim_color::Color, opacity: f32) -> Self {
        self.set_fill(color, opacity);
        self
    }

    /// Consuming builder for [`MobjectExt::set_stroke`].
    fn with_stroke(mut self, color: manim_color::Color, width: f32, opacity: f32) -> Self {
        self.set_stroke(color, width, opacity);
        self
    }

    /// Consuming builder for [`MobjectExt::set_color`].
    fn with_color(mut self, color: manim_color::Color) -> Self {
        self.set_color(color);
        self
    }

    /// Consuming builder for [`MobjectExt::set_z_index`].
    fn with_z_index(mut self, z: i32) -> Self {
        self.set_z_index(z);
        self
    }
}

impl<M: Mobject + Sized> Buildable for M {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Circle, Square};
    use manim_math::{RIGHT, UP};

    #[test]
    fn shift_moves_center_and_bumps_generation() {
        let mut c = Circle::new();
        let gen0 = c.data().generation;
        c.shift(2.0 * RIGHT);
        assert!((c.get_center() - 2.0 * RIGHT).length() < 1e-6);
        assert!(c.data().generation > gen0);
    }

    #[test]
    fn style_does_not_bump_generation() {
        let mut c = Circle::new();
        let gen0 = c.data().generation;
        c.set_fill(manim_color::BLUE, 0.5);
        assert_eq!(c.data().generation, gen0);
    }

    #[test]
    fn scale_preserves_aspect() {
        let mut s = Square::new();
        s.set_width(4.0, false);
        assert!((s.width() - 4.0).abs() < 1e-4);
        assert!((s.height() - 4.0).abs() < 1e-4);
    }

    #[test]
    fn set_width_stretch_only_x() {
        let mut s = Square::new();
        s.set_width(4.0, true);
        assert!((s.width() - 4.0).abs() < 1e-4);
        assert!((s.height() - 2.0).abs() < 1e-4);
    }

    #[test]
    fn next_to_places_with_buffer() {
        let a = Square::new();
        let mut b = Square::new();
        b.next_to(a.bounding_box(), RIGHT, 0.5);
        assert!((b.get_left().x - 1.5).abs() < 1e-5);
        // Same height row.
        assert!((b.get_center().y).abs() < 1e-5);
    }

    #[test]
    fn to_edge_hugs_border() {
        let mut d = Circle::new();
        d.to_edge(UP, 0.5);
        assert!((d.get_top().y - (manim_math::FRAME_HEIGHT / 2.0 - 0.5)).abs() < 1e-4);
        // Untouched on x.
        assert!(d.get_center().x.abs() < 1e-6);
    }

    #[test]
    fn buildable_with_returns_value() {
        let c = Circle::new().with(|c| {
            c.shift(UP);
        });
        assert!((c.get_center() - UP).length() < 1e-6);
    }
}
