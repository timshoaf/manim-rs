//! Integration tests for `MathTex`/`Typst`/`Tex`: typst-backed math typesetting
//! and its integration with the core scene/animation machinery.

use manim_core::animations::Create;
use manim_core::prelude::*;
use manim_text::{MathError, MathTex, Tex, Typst, Write};

#[test]
fn pinned_formula_glyph_counts() {
    assert_eq!(
        MathTex::new(r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}")
            .unwrap()
            .glyph_count(),
        16
    );
    assert_eq!(MathTex::new(r"e^{i\pi} + 1 = 0").unwrap().glyph_count(), 7);
    assert_eq!(MathTex::new(r"\int_0^1 x^2 dx").unwrap().glyph_count(), 7);
    assert_eq!(
        MathTex::new(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}")
            .unwrap()
            .glyph_count(),
        6
    );
}

#[test]
fn typst_and_tex_render() {
    assert_eq!(Typst::new("e^(i pi) + 1 = 0").unwrap().glyph_count(), 7);
    assert_eq!(Tex::new("Hello").unwrap().glyph_count(), 5);
}

#[test]
fn deterministic_across_runs() {
    let a = MathTex::new(r"\sum_{n=1}^{\infty} \frac{1}{n^2}").unwrap();
    let b = MathTex::new(r"\sum_{n=1}^{\infty} \frac{1}{n^2}").unwrap();
    assert_eq!(a.data().path, b.data().path);
    assert_eq!(a.glyph_count(), b.glyph_count());
}

#[test]
fn mathtex_matches_equivalent_typst_baseline() {
    // MathTex("a = b") translates to the same typst as Typst("a = b"), so the
    // laid-out glyph outlines (and thus the '=' baseline/x-height) match.
    let via_latex = MathTex::new("a = b").unwrap();
    let via_typst = Typst::new("a = b").unwrap();
    assert_eq!(via_latex.data().path, via_typst.data().path);
}

#[test]
fn unknown_command_lists_the_token() {
    match MathTex::new(r"\frac{a}{b} + \bogus") {
        Err(MathError::UnknownCommand(cmd)) => assert_eq!(cmd, "bogus"),
        Err(other) => panic!("expected UnknownCommand, got {other:?}"),
        Ok(_) => panic!("expected an error for \\bogus"),
    }
}

#[test]
fn centered_and_sized() {
    let m = MathTex::new("x + y = z").unwrap();
    assert!(m.get_center().length() < 1e-3);
    // Bounded, non-degenerate extent.
    let bb = m.bounding_box();
    assert!(bb.width() > 0.5 && bb.width() < 10.0);
    assert!(bb.height() > 0.2 && bb.height() < 3.0);
}

#[test]
fn font_size_scales() {
    let small = MathTex::new("x")
        .unwrap()
        .font_size(24.0)
        .bounding_box()
        .height();
    let big = MathTex::new("x")
        .unwrap()
        .font_size(96.0)
        .bounding_box()
        .height();
    assert!((big / small - 4.0).abs() < 0.1);
}

#[test]
fn add_to_creates_glyph_children() {
    let mut scene = SceneState::new();
    let m = MathTex::new(r"e^{i\pi}").unwrap().add_to(&mut scene);
    // Parent + one child per glyph.
    let n = scene.get(m).glyph_count();
    assert_eq!(scene.family(m.erase()).len(), 1 + n);
    // Parent draws nothing itself.
    assert!(scene
        .get(m)
        .data()
        .path
        .subpaths
        .iter()
        .all(|s| s.curves.is_empty()));
}

#[test]
fn create_animates_a_formula() {
    let mut scene = Scene::new(Config::low());
    let m = MathTex::new(r"a^2 + b^2 = c^2")
        .unwrap()
        .add_to(scene.state_mut());
    scene.play(Create::new(m)).unwrap();

    let drawn = |dl: &DisplayList| -> f32 {
        dl.0.iter()
            .flat_map(|it| it.path.subpaths.iter())
            .map(|s| s.arc_length())
            .sum()
    };
    let frames: Vec<_> = scene.frames().collect();
    let mid = drawn(&frames[frames.len() / 2].1);
    let end = drawn(&frames.last().unwrap().1);
    assert!(end > mid);
    assert!(end > 1.0);
}

#[test]
fn write_reveals_a_formula_left_to_right() {
    let mut scene = Scene::new(Config::low());
    let m = MathTex::new("x + y").unwrap().add_to(scene.state_mut());
    scene.play(Write::new(m)).unwrap();

    let frames: Vec<_> = scene.frames().collect();
    let children = scene.state().get_dyn(m.erase()).data().children.clone();
    let len_of = |dl: &DisplayList, src: AnyId| -> f32 {
        dl.0.iter()
            .filter(|it| it.source == src)
            .flat_map(|it| it.path.subpaths.iter())
            .map(|s| s.arc_length())
            .sum()
    };
    let early = &frames[frames.len() / 5].1;
    // The leftmost glyph leads the rightmost.
    assert!(len_of(early, children[0]) >= len_of(early, *children.last().unwrap()));
    // Fully written by the end.
    let end = &frames.last().unwrap().1;
    assert!(len_of(end, *children.last().unwrap()) > 0.0);
}
