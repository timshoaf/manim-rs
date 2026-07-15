//! Phase 12 DX fixes: family-aware gradients on `Text`, and `?`-ability of the
//! text crate's fallible construct-time APIs inside a `CoreError` result.

use manim_core::graphing::Axes;
use manim_core::prelude::*;
use manim_text::{AxesLabels, MathTex, Matrix, Text};

#[test]
fn text_gradient_distributes_across_glyphs() {
    let mut scene = SceneState::new();
    let text = Text::new("ABCD").add_to(&mut scene);
    scene.set_color_by_gradient(text.erase(), &[BLUE, RED]);

    let glyphs = scene.get_dyn(text.erase()).data().children.clone();
    assert!(glyphs.len() >= 2, "expected multiple glyphs");
    // First glyph gets the first stop, last glyph the last stop.
    let first = scene.get_dyn(glyphs[0]).data().style.fill_color;
    let last = scene
        .get_dyn(*glyphs.last().unwrap())
        .data()
        .style
        .fill_color;
    assert_eq!(first, Some(BLUE));
    assert_eq!(last, Some(RED));
}

// Compile-level proof that the text crate's fallible APIs return `CoreError`, so
// they compose with `?` inside a function returning `manim_core::Result`.
#[test]
fn text_apis_are_question_mark_able() {
    fn build(scene: &mut SceneState) -> Result<()> {
        let axes = Axes::new([-2.0, 2.0, 1.0], [-2.0, 2.0, 1.0]);
        let _labels = axes.get_axis_labels(scene, "x", "y")?;
        let _formula = MathTex::new(r"e^{i\pi}")?.add_to(scene);
        let _matrix = Matrix::of(scene, &[&["1", "0"], &["0", "1"]])?;
        Ok(())
    }

    let mut scene = SceneState::new();
    build(&mut scene).expect("all text APIs should build");
}
