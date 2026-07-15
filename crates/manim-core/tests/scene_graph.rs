//! Integration tests for the scene graph: build a small scene, group it, move
//! the group, and assert on the extracted display list.

use manim_core::prelude::*;

/// Builds a circle + square in a VGroup, shifts the group, and checks the
/// display list: item count, z ordering, world-space positions, and that
/// removal produces stale handles.
#[test]
fn circle_and_square_in_group() {
    let mut scene = SceneState::new();

    // A blue-filled circle at the origin and a square 3 to the right.
    let circle = scene.add(Circle::new().with_fill(BLUE, 0.5));
    let square = scene.add(Square::new().with_shift(3.0 * RIGHT));

    // Group them and shift the whole family up by 2.
    let group = VGroup::of(&mut scene, [circle.erase(), square.erase()]);
    scene.shift(group.erase(), 2.0 * UP);

    // Children moved with the group.
    assert!((scene.get(circle).get_center() - 2.0 * UP).length() < 1e-5);
    assert!((scene.get(square).get_center() - (3.0 * RIGHT + 2.0 * UP)).length() < 1e-5);

    // The group's own path is empty, so it draws nothing itself: 2 items.
    let dl = scene.display_list();
    assert_eq!(dl.len(), 2);

    // World-space bounding boxes are where we shifted them.
    let circle_item = dl
        .iter()
        .find(|it| it.source == circle.erase())
        .expect("circle in display list");
    let (min, max) = circle_item.path.bounding_box().unwrap();
    let center = (min + max) * 0.5;
    assert!((center - 2.0 * UP).length() < 1e-4);

    // The circle carries a resolved fill (opacity folded into alpha).
    let fill = circle_item.fill.as_ref().expect("circle has fill");
    assert!((fill.color.a - 0.5).abs() < 1e-6);
}

/// z-index controls draw order in the display list regardless of insertion
/// order; ties keep insertion order.
#[test]
fn z_index_orders_display_list() {
    let mut scene = SceneState::new();
    let back = scene.add(Square::new().with_z_index(-5));
    let front = scene.add(Circle::new().with_z_index(10));
    let mid = scene.add(Circle::new().with_shift(RIGHT)); // z = 0

    let dl = scene.display_list();
    assert_eq!(dl.len(), 3);
    // Sorted ascending by z: back (−5), mid (0), front (10).
    assert_eq!(dl.0[0].source, back.erase());
    assert_eq!(dl.0[1].source, mid.erase());
    assert_eq!(dl.0[2].source, front.erase());
}

/// Removing a mobject makes its handle stale (`try_get` → None) and drops it
/// from the display list.
#[test]
fn removal_yields_stale_handles() {
    let mut scene = SceneState::new();
    let a = scene.add(Circle::new());
    let b = scene.add(Square::new());
    assert_eq!(scene.display_list().len(), 2);

    scene.remove(a.erase());
    assert!(scene.try_get(a).is_none());
    assert!(!scene.contains(a.erase()));
    assert!(scene.try_get(b).is_some());
    assert_eq!(scene.display_list().len(), 1);
}

/// A default circle's fill is unset, so with only a stroke it still draws; a
/// fully-transparent mobject is skipped.
#[test]
fn transparent_mobjects_are_skipped() {
    let mut scene = SceneState::new();
    // Stroke-only circle draws.
    scene.add(Circle::new());
    // A circle with no stroke and no fill draws nothing.
    let mut invisible = Circle::new();
    invisible.set_stroke(WHITE, 0.0, 0.0).set_fill(WHITE, 0.0);
    scene.add(invisible);

    assert_eq!(scene.display_list().len(), 1);
}

/// Group-level styling propagates to the whole family.
#[test]
fn group_style_propagates() {
    let mut scene = SceneState::new();
    let a = scene.add(Circle::new());
    let b = scene.add(Square::new());
    let group = VGroup::of(&mut scene, [a.erase(), b.erase()]);

    scene.set_style_family(group.erase(), |s| {
        s.set_fill(RED, 1.0);
    });

    assert_eq!(scene.get(a).data().style.fill_color, Some(RED));
    assert_eq!(scene.get(b).data().style.fill_color, Some(RED));
}

/// Cloning the scene is a deep snapshot (the animation phase relies on this).
#[test]
fn scene_clone_is_independent_snapshot() {
    let mut scene = SceneState::new();
    let c = scene.add(Circle::new());
    let snapshot = scene.clone();

    scene.get_mut(c).shift(5.0 * RIGHT);

    assert!(scene.get(c).get_center().x > 4.9);
    assert!(snapshot.get(c).get_center().length() < 1e-6);
}
