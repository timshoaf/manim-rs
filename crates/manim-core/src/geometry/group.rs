//! [`VGroup`]: a container mobject whose geometry is its children's geometry.

use crate::impl_mobject;
use crate::mobject::{AnyId, MobjectData, MobjectId};
use crate::scene_state::SceneState;
use crate::style::Style;

/// A group mobject with no geometry of its own; it exists to collect
/// submobjects so they transform and draw together. Port of manim CE's `VGroup`.
///
/// A group's own path is empty, so it contributes nothing to the display list
/// directly — only its children draw. Group transforms are just family-aware
/// scene transforms applied to the group's handle.
///
/// ```
/// use manim_core::geometry::{Circle, Square, VGroup};
/// use manim_core::scene_state::SceneState;
/// use manim_core::mobject::{Buildable, Mobject, MobjectExt};
/// use manim_math::RIGHT;
///
/// let mut scene = SceneState::new();
/// let a = scene.add(Circle::new());
/// let b = scene.add(Square::new());
/// // Bundle existing mobjects into a group…
/// let group = VGroup::of(&mut scene, [a.erase(), b.erase()]);
/// // …then transform the whole family at once.
/// scene.shift(group.erase(), 2.0 * RIGHT);
/// assert!((scene.get(a).get_center() - 2.0 * RIGHT).length() < 1e-6);
/// ```
#[derive(Clone)]
pub struct VGroup {
    data: MobjectData,
}
impl_mobject!(VGroup);

/// A group of **arbitrary** mobjects. Port of manim CE's `Group`.
///
/// CE distinguishes `Group` (holds any `Mobject`) from `VGroup` (vector mobjects
/// only). In our arena model every group is type-erased — children are stored as
/// `AnyId`, whatever their concrete type — so a `Group` and a [`VGroup`] are the
/// **same type**. `Group` is provided as an alias for manim-name parity and to
/// signal intent when a group holds non-vector content (an
/// [`ImageMobject`](crate::image_mobject::ImageMobject) alongside vector
/// mobjects), which CE's `VGroup` cannot. Construct with `Group::new()` /
/// `Group::of(scene, ids)`.
///
/// ```
/// use manim_core::geometry::Group;
/// use manim_core::geometry::Circle;
/// use manim_core::scene_state::SceneState;
/// let mut scene = SceneState::new();
/// let c = scene.add(Circle::new());
/// let g = Group::of(&mut scene, [c.erase()]);
/// assert_eq!(scene.family(g.erase()).len(), 2);
/// ```
pub type Group = VGroup;

impl VGroup {
    /// An empty group.
    pub fn new() -> Self {
        Self {
            data: MobjectData::new(Default::default(), Style::default()),
        }
    }

    /// Adds `scene`, wraps the given already-added children in a new group, and
    /// returns the group's handle.
    ///
    /// ```
    /// use manim_core::geometry::{Circle, VGroup};
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let c = scene.add(Circle::new());
    /// let g = VGroup::of(&mut scene, [c.erase()]);
    /// assert_eq!(scene.family(g.erase()).len(), 2);
    /// ```
    pub fn of(
        scene: &mut SceneState,
        children: impl IntoIterator<Item = impl Into<AnyId>>,
    ) -> MobjectId<VGroup> {
        let group = scene.add(VGroup::new());
        for child in children {
            scene.add_child(group, child);
        }
        group
    }
}

impl Default for VGroup {
    fn default() -> Self {
        Self::new()
    }
}
