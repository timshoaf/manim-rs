//! manim_rust Dioxus gallery: a scene picker driving `<ManimPlayer>`.
//!
//! Three text-free scenes (SquareToCircle, an axes plot, a vector field), each
//! with the built-in controls. See the README for build/run steps.

use std::cell::Cell;
use std::rc::Rc;

use dioxus::prelude::*;
use manim_core::animations::{Create, FadeOut, TransformInto};
use manim_core::graphing::{Axes, NumberPlane};
use manim_core::mobject::AnyId;
use manim_core::prelude::*;
use manim_core::vector_field::ArrowVectorField;
use manim_dioxus::{LiveUpdater, ManimPlayer};

/// The canonical square → circle animation.
#[derive(Clone, PartialEq)]
struct SquareToCircle;
impl SceneBuilder for SquareToCircle {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let sq = scene.add(
            Square::new()
                .with_fill(BLUE, 0.7)
                .with_stroke(WHITE, 4.0, 1.0),
        );
        scene.play(Create::new(sq))?;
        scene.play(sq.animate().rotate(PI / 4.0))?;
        scene.play(TransformInto::new(
            sq,
            Circle::new()
                .with_fill(RED, 0.7)
                .with_stroke(WHITE, 4.0, 1.0),
        ))?;
        scene.wait(0.5);
        scene.play(FadeOut::new(sq).shift(DOWN))?;
        Ok(())
    }
}

/// Axes with an animated sine plot (a plotting demo — text-free).
#[derive(Clone, PartialEq)]
struct PlotScene;
impl SceneBuilder for PlotScene {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let axes = Axes::new([-5.0, 5.0, 1.0], [-3.0, 3.0, 1.0]).with_stroke(WHITE, 2.5, 1.0);
        let a = scene.add(axes);
        let curve = scene[a]
            .plot(|x| 2.0 * x.sin(), None)
            .with_stroke(YELLOW, 4.0, 1.0);
        let c = scene.add(curve);
        scene.play(Create::new(c))?;
        scene.wait(0.5);
        Ok(())
    }
}

/// A static rotational vector field, colored by magnitude.
#[derive(Clone, PartialEq)]
struct FieldScene;
impl SceneBuilder for FieldScene {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let field = ArrowVectorField::new(|p: Point| Point::new(-p.y, p.x, 0.0))
            .with_x_range([-4.0, 4.0, 0.8])
            .with_y_range([-2.5, 2.5, 0.8]);
        field.add_to(scene.state_mut());
        Ok(())
    }
}

/// A faint grid backdrop for the interactive cursor demo (static; the moving dot
/// is supplied live by [`cursor_updater`], not by the timeline).
#[derive(Clone, PartialEq)]
struct CursorScene;
impl SceneBuilder for CursorScene {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.add(NumberPlane::new([-7.0, 7.0, 1.0], [-4.0, 4.0, 1.0]));
        Ok(())
    }
}

/// A per-frame updater that makes a dot follow the cursor: it lazily creates the
/// dot on the first frame (remembering its id), moves it to the pointer's scene
/// position, and turns it red while a button is held. This runs entirely on the
/// Dioxus side — no core updater involvement.
fn cursor_updater() -> LiveUpdater {
    let dot: Rc<Cell<Option<AnyId>>> = Rc::new(Cell::new(None));
    LiveUpdater::new(move |state, pointer, _t| {
        let id = match dot.get() {
            Some(id) => id,
            None => {
                let d = state.add(Dot::new()).erase();
                dot.set(Some(d));
                d
            }
        };
        state.move_to(id, pointer.position);
        let color = if pointer.pressed { RED } else { YELLOW };
        state.set_style_family(id, |s| {
            s.set_fill(color, 1.0);
        });
    })
}

/// Which scene the gallery is showing.
#[derive(Clone, Copy, PartialEq)]
enum Which {
    Square,
    Plot,
    Field,
    Cursor,
}

/// The gallery: a picker plus the selected player.
fn app() -> Element {
    let mut which = use_signal(|| Which::Square);
    let pick = |target: Which, current: Which| -> &'static str {
        if target == current {
            "padding:6px 12px;margin-right:6px;background:#4b8;color:#000;border:none;border-radius:4px;"
        } else {
            "padding:6px 12px;margin-right:6px;background:#333;color:#eee;border:none;border-radius:4px;"
        }
    };
    let sel = which();
    rsx! {
        div {
            style: "font-family:system-ui;background:#1a1a1a;color:#eee;min-height:100vh;padding:2rem;",
            h1 { "manim_rust · Dioxus gallery" }
            div { style: "margin:1rem 0;",
                button { style: "{pick(Which::Square, sel)}", onclick: move |_| which.set(Which::Square), "Square → Circle" }
                button { style: "{pick(Which::Plot, sel)}", onclick: move |_| which.set(Which::Plot), "Axes plot" }
                button { style: "{pick(Which::Field, sel)}", onclick: move |_| which.set(Which::Field), "Vector field" }
                button { style: "{pick(Which::Cursor, sel)}", onclick: move |_| which.set(Which::Cursor), "Cursor (live)" }
            }
            div { style: "max-width:720px;",
                match sel {
                    Which::Square => rsx! {
                        ManimPlayer { scene: SquareToCircle, autoplay: true, loop_playback: true, controls: true, width: "100%", height: "405px" }
                    },
                    Which::Plot => rsx! {
                        ManimPlayer { scene: PlotScene, autoplay: true, loop_playback: true, controls: true, width: "100%", height: "405px" }
                    },
                    Which::Field => rsx! {
                        ManimPlayer { scene: FieldScene, autoplay: false, controls: true, width: "100%", height: "405px" }
                    },
                    Which::Cursor => rsx! {
                        ManimPlayer { scene: CursorScene, live: cursor_updater(), autoplay: false, controls: false, width: "100%", height: "405px" }
                        p { style: "color:#aaa;margin-top:8px;", "Move your cursor over the grid — the dot follows in scene space. Hold a button to turn it red. Space/←/→/R work when the player is focused." }
                    },
                }
            }
        }
    }
}

fn main() {
    dioxus::launch(app);
}
