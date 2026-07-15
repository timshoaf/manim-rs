//! Matrix mobjects: [`Matrix`] (tex entries), [`DecimalMatrix`],
//! [`IntegerMatrix`], and [`MobjectMatrix`], all bracketed.

use manim_color::WHITE;
use manim_core::geometry::{VGroup, VMobject};
use manim_core::mobject::{AnyId, MobjectExt, MobjectId};
use manim_core::scene_state::SceneState;
use manim_core::style::Style;
use manim_math::path::Path;
use manim_math::Point;

use crate::decimal::{DecimalNumber, Integer};
use crate::grid::{arrange, Cell, GridLayout};
use crate::latex::MathError;
use crate::math::MathTex;

/// Default entry font size for matrices/tables.
pub const ENTRY_FONT_SIZE: f32 = 40.0;
/// Default horizontal buffer between matrix columns (CE-ish, scaled).
pub const MATRIX_H_BUFF: f32 = 0.5;
/// Default vertical buffer between matrix rows.
pub const MATRIX_V_BUFF: f32 = 0.35;

/// A bracketed matrix of entry mobjects, arranged in an aligned grid. Port of
/// manim CE's `Matrix` family.
///
/// This is a *handle*: [`Matrix::of`] adds the entries and brackets to the scene
/// under one group and returns their ids for querying. Entries share their
/// column's center x (aligned), and the brackets span the grid height.
pub struct Matrix {
    group: MobjectId<VGroup>,
    entries: Vec<Vec<AnyId>>,
    brackets: (AnyId, AnyId),
}

impl Matrix {
    /// A tex matrix from string entries (each typeset as [`MathTex`]).
    ///
    /// ```
    /// use manim_text::Matrix;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// let mut scene = SceneState::new();
    /// let m = Matrix::of(&mut scene, &[&["1", "0"], &["0", "1"]]).unwrap();
    /// // 4 entries + 2 brackets under the group.
    /// assert_eq!(m.get_entries().len(), 2);
    /// assert_eq!(scene.get_dyn(m.group().erase()).data().children.len(), 6);
    /// ```
    pub fn of(scene: &mut SceneState, rows: &[&[&str]]) -> Result<Matrix, MathError> {
        let mut cells: Vec<Vec<Cell>> = Vec::new();
        for row in rows {
            let mut crow = Vec::new();
            for s in *row {
                let entry = MathTex::new(s)?.font_size(ENTRY_FONT_SIZE);
                let bb = entry.bounding_box();
                let id = entry.add_to(scene).erase();
                crow.push(Cell {
                    id,
                    w: bb.width().max(0.15),
                    h: bb.height().max(0.15),
                });
            }
            cells.push(crow);
        }
        Ok(build(scene, cells))
    }

    /// The group holding all entries and brackets.
    pub fn group(&self) -> MobjectId<VGroup> {
        self.group
    }

    /// The entry ids, row-major.
    pub fn get_entries(&self) -> &[Vec<AnyId>] {
        &self.entries
    }

    /// The entry id at `(row, col)`.
    pub fn get_entry(&self, row: usize, col: usize) -> Option<AnyId> {
        self.entries.get(row).and_then(|r| r.get(col)).copied()
    }

    /// The entry ids grouped by row (same as [`get_entries`](Self::get_entries)).
    pub fn get_rows(&self) -> Vec<Vec<AnyId>> {
        self.entries.clone()
    }

    /// The entry ids grouped by column (a transpose of the rows).
    pub fn get_columns(&self) -> Vec<Vec<AnyId>> {
        let ncols = self.entries.iter().map(|r| r.len()).max().unwrap_or(0);
        (0..ncols)
            .map(|c| {
                self.entries
                    .iter()
                    .filter_map(|r| r.get(c).copied())
                    .collect()
            })
            .collect()
    }

    /// The `(left, right)` bracket ids.
    pub fn get_brackets(&self) -> (AnyId, AnyId) {
        self.brackets
    }
}

/// A matrix of `f32` values (each an aligned [`DecimalNumber`]). Port of manim
/// CE's `DecimalMatrix`.
///
/// ```
/// use manim_text::DecimalMatrix;
/// use manim_core::scene_state::SceneState;
/// let mut scene = SceneState::new();
/// let m = DecimalMatrix::of(&mut scene, &[&[1.5, 2.0], &[3.25, 4.0]]);
/// assert_eq!(m.get_entries().len(), 2);
/// ```
pub struct DecimalMatrix;

impl DecimalMatrix {
    /// A decimal matrix (2 decimal places).
    #[allow(clippy::new_ret_no_self)]
    pub fn of(scene: &mut SceneState, rows: &[&[f32]]) -> Matrix {
        let cells = build_number_cells(scene, rows, false);
        build(scene, cells)
    }
}

/// A matrix of integers (each an aligned [`Integer`]). Port of manim CE's
/// `IntegerMatrix`.
pub struct IntegerMatrix;

impl IntegerMatrix {
    /// An integer matrix.
    #[allow(clippy::new_ret_no_self)]
    pub fn of(scene: &mut SceneState, rows: &[&[i64]]) -> Matrix {
        let floats: Vec<Vec<f32>> = rows
            .iter()
            .map(|r| r.iter().map(|&v| v as f32).collect())
            .collect();
        let refs: Vec<&[f32]> = floats.iter().map(|r| r.as_slice()).collect();
        let cells = build_number_cells(scene, &refs, true);
        build(scene, cells)
    }
}

/// A matrix of arbitrary pre-added mobjects. Port of manim CE's `MobjectMatrix`.
pub struct MobjectMatrix;

impl MobjectMatrix {
    /// Arranges and brackets already-added mobjects (given by id) into a matrix.
    #[allow(clippy::new_ret_no_self)]
    pub fn of(scene: &mut SceneState, rows: &[&[AnyId]]) -> Matrix {
        let mut cells: Vec<Vec<Cell>> = Vec::new();
        for row in rows {
            let mut crow = Vec::new();
            for &id in *row {
                let bb = scene.family_bounding_box(id);
                crow.push(Cell {
                    id,
                    w: bb.width().max(0.15),
                    h: bb.height().max(0.15),
                });
            }
            cells.push(crow);
        }
        build(scene, cells)
    }
}

/// Builds numeric cells (DecimalNumber / Integer) for a numeric matrix.
fn build_number_cells(scene: &mut SceneState, rows: &[&[f32]], integral: bool) -> Vec<Vec<Cell>> {
    let mut cells = Vec::new();
    for row in rows {
        let mut crow = Vec::new();
        for &v in *row {
            let number = if integral {
                Integer::new(v.round() as i64).font_size(ENTRY_FONT_SIZE)
            } else {
                DecimalNumber::new(v).font_size(ENTRY_FONT_SIZE)
            };
            let bb = number.bounding_box();
            let id = scene.add(number).erase();
            crow.push(Cell {
                id,
                w: bb.width().max(0.15),
                h: bb.height().max(0.15),
            });
        }
        cells.push(crow);
    }
    cells
}

/// Arranges cells, adds brackets, groups everything, and returns the handle.
fn build(scene: &mut SceneState, cells: Vec<Vec<Cell>>) -> Matrix {
    let layout = arrange(scene, &cells, MATRIX_H_BUFF, MATRIX_V_BUFF);
    let (lb, rb) = add_brackets(scene, &layout);
    let entries: Vec<Vec<AnyId>> = cells
        .iter()
        .map(|r| r.iter().map(|c| c.id).collect())
        .collect();
    let mut all: Vec<AnyId> = entries.iter().flatten().copied().collect();
    all.push(lb);
    all.push(rb);
    let group = VGroup::of(scene, all);
    Matrix {
        group,
        entries,
        brackets: (lb, rb),
    }
}

/// Adds `[` and `]` bracket mobjects spanning the grid height, returning ids.
fn add_brackets(scene: &mut SceneState, layout: &GridLayout) -> (AnyId, AnyId) {
    let pad = 0.15;
    let arm = 0.12;
    let gap = 0.15;
    let top = layout.total_h / 2.0 + pad;
    let bot = -layout.total_h / 2.0 - pad;

    let lx = -layout.total_w / 2.0 - gap;
    let left = VMobject::new(
        Path::from_corners(
            &[
                Point::new(lx, top, 0.0),
                Point::new(lx - arm, top, 0.0),
                Point::new(lx - arm, bot, 0.0),
                Point::new(lx, bot, 0.0),
            ],
            false,
        ),
        Style::stroked(WHITE),
    );

    let rx = layout.total_w / 2.0 + gap;
    let right = VMobject::new(
        Path::from_corners(
            &[
                Point::new(rx, top, 0.0),
                Point::new(rx + arm, top, 0.0),
                Point::new(rx + arm, bot, 0.0),
                Point::new(rx, bot, 0.0),
            ],
            false,
        ),
        Style::stroked(WHITE),
    );

    (scene.add(left).erase(), scene.add(right).erase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_are_aligned() {
        let mut scene = SceneState::new();
        let m = Matrix::of(&mut scene, &[&["1", "22"], &["333", "4"]]).unwrap();
        // Both entries in column 0 share the same center x.
        let x00 = scene
            .family_bounding_box(m.get_entry(0, 0).unwrap())
            .center()
            .x;
        let x10 = scene
            .family_bounding_box(m.get_entry(1, 0).unwrap())
            .center()
            .x;
        assert!((x00 - x10).abs() < 1e-3, "{x00} vs {x10}");
    }

    #[test]
    fn brackets_span_the_height() {
        let mut scene = SceneState::new();
        let m = Matrix::of(&mut scene, &[&["1"], &["2"], &["3"]]).unwrap();
        let (lb, _rb) = m.get_brackets();
        let bracket_h = scene.family_bounding_box(lb).height();
        // Entry extent (all three entries).
        let entries: Vec<AnyId> = m.get_entries().iter().flatten().copied().collect();
        let mut top = f32::NEG_INFINITY;
        let mut bot = f32::INFINITY;
        for e in entries {
            let bb = scene.family_bounding_box(e);
            top = top.max(bb.max.y);
            bot = bot.min(bb.min.y);
        }
        assert!(bracket_h >= top - bot, "{bracket_h} vs {}", top - bot);
    }
}
