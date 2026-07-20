//! A one-file interactive textbook, built on `manim-book`.
//!
//! This is the template Tim writes real chapters in. The shape is:
//!
//! 1. **Scene structs** — ordinary `SceneBuilder`s (plus a `LiveUpdater` if the
//!    figure is interactive). This is the only part that is really *yours*.
//! 2. **The chapter** — rsx: `Chapter` → `Section` → `Prose` / `FigureBlock` /
//!    `Callout` / `MarginNote` / `Ref`. No counters, no ids, no CSS.
//! 3. **The book** — one `Book` with an `outline`; `Toc` and `ChapterNav` are
//!    derived from it.
//!
//! See README.md for how to add chapter three.

use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use dioxus::prelude::*;
use glam::{DMat3, DVec3};
use manim_book::{
    Book, Callout, CalloutKind, Chapter, ChapterEntry, ChapterNav, FigureBlock, MarginNote,
    MathBlock, Prose, Ref, Section, Toc,
};
use manim_color::TEAL_D;
use manim_core::mobject::AnyId;
use manim_core::prelude::*;
use manim_dioxus::{use_parameter, use_parameters, DragHandleLayer, LiveUpdater, Parameters};
use manim_fields::complex::Complex;
use manim_fields::field::ComplexField;
use manim_fields::map::SpaceMap;
use manim_sci::deform::DeformationGrid;
use manim_sci::material_quad::MaterialQuad;

// ---------------------------------------------------------------------------
// 1. Scene structs — the author's actual mathematics.
// ---------------------------------------------------------------------------

/// The square domain (scene units) both figures live over.
const DOMAIN: [f64; 2] = [-2.5, 2.5];
/// Full-resolution field sampling, used once a drag settles.
const HI_RES: usize = 256;
/// Reduced sampling while a handle is being dragged (keeps the frame budget).
const DRAG_RES: usize = 128;

/// `f(z) = e^{iφ} · Π(z − zᵢ) / Π(z − pⱼ)` from scene-space zero/pole handles.
fn rational_field(zeros: &[Point], poles: &[Point], phase: f32) -> ComplexField {
    let zs: Vec<Complex> = zeros
        .iter()
        .map(|p| Complex::new(p.x as f64, p.y as f64))
        .collect();
    let ps: Vec<Complex> = poles
        .iter()
        .map(|p| Complex::new(p.x as f64, p.y as f64))
        .collect();
    let rot = Complex::from_polar(1.0, phase as f64);
    ComplexField::new(move |w| {
        let mut num = rot;
        for z in &zs {
            num = num * (w - *z);
        }
        let mut den = Complex::one();
        for p in &ps {
            den = den * (w - *p);
        }
        num / den
    })
}

/// An empty host scene for Figure 1.1 — the quad and drag handles are built live.
#[derive(Clone, PartialEq)]
struct ColoringScene;
impl SceneBuilder for ColoringScene {
    fn construct(&self, _scene: &mut Scene) -> Result<()> {
        Ok(())
    }
}

/// Figure 1.1's live updater: a domain-coloring quad under four drag handles
/// (two teal zeros, two red poles). Dragging rebuilds the rational field and
/// resamples the quad — reduced resolution while dragging, full on release. The
/// `phase` parameter (a slider) rotates the colouring.
fn coloring_updater(params: Parameters) -> LiveUpdater {
    let handles = Rc::new(RefCell::new(DragHandleLayer::new(
        vec![
            Point::new(-1.0, 0.6, 0.0),  // zero 0
            Point::new(1.0, -0.5, 0.0),  // zero 1
            Point::new(0.4, 1.1, 0.0),   // pole 0
            Point::new(-0.7, -1.0, 0.0), // pole 1
        ],
        0.3,
        vec![TEAL_D, TEAL_D, RED, RED],
    )));
    let quad = Rc::new(Cell::new(None::<AnyId>));
    // (phase, resolution) last sampled — NaN forces the first sample.
    let last = Rc::new(Cell::new((f32::NAN, 0usize)));
    LiveUpdater::new(move |state, pointer, _t| {
        let mut hl = handles.borrow_mut();

        // Frame 1: create the quad *under* the handles.
        if quad.get().is_none() {
            let f = rational_field(&hl.positions()[0..2], &hl.positions()[2..4], params.get("phase"));
            let id =
                MaterialQuad::domain_coloring(DOMAIN, DOMAIN, (DRAG_RES, DRAG_RES), &f).add_to(state);
            quad.set(Some(id.erase()));
        }

        let moved = hl.sync(state, pointer);
        let phase = params.get("phase");
        let res = if hl.is_dragging() { DRAG_RES } else { HI_RES };
        let (last_phase, last_res) = last.get();
        if moved.is_some() || phase != last_phase || res != last_res {
            let f = rational_field(&hl.positions()[0..2], &hl.positions()[2..4], phase);
            let material = MaterialQuad::domain_coloring_material(DOMAIN, DOMAIN, (res, res), &f);
            if let Some(id) = quad.get() {
                MaterialQuad::resample(state, id, material);
            }
            last.set((phase, res));
        }
    })
}

/// `z ↦ z^p` on the principal branch, with its exact conformal Jacobian
/// `w′ = p·z^{p−1}` (holomorphic ⇒ the Jacobian is a rotation-scaling).
fn power_map(p: f64) -> SpaceMap {
    SpaceMap::from_parts(
        move |q| {
            let w = Complex::new(q.x, q.y).powf(p);
            DVec3::new(w.re, w.im, q.z)
        },
        move |q| {
            let wp = Complex::new(q.x, q.y).powf(p - 1.0).scale(p);
            DMat3::from_cols(
                DVec3::new(wp.re, wp.im, 0.0),
                DVec3::new(-wp.im, wp.re, 0.0),
                DVec3::new(0.0, 0.0, 1.0),
            )
        },
    )
}

/// An empty host scene for Figure 1.2 — the grid is rebuilt live per exponent.
#[derive(Clone, PartialEq)]
struct ConformalScene;
impl SceneBuilder for ConformalScene {
    fn construct(&self, _scene: &mut Scene) -> Result<()> {
        Ok(())
    }
}

/// Figure 1.2's live updater: a Cartesian grid carried through `z ↦ z^p`, with
/// `p` driven by the `exponent` slider. The grid is rebuilt (not animated) on
/// each change, so the slider scrubs the deformation continuously.
fn conformal_updater(params: Parameters) -> LiveUpdater {
    let grid = Rc::new(Cell::new(None::<AnyId>));
    let last = Rc::new(Cell::new(f32::NAN));
    LiveUpdater::new(move |state, _pointer, _t| {
        let p = params.get_or("exponent", 1.0);
        if p == last.get() {
            return;
        }
        if let Some(old) = grid.get() {
            state.remove(old);
        }
        let id = DeformationGrid::new([-2.0, 2.0], [-2.0, 2.0], 0.25)
            .faded(0.9)
            .with_map(&power_map(p as f64))
            .pre_deformed()
            .add_to(state);
        grid.set(Some(id.erase()));
        last.set(p);
    })
}

// ---------------------------------------------------------------------------
// 2. Interactive figures: a slider plus the figure it drives. `use_parameter`
//    and `FigureBlock` share the `Book`'s parameter set, so a slider write wakes
//    exactly the figures that read it — no wiring.
// ---------------------------------------------------------------------------

/// Figure 1.1: the domain-coloring plane with a phase slider.
#[component]
fn ColoringFigure() -> Element {
    let params = use_parameters();
    let (_phase, slider) = use_parameter("phase", [-PI as f32, PI as f32], 0.0);
    let updater = use_hook(|| coloring_updater(params.clone()));
    rsx! {
        FigureBlock {
            scene: ColoringScene,
            live: updater.clone(),
            lazy: false,
            label: "coloring",
            caption: "Domain colouring of f(z) = Π(z−zᵢ)/Π(z−pⱼ). Hue is arg f, brightness is |f|. Drag the teal zeros and the red poles; the slider rotates the phase.",
        }
        div { style: "max-width:65ch;margin:0 auto 1.5rem;", {slider} }
    }
}

/// Figure 1.2: the conformal grid with an exponent slider.
#[component]
fn ConformalFigure() -> Element {
    let params = use_parameters();
    let (_p, slider) = use_parameter("exponent", [0.5, 3.0], 1.0);
    let updater = use_hook(|| conformal_updater(params.clone()));
    rsx! {
        FigureBlock {
            scene: ConformalScene,
            live: updater.clone(),
            lazy: false,
            label: "conformal",
            caption: "A Cartesian grid carried through z ↦ zᵖ. Scrub p from 1 (the identity) upward and watch the grid squares stay square — the angles never change, only the scale.",
        }
        div { style: "max-width:65ch;margin:0 auto 1.5rem;", {slider} }
    }
}

// ---------------------------------------------------------------------------
// 3. The chapters — prose interleaved with the figures above.
// ---------------------------------------------------------------------------

/// Chapter 1. The sample chapter: everything the scaffold offers, in one place.
#[component]
fn ChapterOne() -> Element {
    rsx! {
        Chapter { number: 1, title: "Complex Functions as Mappings",
            Prose {
                p {
                    "A real function is a graph: you draw the input on one axis, the output on "
                    "another, and the curve is the whole story. A complex function has no such "
                    "picture — its graph would live in four dimensions. So we stop trying to graph "
                    "it and start watching what it "
                    em { "does" }
                    " to the plane."
                }
                p {
                    "Every figure below is live. Drag things. The claims in the prose are the ones "
                    "you should be able to check with your hands."
                }
            }

            Section { title: "Domain colouring",
                Prose {
                    p {
                        "The first trick is to colour the plane by the output. Give each point z the "
                        "hue of arg f(z) and the brightness of |f(z)|. Zeros become dark points that "
                        "all hues run into; poles become bright ones that all hues run out of."
                    }
                }

                ColoringFigure {}

                MarginNote {
                    "The number of times the hue cycles as you walk a small loop is the order of the "
                    "zero or pole inside it — the argument principle, visible directly."
                }

                Prose {
                    p {
                        "Drag a zero in "
                        Ref { label: "coloring" }
                        " onto a pole. They annihilate: the dark point and the bright point cancel, "
                        "and the colour wheel around them flattens out. That is a factor cancelling "
                        "in the formula, and you just watched it happen."
                    }
                }

                Callout { kind: CalloutKind::Definition, title: "Rational map",
                    "A rational map is a quotient of polynomials, f(z) = P(z)/Q(z). Its zeros are the "
                    "roots of P and its poles are the roots of Q — which is why four draggable points "
                    "are enough to specify the whole picture above."
                }
            }

            Section { title: "Conformality",
                Prose {
                    p {
                        "Domain colouring shows you where a function sends things. It does not show "
                        "you the shape of the sending. For that, deform a grid: draw the image of "
                        "every grid line and watch what happens to the little squares."
                    }
                }

                MathBlock { source: "w = z^p,  w' = p z^(p-1)" }

                ConformalFigure {}

                Prose {
                    p {
                        "Compare the two pictures: "
                        Ref { label: "coloring" }
                        " tells you where each point lands, while "
                        Ref { label: "conformal" }
                        " tells you that the landing is a rotation and a stretch — never a shear. "
                        "The grid squares in the deformed picture are still squares, however "
                        "violently the grid as a whole is bent."
                    }
                }

                Callout { kind: CalloutKind::Theorem, title: "Holomorphic ⇒ conformal",
                    "Where f is holomorphic and f′(z) ≠ 0, f preserves angles. The derivative acts on "
                    "tangent vectors as multiplication by the single complex number f′(z) — a rotation "
                    "by arg f′ and a scaling by |f′|, identical in every direction. That uniformity "
                    "is exactly what you are seeing when the squares stay square."
                }

                Callout { kind: CalloutKind::Warning, title: "At a critical point",
                    "Conformality fails where f′(z) = 0. Push the exponent slider toward its ends and "
                    "look at the origin: angles there are multiplied, not preserved, and the grid "
                    "folds through itself."
                }
            }

            ChapterNav {}
        }
    }
}

/// Chapter 2. A stub — the second half of `ChapterNav`'s job.
#[component]
fn ChapterTwo() -> Element {
    rsx! {
        Chapter { number: 2, title: "The Riemann Sphere",
            Prose {
                p { "Coming soon: the point at infinity, stereographic projection, and Möbius maps as rigid motions of the sphere." }
            }
            ChapterNav {}
        }
    }
}

// ---------------------------------------------------------------------------
// 4. The book — outline plus chapters. This is the whole site.
// ---------------------------------------------------------------------------

/// The chapter outline. Both `Toc` and `ChapterNav` read it; adding a chapter
/// means adding one line here and one `Chapter` block.
fn outline() -> Vec<ChapterEntry> {
    vec![
        ChapterEntry::anchored(1, "Complex Functions as Mappings"),
        ChapterEntry::anchored(2, "The Riemann Sphere"),
    ]
}

fn app() -> Element {
    rsx! {
        Book { title: "An Interactive Textbook", outline: outline(),
            Toc {}
            ChapterOne {}
            ChapterTwo {}
        }
    }
}

fn main() {
    dioxus::launch(app);
}
