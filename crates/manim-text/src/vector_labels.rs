//! [`label_vector`]: a LaTeX label that tracks a vector arrow's tip each frame.
//! The manim-text half of the vector-space helpers (the core half lives in
//! `manim_core::vector_space`), split like [`CoordinateLabels`](crate::CoordinateLabels)
//! because it needs [`MathTex`] typesetting.

use manim_core::error::CoreError;
use manim_core::mobject::{AnyId, MobjectId};
use manim_core::scene_state::SceneState;
use manim_core::vector_space::vector_tip;
use manim_math::{Point, UP};

use crate::math::MathTex;

/// Default font size for a vector label.
pub const VECTOR_LABEL_FONT_SIZE: f32 = 36.0;
/// How far beyond the arrowhead (scene units) the label sits.
pub const VECTOR_LABEL_BUFF: f32 = 0.35;

/// Adds a LaTeX `tex` label for the vector arrow `arrow`, placed just past its
/// tip, with an updater that keeps it at the tip as the arrow is transformed.
/// Port of manim CE's `VectorScene.label_vector`.
///
/// # Errors
///
/// A [`CoreError::Text`] if `tex` fails to typeset.
///
/// ```
/// use manim_text::label_vector;
/// use manim_core::vector_space::add_vector;
/// use manim_core::scene_state::SceneState;
/// use manim_math::Point;
/// let mut scene = SceneState::new();
/// let v = add_vector(&mut scene, Point::new(2.0, 1.0, 0.0));
/// let label = label_vector(&mut scene, v.erase(), r"\vec{v}").unwrap();
/// assert!(scene.contains(label.erase()));
/// ```
pub fn label_vector(
    scene: &mut SceneState,
    arrow: AnyId,
    tex: &str,
) -> Result<MobjectId<MathTex>, CoreError> {
    let label = MathTex::new(tex)?
        .font_size(VECTOR_LABEL_FONT_SIZE)
        .add_to(scene);
    let start = label_point(scene, arrow);
    scene.move_to(label.erase(), start);
    scene.add_updater(label.erase(), move |s, id, _ctx| {
        let target = label_point(s, arrow);
        s.move_to(id, target);
    });
    Ok(label)
}

/// The label anchor: just past the arrow's tip, along the tip direction (or up,
/// for a degenerate zero vector).
fn label_point(scene: &SceneState, arrow: AnyId) -> Point {
    let tip = vector_tip(scene, arrow);
    let len = tip.length();
    let dir = if len > 1e-6 { tip / len } else { UP };
    tip + dir * VECTOR_LABEL_BUFF
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_core::scene_state::UpdaterCtx;
    use manim_core::vector_space::add_vector;

    #[test]
    fn label_tracks_transformed_vector() {
        let mut scene = SceneState::new();
        let arrow = add_vector(&mut scene, Point::new(1.0, 0.0, 0.0));
        let label = label_vector(&mut scene, arrow.erase(), r"\vec{v}").unwrap();

        scene.run_updaters(UpdaterCtx { dt: 0.0, time: 0.0 });
        let before = scene.family_bounding_box(label.erase()).center();

        // Stretch the arrow along x (as a transform would): tip moves to ~x=3.
        scene
            .get_dyn_mut(arrow.erase())
            .data_mut()
            .path
            .apply(|p| Point::new(p.x * 3.0, p.y, 0.0));
        scene.run_updaters(UpdaterCtx { dt: 0.0, time: 0.1 });
        let after = scene.family_bounding_box(label.erase()).center();

        // The label followed the tip rightward.
        assert!(
            after.x > before.x + 1.0,
            "before {before:?} after {after:?}"
        );
    }
}
