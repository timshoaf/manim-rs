//! manim_rust Dioxus gallery: a scene picker driving `<ManimPlayer>`.
//!
//! Three text-free scenes (SquareToCircle, an axes plot, a vector field), each
//! with the built-in controls. See the README for build/run steps.

use std::cell::Cell;
use std::rc::Rc;

use dioxus::prelude::*;
use glam::{Mat4, Quat, Vec3};
use manim_color::TEAL_D;
use manim_core::animations::{Create, FadeOut, TransformInto};
use manim_core::graphing::{Axes, NumberPlane};
use manim_core::mesh::{HeightField, Mesh, MeshMaterial, Surface3D};
use manim_core::mobject::{AnyId, MobjectId};
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
    let last_drag: Rc<Cell<Option<Point>>> = Rc::new(Cell::new(None));
    LiveUpdater::new(move |state, pointer, t| {
        let id = match field.get() {
            Some(id) => id,
            None => {
                let id = state.add(
                    HeightField::from_fn(N, N, (EXTENT, EXTENT), |_, _| 0.0).with_material(
                        MeshMaterial::new(TEAL_D).with_lighting(0.28, 0.72, 0.45),
                    ),
                );
                field.set(Some(id));
                id
            }
        };
        // Drag to orbit: pointer deltas (scene units) become camera angles.
        let (mut phi, mut theta) = angles.get();
        if pointer.pressed {
            if let Some(prev) = last_drag.get() {
                theta -= (pointer.position.x - prev.x) * 0.25;
                phi = (phi + (pointer.position.y - prev.y) * 0.25)
                    .clamp(0.15, std::f32::consts::FRAC_PI_2);
            }
            last_drag.set(Some(pointer.position));
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
                        Which::Mesh3D => rsx! {
                            ManimPlayer { scene: MeshScene, autoplay: true, loop_playback: true, controls: true, width: "100%", height: "428px" }
                        },
                        Which::Orbit => rsx! {
                            ManimPlayer { scene: LiveOrbitScene, live: orbit_updater(), autoplay: false, controls: false, width: "100%", height: "428px" }
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
