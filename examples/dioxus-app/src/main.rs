//! manim_rust Dioxus gallery: a scene picker driving `<ManimPlayer>`.
//!
//! Three text-free scenes (SquareToCircle, an axes plot, a vector field), each
//! with the built-in controls. See the README for build/run steps.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dioxus::prelude::*;
use glam::{DMat3, DVec3, Mat4, Quat, Vec3};
use manim_color::TEAL_D;
use manim_core::animations::{Create, FadeOut, TransformInto};
use manim_core::graphing::{Axes, NumberPlane};
use manim_core::mesh::{HeightField, Mesh, MeshMaterial, Surface3D};
use manim_core::mobject::{AnyId, MobjectId};
use manim_core::prelude::*;
use manim_core::vector_field::ArrowVectorField;
use manim_dioxus::constraint::{CurveRail, DragConstraint};
use manim_dioxus::exercise::{Exercise, ExerciseHandle, ExerciseState, ExerciseTarget};
use manim_dioxus::readout::{AngleMarker, CoordStyle, CoordinateReadout, DecimalReadout};
use manim_dioxus::{
    url, use_exercise, use_parameter, use_parameters, DragHandleLayer, Figure, LiveUpdater,
    ManimGpuProvider, ManimPlayer, OrbitControls, PanZoom, Parameters, ParametersProvider,
};
use manim_fields::complex::{Complex, Mobius};
use manim_fields::field::ComplexField;
use manim_fields::map::SpaceMap;
use manim_sci::complex_viz::RiemannSphere;
use manim_sci::deform::{ApplyMap, DeformationGrid};
use manim_sci::material_quad::MaterialQuad;
use manim_text::Text;

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

/// A depth-tested mesh scene: a shaded saddle plus a translucent sphere sinking
/// through it, under a turntable camera.
///
/// This is the browser end of the mesh pipeline (docs/design/12-mesh-pipeline.md):
/// `<ManimPlayer>` needed no changes for it. The player precomputes
/// `DisplayList`s — which carry the `meshes` channel alongside the 2-D draw
/// items — and hands them to `CanvasSurface`, whose `render`/`render_frame`
/// already run the depth-tested mesh pass before compositing vector content
/// over it. The whole path is WebGL2-clean: no compute shaders, no storage
/// buffers.
#[derive(Clone, PartialEq)]
struct MeshScene;
impl SceneBuilder for MeshScene {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(65_f32.to_radians(), -55_f32.to_radians());

        // A shaded saddle: real geometry, depth-tested and Blinn-Phong shaded.
        scene.add(
            Surface3D::new(
                |u, v| Vec3::new(u as f32, v as f32, 0.4 * (u * u - v * v) as f32),
                (-2.2, 2.2),
                (-2.2, 2.2),
            )
            .with_resolution(40, 40)
            .with_checkerboard(Some([BLUE, TEAL_D]))
            .with_material(MeshMaterial::default().with_lighting(0.26, 0.74, 0.35)),
        );

        // A translucent sphere straddling the saddle — it must be occluded by the
        // near lobe and show the far one through itself. That is exactly what the
        // painter's-algorithm path cannot do.
        let ball = scene.add(
            Mesh::sphere()
                .with_transform(Mat4::from_scale_rotation_translation(
                    Vec3::splat(0.9),
                    Quat::IDENTITY,
                    Vec3::new(0.0, 0.0, 1.4),
                ))
                .with_material(MeshMaterial::new(RED).with_opacity(0.55)),
        );

        // Sink the ball through the surface while the camera orbits.
        let steps = 60;
        for i in 0..steps {
            let z = 1.4 - 2.4 * (i as f32 / steps as f32);
            scene
                .state_mut()
                .get_mut(ball)
                .set_transform(Mat4::from_scale_rotation_translation(
                    Vec3::splat(0.9),
                    Quat::IDENTITY,
                    Vec3::new(0.0, 0.0, z),
                ));
            scene.rotate_camera(TAU / steps as f32);
            scene.wait(0.05);
        }
        Ok(())
    }
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

/// An empty host scene for the live 3-D orbit — everything (field, camera) is
/// built by [`orbit_updater`] on its first frame, like [`cursor_updater`]'s dot.
#[derive(Clone, PartialEq)]
struct LiveOrbitScene;
impl SceneBuilder for LiveOrbitScene {
    fn construct(&self, _scene: &mut Scene) -> Result<()> {
        Ok(())
    }
}

/// A live, interactive 3-D scene (FE-130 / GH #2): an evolving `HeightField`
/// wave rendered under the **live state's own camera**, orbitable by dragging.
///
/// Each frame the updater re-evaluates the wave (one height-texture upload —
/// the grid never re-tessellates) and, while a button is held, converts the
/// pointer drag into camera `(phi, theta)`. The player's live path follows the
/// scene camera exactly like timeline playback follows its per-frame cameras,
/// so this renders depth-tested 3-D with real relief.
fn orbit_updater() -> LiveUpdater {
    const N: usize = 96;
    const EXTENT: f32 = 3.0;
    let field: Rc<Cell<Option<MobjectId<HeightField>>>> = Rc::new(Cell::new(None));
    let angles = Rc::new(Cell::new((62_f32.to_radians(), -45_f32.to_radians())));
    let last_drag: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
    LiveUpdater::new(move |state, pointer, t| {
        let id = match field.get() {
            Some(id) => id,
            None => {
                let id = state.add(
                    HeightField::from_fn(N, N, (EXTENT, EXTENT), |_, _| 0.0)
                        .with_material(MeshMaterial::new(TEAL_D).with_lighting(0.28, 0.72, 0.45)),
                );
                field.set(Some(id));
                id
            }
        };
        // Drag to orbit: element-fraction deltas become camera angles. The
        // camera-independent `frac` is essential here — scene-space positions
        // re-map as the camera orbits, so differencing them oscillates.
        let (mut phi, mut theta) = angles.get();
        if pointer.pressed {
            if let Some((px, py)) = last_drag.get() {
                let (fx, fy) = pointer.frac;
                theta -= (fx - px) * 3.5;
                // Fraction y grows down; dragging up tilts toward top-down.
                phi = (phi + (py - fy) * 2.0).clamp(0.15, std::f32::consts::FRAC_PI_2);
            }
            last_drag.set(Some(pointer.frac));
            angles.set((phi, theta));
        } else {
            last_drag.set(None);
        }
        state.camera_mut().set_camera_orientation(phi, theta);
        // The evolving wave: one 96×96 R32Float texture write per frame.
        state.get_mut(id).update_heights(|x, y| {
            let r2 = x * x + y * y;
            0.5 * (-0.10 * r2).exp() * (2.0 * x - 2.2 * t).sin() * (1.8 * y).cos()
        });
    })
}

// ---------------------------------------------------------------------------
// Textbook-page figures (FE-138): static, render-on-demand `<Figure>`s. Each is
// a zero-/short-duration construction shown at its final frame; on a page a
// dozen of them share one GPU device (via `ManimGpuProvider`) and idle at ~0
// cost until scrolled into view.
// ---------------------------------------------------------------------------

/// Which analytic curve a [`CurveFig`] plots.
#[derive(Clone, Copy, PartialEq)]
enum Curve {
    Sine,
    Cosine,
    Parabola,
    Cubic,
    Gaussian,
    Damped,
}

/// A static figure: labeled axes with one plotted curve.
#[derive(Clone, PartialEq)]
struct CurveFig(Curve);
impl SceneBuilder for CurveFig {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let axes = Axes::new([-5.0, 5.0, 1.0], [-3.0, 3.0, 1.0]).with_stroke(WHITE, 2.0, 1.0);
        let a = scene.add(axes);
        let (f, color): (fn(f32) -> f32, Color) = match self.0 {
            Curve::Sine => (|x| 2.0 * x.sin(), YELLOW),
            Curve::Cosine => (|x| 2.0 * x.cos(), TEAL_D),
            Curve::Parabola => (|x| 0.4 * x * x - 2.0, GREEN),
            Curve::Cubic => (|x| 0.06 * x * x * x, RED),
            Curve::Gaussian => (|x| 2.5 * (-(x * x) / 2.0).exp(), BLUE),
            Curve::Damped => (|x| 2.4 * (-0.25 * x.abs()).exp() * (3.0 * x).sin(), PURPLE),
        };
        let curve = scene[a].plot(f, None).with_stroke(color, 3.5, 1.0);
        scene.add(curve);
        Ok(())
    }
}

/// A static composition of the three primitive shapes.
#[derive(Clone, PartialEq)]
struct GeomFig;
impl SceneBuilder for GeomFig {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.add(
            Circle::new()
                .with_scale(1.7)
                .with_stroke(WHITE, 3.0, 1.0)
                .with_fill(BLUE, 0.22),
        );
        scene.add(
            Square::new()
                .with_scale(1.1)
                .with_stroke(YELLOW, 3.0, 1.0)
                .with_shift(1.5 * LEFT),
        );
        scene.add(
            Triangle::new()
                .with_scale(1.2)
                .with_stroke(GREEN, 3.0, 1.0)
                .with_shift(1.5 * RIGHT),
        );
        Ok(())
    }
}

/// A static number-plane backdrop.
#[derive(Clone, PartialEq)]
struct PlaneFig;
impl SceneBuilder for PlaneFig {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.add(NumberPlane::new([-7.0, 7.0, 1.0], [-4.0, 4.0, 1.0]));
        Ok(())
    }
}

/// A static figure: concentric circles in a warm-to-cool ramp.
#[derive(Clone, PartialEq)]
struct NestedFig;
impl SceneBuilder for NestedFig {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let ramp = [RED, ORANGE, YELLOW, GREEN, BLUE, PURPLE];
        for (i, c) in ramp.iter().enumerate() {
            let r = 0.5 + i as f32 * 0.45;
            scene.add(Circle::new().with_scale(r).with_stroke(*c, 3.0, 1.0));
        }
        Ok(())
    }
}

/// The twelve textbook figures, in reading order: `(scene, caption)`. Boxed as
/// trait objects so a single loop can lay them all out; each is `'static` and
/// its own `SceneBuilder`.
fn textbook_figures() -> Vec<(Element, &'static str)> {
    // Each entry renders a `<Figure>` (static, lazy, shared-device) plus a
    // caption. Each fills its column and takes its height from the scene aspect,
    // so a dozen fit a scrollable column and none distort on a narrow screen.
    fn fig<S: SceneBuilder + Clone + PartialEq + 'static>(scene: S) -> Element {
        rsx! {
            Figure {
                scene,
                width: "100%",
            }
        }
    }
    vec![
        (fig(CurveFig(Curve::Sine)), "Fig 1. y = 2 sin x"),
        (fig(CurveFig(Curve::Cosine)), "Fig 2. y = 2 cos x"),
        (fig(CurveFig(Curve::Parabola)), "Fig 3. y = 0.4x² − 2"),
        (fig(CurveFig(Curve::Cubic)), "Fig 4. y = 0.06x³"),
        (fig(CurveFig(Curve::Gaussian)), "Fig 5. Gaussian e^(−x²/2)"),
        (fig(CurveFig(Curve::Damped)), "Fig 6. Damped sinusoid"),
        (fig(GeomFig), "Fig 7. Primitive shapes"),
        (fig(NestedFig), "Fig 8. Concentric circles"),
        (fig(PlaneFig), "Fig 9. The number plane"),
        (fig(FieldScene), "Fig 10. Rotational field (−y, x)"),
        (fig(ZoomScene), "Fig 11. Magnified inset"),
        (fig(MeshScene), "Fig 12. Depth-tested saddle (final frame)"),
    ]
}

/// The textbook-page route: a scrollable column of a dozen render-on-demand
/// [`Figure`]s, all sharing one GPU device via [`ManimGpuProvider`].
///
/// This is the FE-138 acceptance surface. The single-device guarantee is
/// structural: `ManimGpuProvider` requests exactly one `SharedGpu`, and every
/// descendant `Figure` builds its canvas with `CanvasSurface::with_shared`
/// against a clone of that one reference-counted device/queue — never its own
/// `request_device`. The idle-cost guarantee is the `RenderSchedule` state
/// machine (unit-tested in `manim-dioxus`): each figure draws once when it
/// scrolls into view, then parks — an on-screen-but-idle page renders zero
/// frames until something marks a figure dirty.
#[component]
fn TextbookPage() -> Element {
    let figures = textbook_figures();
    rsx! {
        ManimGpuProvider {
            div { style: "columns:2 320px;column-gap:1rem;",
                for (fig , caption) in figures.into_iter() {
                    div { style: "break-inside:avoid;margin:0 0 1rem;border:1px solid #2a2a2a;border-radius:10px;overflow:hidden;background:#000;",
                        {fig}
                        p { style: "margin:0;padding:8px 10px;color:#9aa;font-size:0.82rem;background:#181818;",
                            "{caption}"
                        }
                    }
                }
            }
        }
    }
}

/// Which scene the gallery is showing.
#[derive(Clone, Copy, PartialEq)]
enum Which {
    Square,
    Plot,
    Field,
    Mesh3D,
    Orbit,
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
        Which::Mesh3D,
        "3D mesh",
        "Depth-tested meshes: a shaded saddle with a translucent sphere sinking through it.",
    ),
    (
        Which::Orbit,
        "Live 3D (drag)",
        "A live HeightField wave under the live camera — drag to orbit it (FE-130).",
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

// ---------------------------------------------------------------------------
// Visual Complex Analysis vertical slice (FE-140): three render-on-demand
// figures sharing one GPU device — the S3 exit criterion.
// ---------------------------------------------------------------------------

/// The square domain (scene units) all three VCA figures live over.
const VCA_DOMAIN: [f64; 2] = [-2.5, 2.5];
/// Full-resolution field sampling (on settle); [`VCA_DRAG_RES`] while dragging.
const VCA_HI_RES: usize = 256;
/// Reduced sampling while a handle is being dragged (kept the frame budget).
const VCA_DRAG_RES: usize = 128;

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
        let mut den = Complex::new(1.0, 0.0);
        for p in &ps {
            den = den * (w - *p);
        }
        num / den
    })
}

/// A Möbius transform `w = (az+b)/(cz+d)` as a [`SpaceMap`] of the plane, with an
/// exact conformal Jacobian `w′ = (ad−bc)/(cz+d)²`.
fn mobius_map(m: Mobius) -> SpaceMap {
    let det = m.a * m.d - m.b * m.c;
    SpaceMap::from_parts(
        move |p| {
            let w = m.apply(Complex::new(p.x, p.y));
            DVec3::new(w.re, w.im, p.z)
        },
        move |p| {
            let denom = m.c * Complex::new(p.x, p.y) + m.d;
            let wp = det / (denom * denom); // complex derivative
                                            // Holomorphic ⇒ conformal Jacobian; z passes through.
            DMat3::from_cols(
                DVec3::new(wp.re, wp.im, 0.0),
                DVec3::new(-wp.im, wp.re, 0.0),
                DVec3::new(0.0, 0.0, 1.0),
            )
        },
    )
}

/// An empty host scene for Fig 1 — the quad and drag handles are built live.
#[derive(Clone, PartialEq)]
struct VcaPlaneScene;
impl SceneBuilder for VcaPlaneScene {
    fn construct(&self, _scene: &mut Scene) -> Result<()> {
        Ok(())
    }
}

/// The fragment key Fig 1's state is shared under (FE-147): `#vca=phase:…,z0:…`.
const VCA_URL_KEY: &str = "vca";

/// Fig 1's starting handle layout: two zeros (teal), two poles (red).
const VCA_START: [Point; 4] = [
    Point::new(-1.0, 0.6, 0.0),
    Point::new(1.0, -0.5, 0.0),
    Point::new(0.4, 1.1, 0.0),
    Point::new(-0.7, -1.0, 0.0),
];

/// Fig 1's exercise: both zeros on the unit circle (the poles are free).
fn zeros_on_unit_circle(s: &ExerciseState) -> bool {
    [s.handle(0), s.handle(1)]
        .iter()
        .all(|h| h.is_some_and(|p| ((p.x * p.x + p.y * p.y).sqrt() - 1.0).abs() < 0.06))
}

/// The live updater for Fig 1: a domain-coloring quad under four drag handles (2
/// zeros, 2 poles). Dragging a handle rebuilds the rational field and resamples
/// the quad — at reduced resolution while dragging, full resolution on settle.
/// A `phase` parameter (slider) rotates the coloring.
///
/// It also carries the FE-144/147 wiring: a [`PanZoom`] consumes the frame's
/// pinch / ctrl+wheel / middle-drag gesture before anything else, the handle
/// layout is restored from (and written back to) the URL fragment on settle, and
/// the enclosing [`Exercise`] is reported to each frame.
fn vca_plane_updater(params: Parameters, exercise: Option<ExerciseHandle>) -> LiveUpdater {
    // Restore a shared link's state at build time; fall back to the default
    // layout for anything the fragment does not carry.
    let saved = url::read_figure(VCA_URL_KEY);
    let mut start = VCA_START.to_vec();
    if let Some(f) = &saved {
        for (i, p) in f.points("z").into_iter().enumerate().take(start.len()) {
            start[i] = p;
        }
        if let Some(phase) = f.scalar("phase") {
            params.set("phase", phase);
        }
    }

    let handles = Rc::new(RefCell::new(DragHandleLayer::new(
        start,
        0.3,
        vec![TEAL_D, TEAL_D, RED, RED],
    )));
    let quad = Rc::new(Cell::new(None::<AnyId>));
    // (phase, resolution) last sampled — NaN forces the first sample.
    let last = Rc::new(Cell::new((f32::NAN, 0usize)));
    let panzoom = Rc::new(Cell::new(PanZoom::new().with_zoom_range(0.5, 8.0)));
    let was_dragging = Rc::new(Cell::new(false));
    let last_epoch = Rc::new(Cell::new(exercise.as_ref().map_or(0, |e| e.reset_epoch())));

    LiveUpdater::new(move |state, pointer, _t| {
        // 1. The view gesture first: a pinch/ctrl-wheel/middle-drag moves the
        //    camera, and never reaches the handles (the router reports the drag
        //    as released for the whole two-finger gesture).
        let mut pz = panzoom.get();
        pz.sync(state, pointer);
        panzoom.set(pz);

        let mut hl = handles.borrow_mut();

        // 2. An exercise reset puts the figure back where it started.
        if let Some(ex) = &exercise {
            if ex.reset_epoch() != last_epoch.get() {
                last_epoch.set(ex.reset_epoch());
                for (i, p) in VCA_START.iter().enumerate() {
                    hl.set_position(i, *p);
                }
                last.set((f32::NAN, 0));
                url::update(VCA_URL_KEY, |f| {
                    f.set_points("z", &VCA_START);
                });
            }
        }

        // Frame 1: create the quad *under* the handles.
        if quad.get().is_none() {
            let f = rational_field(
                &hl.positions()[0..2],
                &hl.positions()[2..4],
                params.get("phase"),
            );
            let id = MaterialQuad::domain_coloring(
                VCA_DOMAIN,
                VCA_DOMAIN,
                (VCA_DRAG_RES, VCA_DRAG_RES),
                &f,
            )
            .add_to(state);
            quad.set(Some(id.erase()));
        }

        let moved = hl.sync(state, pointer);
        let phase = params.get("phase");
        let res = if hl.is_dragging() {
            VCA_DRAG_RES
        } else {
            VCA_HI_RES
        };
        let (last_phase, last_res) = last.get();
        let phase_changed = phase != last_phase;
        if moved.is_some() || phase_changed || res != last_res {
            let f = rational_field(&hl.positions()[0..2], &hl.positions()[2..4], phase);
            let material =
                MaterialQuad::domain_coloring_material(VCA_DOMAIN, VCA_DOMAIN, (res, res), &f);
            if let Some(id) = quad.get() {
                MaterialQuad::resample(state, id, material);
            }
            last.set((phase, res));
        }

        // 3. Report to the enclosing exercise, if any (cheap: the machine only
        //    publishes on a transition).
        if let Some(ex) = &exercise {
            ex.report(
                &ExerciseState::new()
                    .with_handles(hl.positions().to_vec())
                    .with_param("phase", phase),
            );
        }

        // 4. Share the state — on *settle*, never per frame: a drag that just
        //    ended, or a slider move while nothing is being dragged.
        let dragging = hl.is_dragging();
        let settled = was_dragging.get() && !dragging;
        was_dragging.set(dragging);
        if settled || (phase_changed && !dragging && last_phase.is_finite()) {
            let positions = hl.positions().to_vec();
            url::update(VCA_URL_KEY, |f| {
                f.set_scalar("phase", phase);
                f.set_points("z", &positions);
            });
        }
    })
}

/// Fig 1 component: a phase slider ([`use_parameter`]) over the interactive
/// domain-coloring [`Figure`]. Both share the enclosing [`ParametersProvider`]'s
/// [`Parameters`], so the slider wakes the figure for one redraw.
#[component]
fn VcaPlaneFigure() -> Element {
    let params = use_parameters();
    // The phase slider; its value is read by the updater each frame.
    let (_phase, slider) = use_parameter("phase", [-PI, PI], 0.0);
    // Build the updater once so the handle/quad state persists across renders.
    let exercise = use_exercise();
    let updater = use_hook(|| vca_plane_updater(params.clone(), exercise));
    rsx! {
        div { style: "padding:8px 12px;background:#181818;", {slider} }
        Figure {
            scene: VcaPlaneScene,
            live: updater.clone(),
            width: "100%",
            lazy: false,
        }
    }
}

/// Fig 2 scene: a deformation grid animated through `z ↦ z²` then a Möbius map.
#[derive(Clone, PartialEq)]
struct VcaDeformScene;
impl SceneBuilder for VcaDeformScene {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let grid = DeformationGrid::new([-3.0, 3.0], [-3.0, 3.0], 0.5)
            .with_ghost()
            .add_to(scene.state_mut());
        scene.play(ApplyMap::new(grid, &SpaceMap::complex_power(2)).run_time(2.5))?;
        scene.wait(0.5);
        let mobius = mobius_map(Mobius::new(
            Complex::new(1.0, 0.0),
            Complex::new(0.0, 0.45),
            Complex::new(0.35, 0.0),
            Complex::new(1.0, 0.0),
        ));
        scene.play(ApplyMap::new(grid, &mobius).run_time(2.5))?;
        scene.wait(0.5);
        Ok(())
    }
}

/// Fig 3 scene: the Riemann sphere with a stereographic grid (a plane grid
/// wrapped onto the sphere) — orbited by [`OrbitControls`].
#[derive(Clone, PartialEq)]
struct VcaSphereScene;
impl SceneBuilder for VcaSphereScene {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        RiemannSphere::add_to(scene.state_mut());
        // `stereographic()` maps the plane onto the sphere; `pre_deformed` draws
        // the grid already wrapped (a static stereographic net).
        DeformationGrid::new([-4.0, 4.0], [-4.0, 4.0], 0.5)
            .faded(0.85)
            .with_map(&RiemannSphere::stereographic())
            .pre_deformed()
            .add_to(scene.state_mut());
        Ok(())
    }
}

/// The Visual Complex Analysis route: three figures under one
/// [`ManimGpuProvider`], all render-on-demand.
#[component]
fn VcaPage() -> Element {
    let card = "border:1px solid #2a2a2a;border-radius:10px;overflow:hidden;background:#000;margin-bottom:1.3rem;";
    let cap = "margin:0;padding:8px 12px;color:#9aa;font-size:0.84rem;background:#181818;";
    rsx! {
        ManimGpuProvider {
            Exercise {
                prompt: "Place both zeros on the unit circle |z| = 1.",
                target: ExerciseTarget::new(zeros_on_unit_circle),
                hint: "Drag the two teal handles until each sits one unit from the origin. Pinch (or ctrl+scroll) to zoom in while you aim; the link in your address bar carries the layout, so you can share it.",
                hold: 2,
                ParametersProvider { VcaPlaneFigure {} }
                p { style: "{cap}", "Fig 1. Domain coloring of f(z) = Π(z−zᵢ)/Π(z−pⱼ). Drag the teal zeros and red poles; the phase slider rotates the hue. Pinch / ctrl+scroll to zoom, two-finger or middle-drag to pan. Resamples at 128² while dragging, 256² on release." }
            }
            div { style: "{card}",
                ManimPlayer { scene: VcaDeformScene, autoplay: true, loop_playback: true, controls: true, width: "100%" }
                p { style: "{cap}", "Fig 2. A conformal grid carried through z ↦ z², then a Möbius map — play or scrub the timeline." }
            }
            div { style: "{card}",
                Figure { scene: VcaSphereScene, live: OrbitControls::new().updater(), width: "100%", lazy: false }
                p { style: "{cap}", "Fig 3. The Riemann sphere with a stereographic grid. Drag to orbit, wheel to zoom." }
            }
            div { style: "{card}",
                ConstraintFigure {}
                p { style: "{cap}", "Fig 4. Constrained handles (FE-145): teal snaps to a ½-unit grid, yellow rides the unit circle (its angle read off a live marker), green slides along a horizontal rail, blue is clamped inside the box. Each label is a readout following its handle." }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Constrained handles + readouts (FE-145): every constraint kind on one figure,
// each labelled by the readout kit.
// ---------------------------------------------------------------------------

/// The backdrop for the constrained-handle demo: a number plane, the unit circle
/// the angle handle rides, and the box the clamped handle is confined to.
#[derive(Clone, PartialEq)]
struct ConstraintScene;
impl SceneBuilder for ConstraintScene {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.add(NumberPlane::new([-6.0, 6.0, 1.0], [-3.5, 3.5, 1.0]));
        scene.add(Circle::new().with_stroke(WHITE, 2.0, 0.35));
        // The `Region` handle's box, drawn so the clamp is visible rather than
        // felt: an invisible wall reads as a bug.
        scene.add(
            Rectangle::with_size(2.0, 1.4)
                .with_move_to(Point::new(-3.6, 1.6, 0.0))
                .with_stroke(BLUE, 2.0, 0.6),
        );
        Ok(())
    }
}

/// The rail the angle handle is pinned to.
fn unit_circle_rail() -> CurveRail {
    CurveRail::new(|t: f32| Point::new(t.cos(), t.sin(), 0.0), 0.0, TAU)
}

/// The clamp box for the `Region` handle.
fn region_box() -> manim_core::mobject::BoundingBox {
    manim_core::mobject::BoundingBox::new(Point::new(-4.6, 0.9, 0.0), Point::new(-2.6, 2.3, 0.0))
}

/// The live updater for the constrained-handle figure: four handles, one per
/// [`DragConstraint`] kind, each with its own readout.
///
/// - **Grid** — snaps to a half-unit lattice, labelled with its coordinates.
/// - **Curve** — rides the unit circle; an [`AngleMarker`] measures it from the
///   +x axis and a [`DecimalReadout`] shows the angle in degrees.
/// - **Axis** — slides along a horizontal rail, labelled with its x only.
/// - **Region** — clamped inside the blue box, labelled with its coordinates.
fn constraint_updater() -> LiveUpdater {
    let handles = Rc::new(RefCell::new(
        DragHandleLayer::new(
            vec![
                Point::new(2.5, -1.5, 0.0), // grid-snapped
                Point::new(1.0, 0.0, 0.0),  // on the unit circle
                Point::new(3.0, 2.5, 0.0),  // on a horizontal rail
                Point::new(-3.6, 1.6, 0.0), // inside the box
            ],
            0.32,
            vec![TEAL_D, YELLOW, GREEN, BLUE],
        )
        .with_constraint(0, DragConstraint::Grid(0.5))
        .with_constraint(1, DragConstraint::Curve(unit_circle_rail()))
        .with_constraint(2, DragConstraint::Axis(RIGHT))
        .with_constraint(3, DragConstraint::Region(region_box())),
    ));
    // Readouts. The kit takes a text *builder*, so the typesetter dependency
    // lives here in the demo rather than in the component crate.
    let grid_label = Rc::new(RefCell::new(CoordinateReadout::<Text>::new(Point::new(
        0.0, 0.45, 0.0,
    ))));
    let angle_label = Rc::new(RefCell::new(
        DecimalReadout::<Text>::new(Point::new(1.6, 0.5, 0.0))
            .decimals(1)
            .suffix("°"),
    ));
    let rail_label = Rc::new(RefCell::new(
        DecimalReadout::<Text>::new(Point::new(0.0, 3.0, 0.0))
            .decimals(2)
            .prefix("x = "),
    ));
    let box_label = Rc::new(RefCell::new(
        CoordinateReadout::<Text>::new(Point::new(0.0, -0.5, 0.0))
            .with_format(|f| f.decimals(1).style(CoordStyle::Complex)),
    ));
    let marker = Rc::new(RefCell::new(AngleMarker::new(0.45, YELLOW)));
    let radius: Rc<Cell<Option<MobjectId<Line>>>> = Rc::new(Cell::new(None));

    let text = |s: &str| Text::new(s).font_size(22.0);

    LiveUpdater::new(move |state, pointer, _t| {
        let mut hl = handles.borrow_mut();
        hl.sync(state, pointer);
        let (p_grid, p_arc, p_rail, p_box) = (
            hl.position(0),
            hl.position(1),
            hl.position(2),
            hl.position(3),
        );

        // The angle handle's radius line, replaced in place (same arena slot,
        // bumped generation) so the tessellation cache invalidates exactly once.
        let fresh = Line::new(Point::ZERO, p_arc).with_stroke(YELLOW, 3.0, 0.9);
        match radius.get() {
            None => radius.set(Some(state.add(fresh))),
            Some(id) => {
                let generation = state.get(id).data().generation;
                let slot = state.get_mut(id);
                *slot = fresh;
                slot.data_mut().generation = generation + 1;
            }
        }
        let degrees = marker
            .borrow_mut()
            .sync(state, RIGHT, Point::ZERO, p_arc)
            .to_degrees();

        grid_label.borrow_mut().sync(state, p_grid, text);
        angle_label.borrow_mut().sync(state, degrees, text);
        rail_label
            .borrow_mut()
            .move_to(p_rail + Point::new(0.0, 0.45, 0.0));
        rail_label.borrow_mut().sync(state, p_rail.x, text);
        box_label.borrow_mut().sync(state, p_box, text);
    })
}

/// The constrained-handle + readout figure (FE-145) with its caption.
#[component]
fn ConstraintFigure() -> Element {
    let updater = use_hook(constraint_updater);
    rsx! {
        Figure {
            scene: ConstraintScene,
            live: updater.clone(),
            width: "100%",
            lazy: false,
        }
    }
}

/// Top-level view: the single-player gallery, the multi-figure textbook page, or
/// the Visual Complex Analysis slice.
#[derive(Clone, Copy, PartialEq)]
enum View {
    Gallery,
    Textbook,
    Vca,
}

/// The gallery: a scene picker driving the selected `<ManimPlayer>`.
fn app() -> Element {
    let mut view = use_signal(|| View::Gallery);
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
            div { style: if matches!(view(), View::Gallery) { "max-width:760px;margin:0 auto;" } else { "max-width:1040px;margin:0 auto;" },
                h1 { style: "margin:0 0 4px;font-size:1.6rem;", "manim_rust · Dioxus gallery" }
                // Top-level view switch: single-player gallery, the multi-figure
                // textbook page, or the Visual Complex Analysis slice.
                div { style: "display:flex;gap:8px;margin:0 0 1rem;flex-wrap:wrap;",
                    button {
                        style: if matches!(view(), View::Gallery) {
                            "padding:6px 14px;background:#7cd;color:#023;border:none;border-radius:6px;font-weight:600;cursor:pointer;"
                        } else {
                            "padding:6px 14px;background:#222;color:#bcd;border:1px solid #345;border-radius:6px;cursor:pointer;"
                        },
                        onclick: move |_| view.set(View::Gallery),
                        "Gallery"
                    }
                    button {
                        style: if matches!(view(), View::Textbook) {
                            "padding:6px 14px;background:#7cd;color:#023;border:none;border-radius:6px;font-weight:600;cursor:pointer;"
                        } else {
                            "padding:6px 14px;background:#222;color:#bcd;border:1px solid #345;border-radius:6px;cursor:pointer;"
                        },
                        onclick: move |_| view.set(View::Textbook),
                        "Textbook page"
                    }
                    button {
                        style: if matches!(view(), View::Vca) {
                            "padding:6px 14px;background:#7cd;color:#023;border:none;border-radius:6px;font-weight:600;cursor:pointer;"
                        } else {
                            "padding:6px 14px;background:#222;color:#bcd;border:1px solid #345;border-radius:6px;cursor:pointer;"
                        },
                        onclick: move |_| view.set(View::Vca),
                        "Visual Complex Analysis"
                    }
                }
                if matches!(view(), View::Vca) {
                    p { style: "margin:0 0 1.2rem;color:#9aa;",
                        "The v1 exit slice: three complex-analysis figures — an interactive domain-coloring plane, a conformal-map timeline, and the Riemann sphere — sharing one GPU device, all render-on-demand."
                    }
                    VcaPage {}
                } else if matches!(view(), View::Textbook) {
                    p { style: "margin:0 0 1.2rem;color:#9aa;",
                        "A dozen render-on-demand "
                        code { style: "color:#7cd;", "<Figure>" }
                        "s sharing one GPU device (FE-138). Each draws once when scrolled into view, then idles at ~0 cost."
                    }
                    TextbookPage {}
                } else {
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
                                ManimPlayer { scene: SquareToCircle, autoplay: true, loop_playback: true, controls: true, width: "100%" }
                            },
                            Which::Plot => rsx! {
                                ManimPlayer { scene: PlotScene, autoplay: true, loop_playback: true, controls: true, width: "100%" }
                            },
                            Which::Field => rsx! {
                                ManimPlayer { scene: FieldScene, autoplay: false, controls: true, width: "100%" }
                            },
                            Which::Mesh3D => rsx! {
                                ManimPlayer { scene: MeshScene, autoplay: true, loop_playback: true, controls: true, width: "100%" }
                            },
                            Which::Orbit => rsx! {
                                ManimPlayer { scene: LiveOrbitScene, live: orbit_updater(), autoplay: false, controls: false, width: "100%" }
                            },
                            Which::Cursor => rsx! {
                                ManimPlayer { scene: CursorScene, live: cursor_updater(), autoplay: false, controls: false, width: "100%" }
                            },
                            Which::Zoom => rsx! {
                                ManimPlayer { scene: ZoomScene, autoplay: true, loop_playback: true, controls: true, width: "100%" }
                            },
                        }
                    }
                    p { style: "color:#9aa;margin:0.9rem 0 0;min-height:1.2em;", "{caption}" }
                }
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

#[cfg(test)]
mod vca_timing {
    //! Native measurement of the Fig-1 domain-coloring resample cost — the
    //! browser can't be timed here, but the CPU field-sampling that a drag frame
    //! triggers is pure and runs natively. Run with `cargo test -- --nocapture`
    //! to see the per-frame numbers that justify the 128²-while-dragging drop.
    use super::*;
    use std::time::Instant;

    #[test]
    fn report_resample_timing() {
        let zeros = [Point::new(-1.0, 0.6, 0.0), Point::new(1.0, -0.5, 0.0)];
        let poles = [Point::new(0.4, 1.1, 0.0), Point::new(-0.7, -1.0, 0.0)];
        let f = rational_field(&zeros, &poles, 0.5);
        let iters = 30;
        for res in [VCA_DRAG_RES, VCA_HI_RES, 512] {
            let t = Instant::now();
            for _ in 0..iters {
                let m =
                    MaterialQuad::domain_coloring_material(VCA_DOMAIN, VCA_DOMAIN, (res, res), &f);
                std::hint::black_box(&m);
            }
            let per_ms = t.elapsed().as_secs_f64() * 1000.0 / iters as f64;
            println!(
                "VCA resample {res}²: {per_ms:.3} ms/frame ({} samples)",
                res * res
            );
            // Sanity ceiling so a pathological regression fails the suite.
            assert!(
                per_ms < 250.0,
                "resample {res}² far too slow: {per_ms:.1} ms"
            );
        }
    }

    /// Runs each VCA scene's `construct` headlessly — the browser is where a
    /// component actually mounts, so this is the only native guard on the
    /// `ApplyMap` timeline, the Möbius Jacobian, and the stereographic
    /// pre-deform building cleanly.
    #[test]
    fn vca_scenes_build() {
        let cfg = Config::low();

        // Fig 2: z↦z² then Möbius on a deformation grid — a real timeline.
        let deform = Scene::build(&VcaDeformScene, cfg.clone()).expect("deform scene builds");
        assert!(
            deform.total_duration() > 5.0,
            "two 2.5s plays + waits ⇒ >5s timeline"
        );

        // Fig 3: sphere + stereographic grid (pre-deformed, static).
        let sphere = Scene::build(&VcaSphereScene, cfg.clone()).expect("sphere scene builds");
        assert!(!sphere.state().display_list().is_empty());

        // Fig 1 host is empty (quad/handles are built live).
        Scene::build(&VcaPlaneScene, cfg).expect("plane host builds");
    }

    #[test]
    fn mobius_jacobian_matches_finite_difference() {
        // The analytic conformal Jacobian must agree with a numeric one.
        let m = mobius_map(Mobius::new(
            Complex::new(1.0, 0.2),
            Complex::new(0.0, 0.45),
            Complex::new(0.35, 0.1),
            Complex::new(1.0, 0.0),
        ));
        let p = DVec3::new(0.7, -0.4, 0.0);
        let j = m.jacobian(p);
        let h = 1e-6;
        let dfdx = (m.apply(p + DVec3::new(h, 0.0, 0.0)) - m.apply(p - DVec3::new(h, 0.0, 0.0)))
            / (2.0 * h);
        let dfdy = (m.apply(p + DVec3::new(0.0, h, 0.0)) - m.apply(p - DVec3::new(0.0, h, 0.0)))
            / (2.0 * h);
        assert!((j.col(0).truncate() - dfdx.truncate()).length() < 1e-4);
        assert!((j.col(1).truncate() - dfdy.truncate()).length() < 1e-4);
    }
}
