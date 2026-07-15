//! Dioxus `<ManimPlayer>` component: manim_rust scenes as first-class web
//! components (FE-113 + FE-114 interactivity).
//!
//! Give [`ManimPlayer`] a scene (a `SceneBuilder` from `manim-core` that is
//! also `Clone + PartialEq`) and it mounts a `<canvas>`, precomputes the
//! scene's frames, and plays them by wall clock through `manim-render`'s
//! `CanvasSurface` (a wasm-only type, hence no doc link). Playback state lives in a
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
//! # Live interactivity (FE-114)
//!
//! Pointer input is published as a [`PointerState`] (cursor position in **scene**
//! coordinates + pressed flag), readable in the subtree via [`use_pointer`]. To
//! make a scene *react* to the cursor, pass a [`LiveUpdater`]: instead of playing
//! precomputed frames, the loop mutates a live [`SceneState`] each frame (given
//! the current pointer) and renders it. This keeps interactivity entirely on the
//! Dioxus/render side — the core timeline and updater system are untouched (see
//! the design note on [`LiveUpdater`]).
//!
//! # Design-doc divergences (dioxus 0.6 reality)
//!
//! - The scene prop is a **generic** `S: SceneBuilder + Clone + PartialEq`
//!   (dioxus props must be `Clone + PartialEq`), matching the doc's
//!   `scene: SquareToCircle` sketch — the user's scene struct derives those.
//! - Native (non-wasm) targets render a **placeholder** `<div>` so the workspace
//!   `cargo check` passes; real native/desktop rendering is FE-115.

#![allow(missing_docs)] // dioxus macro codegen; hand-written items are documented.

pub mod player;

pub use player::PlayerState;

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use manim_core::config::Config;
use manim_core::prelude::Point;
use manim_core::scene::{Scene, SceneBuilder};
use manim_core::scene_state::SceneState;

/// The pointer's state over a player, in **scene** coordinates.
///
/// `position` is the cursor mapped through the letterbox fit and camera
/// projection (see `CanvasSurface::client_to_scene` in `manim-render`,
/// a wasm-only item), so it is directly usable in a scene (move a mobject to
/// `pointer.position`).
/// Defaults to the origin, not pressed.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct PointerState {
    /// Cursor position in scene coordinates.
    pub position: Point,
    /// Whether a pointer button is currently held down over the canvas.
    pub pressed: bool,
}

/// A per-frame scene mutator for live, input-driven scenes.
///
/// When a [`ManimPlayer`] is given a `live` updater, it stops playing precomputed
/// frames and instead, each animation frame, calls the updater with a mutable
/// live [`SceneState`], the current [`PointerState`], and the elapsed wall-clock
/// time, then renders the resulting display list.
///
/// # Why this seam
///
/// Live input could instead be plumbed into the core updater system (an input
/// field on `UpdaterCtx`), but that couples core to a frontend concern and forces
/// every renderer to carry pointer state. Keeping the updater on the Dioxus side
/// — reading pointer state the controller already owns and mutating a
/// `SceneState` clone before render — keeps core input-agnostic at the cost of
/// live scenes not sharing the timeline/seek machinery (they are "now"-only,
/// which is exactly right for cursor-driven interaction).
#[derive(Clone)]
pub struct LiveUpdater(Rc<LiveFn>);

/// The per-frame mutator a [`LiveUpdater`] wraps: `(scene, pointer, elapsed)`.
type LiveFn = dyn Fn(&mut SceneState, &PointerState, f32);

impl LiveUpdater {
    /// Wraps a per-frame `(scene, pointer, time) -> ()` mutator.
    pub fn new(f: impl Fn(&mut SceneState, &PointerState, f32) + 'static) -> Self {
        Self(Rc::new(f))
    }
}

impl PartialEq for LiveUpdater {
    // Identity comparison: two handles are equal iff they wrap the same closure,
    // which is all dioxus needs to decide the prop is unchanged across renders.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for LiveUpdater {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LiveUpdater(..)")
    }
}

/// Raw pointer input in element (CSS) pixels, written by the DOM event handlers
/// and read by the render loop, which converts it to scene coordinates.
#[derive(Clone, Copy, Default)]
struct RawPointer {
    /// X offset from the canvas's top-left, in CSS pixels.
    x: f32,
    /// Y offset from the canvas's top-left, in CSS pixels.
    y: f32,
    /// Whether a button is currently pressed.
    pressed: bool,
}

/// A shared, framework-independent handle to a player's transport state.
///
/// Obtained inside a [`ManimPlayer`] subtree via [`use_scene_controller`]; the
/// `play`/`pause`/`seek`/`restart` methods drive the same [`PlayerState`] the
/// `requestAnimationFrame` loop reads, so custom UI stays in sync.
#[derive(Clone)]
pub struct SceneController {
    state: Rc<RefCell<PlayerState>>,
    sections: Rc<Vec<(String, f32)>>,
    playing: Signal<bool>,
    progress: Signal<f32>,
    rate: Signal<f32>,
    pointer: Signal<PointerState>,
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

    /// Sets the playback rate (1.0 = normal); also publishes it to the controls.
    pub fn set_playback_rate(&mut self, rate: f32) {
        self.state.borrow_mut().set_playback_rate(rate);
        self.rate.set(rate.max(0.0));
    }

    /// Jumps to the start of the named section (manim's `next_section`), if it
    /// exists. Names are matched exactly; unknown names are ignored.
    pub fn jump_to_section(&mut self, name: &str) {
        if let Some((_, start)) = self.sections.iter().find(|(n, _)| n == name) {
            self.seek(*start);
        }
    }

    /// The scene's sections as `(name, start_seconds)`, in order.
    pub fn sections(&self) -> Rc<Vec<(String, f32)>> {
        Rc::clone(&self.sections)
    }

    /// The current progress `[0, 1]`.
    pub fn progress(&self) -> f32 {
        self.state.borrow().progress()
    }

    /// The total scene duration in seconds.
    pub fn total(&self) -> f32 {
        self.state.borrow().total()
    }

    /// The current playback rate.
    pub fn playback_rate(&self) -> f32 {
        self.state.borrow().playback_rate()
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

/// Returns the live [`PointerState`] signal for the nearest ancestor
/// [`ManimPlayer`] — the cursor position in scene coordinates, updated each
/// frame. Read it reactively by calling it (`use_pointer()()`).
///
/// # Panics
///
/// Panics if called outside a [`ManimPlayer`] subtree.
pub fn use_pointer() -> Signal<PointerState> {
    use_context::<SceneController>().pointer
}

/// The precomputed, immutable scene data shared with the render loop.
///
/// `cameras`/`config`/`initial_state` are consumed only by the wasm render loop.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
struct SceneData {
    frames: Vec<manim_core::display::DisplayList>,
    cameras: Vec<manim_core::camera::CameraFrame>,
    initial_state: SceneState,
    sections: Vec<(String, f32)>,
    total: f32,
    fps: u32,
    config: Config,
}

/// Builds the scene and samples its frames (CPU-only; no GPU needed), also
/// capturing the final live state (for `LiveUpdater` scenes) and section marks.
fn build_scene_data<S: SceneBuilder>(builder: &S, config: Config) -> SceneData {
    let mut scene =
        Scene::build(builder, config.clone()).unwrap_or_else(|_| Scene::new(config.clone()));
    let total = scene.total_duration();
    let sections: Vec<(String, f32)> = scene
        .sections()
        .iter()
        .map(|s| (s.name.clone(), s.start))
        .collect();
    let initial_state = scene.state().clone();
    let mut frames = Vec::new();
    let mut cameras = Vec::new();
    for frame in scene.frames_with_camera() {
        frames.push(frame.display_list);
        cameras.push(frame.camera);
    }
    SceneData {
        frames,
        cameras,
        initial_state,
        sections,
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
/// - `controls`: show the built-in controls bar (default `false`).
/// - `width` / `height`: CSS sizing for the canvas (default `"640px"` /
///   `"360px"`).
/// - `poster`: image URL shown until the first rendered frame presents.
/// - `live`: a [`LiveUpdater`] for cursor-driven scenes (disables frame playback).
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
    #[props(default)] poster: Option<String>,
    #[props(default)] live: Option<LiveUpdater>,
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
    let rate = use_signal(|| 1.0f32);
    let pointer = use_signal(PointerState::default);
    let first_frame = use_signal(|| false);

    // Raw pointer input in CSS pixels, shared with the render loop.
    let raw_pointer: Rc<RefCell<RawPointer>> =
        use_hook(|| Rc::new(RefCell::new(RawPointer::default())));

    // Publish a controller into context for `use_scene_controller`/`use_pointer`.
    let controller = SceneController {
        state: Rc::clone(&state),
        sections: Rc::new(data.sections.clone()),
        playing,
        progress,
        rate,
        pointer,
    };
    use_context_provider(|| controller.clone());
    let mut kbd_ctrl = controller.clone();

    // Stable per-instance canvas id.
    let canvas_id = use_hook(next_canvas_id);

    // Boot the browser render loop after mount (client + wasm only). `use_effect`
    // runs post-commit, so the canvas element already exists; it reads no signals
    // here, so it runs once.
    #[cfg(target_arch = "wasm32")]
    {
        let boot = PlayerBoot {
            data: Rc::clone(&data),
            state: Rc::clone(&state),
            raw_pointer: Rc::clone(&raw_pointer),
            progress,
            pointer,
            first_frame,
            live: live.clone(),
            canvas_id: canvas_id.clone(),
        };
        use_effect(move || {
            wasm::spawn_player(boot.clone());
        });
    }
    // Silence unused warnings on native, where the loop is not spawned.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (&data, &state, &progress, &pointer, &first_frame, &live);
    }

    // Pointer handlers write raw element-space coordinates; the loop converts.
    let rp_move = Rc::clone(&raw_pointer);
    let rp_down = Rc::clone(&raw_pointer);
    let rp_up = Rc::clone(&raw_pointer);

    let show_poster = poster.is_some() && !first_frame();
    let style = format!("position:relative;width:{width};height:{height};");
    rsx! {
        div {
            class: "manim-player",
            style: "{style}",
            tabindex: "0",
            outline: "none",
            onkeydown: move |e| handle_key(&mut kbd_ctrl, e),
            canvas {
                id: "{canvas_id}",
                width: "{config.pixel_width}",
                height: "{config.pixel_height}",
                style: "width:100%;height:100%;display:block;background:#000;touch-action:none;",
                onpointermove: move |e| {
                    let c = e.element_coordinates();
                    let mut p = rp_move.borrow_mut();
                    p.x = c.x as f32;
                    p.y = c.y as f32;
                },
                onpointerdown: move |e| {
                    let c = e.element_coordinates();
                    let mut p = rp_down.borrow_mut();
                    p.x = c.x as f32;
                    p.y = c.y as f32;
                    p.pressed = true;
                },
                onpointerup: move |_| {
                    rp_up.borrow_mut().pressed = false;
                },
            }
            if show_poster {
                img {
                    src: poster.clone().unwrap_or_default(),
                    style: "position:absolute;inset:0;width:100%;height:100%;object-fit:contain;background:#000;pointer-events:none;",
                }
            }
            if controls {
                Controls {}
            }
        }
    }
}

/// Handles a keyboard event on the focused player, mirroring the native preview
/// bindings: Space toggles, ←/→ scrub, R restarts.
fn handle_key(ctrl: &mut SceneController, e: KeyboardEvent) {
    let step = (ctrl.total() * 0.02).max(0.05);
    match e.key() {
        Key::Character(c) if c == " " => {
            e.prevent_default();
            ctrl.toggle();
        }
        Key::ArrowLeft => {
            e.prevent_default();
            let t = ctrl.progress() * ctrl.total() - step;
            ctrl.seek(t.max(0.0));
        }
        Key::ArrowRight => {
            e.prevent_default();
            let t = ctrl.progress() * ctrl.total() + step;
            ctrl.seek(t);
        }
        Key::Character(c) if c.eq_ignore_ascii_case("r") => {
            ctrl.restart();
        }
        _ => {}
    }
}

/// The built-in controls bar: play/pause, a scrubber, a speed selector, an
/// optional section jump, and a `m:ss / m:ss` time readout. Reads the
/// [`SceneController`] from context (provided by the parent [`ManimPlayer`]).
#[component]
fn Controls() -> Element {
    let ctrl = use_scene_controller();
    let mut ctrl_toggle = ctrl.clone();
    let mut ctrl_seek = ctrl.clone();
    let mut ctrl_rate = ctrl.clone();
    let mut ctrl_section = ctrl.clone();
    // Reactive reads: re-render when the player publishes play/pause, progress,
    // or rate.
    let playing = (ctrl.playing)();
    let progress = (ctrl.progress)();
    let rate = (ctrl.rate)();
    let total = ctrl.total();
    let sections = ctrl.sections();
    let now = player::format_time(progress * total);
    let total_str = player::format_time(total);
    rsx! {
        div {
            class: "manim-controls",
            style: "display:flex;gap:8px;align-items:center;padding:6px 4px;font-family:system-ui;font-size:13px;",
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
            span {
                style: "font-variant-numeric:tabular-nums;white-space:nowrap;",
                "{now} / {total_str}"
            }
            select {
                title: "Playback speed",
                style: "padding:3px;",
                value: "{rate}",
                oninput: move |e| {
                    if let Ok(v) = e.value().parse::<f32>() {
                        ctrl_rate.set_playback_rate(v);
                    }
                },
                option { value: "0.25", "0.25×" }
                option { value: "0.5", "0.5×" }
                option { value: "1", "1×" }
                option { value: "2", "2×" }
                option { value: "4", "4×" }
            }
            if sections.len() > 1 {
                select {
                    title: "Jump to section",
                    style: "padding:3px;max-width:140px;",
                    oninput: move |e| ctrl_section.jump_to_section(&e.value()),
                    for (name , _) in sections.iter() {
                        option { value: "{name}", "{name}" }
                    }
                }
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

/// The bundle handed to the wasm render loop (keeps its arg list to one value).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone)]
struct PlayerBoot {
    data: Rc<SceneData>,
    state: Rc<RefCell<PlayerState>>,
    raw_pointer: Rc<RefCell<RawPointer>>,
    progress: Signal<f32>,
    pointer: Signal<PointerState>,
    first_frame: Signal<bool>,
    live: Option<LiveUpdater>,
    canvas_id: String,
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::cell::RefCell;
    use std::rc::Rc;

    use dioxus::prelude::Writable;
    use manim_render::CanvasSurface;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    use super::{PlayerBoot, PointerState};

    /// Builds the canvas surface and starts the `requestAnimationFrame` loop.
    pub(super) fn spawn_player(boot: PlayerBoot) {
        wasm_bindgen_futures::spawn_local(async move {
            let PlayerBoot {
                data,
                state,
                raw_pointer,
                mut progress,
                mut pointer,
                mut first_frame,
                live,
                canvas_id,
            } = boot;

            let Some(canvas) = get_canvas(&canvas_id) else {
                return;
            };
            let surface = match CanvasSurface::new(canvas.clone(), &data.config).await {
                Ok(s) => Rc::new(RefCell::new(s)),
                Err(e) => {
                    web_sys::console::error_1(&format!("manim: surface init failed: {e}").into());
                    return;
                }
            };
            // Live scenes mutate their own SceneState clone each frame.
            let live_state = Rc::new(RefCell::new(data.initial_state.clone()));

            // Self-referential rAF closure, kept alive via Rc.
            type RafCell = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;
            let cb: RafCell = Rc::new(RefCell::new(None));
            let cb2 = Rc::clone(&cb);
            let last = Rc::new(RefCell::new(None::<f64>));
            let start = Rc::new(RefCell::new(None::<f64>));
            let mut last_pub = 0.0f64;
            *cb2.borrow_mut() = Some(Closure::wrap(Box::new(move |ts: f64| {
                let dt = match *last.borrow() {
                    Some(prev) => ((ts - prev) / 1000.0) as f32,
                    None => 0.0,
                };
                *last.borrow_mut() = Some(ts);
                let elapsed = {
                    let mut s = start.borrow_mut();
                    let s0 = *s.get_or_insert(ts);
                    ((ts - s0) / 1000.0) as f32
                };

                // Convert the raw pointer (CSS px) to scene coordinates.
                let ptr = {
                    let raw = *raw_pointer.borrow();
                    let (cw, ch) = (canvas.client_width() as f32, canvas.client_height() as f32);
                    let position = surface
                        .borrow()
                        .client_to_scene(raw.x, raw.y, cw, ch)
                        .unwrap_or_default();
                    PointerState {
                        position,
                        pressed: raw.pressed,
                    }
                };

                let (idx, prog, playing) = {
                    let mut s = state.borrow_mut();
                    s.advance(dt);
                    (s.frame_index(), s.progress(), s.is_playing())
                };

                if let Some(updater) = &live {
                    // Live mode: mutate the scene from the cursor, then render it.
                    let mut sc = live_state.borrow_mut();
                    updater.0(&mut sc, &ptr, elapsed);
                    let _ = surface.borrow_mut().render(&sc.display_list());
                } else if let Some(list) = data.frames.get(idx) {
                    // Playback mode: draw the sampled frame, following its camera.
                    let frame = manim_core::scene::Frame {
                        t: 0.0,
                        display_list: list.clone(),
                        camera: data.cameras[idx],
                    };
                    let _ = surface.borrow_mut().render_frame(&frame);
                }
                if !first_frame() {
                    first_frame.set(true);
                }

                // Throttle signal publishing to ~10 Hz to avoid re-render storms.
                if ts - last_pub > 100.0 {
                    last_pub = ts;
                    progress.set(prog);
                    pointer.set(ptr);
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
