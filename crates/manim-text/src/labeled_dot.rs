//! [`LabeledDot`]: a dot with a centered text label scaled to fit inside it,
//! plus [`LabeledLine`] / [`LabeledArrow`] (a segment with a text label at its
//! midpoint).

use manim_color::{BLACK, BLUE};
use manim_core::geometry::{Arrow, Dot, Line, VGroup};
use manim_core::mobject::{AnyId, Buildable, MobjectExt, MobjectId};
use manim_core::scene_state::SceneState;
use manim_math::{Point, ORIGIN, UP};

use crate::text::Text;

/// manim CE's enlarged default `LabeledDot` radius.
pub const LABELED_DOT_RADIUS: f32 = 0.35;

/// Font size for a [`LabeledLine`] / [`LabeledArrow`] label.
pub const LABELED_LINE_FONT_SIZE: f32 = 28.0;

/// A [`Line`] with a text label at its midpoint. Port of manim CE's
/// `LabeledLine`.
pub struct LabeledLine;

impl LabeledLine {
    /// Adds a labeled line from `start` to `end` to `scene`, returning the group.
    ///
    /// ```
    /// use manim_text::LabeledLine;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// use manim_math::{Point, RIGHT};
    /// let mut scene = SceneState::new();
    /// let g = LabeledLine::of(&mut scene, Point::ZERO, 3.0 * RIGHT, "d");
    /// assert!(scene.family(g.erase()).len() >= 2);
    /// ```
    pub fn of(scene: &mut SceneState, start: Point, end: Point, label: &str) -> MobjectId<VGroup> {
        let line = scene.add(Line::new(start, end)).erase();
        finish_labeled(scene, line, (start + end) * 0.5, label)
    }
}

/// An [`Arrow`] with a text label at its midpoint. Port of manim CE's
/// `LabeledArrow`.
pub struct LabeledArrow;

impl LabeledArrow {
    /// Adds a labeled arrow from `start` to `end` to `scene`, returning the group.
    pub fn of(scene: &mut SceneState, start: Point, end: Point, label: &str) -> MobjectId<VGroup> {
        let arrow = scene.add(Arrow::new(start, end)).erase();
        finish_labeled(scene, arrow, (start + end) * 0.5, label)
    }
}

/// Adds a label above `mid` and groups it with `shape`.
fn finish_labeled(
    scene: &mut SceneState,
    shape: AnyId,
    mid: Point,
    label: &str,
) -> MobjectId<VGroup> {
    let text = Text::new(label)
        .font_size(LABELED_LINE_FONT_SIZE)
        .add_to(scene)
        .erase();
    scene.move_to(text, mid + 0.3 * UP);
    VGroup::of(scene, [shape, text])
}

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
