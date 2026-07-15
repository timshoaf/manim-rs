//! Shared grid arrangement for [`Matrix`](crate::Matrix) and
//! [`Table`](crate::Table): places already-added entry mobjects into aligned
//! rows and columns and reports the layout.

use manim_core::mobject::AnyId;
use manim_core::scene_state::SceneState;
use manim_math::Point;

/// One grid entry: its scene id and its measured size.
pub(crate) struct Cell {
    pub id: AnyId,
    pub w: f32,
    pub h: f32,
}

/// The computed geometry of an arranged grid (centered at the origin).
#[derive(Clone)]
pub(crate) struct GridLayout {
    /// Center x of each column.
    pub col_x: Vec<f32>,
    /// Center y of each row.
    pub row_y: Vec<f32>,
    /// Width of each column.
    pub col_w: Vec<f32>,
    /// Height of each row.
    pub row_h: Vec<f32>,
    /// Total grid width and height.
    pub total_w: f32,
    pub total_h: f32,
    /// The buffers used between columns / rows.
    pub h_gap: f32,
    pub v_gap: f32,
}

/// Arranges `cells` into an aligned grid (uniform column widths / row heights),
/// moving each entry to its cell center, and returns the layout.
pub(crate) fn arrange(
    scene: &mut SceneState,
    cells: &[Vec<Cell>],
    h_gap: f32,
    v_gap: f32,
) -> GridLayout {
    let nrows = cells.len();
    let ncols = cells.iter().map(|r| r.len()).max().unwrap_or(0);

    let mut col_w = vec![0.0_f32; ncols];
    let mut row_h = vec![0.0_f32; nrows];
    for (r, row) in cells.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            col_w[c] = col_w[c].max(cell.w);
            row_h[r] = row_h[r].max(cell.h);
        }
    }

    let total_w: f32 = col_w.iter().sum::<f32>() + h_gap * (ncols.saturating_sub(1)) as f32;
    let total_h: f32 = row_h.iter().sum::<f32>() + v_gap * (nrows.saturating_sub(1)) as f32;

    let mut col_x = vec![0.0_f32; ncols];
    let mut acc = -total_w / 2.0;
    for c in 0..ncols {
        col_x[c] = acc + col_w[c] / 2.0;
        acc += col_w[c] + h_gap;
    }
    let mut row_y = vec![0.0_f32; nrows];
    let mut acc = total_h / 2.0;
    for r in 0..nrows {
        row_y[r] = acc - row_h[r] / 2.0;
        acc -= row_h[r] + v_gap;
    }

    for (r, row) in cells.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let center = Point::new(col_x[c], row_y[r], 0.0);
            let cur = scene.family_bounding_box(cell.id).center();
            scene.shift(cell.id, center - cur);
        }
    }

    GridLayout {
        col_x,
        row_y,
        col_w,
        row_h,
        total_w,
        total_h,
        h_gap,
        v_gap,
    }
}
