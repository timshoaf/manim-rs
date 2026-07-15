//! Integration tests for `manim-text`: shaping, coloring, and animating text
//! through the `manim-core` scene/animation machinery.

use manim_core::animations::Create;
use manim_core::prelude::*;
use manim_text::{Alignment, Paragraph, Slant, Text, Weighting, Write};

#[test]
fn glyph_count_and_indexing() {
    let t = Text::new("Hi!");
    assert_eq!(t.glyph_count(), 3);
    assert_eq!(t.glyph_char(0), Some('H'));
    assert_eq!(t.glyph_char(1), Some('i'));
    assert_eq!(t.glyph_char(2), Some('!'));
}

#[test]
fn t2c_colors_the_right_glyphs() {
    let t = Text::new("RGB").color(WHITE).t2c(&[("G", GREEN)]);
    assert_eq!(t.glyph_color(0), Some(WHITE));
    assert_eq!(t.glyph_color(1), Some(GREEN));
    assert_eq!(t.glyph_color(2), Some(WHITE));

    // Colors carry through to the child mobjects when added.
    let mut scene = SceneState::new();
    let id = t.add_to(&mut scene);
    let children = scene.get(id).glyph_ids().to_vec();
    assert_eq!(
        scene.get_dyn(children[1]).data().style.fill_color,
        Some(GREEN)
    );
}

#[test]
fn deterministic_paths_across_runs() {
    let a = Text::new("Determinism");
    let b = Text::new("Determinism");
    assert_eq!(a.data().path, b.data().path);
}

#[test]
fn multiline_is_taller_and_spacing_widens_it() {
    let one = Text::new("Hello").bounding_box().height();
    let two = Text::new("Hello\nWorld").bounding_box().height();
    assert!(two > one * 1.5, "two lines should be much taller");

    let tight = Text::new("a\nb").line_spacing(1.0).bounding_box().height();
    let loose = Text::new("a\nb").line_spacing(2.0).bounding_box().height();
    assert!(loose > tight, "larger line_spacing increases height");
}

#[test]
fn paragraph_alignment_builds() {
    let p = Paragraph::aligned(&["short", "a longer line"], Alignment::Center);
    assert!(p.glyph_count() > 0);
    // Centered multi-line text is still centered at the origin.
    assert!(p.get_center().length() < 1e-3);
}

#[test]
fn weight_and_slant_change_outline() {
    let normal = Text::new("a");
    let bold = Text::new("a").weight(Weighting::Bold);
    let italic = Text::new("a").slant(Slant::Italic);
    // Different faces → different outlines (bold is wider/heavier).
    assert_ne!(normal.data().path, bold.data().path);
    assert_ne!(normal.data().path, italic.data().path);
}

#[test]
fn create_animates_text_glyphs() {
    let mut scene = Scene::new(Config::low());
    let t = Text::new("Hi").add_to(scene.state_mut());
    scene.play(Create::new(t)).unwrap();

    // Drawn length grows monotonically and ends fully drawn.
    let drawn = |dl: &DisplayList| -> f32 {
        dl.0.iter()
            .flat_map(|it| it.path.subpaths.iter())
            .map(|s| s.arc_length())
            .sum()
    };
    let frames: Vec<_> = scene.frames().collect();
    assert!(frames.len() > 2);
    let first = drawn(&frames[1].1); // frame 0 is empty
    let last = drawn(&frames.last().unwrap().1);
    assert!(last > first);
    assert!(last > 0.5, "text should be fully drawn at the end");

    // Two glyph children under the parent.
    assert_eq!(scene.state().family(t.erase()).len(), 3);
}

#[test]
fn write_reveals_glyphs_in_order() {
    let mut scene = Scene::new(Config::low());
    let t = Text::new("abc").add_to(scene.state_mut());
    scene.play(Write::new(t)).unwrap();

    let frames: Vec<_> = scene.frames().collect();
    // Early in the Write, the first glyph is more drawn than the last.
    let early = &frames[frames.len() / 5].1;
    let children = scene.state().get_dyn(t.erase()).data().children.clone();
    let len_of = |dl: &DisplayList, src: AnyId| -> f32 {
        dl.0.iter()
            .filter(|it| it.source == src)
            .flat_map(|it| it.path.subpaths.iter())
            .map(|s| s.arc_length())
            .sum()
    };
    let first_glyph = len_of(early, children[0]);
    let last_glyph = len_of(early, children[2]);
    assert!(
        first_glyph >= last_glyph,
        "first glyph should lead: {first_glyph} vs {last_glyph}"
    );

    // Fully written at the end.
    let end = &frames.last().unwrap().1;
    assert!(len_of(end, children[2]) > 0.0);
}

#[test]
fn bbox_scales_with_font_size() {
    let h = |fs: f32| Text::new("Xyz").font_size(fs).bounding_box().height();
    // Height is proportional to font size.
    assert!((h(96.0) / h(48.0) - 2.0).abs() < 0.05);
}
