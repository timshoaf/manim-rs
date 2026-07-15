//! Integration tests for FE-96 (moving camera + sections) and FE-97
//! (indication animations).

use manim_core::animations::{Circumscribe, Flash, FocusOn, Indicate, ShowPassingFlash};
use manim_core::prelude::*;

#[test]
fn camera_animation_moves_frame() {
    let mut scene = Scene::new(Config::default());
    let _ = scene.add(Circle::new());
    let w0 = scene.camera().frame_width;
    scene
        .play(scene.camera_frame().animate().scale(0.5).move_to(2.0 * UP))
        .unwrap();
    assert!((scene.camera().frame_width - w0 * 0.5).abs() < 1e-4);
    assert!((scene.camera().frame_center - 2.0 * UP).length() < 1e-4);
}

#[test]
fn camera_motion_shows_in_frames() {
    let mut scene = Scene::new(Config::low());
    let _ = scene.add(Circle::new());
    scene
        .play(scene.camera_frame().animate().move_to(4.0 * RIGHT))
        .unwrap();
    let frames: Vec<_> = scene.frames_with_camera().collect();
    // Camera starts at origin and pans to x = 4.
    assert!(frames[0].camera.center.x.abs() < 1e-4);
    assert!((frames.last().unwrap().camera.center.x - 4.0).abs() < 1e-3);
    // Monotonic pan.
    let mut prev = -1.0;
    for f in &frames {
        assert!(f.camera.center.x >= prev - 1e-4);
        prev = f.camera.center.x;
    }
}

#[test]
fn camera_snapshot_is_captured_per_segment() {
    // The camera lives in SceneState, so seeking to an earlier segment restores
    // the earlier camera.
    let mut scene = Scene::new(Config::low());
    let _ = scene.add(Circle::new());
    scene.wait(1.0); // camera at origin here
    scene
        .play(scene.camera_frame().animate().move_to(4.0 * RIGHT))
        .unwrap();
    let frames: Vec<_> = scene.frames_with_camera().collect();
    // During the wait (t < 1), the camera is still at the origin.
    let early = frames.iter().find(|f| f.t < 0.9).unwrap();
    assert!(early.camera.center.x.abs() < 1e-4);
}

#[test]
fn sections_record_boundaries() {
    let mut scene = Scene::new(Config::default());
    scene.next_section("intro");
    scene.wait(1.0);
    scene.next_section("body");
    scene.wait(2.0);
    let sections = scene.sections();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].name, "intro");
    assert!((sections[0].start).abs() < 1e-6);
    assert_eq!(sections[1].name, "body");
    assert!((sections[1].start - 1.0).abs() < 1e-6);
}

#[test]
fn flash_leaves_no_residue() {
    let mut scene = Scene::new(Config::low());
    let c = scene.add(Circle::new());
    scene.play(Flash::new(ORIGIN)).unwrap();
    scene.wait(0.5);

    let frames: Vec<_> = scene.frames().collect();
    // A mid-flash frame shows the circle plus radiating lines.
    let flash_frame = &frames[frames.len() / 4].1;
    assert!(
        flash_frame.len() > 1,
        "flash lines should be visible mid-play"
    );

    // The wait segment (last frames) shows only the circle: no residue.
    let (_, last) = frames.last().unwrap();
    assert_eq!(last.len(), 1);
    assert_eq!(last.0[0].source, c.erase());

    // The live state also has no leftover temp mobjects.
    assert_eq!(scene.display_list().len(), 1);
}

#[test]
fn focus_on_and_circumscribe_leave_no_residue() {
    let mut scene = Scene::new(Config::low());
    let sq = scene.add(Square::new());
    scene.play(FocusOn::new(ORIGIN)).unwrap();
    scene.play(Circumscribe::new(sq)).unwrap();
    scene.wait(0.5);
    let frames: Vec<_> = scene.frames().collect();
    let (_, last) = frames.last().unwrap();
    assert_eq!(last.len(), 1); // only the square remains
    assert_eq!(scene.display_list().len(), 1);
}

#[test]
fn indicate_returns_to_normal() {
    let mut scene = Scene::new(Config::low());
    let sq = scene.add(Square::new());
    scene.play(Indicate::new(sq)).unwrap();
    // Back to its original size and (white) stroke after the there-and-back.
    assert!((scene[sq].bounding_box().width() - 2.0).abs() < 1e-3);
    assert_eq!(scene[sq].data().style.stroke_color, Some(WHITE));

    // Mid-animation it is enlarged.
    let frames: Vec<_> = scene.frames().collect();
    let mid = &frames[frames.len() / 2].1;
    let (min, max) = mid.0[0].path.bounding_box().unwrap();
    assert!((max.x - min.x) > 2.1, "should be enlarged mid-indicate");
}

#[test]
fn show_passing_flash_restores_outline() {
    let mut scene = Scene::new(Config::low());
    let sq = scene.add(Square::new());
    scene.play(ShowPassingFlash::new(sq)).unwrap();
    // Full perimeter restored at the end.
    let len: f32 = scene[sq]
        .data()
        .path
        .subpaths
        .iter()
        .map(|s| s.arc_length())
        .sum();
    assert!((len - 8.0).abs() < 1e-2);

    // Mid-animation only a window of the outline is drawn.
    let frames: Vec<_> = scene.frames().collect();
    let mid = &frames[frames.len() / 2].1;
    let mid_len: f32 = mid
        .0
        .iter()
        .flat_map(|it| it.path.subpaths.iter())
        .map(|s| s.arc_length())
        .sum();
    assert!(mid_len < 8.0);
}

#[test]
fn frames_delegates_to_frames_with_camera() {
    // The two frame iterators agree on display lists.
    let mut scene = Scene::new(Config::low());
    let c = scene.add(Circle::new());
    scene.play(Indicate::new(c)).unwrap();
    let a: Vec<_> = scene.frames().map(|(t, dl)| (t, dl.len())).collect();
    let b: Vec<_> = scene
        .frames_with_camera()
        .map(|f| (f.t, f.display_list.len()))
        .collect();
    assert_eq!(a, b);
}
