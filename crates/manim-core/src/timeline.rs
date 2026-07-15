//! [`Timeline`]: the explicit animation schedule.
//!
//! A timeline is a sequence of segments, each either a play-group or a wait,
//! carrying a **snapshot of the scene at its start**. Because animations
//! snapshot their own start state in `begin` and are pure in `alpha`,
//! reconstructing the scene at any time is exact and order-independent: restore
//! the active segment's snapshot, then run its animations to the local progress
//! ([`state_at`](Timeline::state_at)). This supersedes manim CE's
//! crossing-based tick — seeking backward or forward is equally exact — at the
//! cost of one cloned [`SceneState`] per segment (cheap at manim scale). See
//! `docs/design/04-animation-system.md`.

use crate::animation::Animation;
use crate::scene_state::{SceneState, UpdaterCtx};

/// The body of a [`Segment`]: a group of concurrent animations, or a hold.
enum SegmentKind {
    /// Concurrent animations run over the segment's duration.
    Play(Vec<Box<dyn Animation>>),
    /// A hold with no animation.
    Wait,
}

/// One scheduled span: its start time, duration, start-state snapshot, and body.
struct Segment {
    kind: SegmentKind,
    start: f32,
    duration: f32,
    snapshot: SceneState,
}

/// An explicit, seekable schedule of animation segments.
///
/// Built up by [`Scene`](crate::scene::Scene) during `construct`; consumed by
/// playback and by [`Scene::frames`](crate::scene::Scene::frames).
#[derive(Default)]
pub struct Timeline {
    segments: Vec<Segment>,
    cursor: f32,
}

impl Timeline {
    /// An empty timeline.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a play-group of `anims` (starting with the scene in `snapshot`),
    /// returning the new segment's duration.
    ///
    /// The duration is the maximum of the animations' durations, matching CE's
    /// concurrent `play`.
    pub fn push_play(&mut self, anims: Vec<Box<dyn Animation>>, snapshot: SceneState) -> f32 {
        let duration = anims.iter().map(|a| a.duration()).fold(0.0_f32, f32::max);
        let start = self.duration();
        self.segments.push(Segment {
            kind: SegmentKind::Play(anims),
            start,
            duration,
            snapshot,
        });
        duration
    }

    /// Appends a wait of `duration` seconds (holding `snapshot`).
    pub fn push_wait(&mut self, duration: f32, snapshot: SceneState) {
        let start = self.duration();
        self.segments.push(Segment {
            kind: SegmentKind::Wait,
            start,
            duration: duration.max(0.0),
            snapshot,
        });
    }

    /// The total scheduled duration in seconds.
    pub fn duration(&self) -> f32 {
        self.segments
            .last()
            .map(|s| s.start + s.duration)
            .unwrap_or(0.0)
    }

    /// The number of segments.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Whether the timeline has no segments.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Reconstructs the scene state at absolute time `t`, purely (no updaters).
    ///
    /// Returns `None` only for an empty timeline. The result is exact for any
    /// `t`, forward or backward, because each segment is self-contained.
    pub fn state_at(&mut self, t: f32) -> Option<SceneState> {
        if self.segments.is_empty() {
            return None;
        }
        let t = t.max(0.0);
        let idx = self
            .segments
            .iter()
            .rposition(|s| s.start <= t + 1e-6)
            .unwrap_or(0);
        let seg = &mut self.segments[idx];
        let mut state = seg.snapshot.clone();
        if let SegmentKind::Play(anims) = &mut seg.kind {
            let local = if seg.duration > 1e-9 {
                ((t - seg.start) / seg.duration).clamp(0.0, 1.0)
            } else {
                1.0
            };
            for anim in anims.iter_mut() {
                anim.begin(&mut state);
                if local >= 1.0 {
                    anim.interpolate(&mut state, anim.rate_fn().apply(1.0));
                    anim.finish(&mut state);
                } else {
                    anim.interpolate(&mut state, anim.rate_fn().apply(local));
                }
            }
        }
        Some(state)
    }

    /// Overwrites `state` with the reconstruction at time `t`.
    ///
    /// Exact for any `t` (see [`state_at`](Self::state_at)); a no-op on an empty
    /// timeline. Also updates the internal advance cursor to `t`.
    pub fn seek(&mut self, state: &mut SceneState, t: f32) {
        if let Some(s) = self.state_at(t) {
            *state = s;
        }
        self.cursor = t.max(0.0);
    }

    /// Advances the cursor by `dt`, reconstructs `state`, then runs updaters
    /// with that `dt`. Returns the new cursor time.
    ///
    /// Unlike a pure [`seek`](Self::seek), this runs the updater pass, so
    /// updater-driven mobjects react to the frame.
    pub fn advance(&mut self, state: &mut SceneState, dt: f32) -> f32 {
        let t = self.cursor + dt;
        self.seek(state, t);
        state.run_updaters(UpdaterCtx { dt, time: t });
        t
    }

    /// The current advance cursor time in seconds.
    pub fn cursor(&self) -> f32 {
        self.cursor
    }
}
