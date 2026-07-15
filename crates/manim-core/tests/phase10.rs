//! Integration tests for FE-110 (boolean ops) and the FE-97 finish
//! (`TransformMatchingShapes`, `AnimatedBoundary`), exercised through the scene /
//! display-list machinery.

use manim_core::animated_boundary::AnimatedBoundary;
use manim_core::animations::TransformMatchingShapes;
use manim_core::boolean::{Difference, Intersection, Union};
use manim_core::prelude::*;
use manim_math::{RIGHT, UP};

#[test]
fn union_spans_both_shapes() {
    // Two unit squares (side 2) whose centers are 1 apart overlap; the union
    // spans from -1 to 2 in x — width 3.
    let a = Square::new();
    let mut b = Square::new();
    b.shift(RIGHT);
    let u = Union::new(&a, &b);
    assert!((u.bounding_box().width() - 3.0).abs() < 1e-2);
    // The result is a single filled contour.
    assert_eq!(u.data().path.subpaths.len(), 1);
}

#[test]
fn intersection_is_overlap() {
    let a = Square::new(); // [-1,1]^2
    let mut b = Square::new();
    b.shift(RIGHT); // [0,2]x[-1,1]
    let i = Intersection::new(&a, &b);
    // Overlap strip x in [0,1] → width 1, height 2.
    assert!((i.bounding_box().width() - 1.0).abs() < 1e-2);
    assert!((i.bounding_box().height() - 2.0).abs() < 1e-2);
}

#[test]
fn difference_keeps_a_bbox() {
    // Diagonal offset gives a clean corner overlap (transversal crossings, not
    // the shared-edge degeneracy): a=[-1,1]^2, b=[0,2]^2 overlap only at [0,1]^2.
    let a = Square::new();
    let mut b = Square::new();
    b.shift(RIGHT + UP);
    let d = Difference::new(&a, &b);
    // a minus the top-right corner is an L-shape that still spans a's full box.
    assert!(
        d.bounding_box().min.x < -0.9,
        "min.x={}",
        d.bounding_box().min.x
    );
    assert!((d.bounding_box().width() - 2.0).abs() < 1e-2);
}

#[test]
fn boolean_result_renders() {
    let mut scene = SceneState::new();
    let a = Square::new();
    let mut b = Square::new();
    b.shift(RIGHT);
    let u = scene.add(Union::new(&a, &b));
    let dl = scene.display_list();
    assert!(dl.0.iter().any(|it| it.source == u.erase()));
}

#[test]
fn transform_matching_shapes_plays() {
    let mut scene = Scene::new(Config::low());
    let a = scene.add(VGroup::new());
    let ca = scene.add(Circle::new());
    scene.state_mut().add_child(a.erase(), ca.erase());

    let b = scene.add(VGroup::new());
    let mut circle = Circle::new();
    circle.shift(3.0 * RIGHT);
    let cb = scene.add(circle);
    scene.state_mut().add_child(b.erase(), cb.erase());

    // The two circles match by shape, so a Transform is scheduled between them.
    let m = TransformMatchingShapes::analyze(scene.state(), a.erase(), b.erase());
    assert_eq!(m.matched.len(), 1);

    scene
        .play(TransformMatchingShapes::new(a.erase(), b.erase()))
        .unwrap();
    assert!(scene.total_duration() > 0.0);
}

#[test]
fn animated_boundary_follows_and_cycles() {
    let mut scene = SceneState::new();
    let circle = scene.add(Circle::new()).erase();
    let boundary = AnimatedBoundary::of(&mut scene, circle);

    // Follows the target outline.
    let target_curves = scene.get_dyn(circle).data().path.n_curves();
    assert_eq!(scene.get(boundary).data().path.n_curves(), target_curves);

    scene.run_updaters(UpdaterCtx { dt: 0.0, time: 0.0 });
    let c0 = scene.get(boundary).data().style.stroke_color.unwrap();
    scene.run_updaters(UpdaterCtx { dt: 0.3, time: 0.3 });
    let c1 = scene.get(boundary).data().style.stroke_color.unwrap();
    assert!(c0 != c1, "boundary stroke color should cycle over time");
}
