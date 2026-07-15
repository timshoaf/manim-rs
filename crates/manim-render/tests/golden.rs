//! Golden-image tests for the offscreen renderer.
//!
//! Each test builds a scene, renders it at 427×240 (half of manim CE's `-ql`
//! 854×480, same 14.222×8 frame), and compares against a checked-in PNG with a
//! per-channel + fraction-of-pixels tolerance. Run with `BLESS=1` to (re)seed
//! the goldens after an intentional visual change.
//!
//! If no GPU adapter is available (headless CI without a software backend), each
//! test prints a warning and returns rather than failing — CI with lavapipe
//! exercises the real path.

use manim_color::{BLUE, RED, WHITE};
use manim_core::config::Config;
use manim_core::geometry::{Arrow, Circle, Square, Triangle};
use manim_core::mobject::Buildable;
use manim_core::scene_state::SceneState;
use manim_math::{LEFT, RIGHT};
use manim_render::golden::assert_golden;
use manim_render::renderer::OffscreenRenderer;

/// The test render size: half of `-ql`, same frame dimensions.
fn test_config() -> Config {
    Config {
        pixel_width: 427,
        pixel_height: 240,
        ..Config::default()
    }
}

/// Builds a renderer, or returns `None` (with a warning) if no GPU is available
/// so tests skip cleanly in adapter-less environments.
fn try_renderer() -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new(&test_config()) {
        Ok(r) => {
            let info = r.context().adapter_info();
            eprintln!(
                "manim-render golden tests: {:?} backend, adapter {:?}",
                info.backend, info.name
            );
            Some(r)
        }
        Err(e) => {
            eprintln!("SKIP golden tests: no GPU adapter available ({e})");
            None
        }
    }
}

/// Sets fill on a mobject's whole family.
fn fill(
    scene: &mut SceneState,
    id: manim_core::mobject::AnyId,
    color: manim_color::Color,
    opacity: f32,
) {
    scene.set_style_family(id, |s| {
        s.set_fill(color, opacity);
    });
}

#[test]
fn empty_scene_is_pure_background() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let scene = SceneState::new();
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    // Every pixel should be the black background.
    assert_eq!(img.get_pixel(0, 0).0, [0, 0, 0, 255]);
    assert_golden("empty_scene", &img);
}

#[test]
fn filled_blue_circle() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let c = scene.add(Circle::new());
    fill(&mut scene, c.erase(), BLUE, 1.0);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    // Center pixel is inside the circle → blue-ish (blue dominates).
    let center = img.get_pixel(img.width() / 2, img.height() / 2).0;
    assert!(
        center[2] > center[0] && center[2] > 40,
        "center = {center:?}"
    );
    assert_golden("filled_blue_circle", &img);
}

#[test]
fn square_and_triangle_z_order() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    // Stroked white square shifted right.
    let sq = scene.add(Square::new().with_shift(2.0 * RIGHT));
    scene.set_style_family(sq.erase(), |s| {
        s.set_stroke(WHITE, 6.0, 1.0);
    });
    // Red filled triangle shifted left.
    let tri = scene.add(Triangle::new().with_shift(2.0 * LEFT));
    fill(&mut scene, tri.erase(), RED, 1.0);

    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("square_and_triangle", &img);
}

#[test]
fn overlapping_half_alpha_fills() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let a = scene.add(Circle::new().with_shift(0.6 * LEFT));
    fill(&mut scene, a.erase(), BLUE, 0.5);
    let b = scene.add(Circle::new().with_shift(0.6 * RIGHT));
    fill(&mut scene, b.erase(), RED, 0.5);

    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("overlapping_half_alpha", &img);
}

#[test]
fn core_geometry_scene() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let c = scene.add(Circle::new().with_shift(3.0 * LEFT));
    fill(&mut scene, c.erase(), BLUE, 1.0);
    let s = scene.add(Square::new());
    scene.set_style_family(s.erase(), |st| {
        st.set_fill(RED, 0.7).set_stroke(WHITE, 4.0, 1.0);
    });
    scene.add(Arrow::new(manim_math::ORIGIN, 3.0 * RIGHT));

    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("core_geometry_scene", &img);
}
