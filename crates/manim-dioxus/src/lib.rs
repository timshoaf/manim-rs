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

pub mod interaction;
pub mod player;
pub mod schedule;

pub use interaction::{DragSet, OrbitState};
pub use player::PlayerState;
pub use schedule::RenderSchedule;

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
    /// Accumulated wheel delta (vertical, stripped of units) since the previous
    /// frame; consumed and reset each frame. Positive = scroll down. Used by
    /// [`OrbitControls`] for dolly zoom; `0.0` when no wheel event occurred.
    pub wheel: f32,
}

/// A per-frame scene mutator for live, input-driven scenes.
///
/// When a [`ManimPlayer`] is given a `live` updater, it stops playing precomputed
/// frames and instead, each animation frame, calls the updater with a mutable
/// live [`SceneState`], the current [`PointerState`], and the elapsed wall-clock
/// time, then renders the resulting display list **following the live state's
/// own camera** (FE-130): an updater that calls
/// `scene.camera_mut().set_camera_orientation(phi, theta)` — from a timer or a
/// pointer drag — renders real, depth-tested 3-D exactly like timeline playback
/// follows its per-frame camera. Background, zoom window, and zoom-adaptive
/// tessellation follow the live camera the same way.
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

/// A page-level shared GPU device for [`Figure`]s and [`ManimPlayer`]s (FE-138).
///
/// Requesting a wgpu device is the expensive part of surface creation and
/// browsers cap how many exist at once — a page with a dozen figures must not
/// spin up a dozen devices. Wrap the subtree in a [`ManimGpuProvider`]; it
/// requests **one** device asynchronously and hands it to every descendant
/// [`Figure`]/[`ManimPlayer`], each of which then creates its canvas surface
/// synchronously (via `CanvasSurface::with_shared`) against that single device.
///
/// The handle is cheap to clone. On native (non-wasm) targets it is an empty
/// placeholder so the workspace still type-checks.
#[derive(Clone)]
pub struct ManimGpu {
    #[cfg(target_arch = "wasm32")]
    slot: Rc<RefCell<Option<manim_render::SharedGpu>>>,
    #[cfg(not(target_arch = "wasm32"))]
    _priv: (),
}

impl ManimGpu {
    /// Starts requesting the shared device. On wasm this spawns the async
    /// adapter/device request immediately; [`ready`](Self::ready) reports `None`
    /// until it resolves, then the [`manim_render::SharedGpu`] every frame after.
    #[cfg(target_arch = "wasm32")]
    fn pending() -> Self {
        let slot: Rc<RefCell<Option<manim_render::SharedGpu>>> = Rc::new(RefCell::new(None));
        let sink = Rc::clone(&slot);
        wasm_bindgen_futures::spawn_local(async move {
            match manim_render::SharedGpu::new().await {
                Ok(gpu) => *sink.borrow_mut() = Some(gpu),
                Err(e) => {
                    web_sys::console::error_1(&format!("manim: shared gpu init failed: {e}").into())
                }
            }
        });
        Self { slot }
    }

    /// Native placeholder: no device is ever created.
    #[cfg(not(target_arch = "wasm32"))]
    fn pending() -> Self {
        Self { _priv: () }
    }

    /// The shared device once it has finished initializing, else `None`.
    ///
    /// A consumer's render loop polls this each frame and, on the first `Some`,
    /// builds its surface with `CanvasSurface::with_shared`.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn ready(&self) -> Option<manim_render::SharedGpu> {
        self.slot.borrow().clone()
    }
}

impl PartialEq for ManimGpu {
    // All clones of one provider's handle are equal (they share the same slot),
    // so a Figure taking `ManimGpu` as context never re-renders on gpu identity.
    #[cfg(target_arch = "wasm32")]
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.slot, &other.slot)
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// Provides one shared GPU device ([`ManimGpu`]) to its subtree.
///
/// Wrap a page of [`Figure`]s (or [`ManimPlayer`]s) in this once; every
/// descendant that finds a [`ManimGpu`] in context creates its surface against
/// the single shared device instead of requesting its own. Descendants without
/// a provider fall back to a private per-canvas device, so the provider is an
/// optimization, never a requirement.
#[component]
pub fn ManimGpuProvider(children: Element) -> Element {
    let gpu = use_hook(ManimGpu::pending);
    use_context_provider(|| gpu);
    rsx! { {children} }
}

/// Reads the page's shared [`ManimGpu`] from context, if a [`ManimGpuProvider`]
/// wraps this component. Returns `None` when there is no provider (the caller
/// then creates its own device). Only the wasm render loops consult it.
#[cfg(target_arch = "wasm32")]
fn shared_gpu_from_context() -> Option<ManimGpu> {
    try_consume_context::<ManimGpu>()
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
    /// Accumulated vertical wheel delta since the render loop last consumed it.
    wheel: f32,
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
            shared: shared_gpu_from_context(),
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

/// A handle that forces a [`Figure`] to redraw. Obtained inside a figure's
/// subtree via [`use_figure_controller`].
///
/// A figure renders on demand — once on mount, then only when woken. After you
/// change anything its scene depends on (a value the surrounding app owns),
/// call [`mark_dirty`](Self::mark_dirty) to schedule exactly **one** redraw,
/// restarting the frame loop even if it had parked. The controller wraps a pure
/// [`RenderSchedule`]; all the wake logic is thin glue over it.
#[derive(Clone)]
pub struct FigureController {
    /// The pure scheduler this controller drives.
    schedule: Rc<RefCell<RenderSchedule>>,
    /// Installed by the running frame loop: schedules a frame when the loop has
    /// parked. `None` until the loop starts (before mount / intersection).
    wake: WakeSlot,
}

/// A slot the frame loop fills with a "schedule another frame" closure; the
/// controller calls it to restart a parked loop.
type WakeSlot = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

impl FigureController {
    /// A fresh controller with a default-settle [`RenderSchedule`]. Create one
    /// when the surrounding app needs to drive a [`Figure`]'s redraws itself
    /// (e.g. a slider or a [`Parameters`] set does `mark_dirty`); pass it to the
    /// figure via its `controller` prop. A figure with no `controller` makes its
    /// own.
    pub fn new() -> Self {
        Self {
            schedule: Rc::new(RefCell::new(RenderSchedule::new())),
            wake: Rc::new(RefCell::new(None)),
        }
    }

    /// A controller whose schedule keeps drawing `settle` seconds after a pointer
    /// release (inertia). See [`RenderSchedule::with_settle`].
    pub fn with_settle(settle: f32) -> Self {
        Self {
            schedule: Rc::new(RefCell::new(RenderSchedule::new().with_settle(settle))),
            wake: Rc::new(RefCell::new(None)),
        }
    }

    /// Requests exactly one redraw, restarting the frame loop if it had idled.
    pub fn mark_dirty(&self) {
        self.schedule.borrow_mut().mark_dirty();
        self.wake();
    }

    /// Marks a pointer interaction active/finished (continuous redraw while
    /// active, then a settle window). Used by a live figure's DOM handlers.
    pub fn set_pointer_active(&self, active: bool) {
        self.schedule.borrow_mut().set_pointer_active(active);
        self.wake();
    }

    /// The scheduler this controller drives.
    pub fn schedule(&self) -> Rc<RefCell<RenderSchedule>> {
        Rc::clone(&self.schedule)
    }

    /// Kicks the frame loop if it installed a wake hook (i.e. is running).
    fn wake(&self) {
        if let Some(w) = self.wake.borrow().as_ref() {
            w();
        }
    }
}

impl PartialEq for FigureController {
    // Two handles are equal iff they drive the same schedule.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.schedule, &other.schedule)
    }
}

impl Default for FigureController {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for FigureController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FigureController(..)")
    }
}

/// A named set of scalar parameters shared between slider widgets and a
/// [`Figure`]'s live closure (FE-139).
///
/// Sliders (built with [`Parameters::slider`] or the [`use_parameter`] hook)
/// **write**; a figure's `live` closure **reads** the current values each frame.
/// A write wakes every figure that [registered](Parameters::register) — so
/// moving a slider redraws exactly the figures that depend on it, once, then they
/// idle again (the render-on-demand contract). Cheap to clone (all state is
/// reference-counted); clones share one value map + listener list.
#[derive(Clone, Default)]
pub struct Parameters {
    values: Rc<RefCell<std::collections::HashMap<String, f32>>>,
    listeners: Rc<RefCell<Vec<FigureController>>>,
}

impl Parameters {
    /// An empty parameter set.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current value of `name`, or `default` if unset.
    pub fn get_or(&self, name: &str, default: f32) -> f32 {
        self.values.borrow().get(name).copied().unwrap_or(default)
    }

    /// The current value of `name`, or `0.0` if unset.
    pub fn get(&self, name: &str) -> f32 {
        self.get_or(name, 0.0)
    }

    /// Sets `name` to `value` and wakes every registered figure (one redraw
    /// each). Seeds the value if new.
    pub fn set(&self, name: &str, value: f32) {
        self.values.borrow_mut().insert(name.to_string(), value);
        for c in self.listeners.borrow().iter() {
            c.mark_dirty();
        }
    }

    /// Seeds `name` with `initial` only if it has no value yet (idempotent mount
    /// seeding — does not wake listeners).
    pub fn seed(&self, name: &str, initial: f32) {
        self.values
            .borrow_mut()
            .entry(name.to_string())
            .or_insert(initial);
    }

    /// Registers a figure controller to be woken on every [`set`](Self::set).
    /// Idempotent per distinct controller.
    pub fn register(&self, controller: FigureController) {
        let mut ls = self.listeners.borrow_mut();
        if !ls.contains(&controller) {
            ls.push(controller);
        }
    }
}

impl PartialEq for Parameters {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.values, &other.values)
    }
}

impl std::fmt::Debug for Parameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parameters({} values)", self.values.borrow().len())
    }
}

impl Parameters {
    /// A labeled range-slider element bound to `name` over `[min, max]`, seeded to
    /// `initial`. Dragging it calls [`set`](Self::set) → wakes every registered
    /// figure. Uncontrolled (the browser owns the thumb), so no reactive context
    /// is needed — call it where you hold the `Parameters` directly. For a
    /// value-readout that updates as you drag, use the [`use_parameter`] hook.
    pub fn slider(&self, name: &str, label: &str, range: [f32; 2], initial: f32) -> Element {
        self.seed(name, initial);
        let params = self.clone();
        let key = name.to_string();
        let [min, max] = range;
        let step = (max - min) / 200.0;
        rsx! {
            label {
                style: "display:flex;align-items:center;gap:8px;font-size:0.82rem;color:#bcd;",
                span { style: "min-width:8ch;", "{label}" }
                input {
                    r#type: "range",
                    min: "{min}",
                    max: "{max}",
                    step: "{step}",
                    value: "{initial}",
                    style: "flex:1;accent-color:#7cd;",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<f32>() {
                            params.set(&key, v);
                        }
                    },
                }
            }
        }
    }
}

/// Provides a fresh [`Parameters`] set to its subtree (FE-139). Sliders
/// ([`use_parameter`]) and [`Figure`]s under it share one value map: a slider
/// write wakes the dependent figures. Wrap a control panel + its figures in one.
#[component]
pub fn ParametersProvider(children: Element) -> Element {
    use_context_provider(Parameters::new);
    rsx! { {children} }
}

/// Reads the enclosing [`ParametersProvider`]'s [`Parameters`] from context.
///
/// # Panics
///
/// Panics if called outside a [`ParametersProvider`].
pub fn use_parameters() -> Parameters {
    use_context::<Parameters>()
}

/// A two-way-bound range slider for a named parameter (FE-139), read from the
/// enclosing [`ParametersProvider`]'s [`Parameters`].
///
/// Returns the current value as a reactive [`Signal`] (so a readout re-renders as
/// you drag) and the slider [`Element`] to place in your layout. Dragging updates
/// both the signal and the shared parameter set — waking every [`Figure`] that
/// depends on it for one redraw.
///
/// # Panics
///
/// Panics if called outside a [`ParametersProvider`].
pub fn use_parameter(name: &str, range: [f32; 2], initial: f32) -> (Signal<f32>, Element) {
    let params = use_context::<Parameters>();
    params.seed(name, initial);
    let mut sig = use_signal(|| params.get_or(name, initial));
    let key = name.to_string();
    let p = params.clone();
    let [min, max] = range;
    let step = (max - min) / 200.0;
    let cur = sig();
    let el = rsx! {
        label {
            style: "display:flex;align-items:center;gap:8px;font-size:0.82rem;color:#bcd;",
            span { style: "min-width:8ch;", "{name}" }
            input {
                r#type: "range",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{cur}",
                style: "flex:1;accent-color:#7cd;",
                oninput: move |e| {
                    if let Ok(v) = e.value().parse::<f32>() {
                        sig.set(v);
                        p.set(&key, v);
                    }
                },
            }
            span { style: "min-width:5ch;text-align:right;color:#9aa;", "{cur:.2}" }
        }
    };
    (sig, el)
}

/// A turntable camera controller (FE-139): a drop-in [`LiveUpdater`] that orbits
/// a 3-D [`Figure`]'s camera by pointer drag and dollies it by wheel, with the
/// [`RenderSchedule`] settle window supplying release inertia.
///
/// Attach to any 3-D scene: the scene's `construct` adds the geometry, and this
/// updater drives only the camera each frame. Drag spins azimuth / tilts polar
/// angle; the wheel zooms.
///
/// ```no_run
/// use manim_dioxus::{Figure, OrbitControls};
/// # use dioxus::prelude::*;
/// # use manim_core::scene::{Scene, SceneBuilder}; use manim_core::error::Result;
/// # #[derive(Clone, PartialEq)] struct Ball;
/// # impl SceneBuilder for Ball { fn construct(&self, _: &mut Scene) -> Result<()> { Ok(()) } }
/// # fn app() -> Element {
/// rsx! { Figure { scene: Ball, live: OrbitControls::new().updater() } }
/// # }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct OrbitControls {
    phi: f32,
    theta: f32,
    sensitivity: f32,
    phi_range: (f32, f32),
    /// Scene-zoom applied per wheel pixel (delta is in CSS pixels, ~100/notch).
    wheel_rate: f32,
}

impl OrbitControls {
    /// A controller starting at a gentle three-quarter view, above the horizon.
    pub fn new() -> Self {
        Self {
            phi: 1.1,
            theta: -0.6,
            sensitivity: 0.25,
            phi_range: (0.05, std::f32::consts::FRAC_PI_2),
            wheel_rate: 0.01,
        }
    }

    /// Sets the initial polar/azimuth angles (radians).
    pub fn angles(mut self, phi: f32, theta: f32) -> Self {
        self.phi = phi;
        self.theta = theta;
        self
    }

    /// Sets the drag sensitivity (radians per scene unit).
    pub fn sensitivity(mut self, s: f32) -> Self {
        self.sensitivity = s;
        self
    }

    /// Allows the camera over the top: use `(0.05, π−0.05)` for a full orbit.
    pub fn phi_range(mut self, min: f32, max: f32) -> Self {
        self.phi_range = (min, max);
        self
    }

    /// Builds the [`LiveUpdater`] that drives the camera.
    pub fn updater(self) -> LiveUpdater {
        use manim_core::camera::ThreeDParams;
        let orbit = std::cell::Cell::new(
            OrbitState::new(self.phi, self.theta)
                .with_sensitivity(self.sensitivity)
                .with_phi_range(self.phi_range.0, self.phi_range.1),
        );
        let last = std::cell::Cell::new(None::<Point>);
        let wheel_rate = self.wheel_rate;
        LiveUpdater::new(move |state, pointer, _t| {
            let mut o = orbit.get();
            if pointer.pressed {
                if let Some(prev) = last.get() {
                    o.drag(pointer.position.x - prev.x, pointer.position.y - prev.y);
                }
                last.set(Some(pointer.position));
            } else {
                last.set(None);
            }
            if pointer.wheel != 0.0 {
                // Scroll up (negative deltaY) zooms in.
                o.zoom_by(-pointer.wheel * wheel_rate);
            }
            orbit.set(o);
            let base = state.camera().three_d.unwrap_or_default();
            state.camera_mut().set_three_d(ThreeDParams {
                phi: o.phi,
                theta: o.theta,
                zoom: o.zoom,
                gamma: base.gamma,
                focal_distance: base.focal_distance,
            });
        })
    }
}

impl Default for OrbitControls {
    fn default() -> Self {
        Self::new()
    }
}

/// A visual layer of draggable handles (a filled dot inside a faint halo) for a
/// live [`Figure`] (FE-139). Wraps a pure [`DragSet`]; each frame
/// [`sync`](Self::sync) drives
/// the drag state from the pointer, lazily creates the handle mobjects, moves
/// them, and highlights the hovered/grabbed one — returning which handle moved so
/// the caller can resample whatever depends on the positions.
///
/// Use it inside a [`LiveUpdater`]: hold one in the closure, call `sync` each
/// frame, and when it returns `Some`, rebuild your field / geometry from
/// [`positions`](Self::positions).
pub struct DragHandleLayer {
    drag: DragSet,
    palette: Vec<manim_core::prelude::Color>,
    hit_radius: f32,
    dot_ids: Vec<Option<manim_core::mobject::AnyId>>,
    halo_ids: Vec<Option<manim_core::mobject::AnyId>>,
}

impl DragHandleLayer {
    /// A layer of handles at `positions`, each grabbable within `hit_radius`
    /// scene units and drawn in the matching `palette` color (cycled if short).
    pub fn new(
        positions: Vec<Point>,
        hit_radius: f32,
        palette: Vec<manim_core::prelude::Color>,
    ) -> Self {
        let n = positions.len();
        Self {
            drag: DragSet::new(positions, hit_radius),
            palette,
            hit_radius,
            dot_ids: vec![None; n],
            halo_ids: vec![None; n],
        }
    }

    /// The current handle positions (scene space).
    pub fn positions(&self) -> &[Point] {
        self.drag.positions()
    }

    /// The position of handle `i`.
    pub fn position(&self, i: usize) -> Point {
        self.drag.position(i)
    }

    /// Whether a handle is being dragged this frame (for a cursor affordance).
    pub fn is_dragging(&self) -> bool {
        self.drag.is_dragging()
    }

    /// The color for handle `i` (cycles the palette).
    fn color(&self, i: usize) -> manim_core::prelude::Color {
        self.palette[i % self.palette.len().max(1)]
    }

    /// Advances one frame: drives the [`DragSet`] from `pointer`, draws/moves the
    /// handles, highlights the hovered one, and returns the moved handle (if any).
    pub fn sync(&mut self, state: &mut SceneState, pointer: &PointerState) -> Option<usize> {
        use manim_core::prelude::{Circle, WHITE};
        use manim_core::Buildable;

        let moved = self.drag.update(pointer.position, pointer.pressed);
        let n = self.dot_ids.len();
        for i in 0..n {
            let pos = self.drag.position(i);
            let col = self.color(i);
            if self.dot_ids[i].is_none() {
                let halo = state
                    .add(
                        Circle::new()
                            .with_scale(self.hit_radius)
                            .with_fill(col, 0.16),
                    )
                    .erase();
                let dot = state
                    .add(
                        Circle::new()
                            .with_scale(self.hit_radius * 0.42)
                            .with_fill(col, 1.0)
                            .with_stroke(WHITE, 2.0, 1.0),
                    )
                    .erase();
                self.halo_ids[i] = Some(halo);
                self.dot_ids[i] = Some(dot);
            }
            let halo = self.halo_ids[i].expect("halo created above");
            let dot = self.dot_ids[i].expect("dot created above");
            state.move_to(halo, pos);
            state.move_to(dot, pos);
            let hot = self.drag.hovered() == Some(i);
            state.set_style_family(halo, |s| {
                s.set_fill(col, if hot { 0.34 } else { 0.16 });
            });
            state.set_style_family(dot, |s| {
                s.set_fill(if hot { WHITE } else { col }, 1.0);
            });
        }
        moved
    }
}

/// Reads the [`FigureController`] for the enclosing [`Figure`] from context.
///
/// # Panics
///
/// Panics if called outside a [`Figure`] subtree.
pub fn use_figure_controller() -> FigureController {
    use_context::<FigureController>()
}

/// A render-on-demand scientific figure: a scene drawn into a `<canvas>` that
/// costs ~0 GPU time while idle.
///
/// Unlike [`ManimPlayer`] (which plays a timeline *every* frame), a `Figure`
/// renders its scene **once** when it scrolls into view, then parks its frame
/// loop until woken — by a pointer interaction (when `live` is set) or a
/// [`FigureController::mark_dirty`] call. A textbook page can hold dozens of
/// figures; only those on screen and actively interacting cost anything.
///
/// Wrap the page in a [`ManimGpuProvider`] so every figure shares one GPU device
/// (FE-138) instead of each requesting its own.
///
/// Props:
/// - `scene`: the [`SceneBuilder`] to draw (also `Clone + PartialEq`).
/// - `config`: render [`Config`] (defaults to [`Config::low`]).
/// - `time`: which moment of the scene to show, in seconds (default: the final
///   frame — a figure is usually the *result* of a construction).
/// - `live`: a [`LiveUpdater`] for cursor-driven figures (drag to explore); when
///   set, pointer activity wakes the loop, which settles after release.
/// - `width` / `height`: CSS sizing (default `"480px"` / `"320px"`).
/// - `lazy`: defer the first render until the figure scrolls into view (default
///   `true`); a placeholder shows until then.
/// - `settle`: seconds to keep drawing after a pointer release (default
///   [`DEFAULT_SETTLE_SECS`](schedule::DEFAULT_SETTLE_SECS)).
#[allow(clippy::too_many_arguments)]
#[component]
pub fn Figure<S: SceneBuilder + Clone + PartialEq + 'static>(
    scene: S,
    #[props(default)] config: Option<Config>,
    #[props(default)] time: Option<f32>,
    #[props(default)] live: Option<LiveUpdater>,
    #[props(default)] width: Option<String>,
    #[props(default)] height: Option<String>,
    #[props(default = true)] lazy: bool,
    #[props(default)] settle: Option<f32>,
    #[props(default)] controller: Option<FigureController>,
) -> Element {
    let config = config.unwrap_or_else(Config::low);
    let width = width.unwrap_or_else(|| "480px".to_string());
    let height = height.unwrap_or_else(|| "320px".to_string());

    // Build the scene + frames once (CPU-only). A static figure is usually a
    // zero-duration construction, so this is a single frame.
    let data: Rc<SceneData> = use_hook(|| Rc::new(build_scene_data(&scene, config.clone())));

    // The render-on-demand controller: use the one the caller passed (so an
    // external slider/`Parameters` can drive redraws), else make our own seeded
    // with the settle window. Published into context for `use_figure_controller`
    // and read by the frame loop.
    let controller = use_hook(|| {
        controller.clone().unwrap_or_else(|| match settle {
            Some(secs) => FigureController::with_settle(secs),
            None => FigureController::new(),
        })
    });
    use_context_provider(|| controller.clone());

    // If a `Parameters` set is in context, subscribe this figure so a slider
    // write wakes it for exactly one redraw.
    {
        let ctrl = controller.clone();
        use_hook(move || {
            if let Some(params) = try_consume_context::<Parameters>() {
                params.register(ctrl);
            }
        });
    }

    let raw_pointer: Rc<RefCell<RawPointer>> =
        use_hook(|| Rc::new(RefCell::new(RawPointer::default())));
    let first_frame = use_signal(|| false);
    let canvas_id = use_hook(next_canvas_id);

    // Boot the render-on-demand loop after mount (client + wasm only).
    #[cfg(target_arch = "wasm32")]
    {
        let boot = FigureBoot {
            data: Rc::clone(&data),
            controller: controller.clone(),
            raw_pointer: Rc::clone(&raw_pointer),
            first_frame,
            live: live.clone(),
            time,
            lazy,
            canvas_id: canvas_id.clone(),
            shared: shared_gpu_from_context(),
        };
        use_effect(move || {
            wasm::spawn_figure(boot.clone());
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (
            &data,
            &raw_pointer,
            &first_frame,
            &live,
            time,
            lazy,
            &controller,
        );
    }

    // Pointer handlers matter only for live figures: they wake the schedule so
    // the loop re-renders while the cursor drives the scene. A static figure
    // ignores the pointer (and never re-renders on hover).
    let interactive = live.is_some();
    let rp_move = Rc::clone(&raw_pointer);
    let rp_down = Rc::clone(&raw_pointer);
    let rp_up = Rc::clone(&raw_pointer);
    let rp_wheel = Rc::clone(&raw_pointer);
    let c_move = controller.clone();
    let c_down = controller.clone();
    let c_up = controller.clone();
    let c_wheel = controller.clone();

    let show_placeholder = !first_frame();
    let style = format!("position:relative;width:{width};height:{height};");
    rsx! {
        div {
            class: "manim-figure",
            style: "{style}",
            canvas {
                id: "{canvas_id}",
                width: "{config.pixel_width}",
                height: "{config.pixel_height}",
                style: "width:100%;height:100%;display:block;background:#000;touch-action:none;",
                onpointermove: move |e| {
                    if interactive {
                        let c = e.element_coordinates();
                        {
                            let mut p = rp_move.borrow_mut();
                            p.x = c.x as f32;
                            p.y = c.y as f32;
                        }
                        c_move.mark_dirty();
                    }
                },
                onpointerdown: move |e| {
                    if interactive {
                        let c = e.element_coordinates();
                        {
                            let mut p = rp_down.borrow_mut();
                            p.x = c.x as f32;
                            p.y = c.y as f32;
                            p.pressed = true;
                        }
                        c_down.set_pointer_active(true);
                    }
                },
                onpointerup: move |_| {
                    if interactive {
                        rp_up.borrow_mut().pressed = false;
                        c_up.set_pointer_active(false);
                    }
                },
                onwheel: move |e| {
                    if interactive {
                        e.prevent_default();
                        let dy = e.delta().strip_units().y as f32;
                        rp_wheel.borrow_mut().wheel += dy;
                        // A discrete zoom: one redraw applies it, then park.
                        c_wheel.mark_dirty();
                    }
                },
            }
            if show_placeholder {
                div {
                    class: "manim-figure-placeholder",
                    style: "position:absolute;inset:0;display:flex;align-items:center;justify-content:center;background:#000;color:#6b7280;font:14px system-ui,sans-serif;pointer-events:none;",
                    "Loading figure…"
                }
            }
        }
    }
}

/// The bundle handed to the wasm figure render loop.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone)]
struct FigureBoot {
    data: Rc<SceneData>,
    controller: FigureController,
    raw_pointer: Rc<RefCell<RawPointer>>,
    first_frame: Signal<bool>,
    live: Option<LiveUpdater>,
    time: Option<f32>,
    lazy: bool,
    canvas_id: String,
    shared: Option<ManimGpu>,
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
    /// The page's shared device, if a [`ManimGpuProvider`] is present. When set,
    /// the surface is built with `CanvasSurface::with_shared` once the device is
    /// ready; otherwise the loop requests a private per-canvas device.
    shared: Option<ManimGpu>,
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use dioxus::prelude::Writable;
    use manim_render::CanvasSurface;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    use super::{FigureBoot, PlayerBoot, PointerState, SceneData};

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
                shared,
            } = boot;

            let Some(canvas) = get_canvas(&canvas_id) else {
                return;
            };
            // Without a shared device, request a private one eagerly (async).
            // With one, defer surface creation to the loop and build it
            // synchronously (`with_shared`) on the first frame the device is
            // ready — no second `request_device`, no extra `await`.
            let surface: Rc<RefCell<Option<CanvasSurface>>> = if shared.is_some() {
                Rc::new(RefCell::new(None))
            } else {
                match CanvasSurface::new(canvas.clone(), &data.config).await {
                    Ok(s) => Rc::new(RefCell::new(Some(s))),
                    Err(e) => {
                        web_sys::console::error_1(
                            &format!("manim: surface init failed: {e}").into(),
                        );
                        return;
                    }
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

                // Lazily build the surface against the shared device on the
                // first frame it is ready. Until then, keep polling.
                if surface.borrow().is_none() {
                    if let Some(gpu) = shared.as_ref().and_then(|g| g.ready()) {
                        match CanvasSurface::with_shared(&gpu, canvas.clone(), &data.config) {
                            Ok(s) => *surface.borrow_mut() = Some(s),
                            Err(e) => web_sys::console::error_1(
                                &format!("manim: shared surface init failed: {e}").into(),
                            ),
                        }
                    }
                    if surface.borrow().is_none() {
                        request_frame(cb.borrow().as_ref().unwrap());
                        return;
                    }
                }

                // Convert the raw pointer (CSS px) to scene coordinates.
                let ptr = {
                    let raw = *raw_pointer.borrow();
                    let (cw, ch) = (canvas.client_width() as f32, canvas.client_height() as f32);
                    let position = surface
                        .borrow()
                        .as_ref()
                        .and_then(|s| s.client_to_scene(raw.x, raw.y, cw, ch))
                        .unwrap_or_default();
                    PointerState {
                        position,
                        pressed: raw.pressed,
                        wheel: 0.0,
                    }
                };

                let (idx, prog, playing) = {
                    let mut s = state.borrow_mut();
                    s.advance(dt);
                    (s.frame_index(), s.progress(), s.is_playing())
                };

                if let Some(updater) = &live {
                    // Live mode: mutate the scene from the cursor, then render
                    // it following the live state's own camera — so an updater
                    // driving `set_camera_orientation` (or a pointer-drag
                    // orbit) renders real 3-D, exactly like timeline playback
                    // follows its per-frame camera.
                    let mut sc = live_state.borrow_mut();
                    updater.0(&mut sc, &ptr, elapsed);
                    let frame = manim_core::scene::Frame {
                        t: 0.0,
                        display_list: sc.display_list(),
                        camera: manim_core::camera::CameraFrame::from(sc.camera()),
                    };
                    if let Some(s) = surface.borrow_mut().as_mut() {
                        let _ = s.render_frame(&frame);
                    }
                } else if let Some(list) = data.frames.get(idx) {
                    // Playback mode: draw the sampled frame, following its camera.
                    let frame = manim_core::scene::Frame {
                        t: 0.0,
                        display_list: list.clone(),
                        camera: data.cameras[idx],
                    };
                    if let Some(s) = surface.borrow_mut().as_mut() {
                        let _ = s.render_frame(&frame);
                    }
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

    /// Boots a [`Figure`](super::Figure): if `lazy`, waits for the canvas to
    /// scroll into view via an `IntersectionObserver`, then starts the loop;
    /// otherwise starts it immediately.
    pub(super) fn spawn_figure(boot: FigureBoot) {
        if boot.lazy {
            observe_then_start(boot);
        } else {
            start_figure_loop(boot);
        }
    }

    /// Watches the figure's canvas and starts its render loop the first time it
    /// intersects the viewport, then disconnects. Falls back to starting
    /// immediately if `IntersectionObserver` is unavailable.
    fn observe_then_start(boot: FigureBoot) {
        let Some(canvas) = get_canvas(&boot.canvas_id) else {
            return;
        };
        // One-shot cells: the callback takes the boot + observer exactly once.
        let boot_cell = Rc::new(RefCell::new(Some(boot)));
        let obs_cell: Rc<RefCell<Option<web_sys::IntersectionObserver>>> =
            Rc::new(RefCell::new(None));
        let boot_cb = Rc::clone(&boot_cell);
        let obs_cb = Rc::clone(&obs_cell);
        let cb = Closure::wrap(Box::new(
            move |entries: js_sys::Array, _obs: web_sys::IntersectionObserver| {
                let visible = entries.iter().any(|e| {
                    e.dyn_into::<web_sys::IntersectionObserverEntry>()
                        .map(|entry| entry.is_intersecting())
                        .unwrap_or(false)
                });
                if visible {
                    if let Some(o) = obs_cb.borrow_mut().take() {
                        o.disconnect();
                    }
                    if let Some(b) = boot_cb.borrow_mut().take() {
                        start_figure_loop(b);
                    }
                }
            },
        )
            as Box<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>);

        match web_sys::IntersectionObserver::new(cb.as_ref().unchecked_ref()) {
            Ok(observer) => {
                observer.observe(&canvas);
                *obs_cell.borrow_mut() = Some(observer);
                // Hand the closure to the browser for the observer's lifetime;
                // it is a one-shot page-lived figure, so leaking it is fine.
                cb.forget();
            }
            Err(_) => {
                if let Some(b) = boot_cell.borrow_mut().take() {
                    start_figure_loop(b);
                }
            }
        }
    }

    /// The frame index to show for `time` (default: the final frame).
    fn frame_index_for(data: &SceneData, time: Option<f32>) -> usize {
        if data.frames.is_empty() {
            return 0;
        }
        let last = data.frames.len() - 1;
        match time {
            Some(t) => {
                let i = (t * data.fps as f32).round();
                (i.max(0.0) as usize).min(last)
            }
            None => last,
        }
    }

    /// The render-on-demand loop: draws only when the [`RenderSchedule`] asks,
    /// and parks (stops scheduling frames) the moment it goes idle — so an
    /// off-screen or settled figure costs nothing.
    ///
    /// [`RenderSchedule`]: super::RenderSchedule
    fn start_figure_loop(boot: FigureBoot) {
        wasm_bindgen_futures::spawn_local(async move {
            let FigureBoot {
                data,
                controller,
                raw_pointer,
                mut first_frame,
                live,
                time,
                canvas_id,
                shared,
                ..
            } = boot;
            let Some(canvas) = get_canvas(&canvas_id) else {
                return;
            };
            let idx = frame_index_for(&data, time);

            // Shared device → build the surface synchronously in-loop once ready;
            // otherwise request a private device up front (async).
            let surface: Rc<RefCell<Option<CanvasSurface>>> = if shared.is_some() {
                Rc::new(RefCell::new(None))
            } else {
                match CanvasSurface::new(canvas.clone(), &data.config).await {
                    Ok(s) => Rc::new(RefCell::new(Some(s))),
                    Err(e) => {
                        web_sys::console::error_1(
                            &format!("manim: figure surface init failed: {e}").into(),
                        );
                        return;
                    }
                }
            };
            let live_state = Rc::new(RefCell::new(data.initial_state.clone()));
            let schedule = controller.schedule();

            type RafCell = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;
            let cb: RafCell = Rc::new(RefCell::new(None));
            let cb2 = Rc::clone(&cb);
            let start = Rc::new(RefCell::new(None::<f64>));
            // `parked` = the loop has stopped scheduling frames; the controller's
            // wake hook restarts it (and only then) on a dirty/pointer event.
            let parked = Rc::new(Cell::new(false));

            {
                let cb_wake = Rc::clone(&cb);
                let parked_wake = Rc::clone(&parked);
                let wake: Rc<dyn Fn()> = Rc::new(move || {
                    if parked_wake.replace(false) {
                        if let Some(c) = cb_wake.borrow().as_ref() {
                            request_frame(c);
                        }
                    }
                });
                *controller.wake.borrow_mut() = Some(wake);
            }

            *cb2.borrow_mut() = Some(Closure::wrap(Box::new(move |ts: f64| {
                let elapsed = {
                    let mut s = start.borrow_mut();
                    let s0 = *s.get_or_insert(ts);
                    ((ts - s0) / 1000.0) as f32
                };

                // Build the shared surface on the first frame the device is
                // ready; until then keep polling (never parks).
                if surface.borrow().is_none() {
                    if let Some(gpu) = shared.as_ref().and_then(|g| g.ready()) {
                        match CanvasSurface::with_shared(&gpu, canvas.clone(), &data.config) {
                            Ok(s) => *surface.borrow_mut() = Some(s),
                            Err(e) => web_sys::console::error_1(
                                &format!("manim: figure shared surface failed: {e}").into(),
                            ),
                        }
                    }
                    if surface.borrow().is_none() {
                        request_frame(cb.borrow().as_ref().unwrap());
                        return;
                    }
                }

                if schedule.borrow_mut().should_render(elapsed) {
                    let ptr = {
                        let (x, y, pressed, wheel) = {
                            let mut raw = raw_pointer.borrow_mut();
                            let wheel = std::mem::take(&mut raw.wheel);
                            (raw.x, raw.y, raw.pressed, wheel)
                        };
                        let (cw, ch) =
                            (canvas.client_width() as f32, canvas.client_height() as f32);
                        let position = surface
                            .borrow()
                            .as_ref()
                            .and_then(|s| s.client_to_scene(x, y, cw, ch))
                            .unwrap_or_default();
                        PointerState {
                            position,
                            pressed,
                            wheel,
                        }
                    };
                    if let Some(updater) = &live {
                        let mut sc = live_state.borrow_mut();
                        updater.0(&mut sc, &ptr, elapsed);
                        let frame = manim_core::scene::Frame {
                            t: 0.0,
                            display_list: sc.display_list(),
                            camera: manim_core::camera::CameraFrame::from(sc.camera()),
                        };
                        if let Some(s) = surface.borrow_mut().as_mut() {
                            let _ = s.render_frame(&frame);
                        }
                    } else if let Some(list) = data.frames.get(idx) {
                        let frame = manim_core::scene::Frame {
                            t: 0.0,
                            display_list: list.clone(),
                            camera: data.cameras[idx],
                        };
                        if let Some(s) = surface.borrow_mut().as_mut() {
                            let _ = s.render_frame(&frame);
                        }
                    }
                    if !first_frame() {
                        first_frame.set(true);
                    }
                }

                // Keep the loop alive only while the schedule wants frames.
                if schedule.borrow().wants_frame() {
                    request_frame(cb.borrow().as_ref().unwrap());
                } else {
                    parked.set(true);
                }
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

#[cfg(test)]
mod figure_tests {
    //! Native proofs of the [`Figure`] render-on-demand contract (FE-138): the
    //! browser loop is unverifiable here, but its whole policy is the pure
    //! [`RenderSchedule`] plus the [`FigureController`] wake plumbing, both of
    //! which run headlessly. These tests are the "idle-frame-counter" acceptance
    //! evidence for the textbook page.
    use super::*;
    use std::cell::Cell;

    fn controller() -> FigureController {
        FigureController {
            schedule: Rc::new(RefCell::new(RenderSchedule::new())),
            wake: Rc::new(RefCell::new(None)),
        }
    }

    /// Installs a wake hook that counts kicks (as the running loop does), then
    /// returns the counter.
    fn count_wakes(c: &FigureController) -> Rc<Cell<u32>> {
        let kicks = Rc::new(Cell::new(0));
        let k = Rc::clone(&kicks);
        *c.wake.borrow_mut() = Some(Rc::new(move || k.set(k.get() + 1)));
        kicks
    }

    #[test]
    fn mark_dirty_sets_schedule_dirty_and_wakes() {
        let c = controller();
        c.schedule().borrow_mut().should_render(0.0); // consume the mount frame
        assert!(
            !c.schedule().borrow().wants_frame(),
            "idle after mount frame"
        );

        let kicks = count_wakes(&c);
        c.mark_dirty();
        assert!(c.schedule().borrow().wants_frame(), "dirty → wants a frame");
        assert_eq!(
            kicks.get(),
            1,
            "mark_dirty kicks the parked loop exactly once"
        );
    }

    #[test]
    fn set_pointer_active_wakes_and_marks_active() {
        let c = controller();
        c.schedule().borrow_mut().should_render(0.0);
        let kicks = count_wakes(&c);
        c.set_pointer_active(true);
        assert!(c.schedule().borrow().is_pointer_active());
        assert_eq!(kicks.get(), 1);
    }

    #[test]
    fn wake_is_noop_without_a_running_loop() {
        // Before the loop installs its hook, mark_dirty must not panic; it just
        // flags the schedule so the loop draws when it starts.
        let c = controller();
        c.schedule().borrow_mut().should_render(0.0);
        c.mark_dirty();
        assert!(c.schedule().borrow().is_dirty());
    }

    /// Models the driver's park/wake loop over a schedule and counts the draws
    /// until it parks (`wants_frame` false). Mirrors `start_figure_loop`.
    fn run_until_idle(sched: &Rc<RefCell<RenderSchedule>>, mut t: f32) -> u32 {
        let mut draws = 0;
        while sched.borrow().wants_frame() {
            if sched.borrow_mut().should_render(t) {
                draws += 1;
            }
            t += 0.016;
        }
        draws
    }

    #[test]
    fn static_figure_draws_once_then_idles_forever() {
        let s = Rc::new(RefCell::new(RenderSchedule::new()));
        assert_eq!(run_until_idle(&s, 0.0), 1, "one mount draw");
        // Thousands of animation-frame ticks: never wakes, never draws.
        for i in 0..5000 {
            let t = 10.0 + i as f32 * 0.016;
            assert!(!s.borrow().wants_frame(), "woke while idle at tick {i}");
            assert!(
                !s.borrow_mut().should_render(t),
                "drew while idle at tick {i}"
            );
        }
    }

    #[test]
    fn page_of_twelve_figures_costs_twelve_draws_then_zero() {
        let figures: Vec<_> = (0..12)
            .map(|_| Rc::new(RefCell::new(RenderSchedule::new())))
            .collect();
        let mount_draws: u32 = figures.iter().map(|f| run_until_idle(f, 0.0)).sum();
        assert_eq!(mount_draws, 12, "12 figures → 12 mount draws");

        // The whole page is now idle: a full second of rAF ticks draws nothing.
        let mut idle_draws = 0;
        for tick in 0..60 {
            let t = 5.0 + tick as f32 * 0.016;
            for f in &figures {
                if f.borrow().wants_frame() && f.borrow_mut().should_render(t) {
                    idle_draws += 1;
                }
            }
        }
        assert_eq!(idle_draws, 0, "an idle textbook page renders zero frames");
    }

    #[test]
    fn waking_one_figure_redraws_only_it_once() {
        let figures: Vec<_> = (0..12)
            .map(|_| Rc::new(RefCell::new(RenderSchedule::new())))
            .collect();
        for f in &figures {
            run_until_idle(f, 0.0);
        }
        // A parameter change on figure 3: exactly one extra draw, only there.
        figures[3].borrow_mut().mark_dirty();
        let extra: u32 = figures.iter().map(|f| run_until_idle(f, 1.0)).sum();
        assert_eq!(extra, 1, "only the dirtied figure redraws, once");
    }

    #[test]
    fn parameter_round_trip_and_wake() {
        let params = Parameters::new();
        params.seed("k", 1.0);
        assert_eq!(params.get_or("k", 0.0), 1.0);
        assert_eq!(params.get("missing"), 0.0);

        // A figure registered on the set is woken (marked dirty) by a write.
        let ctrl = FigureController::new();
        ctrl.schedule().borrow_mut().should_render(0.0); // consume mount → idle
        assert!(!ctrl.schedule().borrow().is_dirty());
        params.register(ctrl.clone());
        params.set("k", 2.5);
        assert_eq!(params.get("k"), 2.5);
        assert!(
            ctrl.schedule().borrow().is_dirty(),
            "slider write wakes the registered figure"
        );

        // register is idempotent per controller.
        params.register(ctrl.clone());
        assert_eq!(params.listeners.borrow().len(), 1);
    }

    #[test]
    fn drag_handle_layer_creates_once_and_moves_grabbed() {
        use manim_core::prelude::{Color, Point, SceneState};
        let mut scene = SceneState::new();
        let mut layer = DragHandleLayer::new(
            vec![Point::new(0.0, 0.0, 0.0), Point::new(2.0, 0.0, 0.0)],
            0.4,
            vec![Color::from_rgba(1.0, 0.0, 0.0, 1.0)],
        );
        let ptr = |x: f32, y: f32, pressed: bool| PointerState {
            position: Point::new(x, y, 0.0),
            pressed,
            wheel: 0.0,
        };
        // First sync: hover only → lazily draws 2 handles (dot + halo each).
        assert_eq!(layer.sync(&mut scene, &ptr(5.0, 5.0, false)), None);
        let after_first = scene.display_list().len();
        assert!(after_first >= 4, "two dots + two halos drawn");
        // Second sync: no new mobjects (memoized).
        layer.sync(&mut scene, &ptr(5.0, 5.0, false));
        assert_eq!(scene.display_list().len(), after_first, "handles reused");
        // Grab handle 1 at its center, drag it: returns Some(1), position moves.
        layer.sync(&mut scene, &ptr(2.0, 0.0, true));
        assert_eq!(layer.sync(&mut scene, &ptr(2.0, 1.0, true)), Some(1));
        assert_eq!(layer.position(1), Point::new(2.0, 1.0, 0.0));
    }

    #[test]
    fn live_drag_draws_every_frame_then_settles_to_idle() {
        // A live figure driven by pointer: continuous draws while dragging, a
        // short settle after release, then park.
        let c = controller();
        let s = c.schedule();
        s.borrow_mut().should_render(0.0); // mount
        c.set_pointer_active(true);
        // 5 drag frames all draw.
        for i in 1..=5 {
            let t = i as f32 * 0.016;
            assert!(s.borrow_mut().should_render(t), "drag frame {i} draws");
            assert!(s.borrow().wants_frame());
        }
        c.set_pointer_active(false); // release at ~0.08 → settle window opens
        assert!(s.borrow_mut().should_render(0.1), "settle frame draws");
        // Past the settle window, it parks.
        assert!(!s.borrow_mut().should_render(1.0));
        assert!(!s.borrow().wants_frame(), "settled → idle");
    }
}
