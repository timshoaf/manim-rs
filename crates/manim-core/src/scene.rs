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

use manim_color::Color;
use manim_math::Point;

use crate::animation::IntoAnimations;
use crate::animations::{AnimBuilder, Animate};
use crate::config::Config;
use crate::display::DisplayList;
use crate::error::{CoreError, Result};
use crate::mobject::{AnyId, Mobject, MobjectId};
use crate::scene_state::{SceneState, UpdaterCtx};
use crate::timeline::Timeline;

/// A minimal 2D camera: the visible frame and background.
///
/// Camera animation lands in a later phase; for now this carries the frame
/// geometry a renderer needs.
///
/// ```
/// use manim_core::scene::Camera2D;
/// use manim_core::config::Config;
/// let cam = Camera2D::from_config(&Config::default());
/// assert_eq!(cam.frame_height, 8.0);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Camera2D {
    /// Center of the visible frame in scene units.
    pub frame_center: Point,
    /// Width of the visible frame in scene units.
    pub frame_width: f32,
    /// Height of the visible frame in scene units.
    pub frame_height: f32,
    /// Background color.
    pub background: Color,
}

impl Camera2D {
    /// Builds a camera from a [`Config`].
    pub fn from_config(config: &Config) -> Self {
        Self {
            frame_center: Point::ZERO,
            frame_width: config.frame_width,
            frame_height: config.frame_height,
            background: config.background_color,
        }
    }
}

/// A scene under construction: the live [`SceneState`], the [`Timeline`] being
/// built, a [`Camera2D`], and the [`Config`].
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
    camera: Camera2D,
    config: Config,
}

impl Scene {
    /// A new, empty scene with the given `config`.
    pub fn new(config: Config) -> Self {
        let camera = Camera2D::from_config(&config);
        Self {
            state: SceneState::new(),
            timeline: Timeline::new(),
            camera,
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
        for anim in &mut anims {
            anim.begin(&mut self.state);
            let end_alpha = anim.rate_fn().apply(1.0);
            anim.interpolate(&mut self.state, end_alpha);
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

    /// The scene camera.
    pub fn camera(&self) -> &Camera2D {
        &self.camera
    }

    /// Mutable access to the camera.
    pub fn camera_mut(&mut self) -> &mut Camera2D {
        &mut self.camera
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
