//! [`LabeledDot`]: a dot with a centered text label scaled to fit inside it.

use manim_color::{BLACK, BLUE};
use manim_core::geometry::{Dot, VGroup};
use manim_core::mobject::{Buildable, MobjectExt, MobjectId};
use manim_core::scene_state::SceneState;
use manim_math::ORIGIN;

use crate::text::Text;

/// manim CE's enlarged default `LabeledDot` radius.
pub const LABELED_DOT_RADIUS: f32 = 0.35;

/// A [`Dot`] with a centered [`Text`] label, auto-scaled to fit within the dot.
/// Port of manim CE's `LabeledDot`.
///
/// [`LabeledDot::of`] adds an enlarged dot and a fitted label to the scene and
/// returns the group.
pub struct LabeledDot;

impl LabeledDot {
    /// Adds a labeled dot showing `label` to `scene`, returning the group.
    ///
    /// ```
    /// use manim_text::LabeledDot;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// let mut scene = SceneState::new();
    /// let d = LabeledDot::of(&mut scene, "P");
    /// // The group holds the dot plus the label's glyph children.
    /// assert!(scene.family(d.erase()).len() >= 2);
    /// ```
    pub fn of(scene: &mut SceneState, label: &str) -> MobjectId<VGroup> {
        let dot = scene.add(
            Dot::new()
                .radius(LABELED_DOT_RADIUS)
                .with_fill(BLUE, 1.0)
                .with_move_to(ORIGIN),
        );

        // Fit the label within ~65% of the dot diameter.
        let mut text = Text::new(label).color(BLACK);
        let bb = text.bounding_box();
        let half_extent = (bb.width().max(bb.height()) / 2.0).max(1e-4);
        let target = LABELED_DOT_RADIUS * 0.65;
        text.scale(target / half_extent).move_to(ORIGIN);
        let text_id = text.add_to(scene);

        VGroup::of(scene, [dot.erase(), text_id.erase()])
    }
}
