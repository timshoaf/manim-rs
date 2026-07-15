//! [`BraceLabel`]: a [`Brace`] measuring an existing mobject, with an attached
//! text label. Port of manim CE's `BraceLabel` (FE-101 parity).

use manim_core::geometry::{Brace, VGroup};
use manim_core::mobject::{AnyId, MobjectId};
use manim_core::scene_state::SceneState;
use manim_math::{Point, DOWN};

use crate::text::Text;

/// Default font size for a brace's label.
pub const BRACE_LABEL_FONT_SIZE: f32 = 32.0;
/// Default gap from the brace tip to the label.
pub const BRACE_LABEL_BUFF: f32 = 0.25;

/// A brace that measures a target mobject's extent plus a text label beyond its
/// tip. Port of manim CE's `BraceLabel`.
///
/// [`BraceLabel::of`] measures `target` via its scene bounding box, adds a
/// [`Brace`] on the requested side, adds a [`Text`] label beyond the brace tip,
/// and returns the group holding both.
pub struct BraceLabel;

impl BraceLabel {
    /// Braces `target`'s extent pointing down, labelling it beneath.
    ///
    /// ```
    /// use manim_text::BraceLabel;
    /// use manim_core::geometry::Square;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// let mut scene = SceneState::new();
    /// let sq = scene.add(Square::new());
    /// let label = BraceLabel::of(&mut scene, sq.erase(), "width");
    /// // The group holds the brace plus the label's glyph children.
    /// assert!(scene.family(label.erase()).len() >= 2);
    /// ```
    pub fn of(scene: &mut SceneState, target: AnyId, label: &str) -> MobjectId<VGroup> {
        Self::of_with(scene, target, label, DOWN)
    }

    /// Braces `target` on the side facing `direction`, labelling it beyond the
    /// brace tip.
    pub fn of_with(
        scene: &mut SceneState,
        target: AnyId,
        label: &str,
        direction: Point,
    ) -> MobjectId<VGroup> {
        let bbox = scene.family_bounding_box(target);
        let brace = Brace::attached_to(bbox, direction);
        let label_at = brace.brace_label_point(BRACE_LABEL_BUFF);
        let brace_id = scene.add(brace).erase();

        let text_id = Text::new(label)
            .font_size(BRACE_LABEL_FONT_SIZE)
            .add_to(scene)
            .erase();
        scene.move_to(text_id, label_at);

        VGroup::of(scene, [brace_id, text_id])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_core::geometry::Square;

    #[test]
    fn brace_and_label_grouped_below() {
        let mut scene = SceneState::new();
        let sq = scene.add(Square::new()); // [-1,1]^2
        let group = BraceLabel::of(&mut scene, sq.erase(), "w");
        // Brace + label glyphs live under the group.
        assert!(scene.get_dyn(group.erase()).data().children.len() >= 2);
        // The whole label group sits below the square.
        assert!(scene.family_bounding_box(group.erase()).max.y < 1.0);
    }
}
