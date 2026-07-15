//! Integration tests for the M1/M2 parity remainders (FE-83/84/87/90/91/92).

use manim_core::animations::{
    ApplyMatrix, CyclicReplace, GrowFromCenter, Homotopy, LaggedStartMap, MoveTo, Rotate,
    ShowIncreasingSubsets, Transform, UpdateFromFunc,
};
use manim_core::geometry::{CurvesAsSubmobjects, DashedVMobject, TracedPath, VDict, VMobject};
use manim_core::prelude::*;
use manim_math::TAU;

#[test]
fn arrange_spaces_children() {
    let mut scene = SceneState::new();
    let a = scene.add(Square::new()); // width 2
    let b = scene.add(Square::new());
    let c = scene.add(Square::new());
    let g = VGroup::of(&mut scene, [a.erase(), b.erase(), c.erase()]);
    scene.arrange(g.erase(), RIGHT, 0.5);
    // Adjacent centers 2 + 0.5 apart, group centered at origin.
    let xa = scene.get(a).get_center().x;
    let xb = scene.get(b).get_center().x;
    let xc = scene.get(c).get_center().x;
    assert!((xb - xa - 2.5).abs() < 1e-4);
    assert!((xc - xb - 2.5).abs() < 1e-4);
    assert!((xa + xb + xc).abs() < 1e-4); // symmetric about origin
}

#[test]
fn arrange_in_grid_centers_group() {
    let mut scene = SceneState::new();
    let ids: Vec<_> = (0..4).map(|_| scene.add(Square::new()).erase()).collect();
    let g = VGroup::of(&mut scene, ids);
    scene.arrange_in_grid(g.erase(), 2, 2, 0.5);
    assert!(scene.family_bounding_box(g.erase()).center().length() < 1e-4);
}

#[test]
fn set_points_and_become() {
    let mut sq = Square::new();
    sq.set_points_as_corners(&[Point::ZERO, RIGHT, RIGHT + UP]);
    assert_eq!(sq.data().path.n_curves(), 2);

    let circle = Circle::new();
    sq.r#become(&circle);
    assert!((sq.bounding_box().width() - 2.0).abs() < 1e-4);
    assert_eq!(
        sq.data().style.stroke_color,
        circle.data().style.stroke_color
    );
}

#[test]
fn dashed_vmobject_dash_count() {
    let circle = Circle::new();
    let dashed = DashedVMobject::new(&circle).num_dashes(20);
    assert_eq!(dashed.data().path.subpaths.len(), 20);
}

#[test]
fn curves_as_submobjects_splits() {
    let mut scene = SceneState::new();
    let sq = scene.add(Square::new()); // 4 edges
    let group = CurvesAsSubmobjects::of(&mut scene, sq.erase());
    assert_eq!(scene.get_dyn(group.erase()).data().children.len(), 4);
}

#[test]
fn vdict_keyed_access() {
    let mut scene = SceneState::new();
    let a = scene.add(Circle::new());
    let dict = VDict::of(&mut scene, [("a".to_string(), a.erase())]);
    assert_eq!(scene.get(dict).get("a"), Some(a.erase()));
    assert_eq!(scene.get(dict).get("missing"), None);
}

#[test]
fn transform_with_path_arc_reaches_target() {
    let mut scene = Scene::new(Config::default());
    let sq = scene.add(Square::new());
    let circle = scene.add(Circle::new().with_shift(3.0 * RIGHT));
    scene
        .play(Transform::new(sq, circle).path_arc(TAU / 4.0))
        .unwrap();
    // Despite the arced path, it lands on the target center.
    assert!((scene[sq].get_center() - 3.0 * RIGHT).length() < 0.2);
}

#[test]
fn transform_path_arc_bows_off_the_straight_line() {
    // At mid-animation, an arc path leaves the straight line between endpoints.
    let mut scene = Scene::new(Config::low());
    let sq = scene.add(Square::new());
    let circle = scene.add(Circle::new().with_shift(4.0 * RIGHT));
    scene
        .play(Transform::new(sq, circle).path_arc(TAU / 4.0))
        .unwrap();
    let frames: Vec<_> = scene.frames().collect();
    let mid = &frames[frames.len() / 2].1;
    let (min, max) = mid.0[0].path.bounding_box().unwrap();
    let cy = (min.y + max.y) / 2.0;
    // The straight path would keep y ≈ 0; the arc bows it away.
    assert!(cy.abs() > 0.1, "mid y was {cy}");
}

#[test]
fn grow_from_center_starts_tiny() {
    let mut scene = Scene::new(Config::low());
    let sq = scene.add(Square::new());
    scene.play(GrowFromCenter::new(sq)).unwrap();
    let frames: Vec<_> = scene.frames().collect();
    // First frame near-zero size, last frame full size.
    let width = |dl: &DisplayList| {
        dl.0.first()
            .and_then(|it| it.path.bounding_box())
            .map(|(mn, mx)| mx.x - mn.x)
            .unwrap_or(0.0)
    };
    assert!(width(&frames[0].1) < 0.5);
    assert!((width(&frames.last().unwrap().1) - 2.0).abs() < 1e-2);
}

#[test]
fn homotopy_drives_points() {
    let mut scene = Scene::new(Config::default());
    let sq = scene.add(Square::new());
    scene.play(Homotopy::new(sq, |p, t| p + t * UP)).unwrap();
    assert!((scene[sq].get_center() - UP).length() < 1e-4);
}

#[test]
fn apply_matrix_scales() {
    let mut scene = Scene::new(Config::default());
    let sq = scene.add(Square::new());
    let m = glam::Mat3::from_cols_array(&[2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 1.0]);
    scene.play(ApplyMatrix::new(sq, m)).unwrap();
    assert!((scene[sq].bounding_box().width() - 4.0).abs() < 1e-3);
    assert!((scene[sq].bounding_box().height() - 6.0).abs() < 1e-3);
}

#[test]
fn cyclic_replace_permutes_positions() {
    let mut scene = Scene::new(Config::default());
    let a = scene.add(Circle::new());
    let b = scene.add(Square::new().with_shift(2.0 * RIGHT));
    let c = scene.add(Circle::new().with_shift(4.0 * RIGHT));
    scene
        .play(CyclicReplace::new([a.erase(), b.erase(), c.erase()]))
        .unwrap();
    assert!((scene[a].get_center() - 2.0 * RIGHT).length() < 1e-3);
    assert!((scene[b].get_center() - 4.0 * RIGHT).length() < 1e-3);
    assert!((scene[c].get_center()).length() < 1e-3);
}

#[test]
fn lagged_start_map_timing() {
    let mut scene = Scene::new(Config::default());
    let ids: Vec<_> = (0..4)
        .map(|_| scene.add(Circle::new().with_fill(BLUE, 1.0)).erase())
        .collect();
    scene
        .play(LaggedStartMap::new(ids, |id| {
            Box::new(MoveTo::new(id, UP)) as Box<dyn manim_core::Animation>
        }))
        .unwrap();
    // 4 one-second anims at lag 0.05 → 1 + 3·0.05 = 1.15 s.
    assert!((scene.total_duration() - 1.15).abs() < 1e-4);
}

#[test]
fn always_redraw_follows_anchor() {
    let mut scene = Scene::new(Config::low());
    let anchor = scene.add(Dot::new());
    let follower = scene.state_mut().always_redraw(move |s| {
        let base = s
            .try_get(anchor)
            .map(|d| d.get_center())
            .unwrap_or(Point::ZERO);
        Dot::at(base + RIGHT)
    });
    scene.play(MoveTo::new(anchor, 4.0 * RIGHT)).unwrap();

    let frames: Vec<_> = scene.frames().collect();
    let (_, dln) = frames.last().unwrap();
    let follower_item = dln
        .iter()
        .find(|it| it.source == follower.erase())
        .expect("follower present");
    let (min, max) = follower_item.path.bounding_box().unwrap();
    let cx = (min.x + max.x) / 2.0;
    // Anchor ends at x=4; follower is one unit right → x≈5.
    assert!((cx - 5.0).abs() < 0.3, "follower x was {cx}");
}

#[test]
fn traced_path_accumulates_points() {
    let mut scene = Scene::new(Config::low());
    let dot = scene.add(Dot::new());
    let trace = TracedPath::of(scene.state_mut(), move |s| {
        s.try_get(dot)
            .map(|d| d.get_center())
            .unwrap_or(Point::ZERO)
    });
    scene.play(MoveTo::new(dot, 4.0 * RIGHT)).unwrap();
    // Drive frames so the updater appends points; the last frame holds the full
    // traced curve.
    let frames: Vec<_> = scene.frames().collect();
    assert!(scene.state().get(trace).point_count() >= 2);
    let (_, dln) = frames.last().unwrap();
    let trace_item = dln
        .iter()
        .find(|it| it.source == trace.erase())
        .expect("traced path present");
    let (min, max) = trace_item.path.bounding_box().unwrap();
    assert!((max.x - min.x) > 3.0);
}

#[test]
fn show_increasing_subsets_reveals_children() {
    let mut scene = Scene::new(Config::low());
    let a = scene.add(Circle::new());
    let b = scene.add(Square::new());
    let g = VGroup::of(scene.state_mut(), [a.erase(), b.erase()]);
    scene.play(ShowIncreasingSubsets::new(g)).unwrap();
    let frames: Vec<_> = scene.frames().collect();
    // First frame: nothing (or one) shown; last frame both shown.
    assert!(frames[0].1.len() <= 1);
    assert_eq!(frames.last().unwrap().1.len(), 2);
}

#[test]
fn vmobject_and_update_from_func() {
    let mut scene = Scene::new(Config::low());
    let m = scene.add(VMobject::from_path(manim_math::path::Path::from_corners(
        &[Point::ZERO, RIGHT],
        false,
    )));
    scene
        .play(UpdateFromFunc::new(m, |s, id, alpha| {
            s.get_dyn_mut(id)
                .data_mut()
                .path
                .apply(|_| 2.0 * RIGHT * alpha);
        }))
        .unwrap();
    assert!((scene[m].get_center() - 2.0 * RIGHT).length() < 1e-3);
    let _ = Rotate::new(m, TAU); // smoke: Rotate constructs
}
