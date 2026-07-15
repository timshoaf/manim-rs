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

/// A `ZoomedScene`: a tiny cluster of shapes magnified into a bordered inset.
#[derive(Clone, PartialEq)]
struct ZoomScene;
impl SceneBuilder for ZoomScene {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.add(Circle::new().with_scale(2.6).with_stroke(WHITE, 3.0, 1.0));
        scene.add(
            Circle::new()
                .with_scale(0.3)
                .with_fill(BLUE, 1.0)
                .with_shift(0.4 * LEFT),
        );
        scene.add(
            Square::new()
                .with_scale(0.24)
                .with_fill(RED, 1.0)
                .with_shift(0.4 * RIGHT),
        );
        scene.add(
            Triangle::new()
                .with_scale(0.24)
                .with_fill(GREEN, 1.0)
                .with_shift(0.4 * UP),
        );
        // ~4× magnifier over a 1.3-unit region into a top-right inset.
        scene.add_zoom_window(ORIGIN, 1.3, [0.60, 0.05, 0.35, 0.35]);
        scene.wait(0.6);
        Ok(())
    }
}

/// Which scene the gallery is showing.
#[derive(Clone, Copy, PartialEq)]
enum Which {
    Square,
    Plot,
    Field,
    Cursor,
    Zoom,
}

/// The scene picker entries, in display order: `(variant, label, caption)`.
const SCENES: &[(Which, &str, &str)] = &[
    (
        Which::Square,
        "Square → Circle",
        "The canonical transform: create, rotate, morph, fade.",
    ),
    (
        Which::Plot,
        "Axes plot",
        "Axes with an animated sine curve — a plotting demo.",
    ),
    (
        Which::Field,
        "Vector field",
        "A rotational field f(x,y) = (−y, x), colored by magnitude.",
    ),
    (
        Which::Cursor,
        "Cursor (live)",
        "Live input: a dot follows your cursor in scene space (hold to turn it red).",
    ),
    (
        Which::Zoom,
        "Zoomed inset",
        "A ZoomedScene: a tiny cluster magnified ~4× into a bordered inset.",
    ),
];

/// The gallery: a scene picker driving the selected `<ManimPlayer>`.
fn app() -> Element {
    let mut which = use_signal(|| Which::Square);
    let sel = which();
    let caption = SCENES
        .iter()
        .find(|(w, ..)| *w == sel)
        .map(|(.., c)| *c)
        .unwrap_or_default();

    rsx! {
        div {
            style: "font-family:system-ui;background:#141414;color:#eee;min-height:100vh;padding:2rem 1.5rem;box-sizing:border-box;",
            div { style: "max-width:760px;margin:0 auto;",
                h1 { style: "margin:0 0 4px;font-size:1.6rem;", "manim_rust · Dioxus gallery" }
                p { style: "margin:0 0 1.2rem;color:#9aa;",
                    "manim scenes rendered to a live "
                    code { style: "color:#7cd;", "<canvas>" }
                    " through wgpu. Pick a scene:"
                }
                div { style: "display:flex;flex-wrap:wrap;gap:8px;margin-bottom:1rem;",
                    for (w , label , _) in SCENES.iter().copied() {
                        button {
                            style: if w == sel {
                                "padding:7px 13px;background:#4b8;color:#062;border:none;border-radius:6px;font-weight:600;cursor:pointer;"
                            } else {
                                "padding:7px 13px;background:#2a2a2a;color:#ddd;border:none;border-radius:6px;cursor:pointer;"
                            },
                            onclick: move |_| which.set(w),
                            "{label}"
                        }
                    }
                }
                div { style: "border:1px solid #2a2a2a;border-radius:10px;overflow:hidden;background:#000;",
                    match sel {
                        Which::Square => rsx! {
                            ManimPlayer { scene: SquareToCircle, autoplay: true, loop_playback: true, controls: true, width: "100%", height: "428px" }
                        },
                        Which::Plot => rsx! {
                            ManimPlayer { scene: PlotScene, autoplay: true, loop_playback: true, controls: true, width: "100%", height: "428px" }
                        },
                        Which::Field => rsx! {
                            ManimPlayer { scene: FieldScene, autoplay: false, controls: true, width: "100%", height: "428px" }
                        },
                        Which::Cursor => rsx! {
                            ManimPlayer { scene: CursorScene, live: cursor_updater(), autoplay: false, controls: false, width: "100%", height: "428px" }
                        },
                        Which::Zoom => rsx! {
                            ManimPlayer { scene: ZoomScene, autoplay: true, loop_playback: true, controls: true, width: "100%", height: "428px" }
                        },
                    }
                }
                p { style: "color:#9aa;margin:0.9rem 0 0;min-height:1.2em;", "{caption}" }
                p { style: "color:#666;margin-top:1.6rem;font-size:0.85rem;",
                    "Keyboard (focus the player): Space play/pause · ←/→ scrub · R restart. Build with "
                    code { style: "color:#888;", "dx serve" }
                    " (see README)."
                }
            }
        }
    }
}

fn main() {
    dioxus::launch(app);
}
