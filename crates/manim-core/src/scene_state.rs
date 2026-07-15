//! [`SceneState`]: the arena scene graph.
//!
//! All mobjects live in a [`slotmap`] arena; users hold cheap, `Copy`, typed
//! [`MobjectId`] handles. Hierarchy (submobjects) is stored as parent/children
//! key lists inside each mobject's [`MobjectData`], exactly like manim CE's
//! `submobjects`. This gives O(1) stable handles, generational stale-handle
//! detection, and a `Clone`-able scene value (needed by the animation phase for
//! state snapshots). See `docs/design/03-mobject-model.md`.
//!
//! # Own-path vs. family transforms
//!
//! Methods on [`MobjectExt`](crate::mobject::MobjectExt) reached through
//! `scene[id]` mutate a single mobject's own path. The family-aware methods here
//! ([`SceneState::shift`], [`SceneState::rotate_about`], …) apply to a mobject
//! **and all its descendants**, which is what group transforms need.

use std::collections::HashMap;
use std::ops::{Index, IndexMut};
use std::sync::{Arc, Mutex};

use manim_math::{Point, OUT};
use slotmap::{DefaultKey, SlotMap};

use crate::display::{DisplayList, DrawItem, Fill, LinearGradient, Stroke};
use crate::mobject::{
    apply_rotate_about, apply_scale_about, apply_shift, bbox_of, AnyId, BoundingBox, Mobject,
    MobjectData, MobjectId,
};
use crate::style::{Gradient, GradientAxis};

/// Context passed to an updater each frame (manim's updater `dt`).
///
/// ```
/// use manim_core::scene_state::UpdaterCtx;
/// let ctx = UpdaterCtx { dt: 1.0 / 60.0, time: 0.5 };
/// assert!((ctx.dt - 1.0 / 60.0).abs() < 1e-6);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UpdaterCtx {
    /// Seconds elapsed since the previous frame.
    pub dt: f32,
    /// Absolute scene time in seconds.
    pub time: f32,
}

/// The stored form of an updater callback.
///
/// The closure receives the whole [`SceneState`] plus the id it was registered
/// on, so it can read any other mobject (e.g. a dot following a tracker). It is
/// held behind `Arc<Mutex<…>>` so [`SceneState`] stays `Clone` (snapshots share
/// the same callback instance) and `Send + Sync`.
type UpdaterFn = dyn FnMut(&mut SceneState, AnyId, UpdaterCtx) + Send + Sync;

/// One registered updater.
#[derive(Clone)]
struct UpdaterEntry {
    id: AnyId,
    active: bool,
    func: Arc<Mutex<UpdaterFn>>,
}

/// One slot in the arena: a boxed mobject and its visibility flag.
struct Entry {
    mobject: Box<dyn Mobject>,
    visible: bool,
}

impl Clone for Entry {
    fn clone(&self) -> Self {
        Self {
            mobject: self.mobject.clone_box(),
            visible: self.visible,
        }
    }
}

/// The scene graph arena.
///
/// ```
/// use manim_core::geometry::{Circle, Square};
/// use manim_core::scene_state::SceneState;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::RIGHT;
///
/// let mut scene = SceneState::new();
/// let circle = scene.add(Circle::new());
/// let square = scene.add(Square::new().with_shift(2.0 * RIGHT));
/// assert_eq!(scene.display_list().len(), 2);
/// // Typed, panicking access with Index sugar:
/// assert!((scene[circle].bounding_box().width() - 2.0).abs() < 1e-4);
/// let _ = square;
/// ```
#[derive(Clone, Default)]
pub struct SceneState {
    arena: SlotMap<DefaultKey, Entry>,
    /// Top-level mobjects (no parent), in insertion order.
    roots: Vec<DefaultKey>,
    /// Saved mobject snapshots for `save_state` / `Restore`.
    saved_states: HashMap<DefaultKey, MobjectData>,
    /// Registered per-mobject updaters, in registration order.
    updaters: Vec<UpdaterEntry>,
    /// The camera / frame state (captured by timeline snapshots).
    camera: crate::camera::Camera2D,
}

impl SceneState {
    /// An empty scene.
    ///
    /// ```
    /// use manim_core::scene_state::SceneState;
    /// let scene = SceneState::new();
    /// assert!(scene.display_list().is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a mobject as a top-level (root) node, returning a typed handle.
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let c = scene.add(Circle::new());
    /// assert!(scene.contains(c.erase()));
    /// ```
    pub fn add<M: Mobject>(&mut self, mobject: M) -> MobjectId<M> {
        let key = self.arena.insert(Entry {
            mobject: Box::new(mobject),
            visible: true,
        });
        self.arena[key].mobject.data_mut().parent = None;
        self.roots.push(key);
        MobjectId::new(key)
    }

    /// Makes `child` a submobject of `parent`, removing it from the root set.
    ///
    /// No-op if either handle is stale or if `child` is already a child of
    /// `parent`.
    ///
    /// ```
    /// use manim_core::geometry::{Circle, VGroup};
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let g = scene.add(VGroup::new());
    /// let c = scene.add(Circle::new());
    /// scene.add_child(g.erase(), c.erase());
    /// assert_eq!(scene.family(g.erase()).len(), 2); // group + circle
    /// ```
    pub fn add_child(&mut self, parent: AnyId, child: AnyId) {
        if !self.arena.contains_key(parent.0) || !self.arena.contains_key(child.0) {
            return;
        }
        if parent == child {
            return;
        }
        // Detach child from any current parent / the root set.
        self.detach(child);
        self.arena[child.0].mobject.data_mut().parent = Some(parent);
        let children = &mut self.arena[parent.0].mobject.data_mut().children;
        if !children.contains(&child) {
            children.push(child);
        }
    }

    /// Alias for [`add_child`](Self::add_child), reading naturally for groups.
    ///
    /// ```
    /// use manim_core::geometry::{Circle, VGroup};
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let g = scene.add(VGroup::new());
    /// let c = scene.add(Circle::new());
    /// scene.add_to_group(g.erase(), c.erase());
    /// assert_eq!(scene.family(g.erase()).len(), 2);
    /// ```
    pub fn add_to_group(&mut self, group: AnyId, child: AnyId) {
        self.add_child(group, child);
    }

    /// Removes `id` and all its descendants from the scene.
    ///
    /// Stale handles to the removed nodes are then detected by
    /// [`try_get`](Self::try_get) / [`contains`](Self::contains).
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let c = scene.add(Circle::new());
    /// scene.remove(c.erase());
    /// assert!(scene.try_get(c).is_none());
    /// ```
    pub fn remove(&mut self, id: AnyId) {
        if !self.arena.contains_key(id.0) {
            return;
        }
        self.detach(id);
        for member in self.family(id) {
            self.arena.remove(member.0);
            self.roots.retain(|k| *k != member.0);
            self.saved_states.remove(&member.0);
            self.updaters.retain(|u| u.id != member);
        }
    }

    /// Detaches `id` from its parent's child list and from the root set, without
    /// removing it from the arena.
    fn detach(&mut self, id: AnyId) {
        let parent = self.arena[id.0].mobject.data().parent;
        match parent {
            Some(p) if self.arena.contains_key(p.0) => {
                self.arena[p.0]
                    .mobject
                    .data_mut()
                    .children
                    .retain(|c| *c != id);
            }
            _ => {}
        }
        self.arena[id.0].mobject.data_mut().parent = None;
        self.roots.retain(|k| *k != id.0);
    }

    /// Whether a handle still refers to a live mobject.
    pub fn contains(&self, id: AnyId) -> bool {
        self.arena.contains_key(id.0)
    }

    /// Typed shared access; panics on a stale handle or type mismatch.
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let c = scene.add(Circle::new());
    /// assert!((scene.get(c).radius_value() - 1.0).abs() < 1e-6);
    /// ```
    pub fn get<M: Mobject>(&self, id: MobjectId<M>) -> &M {
        self.try_get(id)
            .expect("stale or mistyped MobjectId passed to SceneState::get")
    }

    /// Typed mutable access; panics on a stale handle or type mismatch.
    pub fn get_mut<M: Mobject>(&mut self, id: MobjectId<M>) -> &mut M {
        self.try_get_mut(id)
            .expect("stale or mistyped MobjectId passed to SceneState::get_mut")
    }

    /// Fallible typed shared access: `None` if stale or the wrong type.
    pub fn try_get<M: Mobject>(&self, id: MobjectId<M>) -> Option<&M> {
        self.arena
            .get(id.key)
            .and_then(|e| e.mobject.as_any().downcast_ref::<M>())
    }

    /// Fallible typed mutable access: `None` if stale or the wrong type.
    pub fn try_get_mut<M: Mobject>(&mut self, id: MobjectId<M>) -> Option<&mut M> {
        self.arena
            .get_mut(id.key)
            .and_then(|e| e.mobject.as_any_mut().downcast_mut::<M>())
    }

    /// Type-erased shared access; panics on a stale handle.
    pub fn get_dyn(&self, id: AnyId) -> &dyn Mobject {
        self.arena
            .get(id.0)
            .map(|e| e.mobject.as_ref())
            .expect("stale AnyId passed to SceneState::get_dyn")
    }

    /// Type-erased mutable access; panics on a stale handle.
    pub fn get_dyn_mut(&mut self, id: AnyId) -> &mut dyn Mobject {
        self.arena
            .get_mut(id.0)
            .map(|e| e.mobject.as_mut())
            .expect("stale AnyId passed to SceneState::get_dyn_mut")
    }

    /// Whether `id` is currently visible (defaults to `true`).
    pub fn is_visible(&self, id: AnyId) -> bool {
        self.arena.get(id.0).map(|e| e.visible).unwrap_or(false)
    }

    /// Sets the visibility of `id` (invisible mobjects are skipped when drawing).
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let c = scene.add(Circle::new());
    /// scene.set_visible(c.erase(), false);
    /// assert!(scene.display_list().is_empty());
    /// ```
    pub fn set_visible(&mut self, id: AnyId, visible: bool) {
        if let Some(e) = self.arena.get_mut(id.0) {
            e.visible = visible;
        }
    }

    /// The family of `id`: itself followed by all descendants, depth-first in
    /// child order (manim CE's `family_members_with_points` traversal order).
    ///
    /// ```
    /// use manim_core::geometry::{Circle, Square, VGroup};
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let a = scene.add(Circle::new());
    /// let b = scene.add(Square::new());
    /// let g = scene.add(VGroup::new());
    /// scene.add_child(g.erase(), a.erase());
    /// scene.add_child(g.erase(), b.erase());
    /// // group, then its children in insertion order.
    /// assert_eq!(scene.family(g.erase()), vec![g.erase(), a.erase(), b.erase()]);
    /// ```
    pub fn family(&self, id: AnyId) -> Vec<AnyId> {
        let mut out = Vec::new();
        self.collect_family(id, &mut out);
        out
    }

    fn collect_family(&self, id: AnyId, out: &mut Vec<AnyId>) {
        if !self.arena.contains_key(id.0) {
            return;
        }
        out.push(id);
        let children = self.arena[id.0].mobject.data().children.clone();
        for child in children {
            self.collect_family(child, out);
        }
    }

    /// The visible top-level mobjects, in insertion order.
    pub fn iter_visible_roots(&self) -> impl Iterator<Item = AnyId> + '_ {
        self.roots
            .iter()
            .copied()
            .filter(|k| self.arena.get(*k).map(|e| e.visible).unwrap_or(false))
            .map(AnyId)
    }

    /// The union bounding box of `id`'s whole family.
    ///
    /// ```
    /// use manim_core::geometry::{Circle, Square, VGroup};
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
    /// use manim_math::RIGHT;
    /// let mut scene = SceneState::new();
    /// let a = scene.add(Circle::new()); // width 2, centered at origin
    /// let b = scene.add(Circle::new().with_shift(4.0 * RIGHT));
    /// let g = scene.add(VGroup::new());
    /// scene.add_child(g.erase(), a.erase());
    /// scene.add_child(g.erase(), b.erase());
    /// // Spans x ∈ [-1, 5], so width 6.
    /// assert!((scene.family_bounding_box(g.erase()).width() - 6.0).abs() < 1e-4);
    /// ```
    pub fn family_bounding_box(&self, id: AnyId) -> BoundingBox {
        let mut result: Option<BoundingBox> = None;
        for member in self.family(id) {
            let path = &self.arena[member.0].mobject.data().path;
            if path.subpaths.iter().all(|s| s.curves.is_empty()) {
                continue;
            }
            let bb = bbox_of(path);
            result = Some(match result {
                Some(r) => r.union(&bb),
                None => bb,
            });
        }
        result.unwrap_or_else(BoundingBox::empty)
    }

    /// Applies `f` to every family member's mobject (self + descendants).
    ///
    /// This is the primitive behind the family-aware transforms.
    ///
    /// ```
    /// use manim_core::geometry::{Circle, VGroup};
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::Mobject;
    /// let mut scene = SceneState::new();
    /// let g = scene.add(VGroup::new());
    /// let c = scene.add(Circle::new());
    /// scene.add_child(g.erase(), c.erase());
    /// scene.apply_to_family(g.erase(), |m| m.data_mut().z_index = 7);
    /// assert_eq!(scene.get(c).data().z_index, 7);
    /// ```
    pub fn apply_to_family(&mut self, id: AnyId, mut f: impl FnMut(&mut dyn Mobject)) {
        for member in self.family(id) {
            if let Some(e) = self.arena.get_mut(member.0) {
                f(e.mobject.as_mut());
            }
        }
    }

    /// Shifts `id` and its whole family by `delta` (family-aware `shift`).
    ///
    /// ```
    /// use manim_core::geometry::{Circle, VGroup};
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
    /// use manim_math::RIGHT;
    /// let mut scene = SceneState::new();
    /// let g = scene.add(VGroup::new());
    /// let c = scene.add(Circle::new());
    /// scene.add_child(g.erase(), c.erase());
    /// scene.shift(g.erase(), 3.0 * RIGHT);
    /// assert!((scene.get(c).get_center() - 3.0 * RIGHT).length() < 1e-6);
    /// ```
    pub fn shift(&mut self, id: AnyId, delta: Point) {
        self.apply_to_family(id, |m| apply_shift(m.data_mut(), delta));
    }

    /// Scales `id`'s family by `factor` about `point` (family-aware `scale`).
    pub fn scale_about(&mut self, id: AnyId, factor: f32, point: Point) {
        self.apply_to_family(id, |m| apply_scale_about(m.data_mut(), factor, point));
    }

    /// Scales `id`'s family by `factor` about the family's center.
    pub fn scale(&mut self, id: AnyId, factor: f32) {
        let center = self.family_bounding_box(id).center();
        self.scale_about(id, factor, center);
    }

    /// Rotates `id`'s family by `angle` about `point` around `axis`
    /// (family-aware `rotate`).
    pub fn rotate_about(&mut self, id: AnyId, angle: f32, point: Point, axis: Point) {
        self.apply_to_family(id, |m| apply_rotate_about(m.data_mut(), angle, point, axis));
    }

    /// Rotates `id`'s family by `angle` about the family's center around `OUT`.
    pub fn rotate(&mut self, id: AnyId, angle: f32) {
        let center = self.family_bounding_box(id).center();
        self.rotate_about(id, angle, center, OUT);
    }

    /// Moves `id`'s family so the family center lands on `target` (family-aware
    /// `move_to`).
    ///
    /// ```
    /// use manim_core::geometry::{Circle, Square, VGroup};
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
    /// use manim_math::{RIGHT, UP};
    /// let mut scene = SceneState::new();
    /// let a = scene.add(Circle::new());
    /// let b = scene.add(Square::new().with_shift(2.0 * RIGHT));
    /// let g = scene.add(VGroup::new());
    /// scene.add_child(g.erase(), a.erase());
    /// scene.add_child(g.erase(), b.erase());
    /// scene.move_to(g.erase(), 5.0 * UP);
    /// assert!((scene.family_bounding_box(g.erase()).center() - 5.0 * UP).length() < 1e-5);
    /// ```
    pub fn move_to(&mut self, id: AnyId, target: Point) {
        let center = self.family_bounding_box(id).center();
        self.shift(id, target - center);
    }

    /// Applies a style edit to every member of `id`'s family (family-aware
    /// styling, e.g. `set_color`).
    ///
    /// ```
    /// use manim_core::geometry::{Circle, VGroup};
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::Mobject;
    /// use manim_color::RED;
    /// let mut scene = SceneState::new();
    /// let g = scene.add(VGroup::new());
    /// let c = scene.add(Circle::new());
    /// scene.add_child(g.erase(), c.erase());
    /// scene.set_style_family(g.erase(), |s| { s.set_color(RED); });
    /// assert_eq!(scene.get(c).data().style.stroke_color, Some(RED));
    /// ```
    pub fn set_style_family(&mut self, id: AnyId, mut f: impl FnMut(&mut crate::style::Style)) {
        self.apply_to_family(id, |m| f(&mut m.data_mut().style));
    }

    // --- Saved states (manim's `save_state` / `restore`) ---

    /// Saves a snapshot of `id`'s current data, retrievable by
    /// [`restore`](Self::restore) or animated by
    /// [`Restore`](crate::animations::Restore). No-op on a stale handle.
    ///
    /// ```
    /// use manim_core::geometry::Circle;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// use manim_math::RIGHT;
    /// let mut scene = SceneState::new();
    /// let c = scene.add(Circle::new());
    /// scene.save_state(c.erase());
    /// scene.get_mut(c).shift(3.0 * RIGHT);
    /// scene.restore(c.erase());
    /// assert!(scene.get(c).get_center().length() < 1e-6);
    /// ```
    pub fn save_state(&mut self, id: AnyId) {
        if let Some(e) = self.arena.get(id.0) {
            self.saved_states.insert(id.0, e.mobject.data().clone());
        }
    }

    /// Whether a saved snapshot exists for `id`.
    pub fn has_saved_state(&self, id: AnyId) -> bool {
        self.saved_states.contains_key(&id.0)
    }

    /// The saved snapshot for `id`, if any.
    pub fn saved_state(&self, id: AnyId) -> Option<&MobjectData> {
        self.saved_states.get(&id.0)
    }

    /// Restores `id` to its last [`save_state`](Self::save_state) snapshot.
    /// No-op if there is no snapshot or the handle is stale.
    pub fn restore(&mut self, id: AnyId) {
        if let (Some(data), true) = (
            self.saved_states.get(&id.0).cloned(),
            self.arena.contains_key(id.0),
        ) {
            *self.arena[id.0].mobject.data_mut() = data;
        }
    }

    // --- Updaters (manim's `add_updater` / `remove_updater`) ---

    /// Registers an updater on `id`, run each frame with an [`UpdaterCtx`].
    ///
    /// The closure receives the whole scene and its own id, so it can read other
    /// mobjects (a dot following a tracker, say). Updaters run during
    /// [`Scene`](crate::scene::Scene) playback, not during a pure seek.
    ///
    /// ```
    /// use manim_core::geometry::Dot;
    /// use manim_core::scene_state::{SceneState, UpdaterCtx};
    /// use manim_core::mobject::MobjectExt;
    /// use manim_math::{Point, RIGHT};
    /// let mut scene = SceneState::new();
    /// let d = scene.add(Dot::new());
    /// scene.add_updater(d.erase(), |s, id, _ctx| {
    ///     s.get_dyn_mut(id).data_mut().path.apply(|p| p + RIGHT);
    /// });
    /// scene.run_updaters(UpdaterCtx { dt: 1.0 / 60.0, time: 0.0 });
    /// assert!((scene.get(d).get_center() - RIGHT).length() < 1e-6);
    /// ```
    pub fn add_updater(
        &mut self,
        id: AnyId,
        func: impl FnMut(&mut SceneState, AnyId, UpdaterCtx) + Send + Sync + 'static,
    ) {
        self.updaters.push(UpdaterEntry {
            id,
            active: true,
            func: Arc::new(Mutex::new(func)),
        });
    }

    /// Removes all updaters registered on `id`.
    pub fn remove_updaters(&mut self, id: AnyId) {
        self.updaters.retain(|u| u.id != id);
    }

    /// Whether any updater is registered on `id`.
    pub fn has_updaters(&self, id: AnyId) -> bool {
        self.updaters.iter().any(|u| u.id == id)
    }

    /// Suspends (pauses) updaters registered on `id`.
    pub fn suspend_updating(&mut self, id: AnyId) {
        for u in &mut self.updaters {
            if u.id == id {
                u.active = false;
            }
        }
    }

    /// Resumes previously-suspended updaters on `id`.
    pub fn resume_updating(&mut self, id: AnyId) {
        for u in &mut self.updaters {
            if u.id == id {
                u.active = true;
            }
        }
    }

    /// Runs every active updater once with `ctx` (manim's per-frame updater
    /// pass). Updaters whose target has been removed are skipped.
    pub fn run_updaters(&mut self, ctx: UpdaterCtx) {
        // Clone the Arc handles so the borrow of `self.updaters` ends before we
        // hand `&mut self` to each callback.
        let entries: Vec<(AnyId, Arc<Mutex<UpdaterFn>>)> = self
            .updaters
            .iter()
            .filter(|u| u.active)
            .map(|u| (u.id, Arc::clone(&u.func)))
            .collect();
        for (id, func) in entries {
            if !self.contains(id) {
                continue;
            }
            if let Ok(mut f) = func.lock() {
                f(self, id, ctx);
            }
        }
    }

    /// Builds the display list: visible roots, then their families, in
    /// z-then-insertion order, skipping empty paths and fully-transparent styles.
    ///
    /// ```
    /// use manim_core::geometry::{Circle, Square};
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
    /// let mut scene = SceneState::new();
    /// scene.add(Circle::new());
    /// // Higher z draws later (on top).
    /// let mut sq = Square::new();
    /// sq.set_z_index(-1);
    /// scene.add(sq);
    /// let dl = scene.display_list();
    /// assert_eq!(dl.len(), 2);
    /// // The z = -1 square sorts first.
    /// assert_eq!(dl.0[0].z_index, -1);
    /// ```
    pub fn display_list(&self) -> DisplayList {
        let mut items: Vec<DrawItem> = Vec::new();
        for root in self.iter_visible_roots() {
            for member in self.family(root) {
                let entry = match self.arena.get(member.0) {
                    Some(e) if e.visible => e,
                    _ => continue,
                };
                let data = entry.mobject.data();
                if data.path.subpaths.iter().all(|s| s.curves.is_empty()) {
                    continue;
                }
                // Resolve gradient axes against the mobject's bbox, so a
                // bbox-relative gradient follows the mobject. Only computed when
                // a gradient is actually present.
                let bbox = (data.style.fill_gradient.is_some()
                    || data.style.stroke_gradient.is_some())
                .then(|| bbox_of(&data.path));
                let fill = data.style.render_fill().map(|color| Fill {
                    color,
                    gradient: data
                        .style
                        .render_fill_gradient()
                        .map(|g| resolve_gradient(&g, bbox.as_ref().unwrap())),
                });
                let stroke = data.style.render_stroke().map(|(color, width)| Stroke {
                    color,
                    width,
                    gradient: data
                        .style
                        .render_stroke_gradient()
                        .map(|g| resolve_gradient(&g, bbox.as_ref().unwrap())),
                });
                let background_stroke =
                    data.style
                        .render_background_stroke()
                        .map(|(color, width)| Stroke {
                            color,
                            width,
                            gradient: None,
                        });
                if fill.is_none() && stroke.is_none() && background_stroke.is_none() {
                    continue;
                }
                items.push(DrawItem {
                    path: data.path.clone(),
                    fill,
                    stroke,
                    background_stroke,
                    z_index: data.z_index,
                    source: member,
                    generation: data.generation,
                });
            }
        }
        // Stable sort by z-index; ties keep the visited (insertion/pre-order)
        // order.
        items.sort_by_key(|it| it.z_index);
        DisplayList(items)
    }

    // --- Layout (manim's `arrange` / `arrange_in_grid`) ---

    /// Arranges `group`'s direct children in a row along `direction`, separated
    /// by `buff`, then recenters the group at the origin. Port of manim CE's
    /// `arrange`.
    ///
    /// ```
    /// use manim_core::geometry::{Square, VGroup};
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// use manim_math::{RIGHT, MED_SMALL_BUFF};
    /// let mut scene = SceneState::new();
    /// let a = scene.add(Square::new()); // width 2
    /// let b = scene.add(Square::new());
    /// let g = VGroup::of(&mut scene, [a.erase(), b.erase()]);
    /// scene.arrange(g.erase(), RIGHT, MED_SMALL_BUFF);
    /// // Centers are 2 + buff apart.
    /// let dx = scene.get(b).get_center().x - scene.get(a).get_center().x;
    /// assert!((dx - (2.0 + MED_SMALL_BUFF)).abs() < 1e-5);
    /// ```
    pub fn arrange(&mut self, group: AnyId, direction: Point, buff: f32) {
        let children = self.get_dyn(group).data().children.clone();
        for i in 1..children.len() {
            let prev = self.family_bounding_box(children[i - 1]);
            let target_edge = prev.point_in_direction(direction);
            let cur = self.family_bounding_box(children[i]);
            let cur_anchor = cur.point_in_direction(-direction);
            let delta = target_edge - cur_anchor + buff * direction;
            self.shift(children[i], delta);
        }
        self.move_to(group, manim_math::ORIGIN);
    }

    /// Arranges `group`'s direct children into a `rows` × `cols` grid with `buff`
    /// spacing, then recenters the group at the origin. Port of manim CE's
    /// `arrange_in_grid`.
    ///
    /// Cells are laid out left-to-right, top-to-bottom on a uniform pitch sized
    /// to the largest child plus `buff`.
    ///
    /// ```
    /// use manim_core::geometry::{Square, VGroup};
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// let mut scene = SceneState::new();
    /// let ids: Vec<_> = (0..4).map(|_| scene.add(Square::new()).erase()).collect();
    /// let g = VGroup::of(&mut scene, ids.clone());
    /// scene.arrange_in_grid(g.erase(), 2, 2, 0.5);
    /// // 2×2 grid centered at origin → all four cells within the grid extent.
    /// assert!(scene.family_bounding_box(g.erase()).center().length() < 1e-4);
    /// ```
    pub fn arrange_in_grid(&mut self, group: AnyId, rows: usize, cols: usize, buff: f32) {
        let children = self.get_dyn(group).data().children.clone();
        if children.is_empty() || rows == 0 || cols == 0 {
            return;
        }
        // Uniform pitch based on the largest child.
        let mut cell_w = 0.0_f32;
        let mut cell_h = 0.0_f32;
        for c in &children {
            let bb = self.family_bounding_box(*c);
            cell_w = cell_w.max(bb.width());
            cell_h = cell_h.max(bb.height());
        }
        let pitch_x = cell_w + buff;
        let pitch_y = cell_h + buff;
        for (k, child) in children.iter().enumerate() {
            let row = k / cols;
            let col = k % cols;
            let target = Point::new(col as f32 * pitch_x, -(row as f32) * pitch_y, 0.0);
            let cur = self.family_bounding_box(*child).center();
            self.shift(*child, target - cur);
        }
        self.move_to(group, manim_math::ORIGIN);
    }

    /// The camera / frame state (manim's `camera.frame`).
    ///
    /// ```
    /// use manim_core::scene_state::SceneState;
    /// let scene = SceneState::new();
    /// assert_eq!(scene.camera().frame_height, 8.0);
    /// ```
    pub fn camera(&self) -> &crate::camera::Camera2D {
        &self.camera
    }

    /// Mutable access to the camera / frame state.
    pub fn camera_mut(&mut self) -> &mut crate::camera::Camera2D {
        &mut self.camera
    }

    /// Replaces the camera state.
    pub fn set_camera(&mut self, camera: crate::camera::Camera2D) {
        self.camera = camera;
    }

    /// Adds a mobject rebuilt from scratch every updater tick by `build`
    /// (manim's `always_redraw`).
    ///
    /// The closure reads the scene and returns a fresh mobject whose path and
    /// style are copied into the live one each frame — so it can follow trackers
    /// or other mobjects.
    ///
    /// ```
    /// use manim_core::geometry::Dot;
    /// use manim_core::scene_state::{SceneState, UpdaterCtx};
    /// use manim_core::mobject::MobjectExt;
    /// use manim_math::{Point, RIGHT};
    /// let mut scene = SceneState::new();
    /// let anchor = scene.add(Dot::new());
    /// // A dot that always sits one unit right of `anchor`.
    /// let follower = scene.always_redraw(move |s| {
    ///     let base = s.try_get(anchor).map(|d| d.get_center()).unwrap_or(Point::ZERO);
    ///     Dot::at(base + RIGHT)
    /// });
    /// scene.get_dyn_mut(anchor.erase()).data_mut().path.apply(|p| p + 2.0 * RIGHT);
    /// scene.run_updaters(UpdaterCtx { dt: 0.0, time: 0.0 });
    /// assert!((scene.get(follower).get_center() - 3.0 * RIGHT).length() < 1e-4);
    /// ```
    pub fn always_redraw<M: Mobject>(
        &mut self,
        build: impl Fn(&SceneState) -> M + Send + Sync + 'static,
    ) -> MobjectId<M> {
        let initial = build(self);
        let id = self.add(initial);
        self.add_updater(id.erase(), move |s, target, _ctx| {
            let data = build(s).data().clone();
            if s.contains(target) {
                *s.get_dyn_mut(target).data_mut() = data;
            }
        });
        id
    }
}

/// Resolves a style [`Gradient`]'s bbox-relative axis to concrete world-space
/// endpoints, producing the renderer-facing [`LinearGradient`].
fn resolve_gradient(g: &Gradient, bb: &BoundingBox) -> LinearGradient {
    let c = bb.center();
    let (start, end) = match g.axis {
        GradientAxis::Horizontal => (
            Point::new(bb.min.x, c.y, 0.0),
            Point::new(bb.max.x, c.y, 0.0),
        ),
        GradientAxis::Vertical => (
            Point::new(c.x, bb.min.y, 0.0),
            Point::new(c.x, bb.max.y, 0.0),
        ),
        GradientAxis::Points(a, b) => (a, b),
    };
    LinearGradient {
        stops: g.stops.clone(),
        start,
        end,
    }
}

impl<M: Mobject> Index<MobjectId<M>> for SceneState {
    type Output = M;
    fn index(&self, id: MobjectId<M>) -> &M {
        self.get(id)
    }
}

impl<M: Mobject> IndexMut<MobjectId<M>> for SceneState {
    fn index_mut(&mut self, id: MobjectId<M>) -> &mut M {
        self.get_mut(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Circle, Square, VGroup};
    use crate::mobject::MobjectExt;
    use manim_math::{RIGHT, UP};

    #[test]
    fn display_list_carries_fill_gradient() {
        use manim_color::{BLUE, RED};
        let mut scene = SceneState::new();
        let sq = scene.add(Square::new());
        scene.set_style_family(sq.erase(), |s| {
            s.set_fill_gradient(Gradient::from_colors(&[BLUE, RED]));
        });
        let dl = scene.display_list();
        let g = dl.0[0].fill.as_ref().unwrap().gradient.as_ref().unwrap();
        assert_eq!(g.stops.len(), 2);
        // Horizontal axis resolved from the bbox: start left of end, same y.
        assert!(g.start.x < g.end.x);
        assert!((g.start.y - g.end.y).abs() < 1e-6);
    }

    #[test]
    fn display_list_carries_background_stroke() {
        use manim_color::{BLUE, RED};
        let mut scene = SceneState::new();
        let c = scene.add(Circle::new());
        scene.set_style_family(c.erase(), |s| {
            s.set_fill(BLUE, 1.0);
            s.set_background_stroke(RED, 8.0, 1.0);
        });
        let dl = scene.display_list();
        let bg = dl.0[0].background_stroke.as_ref().unwrap();
        assert!((bg.width - 8.0).abs() < 1e-6);
        assert_eq!(bg.color, RED);
    }

    #[test]
    fn add_and_typed_access() {
        let mut scene = SceneState::new();
        let c = scene.add(Circle::new());
        assert!((scene[c].radius_value() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn index_mut_mutates_own_path() {
        let mut scene = SceneState::new();
        let c = scene.add(Circle::new());
        scene[c].shift(2.0 * RIGHT);
        assert!((scene[c].get_center() - 2.0 * RIGHT).length() < 1e-6);
    }

    #[test]
    fn remove_makes_handle_stale() {
        let mut scene = SceneState::new();
        let c = scene.add(Circle::new());
        assert!(scene.contains(c.erase()));
        scene.remove(c.erase());
        assert!(!scene.contains(c.erase()));
        assert!(scene.try_get(c).is_none());
    }

    #[test]
    fn remove_group_removes_children() {
        let mut scene = SceneState::new();
        let g = scene.add(VGroup::new());
        let a = scene.add(Circle::new());
        let b = scene.add(Square::new());
        scene.add_child(g.erase(), a.erase());
        scene.add_child(g.erase(), b.erase());
        scene.remove(g.erase());
        assert!(!scene.contains(a.erase()));
        assert!(!scene.contains(b.erase()));
    }

    #[test]
    fn family_transform_moves_children() {
        let mut scene = SceneState::new();
        let g = scene.add(VGroup::new());
        let a = scene.add(Circle::new());
        scene.add_child(g.erase(), a.erase());
        scene.shift(g.erase(), 3.0 * UP);
        assert!((scene.get(a).get_center() - 3.0 * UP).length() < 1e-6);
    }

    #[test]
    fn children_are_not_roots() {
        let mut scene = SceneState::new();
        let g = scene.add(VGroup::new());
        let a = scene.add(Circle::new());
        scene.add_child(g.erase(), a.erase());
        let roots: Vec<AnyId> = scene.iter_visible_roots().collect();
        assert_eq!(roots, vec![g.erase()]);
    }

    #[test]
    fn clone_is_deep() {
        let mut scene = SceneState::new();
        let c = scene.add(Circle::new());
        let snapshot = scene.clone();
        scene[c].shift(RIGHT);
        // The snapshot is unaffected by later mutation.
        assert!(snapshot.get(c).get_center().length() < 1e-6);
        assert!((scene.get(c).get_center() - RIGHT).length() < 1e-6);
    }
}
