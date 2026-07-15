//! Integration tests for phase 8: coordinate labels, `LabeledDot`, and
//! `TransformMatchingTex`.

use manim_core::graphing::{Axes, NumberLine};
use manim_core::prelude::*;
use manim_text::{
    AxesLabels, CoordinateLabels, DecimalNumber, GraphLabel, LabeledDot, MathTex,
    TransformMatchingTex,
};

#[test]
fn number_line_labels_match_ticks_and_sit_below() {
    let mut scene = SceneState::new();
    let nl = NumberLine::new(0.0, 5.0, 1.0);
    let labels = nl.add_numbers(&mut scene);
    let children = scene.get_dyn(labels.erase()).data().children.clone();
    // One label per tick (0 … 5).
    assert_eq!(children.len(), 6);
    // Labels sit below the axis (y = 0).
    for c in &children {
        assert!(scene.family_bounding_box(*c).center().y < 0.0);
    }
    // An integral range uses Integer labels (no decimal point).
    let first = scene
        .get_dyn(children[0])
        .as_any()
        .downcast_ref::<DecimalNumber>()
        .unwrap();
    assert!(!first.formatted().contains('.'));
}

#[test]
fn axes_coordinates_exclude_origin() {
    let mut scene = SceneState::new();
    let axes = Axes::new([-2.0, 2.0, 1.0], [-2.0, 2.0, 1.0]);
    let labels = axes.add_coordinates(&mut scene);
    // x ticks {-2,-1,1,2} + y ticks {-2,-1,1,2} (0 excluded on each) = 8.
    assert_eq!(scene.get_dyn(labels.erase()).data().children.len(), 8);
}

#[test]
fn axis_and_graph_labels_typeset() {
    let mut scene = SceneState::new();
    let axes = Axes::new([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0]);
    let axis_labels = axes.get_axis_labels(&mut scene, "x", "y").unwrap();
    assert_eq!(scene.get_dyn(axis_labels.erase()).data().children.len(), 2);

    let graph = axes.plot(|x| x * 0.5, None);
    let label = axes
        .get_graph_label(&mut scene, &graph, "f(x)", 2.0)
        .unwrap();
    // The label sits near the graph point at x = 2 (glyphs live in the family).
    let anchor = axes.input_to_graph_point(2.0, &graph);
    assert!((scene.family_bounding_box(label.erase()).center() - anchor).length() < 1.0);
}

#[test]
fn labeled_dot_fits_label_inside() {
    let mut scene = SceneState::new();
    let d = LabeledDot::of(&mut scene, "P");
    let family = scene.family(d.erase());
    assert!(family.len() >= 2); // group + dot + label glyphs
                                // The label's extent is within the dot radius.
    let bb = scene.family_bounding_box(d.erase());
    assert!(bb.width() <= manim_text::LABELED_DOT_RADIUS * 2.0 + 1e-3);
}

#[test]
fn transform_matching_tex_matches_shared_glyphs() {
    let mut scene = Scene::new(Config::low());
    let a = MathTex::new(r"e^{i\pi} + 1 = 0")
        .unwrap()
        .add_to(scene.state_mut());
    let b = MathTex::new(r"e^{i\pi} = -1")
        .unwrap()
        .add_to(scene.state_mut());

    let m = TransformMatchingTex::analyze(scene.state(), a, b);
    // e, i, π, =, 1 are shared → at least 5 matched pairs.
    assert!(
        m.matched.len() >= 5,
        "matched {} pairs: {:?}",
        m.matched.len(),
        m.matched.len()
    );
    // "+" and "0" have no match in the target; "-" has none in the source.
    assert!(!m.unmatched_source.is_empty());
    assert!(!m.unmatched_target.is_empty());

    // The animation plays through the standard machinery.
    scene.play(TransformMatchingTex::new(a, b)).unwrap();
    assert!((scene.total_duration() - 1.0).abs() < 1e-6);
}

#[test]
fn transform_matching_identical_matches_all() {
    let mut scene = SceneState::new();
    let a = MathTex::new("abc").unwrap().add_to(&mut scene);
    let b = MathTex::new("abc").unwrap().add_to(&mut scene);
    let m = TransformMatchingTex::analyze(&scene, a, b);
    // Identical formulas match every glyph, none left over.
    assert_eq!(m.matched.len(), scene.get(a).glyph_count());
    assert!(m.unmatched_source.is_empty());
    assert!(m.unmatched_target.is_empty());
}
