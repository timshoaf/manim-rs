//! Dioxus `<ManimPlayer>` component: manim_rust scenes as first-class web
//! components (FE-113).
//!
//! Give [`ManimPlayer`] a scene ([`SceneBuilder`](manim_core::scene::SceneBuilder)
//! that is also `Clone + PartialEq`) and it mounts a `<canvas>`, precomputes the
//! scene's frames, and plays them by wall clock through
//! [`CanvasSurface`](manim_render::CanvasSurface). Playback state lives in a
//! framework-independent [`PlayerState`] driven by a `requestAnimationFrame`
//! loop that runs *outside* the Dioxus virtual DOM; the component only touches
//! it through [`Signal`]s and the [`SceneController`] handle.
//!
//! ```no_run
//! use dioxus::prelude::*;
//! use manim_dioxus::ManimPlayer;
//! use manim_core::prelude::*;
//! use manim_core::animations::Create;
//!
//! #[derive(Clone, PartialEq)]
//! struct Demo;
//! impl SceneBuilder for Demo {
//!     fn construct(&self, scene: &mut Scene) -> Result<()> {
//!         let c = scene.add(Circle::new());
//!         scene.play(Create::new(c))?;
//!         Ok(())
//!     }
//! }
//!
//! fn app() -> Element {
//!     rsx! {
//!         ManimPlayer {
//!             scene: Demo,
//!             autoplay: true,
//!             controls: true,
//!             width: "640px",
//!             height: "360px",
//!         }
//!     }
//! }
//! ```
//!
//! # Design-doc divergences (dioxus 0.6 reality)
//!
//! - The scene prop is a **generic** `S: SceneBuilder + Clone + PartialEq`
//!   (dioxus props must be `Clone + PartialEq`), matching the doc's
//!   `scene: SquareToCircle` sketch — the user's scene struct derives those.
//! - Native (non-wasm) targets render a **placeholder** `<div>` so the workspace
//!   `cargo check` passes; real native/desktop rendering is FE-115.
//! - `on_pointer` interactivity and the `poster` prop from the design doc are
//!   deferred (documented inline).

#![allow(missing_docs)] // dioxus macro codegen; hand-written items are documented.

pub mod player;

pub use player::PlayerState;

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use manim_core::config::Config;
use manim_core::scene::{Scene, SceneBuilder};

/// A shared, framework-independent handle to a player's transport state.
///
/// Obtained inside a [`ManimPlayer`] subtree via [`use_scene_controller`]; the
/// `play`/`pause`/`seek`/`restart` methods drive the same [`PlayerState`] the
/// `requestAnimationFrame` loop reads, so custom UI stays in sync.
#[derive(Clone)]
pub struct SceneController {
    state: Rc<RefCell<PlayerState>>,
    playing: Signal<bool>,
    progress: Signal<f32>,
}

impl SceneController {
    /// Starts playback.
    pub fn play(&mut self) {
        self.state.borrow_mut().play();
        self.playing.set(true);
    }

    /// Pauses playback.
    pub fn pause(&mut self) {
        self.state.borrow_mut().pause();
        self.playing.set(false);
    }

    /// Toggles play/pause.
    pub fn toggle(&mut self) {
        let now = {
            let mut s = self.state.borrow_mut();
            s.toggle();
            s.is_playing()
        };
        self.playing.set(now);
    }

    /// Seeks to absolute time `t` seconds (clamped).
    pub fn seek(&mut self, t: f32) {
        let mut s = self.state.borrow_mut();
        s.seek(t);
        self.progress.set(s.progress());
    }

    /// Sets the playhead from a `[0, 1]` progress fraction (for a scrubber).
    pub fn set_progress(&mut self, fraction: f32) {
        let mut s = self.state.borrow_mut();
        s.set_progress(fraction);
        self.progress.set(s.progress());
    }

    /// Restarts from the beginning and plays.
    pub fn restart(&mut self) {
        self.state.borrow_mut().restart();
        self.playing.set(true);
    }

    /// Sets the playback rate (1.0 = normal).
    pub fn set_playback_rate(&mut self, rate: f32) {
        self.state.borrow_mut().set_playback_rate(rate);
    }

    /// The current progress `[0, 1]`.
    pub fn progress(&self) -> f32 {
        self.state.borrow().progress()
    }

    /// Whether playback is running.
    pub fn is_playing(&self) -> bool {
        self.state.borrow().is_playing()
    }
}

/// Returns the [`SceneController`] provided by the nearest ancestor
/// [`ManimPlayer`], for building custom playback UI.
///
/// # Panics
///
/// Panics if called outside a [`ManimPlayer`] subtree (no controller in context).
pub fn use_scene_controller() -> SceneController {
    use_context::<SceneController>()
}

/// The precomputed, immutable scene data shared with the render loop.
///
/// `cameras`/`config` are consumed only by the wasm render loop.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
struct SceneData {
    frames: Vec<manim_core::display::DisplayList>,
    cameras: Vec<manim_core::camera::CameraFrame>,
    total: f32,
    fps: u32,
    config: Config,
}

/// Builds the scene and samples its frames (CPU-only; no GPU needed).
fn build_scene_data<S: SceneBuilder>(builder: &S, config: Config) -> SceneData {
    let mut scene =
        Scene::build(builder, config.clone()).unwrap_or_else(|_| Scene::new(config.clone()));
    let mut frames = Vec::new();
    let mut cameras = Vec::new();
    let total = scene.total_duration();
    for frame in scene.frames_with_camera() {
        frames.push(frame.display_list);
        cameras.push(frame.camera);
    }
    SceneData {
        frames,
        cameras,
        total,
        fps: config.fps,
        config,
    }
}

/// The manim player component.
///
/// Props:
/// - `scene`: the [`SceneBuilder`] to play (also `Clone + PartialEq`).
/// - `config`: render [`Config`] (defaults to [`Config::low`]).
/// - `autoplay`: start playing on mount (default `true`).
/// - `loop_playback`: restart at the end instead of stopping (default `false`).
/// - `controls`: show the built-in play/pause + scrubber bar (default `false`).
/// - `width` / `height`: CSS sizing for the canvas (default `"640px"` /
///   `"360px"`).
#[allow(clippy::too_many_arguments)]
#[component]
pub fn ManimPlayer<S: SceneBuilder + Clone + PartialEq + 'static>(
    scene: S,
    #[props(default)] config: Option<Config>,
    #[props(default = true)] autoplay: bool,
    #[props(default = false)] loop_playback: bool,
    #[props(default = false)] controls: bool,
    #[props(default)] width: Option<String>,
    #[props(default)] height: Option<String>,
) -> Element {
    let config = config.unwrap_or_else(Config::low);
    let width = width.unwrap_or_else(|| "640px".to_string());
    let height = height.unwrap_or_else(|| "360px".to_string());

    // Build the scene + frames once (synchronous, CPU-only).
    let data: Rc<SceneData> = use_hook(|| Rc::new(build_scene_data(&scene, config.clone())));

    // Shared transport state, seeded from the sampled frames.
    let state: Rc<RefCell<PlayerState>> = use_hook(|| {
        Rc::new(RefCell::new(PlayerState::new(
            data.total,
            data.fps,
            data.frames.len(),
            autoplay,
            loop_playback,
        )))
    });

    let playing = use_signal(|| autoplay);
    let progress = use_signal(|| 0.0f32);

    // Publish a controller into context for `use_scene_controller`.
    let controller = SceneController {
        state: Rc::clone(&state),
        playing,
        progress,
    };
    use_context_provider(|| controller.clone());

    // Stable per-instance canvas id.
    let canvas_id = use_hook(next_canvas_id);

    // Boot the browser render loop after mount (client + wasm only). `use_effect`
    // runs post-commit, so the canvas element already exists; it reads no signals
    // here, so it runs once.
    #[cfg(target_arch = "wasm32")]
    {
        let data = Rc::clone(&data);
        let state = Rc::clone(&state);
        let id = canvas_id.clone();
        use_effect(move || {
            wasm::spawn_player(id.clone(), Rc::clone(&data), Rc::clone(&state), progress);
        });
    }
    // Silence unused warnings on native, where the loop is not spawned.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (&data, &state, &progress);
    }

    let style = format!("width:{width};height:{height};");
    rsx! {
        div { class: "manim-player", style: "{style}",
            canvas {
                id: "{canvas_id}",
                width: "{config.pixel_width}",
                height: "{config.pixel_height}",
                style: "width:100%;height:100%;display:block;background:#000;",
            }
            if controls {
                Controls {}
            }
        }
    }
}

/// The built-in controls bar: a play/pause button and a progress scrubber. Reads
/// the [`SceneController`] from context (provided by the parent [`ManimPlayer`]).
#[component]
fn Controls() -> Element {
    let ctrl = use_scene_controller();
    let mut ctrl_toggle = ctrl.clone();
    let mut ctrl_seek = ctrl.clone();
    // Reactive reads: re-render when the player publishes play/pause or progress.
    let playing = (ctrl.playing)();
    let progress = (ctrl.progress)();
    rsx! {
        div {
            class: "manim-controls",
            style: "display:flex;gap:8px;align-items:center;padding:6px 4px;font-family:system-ui;",
            button {
                style: "min-width:64px;padding:4px 8px;",
                onclick: move |_| ctrl_toggle.toggle(),
                if playing { "⏸ Pause" } else { "▶ Play" }
            }
            input {
                r#type: "range",
                min: "0",
                max: "1000",
                value: "{(progress * 1000.0) as i32}",
                style: "flex:1;",
                oninput: move |e| {
                    if let Ok(v) = e.value().parse::<f32>() {
                        ctrl_seek.set_progress(v / 1000.0);
                    }
                },
            }
        }
    }
}

/// A process-wide counter for unique canvas element ids.
fn next_canvas_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    format!("manim-canvas-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::cell::RefCell;
    use std::rc::Rc;

    use dioxus::prelude::Writable;
    use manim_render::CanvasSurface;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    use super::{PlayerState, SceneData};

    /// Builds the canvas surface and starts the `requestAnimationFrame` loop.
    pub(super) fn spawn_player(
        canvas_id: String,
        data: Rc<SceneData>,
        state: Rc<RefCell<PlayerState>>,
        mut progress: dioxus::prelude::Signal<f32>,
    ) {
        wasm_bindgen_futures::spawn_local(async move {
            let Some(canvas) = get_canvas(&canvas_id) else {
                return;
            };
            let surface = match CanvasSurface::new(canvas, &data.config).await {
                Ok(s) => Rc::new(RefCell::new(s)),
                Err(e) => {
                    web_sys::console::error_1(&format!("manim: surface init failed: {e}").into());
                    return;
                }
            };

            // Self-referential rAF closure, kept alive via Rc.
            let cb: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
            let cb2 = Rc::clone(&cb);
            let last = Rc::new(RefCell::new(None::<f64>));
            let mut last_pub = 0.0f64;
            *cb2.borrow_mut() = Some(Closure::wrap(Box::new(move |ts: f64| {
                let dt = match *last.borrow() {
                    Some(prev) => ((ts - prev) / 1000.0) as f32,
                    None => 0.0,
                };
                *last.borrow_mut() = Some(ts);

                let (idx, prog, playing) = {
                    let mut s = state.borrow_mut();
                    s.advance(dt);
                    (s.frame_index(), s.progress(), s.is_playing())
                };
                if let Some(list) = data.frames.get(idx) {
                    // CanvasSurface::render_frame follows the per-frame camera.
                    let frame = manim_core::scene::Frame {
                        t: 0.0,
                        display_list: list.clone(),
                        camera: data.cameras[idx],
                    };
                    let _ = surface.borrow_mut().render_frame(&frame);
                }
                // Throttle progress publishing to ~10 Hz to avoid re-render storms.
                if ts - last_pub > 100.0 {
                    last_pub = ts;
                    progress.set(prog);
                }
                let _ = playing;
                request_frame(cb.borrow().as_ref().unwrap());
            }) as Box<dyn FnMut(f64)>));
            request_frame(cb2.borrow().as_ref().unwrap());
        });
    }

    /// Looks up a canvas element by id.
    fn get_canvas(id: &str) -> Option<web_sys::HtmlCanvasElement> {
        web_sys::window()?
            .document()?
            .get_element_by_id(id)?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .ok()
    }

    /// Schedules `f` on the next animation frame.
    fn request_frame(f: &Closure<dyn FnMut(f64)>) {
        if let Some(win) = web_sys::window() {
            let _ = win.request_animation_frame(f.as_ref().unchecked_ref());
        }
    }
}
