//! A `Matrix` and a `DecimalTable` side by side, written on and then a table
//! cell highlighted.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim --example matrix_table
//! ```
//!
//! Frames land in `out/matrix_table/frame_NNNNN.png`.
//!
//! API notes vs CE:
//! - `Matrix::of` / `DecimalTable::of` add themselves to the scene and return a
//!   handle whose `group()` is the id to animate/position.
//! - `highlight_cell(scene, row, col, color)` inserts a background rect (z = -1)
//!   behind the cell and returns its id.

use manim::prelude::*;

/// Scene builder for this gallery example.
pub struct MatrixTable;

impl SceneBuilder for MatrixTable {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let matrix = Matrix::of(
            scene.state_mut(),
            &[&["1", "2", "0"], &["0", "1", "3"], &["2", "0", "1"]],
        )?;
        scene.state_mut().shift(matrix.group(), 3.4 * LEFT);

        let table = DecimalTable::of(scene.state_mut(), &[&[1.0, 2.0], &[3.0, 4.0]]);
        scene.state_mut().shift(table.group(), 3.4 * RIGHT);

        scene.play(Write::new(matrix.group()).run_time(1.5))?;
        scene.play(Write::new(table.group()).run_time(1.5))?;

        let highlight = table.highlight_cell(scene.state_mut(), 1, 1, YELLOW);
        scene.play(FadeIn::new(highlight))?;
        scene.wait(0.5);
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&MatrixTable, Config::low())?;
    manim::render::export::VideoExporter::render_to_png_sequence(&mut scene, "out/matrix_table")?;
    println!("Rendered frames to out/matrix_table");
    Ok(())
}
