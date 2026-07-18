//! Weight and attention heatmaps, painted through `manim_sci`'s
//! [`MaterialQuad`] and overlaid with optional cell grids and attention links.
//!
//! A matrix `M` (rows × cols) is drawn as an axis-aligned rectangle spanning
//! `x ∈ [0, cols]`, `y ∈ [0, rows]`, with **row 0 at the top**. Each cell is a
//! unit square; the value painted over cell `(row, col)` is `M[row][col]`. The
//! colour comes from a GPU [`Colormap`], auto-ranged to the matrix's min/max by
//! [`MaterialQuad::heatmap`].
//!
//! ```
//! use manim_core::display::Colormap;
//! use manim_core::scene_state::SceneState;
//! use manim_nn::heatmap::matrix_heatmap;
//! let mut scene = SceneState::new();
//! let m = vec![vec![0.0_f32, 1.0], vec![2.0, 3.0]];
//! let g = matrix_heatmap(&mut scene, &m, Colormap::Viridis);
//! assert!(scene.contains(g.erase()));
//! ```

use manim_core::display::Colormap;
use manim_core::geometry::{Line, VGroup};
use manim_core::mobject::{AnyId, MobjectId};
use manim_core::scene_state::SceneState;
use manim_fields::ad::Scalar;
use manim_fields::field::{ScalarClosure, ScalarField};
use manim_fields::Point as FieldPoint;
use manim_math::Point;
use manim_sci::material_quad::MaterialQuad;

/// Sample density (texels) per matrix cell along each axis. The field is
/// piecewise-constant, so a handful of texels per cell keeps the boundaries
/// crisp without wasting texture memory.
const TEXELS_PER_CELL: usize = 8;

/// A piecewise-constant [`ScalarClosure`]: the field value at `(x, y)` is the
/// matrix entry of the cell containing that point, with row 0 at the top.
struct MatrixField {
    rows: usize,
    cols: usize,
    /// Row-major values (`row * cols + col`).
    values: Vec<f64>,
}

impl ScalarClosure for MatrixField {
    fn eval<S: Scalar>(&self, p: [S; 3]) -> S {
        if self.rows == 0 || self.cols == 0 {
            return S::constant(0.0);
        }
        let (x, y) = (p[0].value(), p[1].value());
        let col = (x.floor() as isize).clamp(0, self.cols as isize - 1) as usize;
        let from_bottom = (y.floor() as isize).clamp(0, self.rows as isize - 1) as usize;
        // Row 0 sits at the top (largest y), so invert.
        let row = self.rows - 1 - from_bottom;
        S::constant(self.values[row * self.cols + col])
    }
}

/// The `(rows, cols)` shape of a rectangular matrix (`cols` is taken from row 0).
fn shape(matrix: &[Vec<f32>]) -> (usize, usize) {
    (matrix.len(), matrix.first().map_or(0, Vec::len))
}

/// Builds the piecewise-constant [`ScalarField`] that a heatmap paints — the
/// same field the value-mapping test samples.
///
/// ```
/// use manim_nn::heatmap::{cell_center, matrix_field};
/// let m = vec![vec![1.0_f32, 2.0], vec![3.0, 4.0]];
/// let f = matrix_field(&m);
/// // Row 0 is on top; cell (0, 1) holds 2.0.
/// assert_eq!(f.at(cell_center(2, 2, 0, 1)), 2.0);
/// ```
pub fn matrix_field(matrix: &[Vec<f32>]) -> ScalarField {
    let (rows, cols) = shape(matrix);
    let mut values = Vec::with_capacity(rows * cols);
    for row in matrix {
        for &v in row.iter().take(cols) {
            values.push(v as f64);
        }
        // Pad ragged rows so indexing stays in bounds.
        if row.len() < cols {
            values.resize(values.len() + (cols - row.len()), 0.0);
        }
    }
    ScalarField::from_closure(MatrixField { rows, cols, values })
}

/// The world-space centre of cell `(row, col)` in a `rows × cols` heatmap
/// (row 0 at the top). Cells are unit squares over `[0, cols] × [0, rows]`.
///
/// ```
/// use manim_nn::heatmap::cell_center;
/// let c = cell_center(2, 2, 0, 0); // top-left cell
/// assert_eq!((c.x, c.y), (0.5, 1.5));
/// ```
pub fn cell_center(rows: usize, cols: usize, row: usize, col: usize) -> FieldPoint {
    debug_assert!(col < cols || cols == 0, "column out of range");
    let x = col as f64 + 0.5;
    let y = (rows - 1 - row) as f64 + 0.5;
    FieldPoint::new(x, y, 0.0)
}

/// The thin cell-boundary lines of a `rows × cols` heatmap: `cols + 1` vertical
/// lines and `rows + 1` horizontal lines, so `(rows + 1) + (cols + 1)` in total.
///
/// ```
/// use manim_nn::heatmap::grid_lines;
/// assert_eq!(grid_lines(3, 5).len(), (3 + 1) + (5 + 1));
/// ```
pub fn grid_lines(rows: usize, cols: usize) -> Vec<Line> {
    let (w, h) = (cols as f32, rows as f32);
    let mut lines = Vec::with_capacity((rows + 1) + (cols + 1));
    for c in 0..=cols {
        let x = c as f32;
        lines.push(Line::new(Point::new(x, 0.0, 0.0), Point::new(x, h, 0.0)));
    }
    for r in 0..=rows {
        let y = r as f32;
        lines.push(Line::new(Point::new(0.0, y, 0.0), Point::new(w, y, 0.0)));
    }
    lines
}

/// Adds the material-quad heatmap for `matrix` and returns its id plus the
/// matrix shape.
fn add_quad(
    scene: &mut SceneState,
    matrix: &[Vec<f32>],
    colormap: Colormap,
) -> (MobjectId<MaterialQuad>, usize, usize) {
    let (rows, cols) = shape(matrix);
    let field = matrix_field(matrix);
    let resolution = (
        (cols * TEXELS_PER_CELL).max(2),
        (rows * TEXELS_PER_CELL).max(2),
    );
    let quad = MaterialQuad::heatmap(
        [0.0, cols as f64],
        [0.0, rows as f64],
        resolution,
        &field,
        colormap,
    )
    .add_to(scene);
    (quad, rows, cols)
}

/// Draws `matrix` as a heatmap with thin cell grid lines, grouped into a
/// [`VGroup`]. Row 0 is at the top; the colour scale is auto-ranged to the
/// matrix.
///
/// See [`matrix_heatmap_opts`] to toggle the grid.
///
/// ```
/// use manim_core::display::Colormap;
/// use manim_core::scene_state::SceneState;
/// use manim_nn::heatmap::matrix_heatmap;
/// let mut scene = SceneState::new();
/// let m = vec![vec![1.0_f32, 2.0], vec![3.0, 4.0]];
/// let g = matrix_heatmap(&mut scene, &m, Colormap::Magma);
/// assert!(scene.contains(g.erase()));
/// ```
pub fn matrix_heatmap(
    scene: &mut SceneState,
    matrix: &[Vec<f32>],
    colormap: Colormap,
) -> MobjectId<VGroup> {
    matrix_heatmap_opts(scene, matrix, colormap, true)
}

/// Like [`matrix_heatmap`], but `show_grid` toggles the cell grid lines. With
/// the grid enabled the group holds the quad plus `(rows + 1) + (cols + 1)`
/// [`Line`]s.
///
/// ```
/// use manim_core::display::Colormap;
/// use manim_core::scene_state::SceneState;
/// use manim_nn::heatmap::matrix_heatmap_opts;
/// let mut scene = SceneState::new();
/// let m = vec![vec![1.0_f32, 2.0], vec![3.0, 4.0]];
/// // No grid: the group contains only the heatmap quad.
/// let g = matrix_heatmap_opts(&mut scene, &m, Colormap::Turbo, false);
/// assert!(scene.contains(g.erase()));
/// ```
pub fn matrix_heatmap_opts(
    scene: &mut SceneState,
    matrix: &[Vec<f32>],
    colormap: Colormap,
    show_grid: bool,
) -> MobjectId<VGroup> {
    let (quad, rows, cols) = add_quad(scene, matrix, colormap);
    let mut children: Vec<AnyId> = vec![quad.erase()];
    if show_grid {
        for line in grid_lines(rows, cols) {
            children.push(scene.add(line).erase());
        }
    }
    VGroup::of(scene, children)
}

/// The argmax key column for each query row (the top-1 attention link). Rows
/// with no columns are skipped.
fn top1_links(weights: &[Vec<f32>]) -> Vec<(usize, usize)> {
    weights
        .iter()
        .enumerate()
        .filter_map(|(row, w)| {
            let (col, _) = w
                .iter()
                .enumerate()
                .fold(None, |best, (c, &v)| match best {
                    Some((_, bv)) if bv >= v => best,
                    _ => Some((c, v)),
                })?;
            Some((row, col))
        })
        .collect()
}

/// Draws an attention pattern: the `weights` matrix (queries = rows, keys =
/// cols) as a heatmap, overlaid with one link per query connecting its
/// left-margin marker to the top-margin marker of the **strongest** key.
///
/// The overlay is **top-1** per row (`k = 1`, the argmax key) — the single
/// dominant attention target, the clearest read on a dense pattern. The heatmap
/// itself carries the full distribution.
///
/// ```
/// use manim_core::scene_state::SceneState;
/// use manim_nn::heatmap::attention_pattern;
/// let mut scene = SceneState::new();
/// // Query 0 attends to key 1; query 1 to key 0.
/// let w = vec![vec![0.1_f32, 0.9], vec![0.8, 0.2]];
/// let g = attention_pattern(&mut scene, &w);
/// assert!(scene.contains(g.erase()));
/// ```
pub fn attention_pattern(scene: &mut SceneState, weights: &[Vec<f32>]) -> MobjectId<VGroup> {
    let (rows, cols) = shape(weights);
    // Keep the heatmap grid off so the attention links read clearly on top.
    let heat = matrix_heatmap_opts(scene, weights, Colormap::Viridis, false);
    let mut children: Vec<AnyId> = vec![heat.erase()];
    for (row, col) in top1_links(weights) {
        let center = cell_center(rows, cols, row, col);
        // Query marker: just left of the row. Key marker: just above the column.
        let query = Point::new(-0.5, center.y as f32, 0.0);
        let key = Point::new(center.x as f32, rows as f32 + 0.5, 0.0);
        children.push(scene.add(Line::new(query, key)).erase());
    }
    VGroup::of(scene, children)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_maps_to_correct_cell() {
        // Row 0 on top: [[1, 2], [3, 4]].
        let m = vec![vec![1.0_f32, 2.0], vec![3.0, 4.0]];
        let f = matrix_field(&m);
        assert_eq!(f.at(cell_center(2, 2, 0, 0)), 1.0);
        assert_eq!(f.at(cell_center(2, 2, 0, 1)), 2.0);
        assert_eq!(f.at(cell_center(2, 2, 1, 0)), 3.0);
        assert_eq!(f.at(cell_center(2, 2, 1, 1)), 4.0);
    }

    #[test]
    fn grid_line_count_matches_formula() {
        assert_eq!(grid_lines(2, 2).len(), (2 + 1) + (2 + 1));
        assert_eq!(grid_lines(3, 5).len(), (3 + 1) + (5 + 1));
    }

    #[test]
    fn heatmap_group_builds() {
        let mut scene = SceneState::new();
        let m = vec![vec![1.0_f32, 2.0], vec![3.0, 4.0]];
        let g = matrix_heatmap(&mut scene, &m, Colormap::Viridis);
        assert!(scene.contains(g.erase()));
        let g2 = matrix_heatmap_opts(&mut scene, &m, Colormap::Viridis, false);
        assert!(scene.contains(g2.erase()));
    }

    #[test]
    fn attention_top1_picks_argmax_key() {
        let w = vec![vec![0.1_f32, 0.9], vec![0.8, 0.2]];
        assert_eq!(top1_links(&w), vec![(0, 1), (1, 0)]);
    }
}
