//! [`Scene`]: the user-facing builder over a [`SceneState`] and a [`Timeline`].
//!
//! A [`SceneBuilder::construct`] runs once, eagerly building the timeline (no
//! rendering), then playback consumes the timeline at leisure — the deliberate
//! decoupling from `docs/design/04-animation-system.md` that makes scrubbing and
//! re-rendering cheap.
//!
//! # The eager-apply contract
//!
//! `construct` code is sequential and imperative:
//!
//! ```
//! use manim_core::prelude::*;
//! use manim_core::animations::TransformInto;
//! let mut scene = Scene::new(Config::default());
//! let sq = scene.add(Square::new());
//! scene.play(TransformInto::new(sq, Circle::new())).unwrap();
//! // After play(), the live state already reflects the animation's END, so the
//! // rest of construct sees final positions (manim's semantics).
//! assert!((scene[sq].bounding_box().width() - 2.0).abs() < 0.1);
//! ```
//!
//! To make this correct *and* keep playback seekable, [`play`](Scene::play):
//! 1. snapshots the pre-segment state into the [`Timeline`],
//! 2. eagerly runs `begin → interpolate(1) → finish` on the live state,
//!
//! so construct sees the end state while the timeline stores the snapshot needed
//! to replay the segment. See [`Timeline`].

use std::ops::{Index, IndexMut};

use crate::animation::IntoAnimations;
use crate::animations::{AnimBuilder, Animate, CameraFrameHandle};
pub use crate::camera::{Camera2D, CameraFrame};
use crate::config::Config;
use crate::display::DisplayList;
use crate::error::{CoreError, Result};
use crate::mobject::{AnyId, Mobject, MobjectId};
use crate::scene_state::{SceneState, UpdaterCtx};
use crate::timeline::{Section, Timeline};
use manim_color::Color;
use manim_math::Point;

/// One sampled frame: the time, its display list, and the camera at that time.
///
/// Yielded by [`Scene::frames_with_camera`]; renderers consume this to follow
/// camera motion.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Absolute time in seconds.
    pub t: f32,
    /// The display list at time `t`.
    pub display_list: DisplayList,
    /// The camera state at time `t`.
    pub camera: CameraFrame,
}

/// A scene under construction: the live [`SceneState`], the [`Timeline`] being
/// built, and the [`Config`]. The camera lives inside the [`SceneState`], so it
/// is captured by timeline snapshots.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::{Create, FadeOut};
/// let mut scene = Scene::new(Config::default());
/// let c = scene.add(Circle::new());
/// scene.play(Create::new(c)).unwrap();
/// scene.wait(0.5);
/// scene.play(FadeOut::new(c)).unwrap();
/// assert!((scene.total_duration() - 2.5).abs() < 1e-6);
/// ```
pub struct Scene {
    state: SceneState,
    timeline: Timeline,
    config: Config,
}

impl Scene {
    /// A new, empty scene with the given `config`.
    pub fn new(config: Config) -> Self {
        let mut state = SceneState::new();
        state.set_camera(Camera2D::from_config(&config));
        Self {
            state,
            timeline: Timeline::new(),
            config,
        }
    }

    /// Builds a scene by running `builder`'s [`construct`](SceneBuilder::construct).
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_core::animations::TransformInto;
    /// use manim_core::error::Result;
    ///
    /// struct SquareToCircle;
    /// impl SceneBuilder for SquareToCircle {
    ///     fn construct(&self, scene: &mut Scene) -> Result<()> {
    ///         let sq = scene.add(Square::new().with_fill(BLUE, 0.7));
    ///         scene.play(TransformInto::new(sq, Circle::new().with_fill(RED, 0.7)))?;
    ///         scene.wait(1.0);
    ///         Ok(())
    ///     }
    /// }
    ///
    /// let scene = Scene::build(&SquareToCircle, Config::default()).unwrap();
    /// assert!((scene.total_duration() - 2.0).abs() < 1e-6);
    /// ```
    pub fn build(builder: &dyn SceneBuilder, config: Config) -> Result<Self> {
        let mut scene = Scene::new(config);
        builder.construct(&mut scene)?;
        Ok(scene)
    }

    /// Adds a mobject to the scene, returning a typed handle.
    pub fn add<M: Mobject>(&mut self, mobject: M) -> MobjectId<M> {
        self.state.add(mobject)
    }

    // --- Post-add ergonomics ---
    //
    // Thin, family-aware delegates so positioning/styling a mobject *after*
    // `add` reads `scene.shift(id, ..)` instead of `scene.state_mut().shift(..)`.
    // Each accepts `impl Into<AnyId>` and returns `&mut Self` for chaining.

    /// Shifts `id`'s family by `delta` (manim's `shift`).
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_math::RIGHT;
    /// let mut scene = Scene::new(Config::default());
    /// let sq = scene.add(Square::new());
    /// scene.shift(sq, 2.0 * RIGHT);
    /// assert!((scene[sq].get_center().x - 2.0).abs() < 1e-5);
    /// ```
    pub fn shift(&mut self, id: impl Into<AnyId>, delta: Point) -> &mut Self {
        self.state.shift(id, delta);
        self
    }

    /// Moves `id`'s family so its center lands on `target` (manim's `move_to`).
    pub fn move_to(&mut self, id: impl Into<AnyId>, target: Point) -> &mut Self {
        self.state.move_to(id, target);
        self
    }

    /// Uniformly scales `id`'s family about its center by `factor` (manim's
    /// `scale`).
    pub fn scale(&mut self, id: impl Into<AnyId>, factor: f32) -> &mut Self {
        self.state.scale(id, factor);
        self
    }

    /// Rotates `id`'s family about its center by `angle` radians (manim's
    /// `rotate`).
    pub fn rotate(&mut self, id: impl Into<AnyId>, angle: f32) -> &mut Self {
        self.state.rotate(id, angle);
        self
    }

    /// Sets the fill color and opacity across `id`'s family (manim's `set_fill`).
    pub fn set_fill(&mut self, id: impl Into<AnyId>, color: Color, opacity: f32) -> &mut Self {
        self.state.set_style_family(id, |s| {
            s.set_fill(color, opacity);
        });
        self
    }

    /// Sets the stroke color, width, and opacity across `id`'s family (manim's
    /// `set_stroke`).
    pub fn set_stroke(
        &mut self,
        id: impl Into<AnyId>,
        color: Color,
        width: f32,
        opacity: f32,
    ) -> &mut Self {
        self.state.set_style_family(id, |s| {
            s.set_stroke(color, width, opacity);
        });
        self
    }

    /// Sets both fill and stroke color across `id`'s family (manim's `set_color`).
    pub fn set_color(&mut self, id: impl Into<AnyId>, color: Color) -> &mut Self {
        self.state.set_style_family(id, |s| {
            s.set_color(color);
        });
        self
    }

    /// Positions `id`'s family next to `target`'s family in direction `dir`,
    /// separated by `buff` scene units (manim's `next_to`).
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_math::{RIGHT, MED_SMALL_BUFF};
    /// let mut scene = Scene::new(Config::default());
    /// let a = scene.add(Square::new()); // spans x ∈ [-1, 1]
    /// let b = scene.add(Square::new());
    /// scene.next_to(b, a, RIGHT, MED_SMALL_BUFF);
    /// assert!((scene[b].get_left().x - (1.0 + MED_SMALL_BUFF)).abs() < 1e-4);
    /// ```
    pub fn next_to(
        &mut self,
        id: impl Into<AnyId>,
        target: impl Into<AnyId>,
        dir: Point,
        buff: f32,
    ) -> &mut Self {
        let id = id.into();
        let target_point = self
            .state
            .family_bounding_box(target)
            .point_in_direction(dir);
        let point_to_align = self.state.family_bounding_box(id).point_in_direction(-dir);
        let delta = target_point - point_to_align + buff * dir;
        self.state.shift(id, delta);
        self
    }

    /// Moves `id`'s family to a frame edge in direction `dir`, `buff` from the
    /// border, using this scene's configured frame size (manim's `to_edge`).
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_math::LEFT;
    /// let mut scene = Scene::new(Config::default());
    /// let d = scene.add(Dot::new());
    /// scene.to_edge(d, LEFT, 0.5);
    /// let half_w = Config::default().frame_width / 2.0;
    /// assert!((scene[d].get_left().x - (-half_w + 0.5)).abs() < 1e-4);
    /// ```
    pub fn to_edge(&mut self, id: impl Into<AnyId>, dir: Point, buff: f32) -> &mut Self {
        let id = id.into();
        let bbox = self.state.family_bounding_box(id);
        let radius = Point::new(
            self.config.frame_width / 2.0,
            self.config.frame_height / 2.0,
            0.0,
        );
        let sign = |v: f32| {
            if v > 0.0 {
                1.0
            } else if v < 0.0 {
                -1.0
            } else {
                0.0
            }
        };
        let target_point = Point::new(sign(dir.x), sign(dir.y), sign(dir.z)) * radius;
        let point_to_align = bbox.point_in_direction(dir);
        let mut delta = target_point - point_to_align - buff * dir;
        for axis in 0..3 {
            if dir[axis] == 0.0 {
                delta[axis] = 0.0;
            }
        }
        self.state.shift(id, delta);
        self
    }

    /// Adds a mobject rebuilt from scratch every frame by `build` (manim's
    /// `always_redraw`), a shorthand for
    /// [`state_mut().always_redraw`](crate::scene_state::SceneState::always_redraw).
    ///
    /// The closure reads the scene each tick and returns a fresh mobject whose
    /// path and style are copied into the live one — ideal for geometry that
    /// tracks a [`ValueTracker`](crate::animations::ValueTracker) or another
    /// mobject, without hand-writing an updater.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_core::animations::{SetValue, ValueTracker};
    /// use manim_math::RIGHT;
    /// let mut scene = Scene::new(Config::low());
    /// let t = scene.add(ValueTracker::new(0.0));
    /// // A dot that always sits at x = t.
    /// let dot = scene.always_redraw(move |s| Dot::at(s.get(t).get_value() * RIGHT));
    /// scene.play(SetValue::new(t, 3.0)).unwrap();
    /// // The redraw runs while frames are produced; tick it here to observe it.
    /// scene.state_mut().run_updaters(UpdaterCtx { dt: 0.0, time: 0.0 });
    /// assert!((scene[dot].get_center().x - 3.0).abs() < 1e-4);
    /// ```
    pub fn always_redraw<M: Mobject>(
        &mut self,
        build: impl Fn(&SceneState) -> M + Send + Sync + 'static,
    ) -> MobjectId<M> {
        self.state.always_redraw(build)
    }

    /// Removes a mobject (and its descendants) from the scene.
    pub fn remove(&mut self, id: impl Into<AnyId>) {
        self.state.remove(id.into());
    }

    /// Schedules a group of concurrent animations (manim's `play`).
    ///
    /// Eagerly applies the animations' end state to the live scene and records a
    /// replayable segment in the timeline (see the module docs).
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::EmptyPlay`] if no animations were supplied.
    pub fn play(&mut self, anims: impl IntoAnimations) -> Result<()> {
        let mut anims = anims.into_animations();
        if anims.is_empty() {
            return Err(CoreError::EmptyPlay);
        }
        let snapshot = self.state.clone();
        // Begin all concurrently, then interpolate to the end, then finish —
        // so interacting animations (e.g. a follower) see consistent state.
        for anim in &mut anims {
            anim.begin(&mut self.state);
        }
        for anim in &mut anims {
            let end_alpha = anim.rate_fn().apply(1.0);
            anim.interpolate(&mut self.state, end_alpha);
        }
        for anim in &mut anims {
            anim.finish(&mut self.state);
        }
        self.timeline.push_play(anims, snapshot);
        Ok(())
    }

    /// Schedules a hold of `duration` seconds (manim's `wait`).
    pub fn wait(&mut self, duration: f32) {
        let snapshot = self.state.clone();
        self.timeline.push_wait(duration, snapshot);
    }

    /// Waits until absolute scene time reaches `t` (no-op if already past).
    pub fn wait_until(&mut self, t: f32) {
        let remaining = t - self.total_duration();
        if remaining > 0.0 {
            self.wait(remaining);
        }
    }

    /// Begins a `.animate()` builder for `id` (manim's `mobject.animate`).
    pub fn animate<M: Mobject>(&self, id: MobjectId<M>) -> AnimBuilder<M> {
        id.animate()
    }

    /// The live scene state (final constructed state).
    pub fn state(&self) -> &SceneState {
        &self.state
    }

    /// Mutable access to the live scene state.
    pub fn state_mut(&mut self) -> &mut SceneState {
        &mut self.state
    }

    /// The scene camera (stored in the scene state).
    pub fn camera(&self) -> &Camera2D {
        self.state.camera()
    }

    /// Mutable access to the camera.
    pub fn camera_mut(&mut self) -> &mut Camera2D {
        self.state.camera_mut()
    }

    /// Switches the camera to 3-D and sets its orbit angles, mirroring manim's
    /// `ThreeDScene.set_camera_orientation(phi, theta)`. A thin passthrough to
    /// [`Camera2D::set_camera_orientation`](crate::camera::Camera2D::set_camera_orientation).
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// let mut scene = Scene::new(Config::default());
    /// scene.set_camera_orientation(1.3, -0.6);
    /// assert!(scene.camera().is_3d());
    /// ```
    pub fn set_camera_orientation(&mut self, phi: f32, theta: f32) {
        self.state.camera_mut().set_camera_orientation(phi, theta);
    }

    /// Advances ambient camera rotation by `d_theta` radians (manim's
    /// `begin_ambient_camera_rotation` applied per step). No-op in 2-D.
    pub fn rotate_camera(&mut self, d_theta: f32) {
        self.state.camera_mut().rotate_ambient(d_theta);
    }

    /// A handle to the camera frame for animation, mirroring manim's
    /// `self.camera.frame`.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// let mut scene = Scene::new(Config::default());
    /// let _ = scene.add(Circle::new());
    /// scene.play(scene.camera_frame().animate().scale(0.5)).unwrap();
    /// assert!(scene.camera().frame_width < Config::default().frame_width);
    /// ```
    pub fn camera_frame(&self) -> CameraFrameHandle {
        CameraFrameHandle
    }

    /// Marks the start of a named section (manim's `next_section`).
    pub fn next_section(&mut self, name: impl Into<String>) {
        self.timeline.push_section(name);
    }

    /// The recorded section boundaries, in order.
    pub fn sections(&self) -> &[Section] {
        self.timeline.sections()
    }

    /// The scene configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The display list of the live (final) state.
    pub fn display_list(&self) -> DisplayList {
        self.state.display_list()
    }

    /// The total scheduled duration in seconds (manim's `renderer.time`).
    pub fn total_duration(&self) -> f32 {
        self.timeline.duration()
    }

    /// Alias for [`total_duration`](Self::total_duration).
    pub fn time(&self) -> f32 {
        self.total_duration()
    }

    /// Samples the scene at the configured frame rate, yielding `(time,
    /// display_list)` for each frame — what a renderer and the golden tests
    /// consume.
    ///
    /// Frames are produced by pure snapshot-seeking (deterministic) with the
    /// updater pass applied per frame.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_core::animations::Create;
    /// let mut scene = Scene::new(Config::low()); // 15 fps
    /// let c = scene.add(Circle::new());
    /// scene.play(Create::new(c)).unwrap();
    /// let frames: Vec<_> = scene.frames().collect();
    /// // A 1 s animation at 15 fps yields ~16 frames (t = 0 … 1).
    /// assert!(frames.len() >= 15);
    /// assert_eq!(frames[0].0, 0.0);
    /// ```
    pub fn frames(&mut self) -> impl Iterator<Item = (f32, DisplayList)> + '_ {
        let dt = self.config.frame_dt();
        let total = self.timeline.duration();
        let mut out: Vec<(f32, DisplayList)> = Vec::new();
        if self.timeline.is_empty() || total <= 0.0 {
            out.push((0.0, self.state.display_list()));
            return out.into_iter();
        }
        let n = (total / dt).ceil() as usize;
        for i in 0..=n {
            let t = (i as f32 * dt).min(total);
            let reconstructed = self.timeline.state_at(t);
            let mut frame_state = reconstructed.unwrap_or_else(|| self.state.clone());
            frame_state.run_updaters(UpdaterCtx { dt, time: t });
            out.push((t, frame_state.display_list()));
            if t >= total {
                break;
            }
        }
        out.into_iter()
    }

    /// Samples the scene like [`frames`](Self::frames), but yields the camera
    /// state too — what renderers should consume to follow camera motion.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_math::UP;
    /// let mut scene = Scene::new(Config::low());
    /// let _ = scene.add(Circle::new());
    /// scene.play(scene.camera_frame().animate().move_to(2.0 * UP)).unwrap();
    /// let frames: Vec<_> = scene.frames_with_camera().collect();
    /// // The camera pans up to y = 2 by the last frame.
    /// assert!((frames.last().unwrap().camera.center.y - 2.0).abs() < 1e-3);
    /// ```
    pub fn frames_with_camera(&mut self) -> impl Iterator<Item = Frame> + '_ {
        let dt = self.config.frame_dt();
        let total = self.timeline.duration();
        let mut out: Vec<Frame> = Vec::new();
        if self.timeline.is_empty() || total <= 0.0 {
            out.push(Frame {
                t: 0.0,
                display_list: self.state.display_list(),
                camera: CameraFrame::from(self.state.camera()),
            });
            return out.into_iter();
        }
        let n = (total / dt).ceil() as usize;
        for i in 0..=n {
            let t = (i as f32 * dt).min(total);
            let reconstructed = self.timeline.state_at(t);
            let mut frame_state = reconstructed.unwrap_or_else(|| self.state.clone());
            frame_state.run_updaters(UpdaterCtx { dt, time: t });
            out.push(Frame {
                t,
                display_list: frame_state.display_list(),
                camera: CameraFrame::from(frame_state.camera()),
            });
            if t >= total {
                break;
            }
        }
        out.into_iter()
    }
}

/// A declarative scene definition, run once to build the timeline (manim's
/// `Scene.construct`).
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::animations::Create;
/// use manim_core::error::Result;
///
/// struct Demo;
/// impl SceneBuilder for Demo {
///     fn construct(&self, scene: &mut Scene) -> Result<()> {
///         let c = scene.add(Circle::new());
///         scene.play(Create::new(c))?;
///         Ok(())
///     }
/// }
/// let scene = Scene::build(&Demo, Config::default()).unwrap();
/// assert_eq!(scene.total_duration(), 1.0);
/// ```
pub trait SceneBuilder {
    /// Populates `scene`: add mobjects, `play` animations, `wait`.
    fn construct(&self, scene: &mut Scene) -> Result<()>;
}

impl<M: Mobject> Index<MobjectId<M>> for Scene {
    type Output = M;
    fn index(&self, id: MobjectId<M>) -> &M {
        self.state.get(id)
    }
}

impl<M: Mobject> IndexMut<MobjectId<M>> for Scene {
    fn index_mut(&mut self, id: MobjectId<M>) -> &mut M {
        self.state.get_mut(id)
    }
}
