//! Composite text layouts: [`BulletedList`] and [`Title`].

use manim_core::geometry::{Line, VGroup};
use manim_core::mobject::{AnyId, MobjectId};
use manim_core::scene_state::SceneState;
use manim_math::{Point, DOWN, FRAME_HEIGHT, UP};

use crate::text::Text;

/// manim CE's default buffer between bulleted-list items.
pub const LIST_BUFF: f32 = 0.5;

/// A vertical, dot-bulleted list of text items. Port of manim CE's
/// `BulletedList`.
///
/// [`BulletedList::of`] adds one bulleted [`Text`] per item, stacked top to
/// bottom, and returns the group.
pub struct BulletedList;

impl BulletedList {
    /// Adds a bulleted list of `items` to `scene`, returning the group handle.
    ///
    /// ```
    /// use manim_text::BulletedList;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// let mut scene = SceneState::new();
    /// let list = BulletedList::of(&mut scene, &["First", "Second", "Third"]);
    /// // Three rows, stacked so the group is taller than one row.
    /// assert_eq!(scene.get_dyn(list.erase()).data().children.len(), 3);
    /// ```
    pub fn of(scene: &mut SceneState, items: &[&str]) -> MobjectId<VGroup> {
        let ids: Vec<AnyId> = items
            .iter()
            .map(|item| Text::new(format!("• {item}")).add_to(scene).erase())
            .collect();
        let group = VGroup::of(scene, ids);
        scene.arrange(group.erase(), DOWN, LIST_BUFF);
        group
    }
}

/// A title: text with an underline rule beneath it, pinned to the top edge of
/// the frame. Port of manim CE's `Title`.
pub struct Title;

impl Title {
    /// Adds a titled heading to `scene` (text + underline, at the top edge) and
    /// returns the group handle.
    ///
    /// ```
    /// use manim_text::Title;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// use manim_math::FRAME_HEIGHT;
    /// let mut scene = SceneState::new();
    /// let title = Title::of(&mut scene, "Chapter One");
    /// // Sits near the top of the frame.
    /// assert!(scene.family_bounding_box(title.erase()).max.y > FRAME_HEIGHT / 2.0 - 1.0);
    /// ```
    pub fn of(scene: &mut SceneState, text: &str) -> MobjectId<VGroup> {
        let text_id = Text::new(text).add_to(scene);
        let bb = scene.family_bounding_box(text_id.erase());
        // Underline spanning the text width, just below it.
        let y = bb.min.y - 0.15;
        let left = Point::new(bb.min.x, y, 0.0);
        let right = Point::new(bb.max.x, y, 0.0);
        let line = scene.add(Line::new(left, right));

        let group = VGroup::of(scene, [text_id.erase(), line.erase()]);
        // Pin the group's top edge near the top of the frame.
        let gbb = scene.family_bounding_box(group.erase());
        let target_top = FRAME_HEIGHT / 2.0 - 0.5;
        scene.shift(group.erase(), UP * (target_top - gbb.max.y));
        group
    }
}
