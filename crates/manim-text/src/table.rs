//! Table mobjects: [`Table`] (text entries), [`MathTable`], and [`DecimalTable`],
//! with optional grid lines and cell highlighting.

use manim_color::Color;
use manim_core::geometry::{Line, VGroup, VMobject};
use manim_core::mobject::{AnyId, MobjectExt, MobjectId};
use manim_core::scene_state::SceneState;
use manim_core::style::Style;
use manim_math::path::Path;
use manim_math::Point;

use crate::decimal::DecimalNumber;
use crate::grid::{arrange, Cell, GridLayout};
use crate::math::MathTex;
use crate::matrix::ENTRY_FONT_SIZE;
use crate::text::Text;
use manim_core::error::CoreError;

/// Horizontal buffer between table columns.
pub const TABLE_H_BUFF: f32 = 0.6;
/// Vertical buffer between table rows.
pub const TABLE_V_BUFF: f32 = 0.4;
/// Padding from cell content to grid lines / highlights.
pub const CELL_PAD: f32 = 0.15;

/// A grid of entry mobjects with optional separator lines and cell highlights.
/// Port of manim CE's `Table` family.
///
/// A *handle*: [`Table::of`] adds the cell entries to the scene under one group;
/// [`with_lines`](Self::with_lines) and [`highlight_cell`](Self::highlight_cell)
/// add decorations afterward.
pub struct Table {
    group: MobjectId<VGroup>,
    cells: Vec<Vec<AnyId>>,
    layout: GridLayout,
}

impl Table {
    /// A table of text entries.
    ///
    /// ```
    /// use manim_text::Table;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// let mut scene = SceneState::new();
    /// let t = Table::of(&mut scene, &[&["a", "b"], &["c", "d"]]);
    /// assert_eq!(t.get_rows().len(), 2);
    /// assert_eq!(t.get_columns().len(), 2);
    /// ```
    pub fn of(scene: &mut SceneState, rows: &[&[&str]]) -> Table {
        let cells = build_text_cells(scene, rows);
        Self::finish(scene, cells)
    }

    /// The group holding the cell entries.
    pub fn group(&self) -> MobjectId<VGroup> {
        self.group
    }

    /// The cell id at `(row, col)`.
    pub fn get_cell(&self, row: usize, col: usize) -> Option<AnyId> {
        self.cells.get(row).and_then(|r| r.get(col)).copied()
    }

    /// The cells grouped by row.
    pub fn get_rows(&self) -> Vec<Vec<AnyId>> {
        self.cells.clone()
    }

    /// The cells grouped by column.
    pub fn get_columns(&self) -> Vec<Vec<AnyId>> {
        let ncols = self.cells.iter().map(|r| r.len()).max().unwrap_or(0);
        (0..ncols)
            .map(|c| {
                self.cells
                    .iter()
                    .filter_map(|r| r.get(c).copied())
                    .collect()
            })
            .collect()
    }

    /// Adds internal separator [`Line`]s (between rows and columns) to the table,
    /// returning their ids. There are `(rows-1) + (cols-1)` of them.
    ///
    /// ```
    /// use manim_text::Table;
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let t = Table::of(&mut scene, &[&["a", "b", "c"], &["d", "e", "f"]]);
    /// // 2 rows, 3 cols → 1 horizontal + 2 vertical = 3 lines.
    /// assert_eq!(t.with_lines(&mut scene).len(), 3);
    /// ```
    pub fn with_lines(&self, scene: &mut SceneState) -> Vec<AnyId> {
        let l = &self.layout;
        let x_left = -l.total_w / 2.0 - CELL_PAD;
        let x_right = l.total_w / 2.0 + CELL_PAD;
        let y_top = l.total_h / 2.0 + CELL_PAD;
        let y_bot = -l.total_h / 2.0 - CELL_PAD;
        let mut ids = Vec::new();

        for r in 0..l.row_y.len().saturating_sub(1) {
            let y = l.row_y[r] - l.row_h[r] / 2.0 - l.v_gap / 2.0;
            let line = scene.add(Line::new(
                Point::new(x_left, y, 0.0),
                Point::new(x_right, y, 0.0),
            ));
            scene.add_child(self.group.erase(), line.erase());
            ids.push(line.erase());
        }
        for c in 0..l.col_x.len().saturating_sub(1) {
            let x = l.col_x[c] + l.col_w[c] / 2.0 + l.h_gap / 2.0;
            let line = scene.add(Line::new(
                Point::new(x, y_top, 0.0),
                Point::new(x, y_bot, 0.0),
            ));
            scene.add_child(self.group.erase(), line.erase());
            ids.push(line.erase());
        }
        ids
    }

    /// Adds a filled background rectangle behind cell `(row, col)` (drawn under
    /// the entries via a negative z-index), returning its id.
    ///
    /// ```
    /// use manim_text::Table;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::Mobject;
    /// use manim_color::YELLOW;
    /// let mut scene = SceneState::new();
    /// let t = Table::of(&mut scene, &[&["a", "b"], &["c", "d"]]);
    /// let hl = t.highlight_cell(&mut scene, 0, 1, YELLOW);
    /// // The highlight sits behind the entries.
    /// assert!(scene.get_dyn(hl).data().z_index < 0);
    /// ```
    pub fn highlight_cell(
        &self,
        scene: &mut SceneState,
        row: usize,
        col: usize,
        color: Color,
    ) -> AnyId {
        let l = &self.layout;
        let cx = l.col_x[col];
        let cy = l.row_y[row];
        let hw = l.col_w[col] / 2.0 + l.h_gap / 2.0;
        let hh = l.row_h[row] / 2.0 + l.v_gap / 2.0;
        let rect = Path::from_corners(
            &[
                Point::new(cx - hw, cy - hh, 0.0),
                Point::new(cx + hw, cy - hh, 0.0),
                Point::new(cx + hw, cy + hh, 0.0),
                Point::new(cx - hw, cy + hh, 0.0),
            ],
            true,
        );
        let mut mob = VMobject::new(rect, Style::filled(color));
        mob.set_z_index(-1);
        let id = scene.add(mob).erase();
        scene.add_child(self.group.erase(), id);
        id
    }

    /// Arranges cells, groups them, and stores the layout.
    fn finish(scene: &mut SceneState, cells: Vec<Vec<Cell>>) -> Table {
        let layout = arrange(scene, &cells, TABLE_H_BUFF, TABLE_V_BUFF);
        let ids: Vec<Vec<AnyId>> = cells
            .iter()
            .map(|r| r.iter().map(|c| c.id).collect())
            .collect();
        let all: Vec<AnyId> = ids.iter().flatten().copied().collect();
        let group = VGroup::of(scene, all);
        Table {
            group,
            cells: ids,
            layout,
        }
    }
}

/// A table of tex entries (each an aligned [`MathTex`]). Port of manim CE's
/// `MathTable`.
pub struct MathTable;

impl MathTable {
    /// A tex-entry table.
    #[allow(clippy::new_ret_no_self)]
    pub fn of(scene: &mut SceneState, rows: &[&[&str]]) -> Result<Table, CoreError> {
        let mut cells: Vec<Vec<Cell>> = Vec::new();
        for row in rows {
            let mut crow = Vec::new();
            for s in *row {
                let entry = MathTex::new(s)?.font_size(ENTRY_FONT_SIZE);
                let bb = entry.bounding_box();
                let id = entry.add_to(scene).erase();
                crow.push(cell(id, bb.width(), bb.height()));
            }
            cells.push(crow);
        }
        Ok(Table::finish(scene, cells))
    }
}

/// A table of `f32` values (each an aligned [`DecimalNumber`]). Port of manim
/// CE's `DecimalTable`.
pub struct DecimalTable;

impl DecimalTable {
    /// A decimal-entry table.
    #[allow(clippy::new_ret_no_self)]
    pub fn of(scene: &mut SceneState, rows: &[&[f32]]) -> Table {
        let mut cells: Vec<Vec<Cell>> = Vec::new();
        for row in rows {
            let mut crow = Vec::new();
            for &v in *row {
                let number = DecimalNumber::new(v).font_size(ENTRY_FONT_SIZE);
                let bb = number.bounding_box();
                let id = scene.add(number).erase();
                crow.push(cell(id, bb.width(), bb.height()));
            }
            cells.push(crow);
        }
        Table::finish(scene, cells)
    }
}

/// Builds text-entry cells.
fn build_text_cells(scene: &mut SceneState, rows: &[&[&str]]) -> Vec<Vec<Cell>> {
    let mut cells = Vec::new();
    for row in rows {
        let mut crow = Vec::new();
        for s in *row {
            let entry = Text::new(*s).font_size(ENTRY_FONT_SIZE);
            let bb = entry.bounding_box();
            let id = entry.add_to(scene).erase();
            crow.push(cell(id, bb.width(), bb.height()));
        }
        cells.push(crow);
    }
    cells
}

/// A grid cell with a floor on its measured size.
fn cell(id: AnyId, w: f32, h: f32) -> Cell {
    Cell {
        id,
        w: w.max(0.2),
        h: h.max(0.2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_counts() {
        let mut scene = SceneState::new();
        let t = Table::of(&mut scene, &[&["a", "b", "c"], &["d", "e", "f"]]);
        // 2 rows, 3 cols → 1 horizontal + 2 vertical.
        assert_eq!(t.with_lines(&mut scene).len(), 3);
    }

    #[test]
    fn cell_lookup() {
        let mut scene = SceneState::new();
        let t = Table::of(&mut scene, &[&["a", "b"], &["c", "d"]]);
        assert!(t.get_cell(1, 1).is_some());
        assert!(t.get_cell(2, 0).is_none());
    }

    #[test]
    fn highlight_is_behind() {
        let mut scene = SceneState::new();
        let t = Table::of(&mut scene, &[&["a", "b"], &["c", "d"]]);
        let hl = t.highlight_cell(&mut scene, 0, 0, manim_color::YELLOW);
        assert!(scene.get_dyn(hl).data().z_index < 0);
        // An entry keeps the default z-index of 0, so it draws on top.
        let entry = t.get_cell(0, 0).unwrap();
        assert!(scene.get_dyn(entry).data().z_index >= 0);
    }
}
