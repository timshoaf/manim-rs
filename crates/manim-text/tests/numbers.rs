//! Integration tests for FE-102 (numbers) and FE-100 (markup / lists / title).

use manim_core::animations::ValueTracker;
use manim_core::prelude::*;
use manim_math::FRAME_HEIGHT;
use manim_text::{
    BulletedList, ChangeDecimalToValue, ChangingDecimal, DecimalNumber, Integer, MarkupText, Text,
    Title, Variable,
};

/// The left-edge x of the single drawn item in a display list.
fn left_edge_x(dl: &DisplayList) -> f32 {
    dl.0.iter()
        .flat_map(|it| it.path.bounding_box())
        .map(|(min, _)| min.x)
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn value_sweep_does_not_drift() {
    let mut scene = Scene::new(Config::low());
    let d = scene.add(DecimalNumber::new(0.0).num_decimal_places(0));
    scene.play(ChangeDecimalToValue::new(d, 100.0)).unwrap();

    // Across the whole sweep (spanning the 1→2→3 digit-count boundaries) the
    // left edge must not move.
    let frames: Vec<_> = scene.frames().collect();
    let x0 = left_edge_x(&frames[0].1);
    for (_, dl) in &frames {
        assert!(
            (left_edge_x(dl) - x0).abs() < 1e-3,
            "left edge drifted: {} vs {x0}",
            left_edge_x(dl)
        );
    }
    // The value reached 100 and re-typeset ("100" has 3 glyphs).
    assert!((scene[d].value() - 100.0).abs() < 1e-3);
    assert_eq!(scene[d].glyph_count(), 3);
}

#[test]
fn changing_decimal_from_function() {
    let mut scene = Scene::new(Config::low());
    let d = scene.add(DecimalNumber::new(0.0));
    scene
        .play(ChangingDecimal::new(d, |a| a * 8.0).run_time(1.0))
        .unwrap();
    assert!((scene[d].value() - 8.0).abs() < 1e-3);
}

#[test]
fn integer_and_formatting() {
    let n = Integer::new(1000).group_with_commas(true);
    assert_eq!(n.formatted(), "1,000");
    let pct = DecimalNumber::new(50.0).num_decimal_places(0).unit("%");
    assert_eq!(pct.formatted(), "50%");
}

#[test]
fn variable_tracks_value() {
    let mut scene = SceneState::new();
    let tracker = scene.add(ValueTracker::new(3.0));
    let group = Variable::of(&mut scene, tracker, "x");
    // Bump the tracker and tick; the displayed number follows.
    scene.get_mut(tracker).set_value(7.0);
    scene.run_updaters(UpdaterCtx { dt: 0.0, time: 0.0 });
    // Find the DecimalNumber child and check its value.
    let mut found = None;
    for id in scene.family(group.erase()) {
        if let Some(d) = scene.get_dyn(id).as_any().downcast_ref::<DecimalNumber>() {
            found = Some(d.value());
        }
    }
    assert_eq!(found, Some(7.0));
}

#[test]
fn bulleted_list_stacks_rows() {
    let mut scene = SceneState::new();
    let list = BulletedList::of(&mut scene, &["First", "Second", "Third"]);
    let children = scene.get_dyn(list.erase()).data().children.clone();
    assert_eq!(children.len(), 3);
    // Rows descend: each child's center y is below the previous.
    let ys: Vec<f32> = children
        .iter()
        .map(|c| scene.family_bounding_box(*c).center().y)
        .collect();
    assert!(ys[0] > ys[1] && ys[1] > ys[2]);
}

#[test]
fn title_underline_below_text_and_near_top() {
    let mut scene = SceneState::new();
    let title = Title::of(&mut scene, "Heading");
    let children = scene.get_dyn(title.erase()).data().children.clone();
    assert_eq!(children.len(), 2); // text + underline
    let text_bb = scene.family_bounding_box(children[0]);
    let line_bb = scene.family_bounding_box(children[1]);
    // Underline sits below the text…
    assert!(line_bb.max.y < text_bb.center().y);
    // …and spans roughly the text width.
    assert!((line_bb.width() - text_bb.width()).abs() < 0.3);
    // The whole title is near the top of the frame.
    assert!(scene.family_bounding_box(title.erase()).max.y > FRAME_HEIGHT / 2.0 - 1.0);
}

#[test]
fn markup_bold_changes_outline_and_colors_children() {
    // Bold vs plain differ in outline.
    let plain = MarkupText::new("word").unwrap();
    let bold = MarkupText::new("<b>word</b>").unwrap();
    assert_ne!(plain.data().path, bold.data().path);

    // A colored span colors its glyph children red.
    let mut scene = SceneState::new();
    let m = MarkupText::new(r##"a<span foreground="#FF0000">b</span>"##)
        .unwrap()
        .add_to(&mut scene);
    let children = scene.get(m).glyph_ids().to_vec();
    // The "b" span glyph is pure red (#FF0000).
    let pure_red = Color::from_hex("#FF0000").unwrap();
    let red = children
        .iter()
        .filter_map(|c| scene.get_dyn(*c).data().style.fill_color)
        .any(|c| c == pure_red);
    assert!(red, "expected a red glyph child");
}

#[test]
fn markup_underline_adds_a_rule_child() {
    let mut scene = SceneState::new();
    let plain = MarkupText::new("abc").unwrap().add_to(&mut scene);
    let n_plain = scene.get_dyn(plain.erase()).data().children.len();
    let underlined = MarkupText::new("<u>abc</u>").unwrap().add_to(&mut scene);
    let n_under = scene.get_dyn(underlined.erase()).data().children.len();
    // The underline adds exactly one extra child (the rule).
    assert_eq!(n_under, n_plain + 1);
}

#[test]
fn markup_animates_with_write() {
    use manim_text::Write;
    let mut scene = Scene::new(Config::low());
    let m = MarkupText::new("<b>Hi</b>")
        .unwrap()
        .add_to(scene.state_mut());
    scene.play(Write::new(m)).unwrap();
    assert!(scene.total_duration() > 0.0);
    let _ = Text::new("smoke"); // Text still usable alongside markup
}
