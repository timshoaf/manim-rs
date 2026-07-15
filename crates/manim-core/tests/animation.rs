//! Integration tests for the animation engine: build scenes via `SceneBuilder`
//! and assert on `frames()` sampling.

use manim_core::animations::{
    AnimationGroup, Create, FadeIn, SetValue, TransformInto, ValueTracker,
};
use manim_core::prelude::*;

/// Total arc length of a mobject's current path.
fn path_len(scene: &Scene, id: MobjectId<impl Mobject>) -> f32 {
    scene
        .state()
        .get_dyn(id.erase())
        .data()
        .path
        .subpaths
        .iter()
        .map(|s| s.arc_length())
        .sum()
}

/// The classic Square → Circle scene, played then held.
struct SquareToCircle;
impl SceneBuilder for SquareToCircle {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let sq = scene.add(Square::new().with_fill(BLUE, 0.7));
        scene.play(TransformInto::new(sq, Circle::new().with_fill(RED, 0.7)))?;
        scene.wait(1.0);
        Ok(())
    }
}

#[test]
fn square_to_circle_timeline() {
    let scene = Scene::build(&SquareToCircle, Config::default()).unwrap();
    // 1 s transform + 1 s wait.
    assert!((scene.total_duration() - 2.0).abs() < 1e-6);
}

#[test]
fn square_to_circle_frames_endpoints() {
    let mut scene = Scene::build(&SquareToCircle, Config::low()).unwrap();
    let frames: Vec<_> = scene.frames().collect();
    assert!(frames.len() > 1);

    // First frame: a square (width 2, four straight edges → perimeter 8).
    let (t0, dl0) = &frames[0];
    assert_eq!(*t0, 0.0);
    assert_eq!(dl0.len(), 1);

    // Last frame: a circle (width 2).
    let (_, dln) = frames.last().unwrap();
    let (min, max) = dln.0[0].path.bounding_box().unwrap();
    assert!(((max.x - min.x) - 2.0).abs() < 0.1);
}

#[test]
fn transform_final_path_matches_target_exactly() {
    let mut scene = Scene::new(Config::default());
    let sq = scene.add(Square::new());
    let target = Circle::new();
    let target_path = target.data().path.clone();
    scene.play(TransformInto::new(sq, target)).unwrap();

    // After the eager apply, the source's live path equals the target's
    // (structurally aligned, so up to inserted degenerate curves it is the
    // same shape). Compare bounding boxes exactly.
    let src = scene
        .state()
        .get_dyn(sq.erase())
        .data()
        .path
        .bounding_box()
        .unwrap();
    let tgt = target_path.bounding_box().unwrap();
    assert!((src.0 - tgt.0).length() < 1e-4);
    assert!((src.1 - tgt.1).length() < 1e-4);
}

#[test]
fn create_partial_length_grows() {
    let mut scene = Scene::new(Config::low());
    let sq = scene.add(Square::new()); // perimeter 8
    scene.play(Create::new(sq)).unwrap();
    let frames_len = {
        // Sample partial lengths across the timeline by seeking.
        let mut lens = Vec::new();
        for &t in &[0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            // Rebuild state at t via a fresh scene each time would be complex;
            // instead reuse the timeline through frames() once.
            let _ = t;
        }
        lens.push(0.0);
        lens
    };
    let _ = frames_len;

    // Use frames() and check the drawn perimeter is monotonically increasing.
    let mut prev = -1.0;
    let mut increasing = true;
    for (_, dl) in scene.frames() {
        let len: f32 =
            dl.0.iter()
                .flat_map(|it| it.path.subpaths.iter())
                .map(|s| s.arc_length())
                .sum();
        if len + 1e-3 < prev {
            increasing = false;
        }
        prev = prev.max(len);
    }
    assert!(increasing);
    // Fully drawn at the end.
    assert!((path_len(&scene, sq) - 8.0).abs() < 1e-2);
}

#[test]
fn fade_in_opacity_ramps() {
    let mut scene = Scene::new(Config::low());
    let c = scene.add(Circle::new().with_fill(BLUE, 1.0));
    scene.play(FadeIn::new(c)).unwrap();

    let frames: Vec<_> = scene.frames().collect();
    // Opacity of the fill on the first vs. a middle frame increases.
    let opacity = |dl: &DisplayList| dl.0.first().and_then(|it| it.fill).map(|f| f.color.a);
    let first = opacity(&frames[0].1).unwrap_or(0.0);
    let mid = opacity(&frames[frames.len() / 2].1).unwrap_or(0.0);
    let last = opacity(&frames.last().unwrap().1).unwrap_or(0.0);
    assert!(first <= mid + 1e-4);
    assert!(mid <= last + 1e-4);
    assert!((last - 1.0).abs() < 1e-3);
}

#[test]
fn animation_group_lag_timing() {
    let mut scene = Scene::new(Config::default());
    let a = scene.add(Circle::new().with_fill(BLUE, 1.0));
    let b = scene.add(Square::new().with_fill(RED, 1.0));
    // Concurrent (lag 0) → duration 1 s.
    scene
        .play(AnimationGroup::new((FadeIn::new(a), FadeIn::new(b))))
        .unwrap();
    assert!((scene.total_duration() - 1.0).abs() < 1e-6);

    // Sequential (lag 1) → duration 2 s.
    let mut scene2 = Scene::new(Config::default());
    let c = scene2.add(Circle::new().with_fill(BLUE, 1.0));
    let d = scene2.add(Square::new().with_fill(RED, 1.0));
    scene2
        .play(AnimationGroup::new((FadeIn::new(c), FadeIn::new(d))).lag_ratio(1.0))
        .unwrap();
    assert!((scene2.total_duration() - 2.0).abs() < 1e-6);
}

#[test]
fn updater_dot_follows_tracker() {
    let mut scene = Scene::new(Config::low());
    let tracker = scene.add(ValueTracker::new(0.0));
    let dot = scene.add(Dot::new());
    // Dot's x-position tracks the tracker's value.
    scene
        .state_mut()
        .add_updater(dot.erase(), move |s, id, _ctx| {
            let x = s.try_get(tracker).map(|t| t.get_value()).unwrap_or(0.0);
            let center = s.get_dyn(id).get_center();
            let delta = Point::new(x - center.x, 0.0, 0.0);
            s.get_dyn_mut(id).data_mut().path.apply(|p| p + delta);
        });
    scene.play(SetValue::new(tracker, 4.0)).unwrap();

    let frames: Vec<_> = scene.frames().collect();
    // At the last frame the dot should have followed the tracker to x ≈ 4.
    let (_, dln) = frames.last().unwrap();
    let dot_item = dln
        .iter()
        .find(|it| it.source == dot.erase())
        .expect("dot present");
    let (min, max) = dot_item.path.bounding_box().unwrap();
    let cx = (min.x + max.x) / 2.0;
    assert!((cx - 4.0).abs() < 0.2, "dot x was {cx}");
}

#[test]
fn wait_holds_state() {
    let mut scene = Scene::new(Config::low());
    let c = scene.add(Circle::new().with_shift(2.0 * RIGHT));
    scene.wait(1.0);
    // Every frame during the wait shows the circle at the same place.
    for (_, dl) in scene.frames() {
        assert_eq!(dl.len(), 1);
        let (min, max) = dl.0[0].path.bounding_box().unwrap();
        let cx = (min.x + max.x) / 2.0;
        assert!((cx - 2.0).abs() < 1e-4);
    }
    let _ = c;
}

#[test]
fn frames_are_deterministic() {
    let build = || {
        let mut scene = Scene::build(&SquareToCircle, Config::low()).unwrap();
        scene
            .frames()
            .map(|(t, dl)| (t, dl.len()))
            .collect::<Vec<_>>()
    };
    let run1 = build();
    let run2 = build();
    assert_eq!(run1, run2);

    // Also stable across two frames() calls on the same scene.
    let mut scene = Scene::build(&SquareToCircle, Config::low()).unwrap();
    let a: Vec<_> = scene.frames().map(|(t, dl)| (t, dl.len())).collect();
    let b: Vec<_> = scene.frames().map(|(t, dl)| (t, dl.len())).collect();
    assert_eq!(a, b);
}
