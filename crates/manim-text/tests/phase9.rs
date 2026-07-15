//! Integration tests for FE-104: BarChart labels, Matrix, and Table, exercised
//! through the scene / display-list machinery.

use manim_core::animations::Transform;
use manim_core::graphing::BarChart;
use manim_core::prelude::*;
use manim_text::{BarChartLabels, DecimalTable, Matrix, Table};

#[test]
fn bar_chart_renders_and_labels_match() {
    let mut scene = SceneState::new();
    let chart = BarChart::new(&[1.0, 2.0, 3.0]);
    let labels = chart.get_bar_labels(&mut scene);
    assert_eq!(scene.get_dyn(labels.erase()).data().children.len(), 3);

    // The chart itself draws three bars.
    let id = scene.add(chart);
    let item = scene
        .display_list()
        .0
        .into_iter()
        .find(|it| it.source == id.erase())
        .expect("chart in display list");
    assert_eq!(item.path.subpaths.len(), 3);
}

#[test]
fn bar_chart_transform_between_value_sets() {
    // A BarChart is a single mobject whose path rebuilds on change_bar_values,
    // so Transform between two charts morphs the bars.
    let mut scene = Scene::new(Config::low());
    let a = scene.add(BarChart::new(&[1.0, 1.0, 1.0]));
    let b = scene.add(BarChart::new(&[3.0, 1.0, 2.0]));
    scene.play(Transform::new(a, b)).unwrap();
    // After the transform, a matches b's taller first bar.
    let (base, top) = scene[a].get_bar_span(0);
    assert!((top.y - base.y).abs() > 1.5);
}

#[test]
fn matrix_entries_and_brackets_draw() {
    let mut scene = SceneState::new();
    let m = Matrix::of(&mut scene, &[&["1", "2"], &["3", "4"]]).unwrap();
    // 4 entries + 2 brackets = 6 direct children.
    assert_eq!(scene.get_dyn(m.group().erase()).data().children.len(), 6);
    // The whole matrix (entries + brackets) draws several items.
    let dl = scene.display_list();
    assert!(dl.len() >= 6);
    // Columns transpose correctly.
    assert_eq!(m.get_columns().len(), 2);
    assert_eq!(m.get_columns()[0].len(), 2);
}

#[test]
fn decimal_table_lines_and_highlight() {
    let mut scene = SceneState::new();
    let t = DecimalTable::of(&mut scene, &[&[1.0, 2.0], &[3.0, 4.0]]);
    let lines = t.with_lines(&mut scene);
    assert_eq!(lines.len(), 2); // 1 horizontal + 1 vertical

    let hl = t.highlight_cell(&mut scene, 1, 1, YELLOW);
    // Highlight is behind the entries in z-order.
    let dl = scene.display_list();
    let hl_pos = dl.0.iter().position(|it| it.source == hl).unwrap();
    let entry = t.get_cell(1, 1).unwrap();
    // The highlight's z-index is negative, so it sorts before any entry.
    assert!(scene.get_dyn(hl).data().z_index < 0);
    // And it appears before the cell's glyphs in the (z-sorted) display list.
    let entry_children = scene.get_dyn(entry).data().children.clone();
    if let Some(glyph) = entry_children.first() {
        let g_pos = dl.0.iter().position(|it| it.source == *glyph);
        if let Some(g_pos) = g_pos {
            assert!(
                hl_pos < g_pos,
                "highlight should draw before the cell glyphs"
            );
        }
    }
}

#[test]
fn text_table_lookup() {
    let mut scene = SceneState::new();
    let t = Table::of(&mut scene, &[&["x", "y"], &["1", "2"]]);
    assert!(t.get_cell(0, 0).is_some());
    assert_eq!(t.get_rows().len(), 2);
}
