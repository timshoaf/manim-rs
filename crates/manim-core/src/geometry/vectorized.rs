//! General vectorized mobjects and containers: [`VMobject`], [`VectorizedPoint`],
//! [`VDict`], [`DashedVMobject`], [`CurvesAsSubmobjects`], and [`TracedPath`].

use std::collections::HashMap;

use manim_color::WHITE;
use manim_math::path::{Path, SubPath};
use manim_math::Point;

use super::VGroup;
use crate::impl_mobject;
use crate::mobject::{AnyId, Mobject, MobjectData, MobjectId};
use crate::scene_state::SceneState;
use crate::style::Style;

/// A general vectorized mobject: an arbitrary [`Path`] plus a [`Style`]. The
/// workhorse other shapes specialize (manim CE's `VMobject`).
///
/// ```
/// use manim_core::geometry::VMobject;
/// use manim_core::mobject::{Mobject, MobjectExt};
/// use manim_math::path::Path;
/// use manim_math::{Point, RIGHT, UP};
/// let m = VMobject::from_path(Path::from_corners(&[Point::ZERO, RIGHT, UP], false));
/// assert_eq!(m.data().path.n_curves(), 2);
/// ```
#[derive(Clone)]
pub struct VMobject {
    data: MobjectData,
}
impl_mobject!(VMobject);

impl VMobject {
    /// A vectorized mobject from a path and style.
    pub fn new(path: Path, style: Style) -> Self {
        Self {
            data: MobjectData::new(path, style),
        }
    }

    /// A vectorized mobject from a path, with the default stroke style.
    pub fn from_path(path: Path) -> Self {
        Self::new(path, Style::stroked(WHITE))
    }
}

/// A mobject that is a single point with no drawable geometry, used as a movable
/// anchor. Port of manim CE's `VectorizedPoint`.
///
/// ```
/// use manim_core::geometry::VectorizedPoint;
/// use manim_math::{Point, RIGHT};
/// let mut p = VectorizedPoint::new(RIGHT);
/// assert_eq!(p.get_location(), RIGHT);
/// p.set_location(2.0 * RIGHT);
/// assert_eq!(p.get_location(), 2.0 * RIGHT);
/// ```
#[derive(Clone)]
pub struct VectorizedPoint {
    data: MobjectData,
    location: Point,
}
impl_mobject!(VectorizedPoint);

impl VectorizedPoint {
    /// A vectorized point at `location`.
    pub fn new(location: Point) -> Self {
        Self {
            data: MobjectData::new(point_path(location), Style::default()),
            location,
        }
    }

    /// The point's location.
    pub fn get_location(&self) -> Point {
        self.location
    }

    /// Moves the point to `location`.
    pub fn set_location(&mut self, location: Point) {
        self.location = location;
        self.data.path = point_path(location);
        self.data.bump_generation();
    }
}

/// A degenerate single-anchor path at `p`.
fn point_path(p: Point) -> Path {
    use manim_math::bezier::CubicBezier;
    Path {
        subpaths: vec![SubPath {
            curves: vec![CubicBezier::new(p, p, p, p)],
            closed: false,
        }],
    }
}

/// A string-keyed group of mobjects. Port of manim CE's `VDict`.
///
/// The mapping is stored on the group; children still live in the arena and
/// transform as a family.
///
/// ```
/// use manim_core::geometry::{Circle, Square, VDict};
/// use manim_core::scene_state::SceneState;
/// let mut scene = SceneState::new();
/// let a = scene.add(Circle::new());
/// let b = scene.add(Square::new());
/// let dict = VDict::of(&mut scene, [("circ".to_string(), a.erase()), ("sq".to_string(), b.erase())]);
/// assert_eq!(scene.get(dict).get("circ"), Some(a.erase()));
/// assert_eq!(scene.get(dict).len(), 2);
/// ```
#[derive(Clone)]
pub struct VDict {
    data: MobjectData,
    map: HashMap<String, AnyId>,
}
impl_mobject!(VDict);

impl VDict {
    /// An empty dictionary group.
    pub fn new() -> Self {
        Self {
            data: MobjectData::new(Default::default(), Style::default()),
            map: HashMap::new(),
        }
    }

    /// Adds `scene`, wraps the given `(key, id)` pairs into a new dict group, and
    /// returns its handle.
    pub fn of(
        scene: &mut SceneState,
        pairs: impl IntoIterator<Item = (String, AnyId)>,
    ) -> MobjectId<VDict> {
        let dict = scene.add(VDict::new());
        for (key, id) in pairs {
            scene.add_child(dict.erase(), id);
            scene.get_mut(dict).map.insert(key, id);
        }
        dict
    }

    /// The id stored under `key`, if any.
    pub fn get(&self, key: &str) -> Option<AnyId> {
        self.map.get(key).copied()
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the dictionary is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// The keys, in arbitrary order.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.map.keys()
    }
}

impl Default for VDict {
    fn default() -> Self {
        Self::new()
    }
}

/// manim CE's default `DashedVMobject` dash count.
pub const DEFAULT_NUM_DASHES: usize = 15;
/// manim CE's default `DashedVMobject` drawn-fraction per dash.
pub const DEFAULT_DASHED_RATIO: f32 = 0.5;

/// A dashed copy of another mobject's outline, built by geometrically slicing
/// each subpath into dashes. Port of manim CE's `DashedVMobject`.
///
/// ```
/// use manim_core::geometry::{Circle, DashedVMobject};
/// use manim_core::mobject::Mobject;
/// let circle = Circle::new();
/// let dashed = DashedVMobject::new(&circle);
/// // 15 dashes around the single circle outline.
/// assert_eq!(dashed.data().path.subpaths.len(), 15);
/// ```
#[derive(Clone)]
pub struct DashedVMobject {
    data: MobjectData,
    source: Path,
    num_dashes: usize,
    dashed_ratio: f32,
}
impl_mobject!(DashedVMobject);

impl DashedVMobject {
    /// A dashed version of `source` with the manim defaults (15 dashes, ratio
    /// 0.5).
    pub fn new(source: &dyn Mobject) -> Self {
        let src = source.data();
        let mut me = Self {
            data: MobjectData::new(Path::default(), src.style.clone()),
            source: src.path.clone(),
            num_dashes: DEFAULT_NUM_DASHES,
            dashed_ratio: DEFAULT_DASHED_RATIO,
        };
        me.rebuild();
        me
    }

    /// Sets the number of dashes (construction-time builder).
    pub fn num_dashes(mut self, num_dashes: usize) -> Self {
        self.num_dashes = num_dashes.max(1);
        self.rebuild();
        self
    }

    /// Sets the fraction of each dash cell that is drawn (construction-time
    /// builder).
    pub fn dashed_ratio(mut self, dashed_ratio: f32) -> Self {
        self.dashed_ratio = dashed_ratio.clamp(0.0, 1.0);
        self.rebuild();
        self
    }

    fn rebuild(&mut self) {
        self.data.path = dash_path(&self.source, self.num_dashes, self.dashed_ratio);
        self.data.bump_generation();
    }
}

/// Slices each subpath of `source` into `num_dashes` dashes, each drawing
/// `ratio` of its cell.
fn dash_path(source: &Path, num_dashes: usize, ratio: f32) -> Path {
    let mut subpaths = Vec::new();
    let full = 1.0 / num_dashes as f32;
    let dash = full * ratio;
    for sp in &source.subpaths {
        if sp.curves.is_empty() {
            continue;
        }
        let whole = Path {
            subpaths: vec![sp.clone()],
        };
        for i in 0..num_dashes {
            let a = i as f32 * full;
            let b = a + dash;
            if let Some(seg) = whole.get_subcurve(a, b).subpaths.into_iter().next() {
                if !seg.curves.is_empty() {
                    subpaths.push(seg);
                }
            }
        }
    }
    Path { subpaths }
}

/// Splits a mobject's outline into one child [`VMobject`] per Bézier curve.
/// Port of manim CE's `CurvesAsSubmobjects`.
///
/// ```
/// use manim_core::geometry::{CurvesAsSubmobjects, Square};
/// use manim_core::scene_state::SceneState;
/// let mut scene = SceneState::new();
/// let sq = scene.add(Square::new()); // 4 edges
/// let group = CurvesAsSubmobjects::of(&mut scene, sq.erase());
/// assert_eq!(scene.family(group.erase()).len(), 1 + 4); // group + 4 curves
/// ```
pub struct CurvesAsSubmobjects;

impl CurvesAsSubmobjects {
    /// Adds a group whose children are one [`VMobject`] per curve of `source`.
    pub fn of(scene: &mut SceneState, source: AnyId) -> MobjectId<VGroup> {
        let src = scene.get_dyn(source).data();
        let style = src.style.clone();
        let mut curves = Vec::new();
        for sp in &src.path.subpaths {
            for c in &sp.curves {
                curves.push(*c);
            }
        }
        let group = scene.add(VGroup::new());
        for c in curves {
            let path = Path {
                subpaths: vec![SubPath {
                    curves: vec![c],
                    closed: false,
                }],
            };
            let child = scene.add(VMobject::new(path, style.clone()));
            scene.add_child(group.erase(), child.erase());
        }
        group
    }
}

/// A mobject that traces the path of a point over time, appending the current
/// point each updater tick. Port of manim CE's `TracedPath`.
///
/// The accumulated points live in a shared buffer that survives the per-frame
/// state reconstruction of [`Scene::frames`](crate::scene::Scene::frames), and
/// is reset whenever an updater tick reports time `0.0` — so a fresh playback
/// (or a re-run of `frames()`) traces the same curve deterministically.
///
/// ```
/// use manim_core::geometry::{Dot, TracedPath};
/// use manim_core::scene_state::{SceneState, UpdaterCtx};
/// use manim_core::mobject::{Mobject, MobjectExt};
/// use manim_math::{Point, RIGHT};
/// let mut scene = SceneState::new();
/// let dot = scene.add(Dot::new());
/// // Trace the dot's center.
/// let trace = TracedPath::of(&mut scene, move |s| {
///     s.try_get(dot).map(|d| d.get_center()).unwrap_or(Point::ZERO)
/// });
/// // Move the dot and tick twice (at increasing times).
/// scene.get_dyn_mut(dot.erase()).data_mut().path.apply(|p| p + 2.0 * RIGHT);
/// scene.run_updaters(UpdaterCtx { dt: 0.0, time: 0.0 });
/// scene.get_dyn_mut(dot.erase()).data_mut().path.apply(|p| p + 2.0 * RIGHT);
/// scene.run_updaters(UpdaterCtx { dt: 0.0, time: 0.1 });
/// // The traced path now spans two points.
/// assert!(scene.get(trace).point_count() >= 2);
/// ```
#[derive(Clone)]
pub struct TracedPath {
    data: MobjectData,
    points: std::sync::Arc<std::sync::Mutex<Vec<Point>>>,
}
impl_mobject!(TracedPath);

impl TracedPath {
    /// An empty traced path.
    pub fn new() -> Self {
        Self {
            data: MobjectData::new(Path::default(), Style::stroked(WHITE)),
            points: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Adds a traced path to `scene` that follows `traced` each updater tick.
    pub fn of(
        scene: &mut SceneState,
        traced: impl Fn(&SceneState) -> Point + Send + Sync + 'static,
    ) -> MobjectId<TracedPath> {
        let id = scene.add(TracedPath::new());
        scene.add_updater(id.erase(), move |s, target, ctx| {
            let point = traced(s);
            if let Some(tp) = s
                .get_dyn_mut(target)
                .as_any_mut()
                .downcast_mut::<TracedPath>()
            {
                if ctx.time == 0.0 {
                    tp.clear();
                }
                tp.push_point(point);
            }
        });
        id
    }

    /// Appends `point` to the trace and rebuilds the path.
    pub fn push_point(&mut self, point: Point) {
        let corners = {
            let mut pts = self.points.lock().expect("traced-path buffer poisoned");
            if pts.last() != Some(&point) {
                pts.push(point);
            }
            pts.clone()
        };
        if corners.len() >= 2 {
            self.data.path = Path::from_corners(&corners, false);
            self.data.bump_generation();
        }
    }

    /// Clears the accumulated trace.
    pub fn clear(&mut self) {
        self.points
            .lock()
            .expect("traced-path buffer poisoned")
            .clear();
        self.data.path = Path::default();
        self.data.bump_generation();
    }

    /// The number of traced points so far.
    pub fn point_count(&self) -> usize {
        self.points
            .lock()
            .expect("traced-path buffer poisoned")
            .len()
    }
}

impl Default for TracedPath {
    fn default() -> Self {
        Self::new()
    }
}
