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

#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use glam::Vec3;
use manim_color::{BLUE, GREEN, RED, WHITE, YELLOW};
use manim_core::animations::{Flash, Indicate};
use manim_core::camera::ThreeDParams;
use manim_core::config::Config;
use manim_core::display::{
    Colormap, ContourParams, DisplayList, DrawItem, FieldChannels, Material, MaterialKind,
    TextureData,
};
use manim_core::geometry::{Arrow, Circle, Line, Square, Triangle};
use manim_core::graphing::{Axes, NumberPlane};
use manim_core::image_mobject::ImageMobject;
use manim_core::mesh::Surface3D;
use manim_core::mobject::Buildable;
use manim_core::network::{Graph, GraphLayout};
use manim_core::scene::Scene;
use manim_core::scene_state::SceneState;
use manim_core::style::Gradient;
use manim_core::threed::{Cube, Sphere, Surface, ThreeDAxes, Torus};
use manim_core::vector_field::{ArrowVectorField, StreamLines};
use manim_math::path::Path;
use manim_math::{Point, DOWN, LEFT, ORIGIN, RIGHT, UP};
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
///
/// Set `REQUIRE_GPU=1` (CI does, with a software rasterizer) to turn a missing
/// adapter into a hard failure instead of a silent skip — so the golden job can
/// never pass by simply not running.
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
            if std::env::var("REQUIRE_GPU").is_ok_and(|v| v != "0" && !v.is_empty()) {
                panic!(
                    "REQUIRE_GPU is set but no GPU adapter is available ({e}); \
                     install a software rasterizer (e.g. mesa lavapipe) or unset REQUIRE_GPU"
                );
            }
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

/// Camera-follow: a fixed square with the camera zoomed to 0.5 renders the
/// square twice as large (closes FE-96's "camera-follow rendered correctly").
#[test]
fn camera_zoom_follows() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = Scene::new(test_config());
    let sq = scene.add(Square::new().with_fill(BLUE, 1.0));
    let _ = sq;
    // Zoom the camera to half-size (2× magnification) over the play.
    scene
        .play(scene.camera_frame().animate().scale(0.5))
        .unwrap();

    let frames: Vec<_> = scene.frames_with_camera().collect();
    // Last frame: camera fully at 0.5 zoom. render_frame follows it.
    let last = frames.last().expect("frames");
    assert!((last.camera.height - 4.0).abs() < 1e-3, "zoom not applied");
    let img = renderer.render_frame(last).unwrap();
    assert_golden("camera_zoom", &img);
}

/// FE-97: `Indicate` at its midpoint — the square is scaled up and tinted.
#[test]
fn indicate_midframe() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = Scene::new(test_config());
    let sq = scene.add(Square::new().with_fill(BLUE, 1.0));
    scene.play(Indicate::new(sq)).unwrap();

    let frames: Vec<_> = scene.frames_with_camera().collect();
    let mid = &frames[frames.len() / 2];
    let img = renderer.render_frame(mid).unwrap();
    assert_golden("indicate_mid", &img);
}

/// FE-97: `Flash` at its midpoint — yellow lines radiate from the origin over a
/// blue reference circle.
#[test]
fn flash_midframe() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = Scene::new(test_config());
    scene.add(Circle::new().with_fill(BLUE, 1.0));
    scene.play(Flash::new(ORIGIN)).unwrap();

    let frames: Vec<_> = scene.frames_with_camera().collect();
    let mid = &frames[frames.len() / 2];
    let img = renderer.render_frame(mid).unwrap();
    assert_golden("flash_mid", &img);
}

/// FE-83: a linear fill gradient (BLUE → RED, left to right) across a square.
#[test]
fn gradient_fill() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let sq = scene.add(Square::new());
    scene.set_style_family(sq.erase(), |s| {
        s.set_fill_gradient(Gradient::from_colors(&[BLUE, RED]));
    });
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    // Sample inside the square (~60px wide, centered): its left side is bluer,
    // its right side is redder.
    let (cx, cy) = (img.width() / 2, img.height() / 2);
    let left = img.get_pixel(cx - 20, cy).0;
    let right = img.get_pixel(cx + 20, cy).0;
    assert!(
        left[2] > right[2],
        "left should be bluer: {left:?} vs {right:?}"
    );
    assert!(
        right[0] > left[0],
        "right should be redder: {right:?} vs {left:?}"
    );
    assert_golden("gradient_fill", &img);
}

/// FE-83: a color gradient along a thick stroked line (`set_color_by_gradient`).
#[test]
fn gradient_stroke() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let l = scene.add(Line::new(4.0 * LEFT, 4.0 * RIGHT));
    scene.set_style_family(l.erase(), |s| {
        s.set_stroke(WHITE, 30.0, 1.0)
            .set_color_by_gradient(&[BLUE, RED]);
    });
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("gradient_stroke", &img);
}

/// FE-83: a background stroke (thick red) shows behind a translucent blue fill,
/// forming an outline — the text-outline use case.
#[test]
fn background_stroke() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let c = scene.add(Circle::new());
    scene.set_style_family(c.erase(), |s| {
        s.set_fill(BLUE, 0.5);
        s.set_stroke(WHITE, 2.0, 1.0);
        s.set_background_stroke(RED, 40.0, 1.0);
    });
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("background_stroke", &img);
}

/// FE-103: axes with a plotted sin curve.
#[test]
fn axes_with_graph() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let axes = Axes::new([-5.0, 5.0, 1.0], [-3.0, 3.0, 1.0]);
    let graph = axes.plot(|x| 2.0 * x.sin(), None);
    let a = scene.add(axes);
    scene.set_style_family(a.erase(), |s| {
        s.set_stroke(WHITE, 2.5, 1.0);
    });
    let g = scene.add(graph);
    scene.set_style_family(g.erase(), |s| {
        s.set_stroke(YELLOW, 4.0, 1.0);
    });
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("axes_with_graph", &img);
}

/// FE-103: a Cartesian number plane (faded grid).
#[test]
fn number_plane() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    scene.add(NumberPlane::new([-6.0, 6.0, 1.0], [-4.0, 4.0, 1.0]));
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("number_plane", &img);
}

/// FE-103: Riemann rectangles under x² with the axes and curve.
#[test]
fn riemann_rectangles() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let axes = Axes::with_lengths([0.0, 4.0, 1.0], [0.0, 16.0, 4.0], 8.0, 6.0);
    let graph = axes.plot(|x| x * x, None);
    let rects = axes.get_riemann_rectangles(&graph, 0.0, 4.0, 0.5, 0.6);
    // Center everything on screen (axes range is asymmetric).
    let r = scene.add(rects);
    scene.set_style_family(r.erase(), |s| {
        s.set_fill(BLUE, 0.6).set_stroke(WHITE, 1.5, 1.0);
    });
    let a = scene.add(axes);
    scene.set_style_family(a.erase(), |s| {
        s.set_stroke(WHITE, 2.5, 1.0);
    });
    let g = scene.add(graph);
    scene.set_style_family(g.erase(), |s| {
        s.set_stroke(GREEN, 4.0, 1.0);
    });
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("riemann_rectangles", &img);
}

/// FE-106: an arrow vector field of the rotational field f(x,y)=(-y,x),
/// colored by magnitude.
#[test]
fn arrow_vector_field() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let field = ArrowVectorField::new(|p: Point| Point::new(-p.y, p.x, 0.0))
        .with_x_range([-3.0, 3.0, 0.75])
        .with_y_range([-2.0, 2.0, 0.75]);
    field.add_to(&mut scene);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("arrow_vector_field", &img);
}

/// FE-106: stream lines of the rotational field (concentric orbits).
#[test]
fn stream_lines() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let lines = StreamLines::new(|p: Point| Point::new(-p.y, p.x, 0.0))
        .with_x_range([-3.0, 3.0, 0.6])
        .with_y_range([-2.5, 2.5, 0.6])
        .with_integration(0.05, 130);
    lines.add_to(&mut scene);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("stream_lines", &img);
}

/// FE-105: a 6-cycle with chords, circular layout.
#[test]
fn graph_circular() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 4),
        (4, 5),
        (5, 0), // the 6-cycle
        (0, 3),
        (1, 4),
        (2, 5), // long chords
    ];
    let graph = Graph::new(6, &edges, GraphLayout::Circular { radius: 2.5 });
    scene.add(graph);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("graph_circular", &img);
}

/// FE-107: an axis-aligned cube wireframe (12 `Line`s with 3-D endpoints) under a
/// perspective camera orbited to phi=75°, theta=30°. Exercises the 3-D
/// view-projection and the camera-space depth sort. The 4 world-z-parallel
/// vertical edges render because strokes tessellate in a path-fitted plane (see
/// [`manim_render::tessellate`]) — no tilt hack needed.
#[test]
fn cube_wireframe_3d() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let s = 1.5_f32;
    let v = |x: f32, y: f32, z: f32| Point::new(x * s, y * s, z * s);
    let corners = [
        v(-1.0, -1.0, -1.0),
        v(1.0, -1.0, -1.0),
        v(1.0, 1.0, -1.0),
        v(-1.0, 1.0, -1.0),
        v(-1.0, -1.0, 1.0),
        v(1.0, -1.0, 1.0),
        v(1.0, 1.0, 1.0),
        v(-1.0, 1.0, 1.0),
    ];
    // 4 bottom edges, 4 top edges, 4 verticals.
    let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    for (a, b) in edges {
        let l = scene.add(Line::new(corners[a], corners[b]));
        scene.set_style_family(l.erase(), |st| {
            st.set_stroke(WHITE, 3.0, 1.0);
        });
    }
    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    // Something rendered (not a blank frame): at least one bright edge pixel.
    assert!(
        img.pixels().any(|p| p.0[0] > 100 && p.0[1] > 100),
        "cube wireframe rendered no visible edges"
    );
    assert_golden("cube_wireframe_3d", &img);
}

/// Orbits `renderer`'s camera to manim's classic 3-D angles (phi=75°, theta=30°).
fn orbit_camera(renderer: &mut OffscreenRenderer) {
    let deg = std::f32::consts::PI / 180.0;
    renderer.camera_mut().three_d = Some(ThreeDParams {
        phi: 75.0 * deg,
        theta: 30.0 * deg,
        ..ThreeDParams::default()
    });
}

/// FE-108: a checkerboard `Sphere` (BLUE_D/BLUE_E faces) under the 3-D camera.
/// Near faces occlude far ones via the camera-space depth sort.
#[test]
fn sphere_3d() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    Sphere::new(2.0).add_to(&mut scene);
    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("sphere_3d", &img);
}

/// FE-108: a solid `Cube` rotated in 3-D — depth-sort sanity: the three faces
/// nearest the camera occlude the far three.
#[test]
fn cube_solid_3d() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let cube = Cube::new(2.5).add_to(&mut scene);
    // A genuine 3-D rotation (about X then Y) so it reads as a rotated solid.
    scene.rotate_about(cube.erase(), 0.5, ORIGIN, RIGHT);
    scene.rotate_about(cube.erase(), 0.4, ORIGIN, UP);
    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("cube_solid_3d", &img);
}

/// FE-108: `ThreeDAxes` (including the world-z-parallel z-axis, which renders via
/// the plane-fitted stroke path) with a parametric saddle `Surface`.
#[test]
fn axes_surface_3d() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    ThreeDAxes::with_ranges([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0])
        .add_to(&mut scene);
    // A hyperbolic-paraboloid (saddle) surface z = 0.35·(u² − v²).
    Surface::new(
        |u, v| Point::new(u, v, 0.35 * (u * u - v * v)),
        [-2.0, 2.0],
        [-2.0, 2.0],
    )
    .with_resolution(12, 12)
    .add_to(&mut scene);
    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("axes_surface_3d", &img);
}

/// FE-108: a checkerboard `Torus` under the 3-D camera (front ring occludes back).
#[test]
fn torus_3d() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    Torus::new(2.0, 0.7).add_to(&mut scene);
    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("torus_3d", &img);
}

/// FE-120: a `ZoomedScene` inset — a cluster of small shapes near the origin,
/// magnified ~4× into a bordered top-right inset via `add_zoom_window`.
#[test]
fn zoomed_inset() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = Scene::new(test_config());
    // A big reference ring so the full frame reads, plus a tiny cluster at the
    // origin that is only legible through the magnifier.
    scene.add(Circle::new().with_scale(2.5).with_stroke(WHITE, 3.0, 1.0));
    scene.add(
        Circle::new()
            .with_scale(0.28)
            .with_fill(BLUE, 1.0)
            .with_shift(0.35 * LEFT),
    );
    scene.add(
        Square::new()
            .with_scale(0.22)
            .with_fill(RED, 1.0)
            .with_shift(0.35 * RIGHT),
    );
    scene.add(
        Triangle::new()
            .with_scale(0.22)
            .with_fill(GREEN, 1.0)
            .with_shift(0.35 * UP),
    );
    // ~4× magnification of a 1.2-unit region into a top-right inset.
    scene.add_zoom_window(ORIGIN, 1.2, [0.60, 0.05, 0.35, 0.35]);

    let frame = manim_core::scene::Frame {
        t: 0.0,
        display_list: scene.display_list(),
        camera: manim_core::camera::CameraFrame::from(scene.camera()),
    };
    assert!(frame.camera.zoom_window.is_some());
    let img = renderer.render_frame(&frame).unwrap();
    // The inset's top-right corner region should contain a bright border pixel.
    let (w, h) = (img.width(), img.height());
    let inset_x = (0.60 * w as f32) as u32;
    let inset_y = (0.05 * h as f32) as u32;
    assert!(
        (inset_x..w).any(|x| (inset_y..(inset_y + 20).min(h)).any(|y| {
            let p = img.get_pixel(x, y).0;
            p[0] > 150 && p[1] > 150 && p[2] > 150
        })),
        "zoom inset border not found"
    );
    assert_golden("zoomed_inset", &img);
}

/// FE-101: an embedded raster image drawn between two vector shapes (z-order).
#[test]
fn image_between_shapes() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    // An 8×8 green/white checkerboard.
    let mut px = Vec::with_capacity(8 * 8 * 4);
    for y in 0..8u32 {
        for x in 0..8u32 {
            if (x + y) % 2 == 0 {
                px.extend([0, 200, 80, 255]);
            } else {
                px.extend([255, 255, 255, 255]);
            }
        }
    }

    let mut scene = SceneState::new();
    // Behind (z = -1): a large red square, so its border frames the image.
    scene.add(
        Square::new()
            .with_fill(RED, 1.0)
            .with_scale(1.6)
            .with_z_index(-1),
    );
    // Middle (z = 0): the checkerboard image (default 2×2 scene units).
    scene.add(ImageMobject::from_rgba(8, 8, px).with_z_index(0));
    // Front (z = 1): a translucent blue circle overlapping a corner.
    scene.add(
        Circle::new()
            .with_fill(BLUE, 0.7)
            .with_shift(0.9 * RIGHT + 0.9 * DOWN)
            .with_z_index(1),
    );

    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("image_between_shapes", &img);
}

// ---------------------------------------------------------------------------
// FE-135 / FE-136: GPU material system (domain coloring, heatmaps, fill-by-value).
// ---------------------------------------------------------------------------

/// Samples a scalar closure `f(x, y)` over the scene rectangle centered at
/// `center` with size `(w, h)` into an `R32Float` grid — row 0 is the top edge.
fn scalar_grid(
    center: Point,
    w: f32,
    h: f32,
    res: u32,
    f: impl Fn(f32, f32) -> f32,
) -> TextureData {
    let mut data = Vec::with_capacity((res * res) as usize);
    for j in 0..res {
        let v = j as f32 / (res - 1) as f32;
        let y = center.y + h * 0.5 - v * h;
        for i in 0..res {
            let u = i as f32 / (res - 1) as f32;
            let x = center.x - w * 0.5 + u * w;
            data.push(f(x, y));
        }
    }
    TextureData {
        width: res,
        height: res,
        channels: FieldChannels::R,
        data,
        center,
        size: [w, h],
    }
}

/// Samples a complex closure `f(x, y) -> (re, im)` into an `Rg32Float` grid.
fn complex_grid(
    center: Point,
    w: f32,
    h: f32,
    res: u32,
    f: impl Fn(f32, f32) -> (f32, f32),
) -> TextureData {
    let mut data = Vec::with_capacity((res * res * 2) as usize);
    for j in 0..res {
        let v = j as f32 / (res - 1) as f32;
        let y = center.y + h * 0.5 - v * h;
        for i in 0..res {
            let u = i as f32 / (res - 1) as f32;
            let x = center.x - w * 0.5 + u * w;
            let (re, im) = f(x, y);
            data.push(re);
            data.push(im);
        }
    }
    TextureData {
        width: res,
        height: res,
        channels: FieldChannels::Rg,
        data,
        center,
        size: [w, h],
    }
}

/// Renders a single material quad over the rectangle centered at `center`, sized
/// `(w, h)`, on a black background.
fn render_material(
    renderer: &mut OffscreenRenderer,
    center: Point,
    w: f32,
    h: f32,
    material: Material,
) -> image::RgbaImage {
    // A throwaway mobject just supplies a valid (arena, source) id.
    let mut scene = SceneState::new();
    let src = scene.add(Square::new()).erase();
    let arena = scene.display_list().arena();
    let (hw, hh) = (w * 0.5, h * 0.5);
    let tl = Point::new(center.x - hw, center.y + hh, 0.0);
    let tr = Point::new(center.x + hw, center.y + hh, 0.0);
    let br = Point::new(center.x + hw, center.y - hh, 0.0);
    let bl = Point::new(center.x - hw, center.y - hh, 0.0);
    // 3 curves (TL→TR→BR→BL): `quad_corners` reads the 4 corners for UVs
    // (0,0),(1,0),(1,1),(0,1).
    let path = Path::from_corners(&[tl, tr, br, bl], false);
    let item = DrawItem {
        path,
        fill: None,
        stroke: None,
        background_stroke: None,
        image: None,
        material: Some(material),
        fixed_in_frame: false,
        z_test: false,
        z_index: 0,
        source: src,
        generation: 1,
    };
    let dl = DisplayList::with_meshes(vec![item], vec![]).in_arena(arena);
    renderer.render_display_list(&dl).unwrap()
}

/// FE-135: complex domain coloring of `(z² − 1)/(z² + 1)` — phase→hue with
/// log-modulus rings. Shows two zeros (z = ±1) and two poles (z = ±i).
#[test]
fn material_domain_coloring() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let center = ORIGIN;
    let (w, h) = (4.4, 4.4);
    let cf = |x: f32, y: f32| -> (f32, f32) {
        let z2 = (x * x - y * y, 2.0 * x * y);
        let num = (z2.0 - 1.0, z2.1);
        let den = (z2.0 + 1.0, z2.1);
        let d = (den.0 * den.0 + den.1 * den.1).max(1e-6);
        (
            (num.0 * den.0 + num.1 * den.1) / d,
            (num.1 * den.0 - num.0 * den.1) / d,
        )
    };
    let tex = Arc::new(complex_grid(center, w, h, 320, cf));
    let material = Material {
        kind: MaterialKind::PhaseHue {
            modulus_contours: true,
        },
        texture: tex,
        value_range: [0.0, 1.0],
        opacity: 1.0,
    };
    let img = render_material(&mut renderer, center, w, h, material);
    assert_golden("material_domain_coloring", &img);
}

/// FE-135: a viridis heatmap of a Gaussian bump.
#[test]
fn material_heatmap_gaussian() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let center = ORIGIN;
    let (w, h) = (6.0, 6.0);
    let tex = Arc::new(scalar_grid(center, w, h, 256, |x, y| {
        (-(x * x + y * y) * 0.5).exp()
    }));
    let material = Material {
        kind: MaterialKind::Heatmap {
            colormap: Colormap::Viridis,
        },
        texture: tex,
        value_range: [0.0, 1.0],
        opacity: 1.0,
    };
    let img = render_material(&mut renderer, center, w, h, material);
    assert_golden("material_heatmap_gaussian", &img);
}

/// FE-135: a coolwarm field of a saddle `x² − y²` with white iso-contour lines.
#[test]
fn material_field_contours() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let center = ORIGIN;
    let (w, h) = (5.0, 5.0);
    let tex = Arc::new(scalar_grid(center, w, h, 256, |x, y| x * x - y * y));
    let material = Material {
        kind: MaterialKind::FieldTexture {
            colormap: Colormap::Coolwarm,
            contours: Some(ContourParams {
                spacing: 1.0,
                width: 1.5,
                color: WHITE,
            }),
        },
        texture: tex,
        value_range: [-6.0, 6.0],
        opacity: 1.0,
    };
    let img = render_material(&mut renderer, center, w, h, material);
    assert_golden("material_field_contours", &img);
}

/// FE-136: a sphere colored by height (`z`) through the viridis colormap
/// (`set_fill_by_value`), clearing the M6 deferral.
#[test]
fn surface_fill_by_value() {
    use std::f64::consts::{PI, TAU};
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let sphere = Surface3D::new(
        |phi, theta| {
            Vec3::new(
                (phi.sin() * theta.cos()) as f32,
                (phi.sin() * theta.sin()) as f32,
                phi.cos() as f32,
            )
        },
        (0.0, PI),
        (0.0, TAU),
    )
    .with_resolution(24, 48)
    .with_scale(2.4)
    .with_fill_by_value(|p| p.z, Colormap::Viridis, -1.0, 1.0);
    scene.add(sphere);

    let deg = std::f32::consts::PI / 180.0;
    renderer.camera_mut().three_d = Some(ThreeDParams {
        phi: 70.0 * deg,
        theta: 25.0 * deg,
        ..ThreeDParams::default()
    });
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("surface_fill_by_value", &img);
}
