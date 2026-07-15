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
