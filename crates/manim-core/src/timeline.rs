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

use std::path::PathBuf;

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

/// A named boundary in the timeline (manim's `next_section`), marking where a
/// section begins.
///
/// ```
/// use manim_core::timeline::Section;
/// let s = Section { name: "intro".to_string(), start: 0.0 };
/// assert_eq!(s.name, "intro");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// The section name.
    pub name: String,
    /// The absolute start time in seconds.
    pub start: f32,
}

/// A scheduled sound (manim's `add_sound`): an audio file mixed into the exported
/// video's audio track, starting at absolute time `start`.
///
/// Plain data — cues do not affect frames or seeking, only audio muxing at export
/// time. The `path` only takes effect for native video export (there is no audio
/// on the web), but the type compiles on every target.
///
/// ```
/// use manim_core::timeline::SoundCue;
/// let cue = SoundCue { path: "click.wav".into(), start: 1.5, gain: None };
/// assert_eq!(cue.start, 1.5);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct SoundCue {
    /// The audio file to play.
    pub path: PathBuf,
    /// Absolute start time in seconds (scene time when scheduled, plus offset).
    pub start: f32,
    /// Optional linear gain multiplier (`1.0` = unchanged); `None` leaves the
    /// clip at its recorded level.
    pub gain: Option<f32>,
}

/// An explicit, seekable schedule of animation segments.
///
/// Built up by [`Scene`](crate::scene::Scene) during `construct`; consumed by
/// playback and by [`Scene::frames`](crate::scene::Scene::frames).
#[derive(Default)]
pub struct Timeline {
    segments: Vec<Segment>,
    sections: Vec<Section>,
    sound_cues: Vec<SoundCue>,
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
            // All concurrent animations begin at the segment start (before any
            // interpolate), so animations that read each other — e.g. a follower
            // — capture the correct relationship. Matches manim CE.
            for anim in anims.iter_mut() {
                anim.begin(&mut state);
            }
            for anim in anims.iter_mut() {
                let eased = anim.rate_fn().apply(local);
                anim.interpolate(&mut state, eased);
            }
            if local >= 1.0 {
                for anim in anims.iter_mut() {
                    anim.finish(&mut state);
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

    /// Records a named section boundary starting at the current end time
    /// (manim's `next_section`).
    pub fn push_section(&mut self, name: impl Into<String>) {
        let start = self.duration();
        self.sections.push(Section {
            name: name.into(),
            start,
        });
    }

    /// The recorded section boundaries, in order.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// let mut scene = Scene::new(Config::default());
    /// scene.next_section("intro");
    /// scene.wait(1.0);
    /// scene.next_section("body");
    /// let sections = scene.sections();
    /// assert_eq!(sections.len(), 2);
    /// assert_eq!(sections[1].name, "body");
    /// assert!((sections[1].start - 1.0).abs() < 1e-6);
    /// ```
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// Records a [`SoundCue`] for `path`, starting `offset` seconds from the
    /// current end time (manim's `add_sound`; `offset` may be negative, clamped
    /// to `0`). Does not change the timeline duration.
    pub fn push_sound(&mut self, path: PathBuf, offset: f32) {
        let start = (self.duration() + offset).max(0.0);
        self.sound_cues.push(SoundCue {
            path,
            start,
            gain: None,
        });
    }

    /// The scheduled [`SoundCue`]s, in insertion order.
    pub fn sound_cues(&self) -> &[SoundCue] {
        &self.sound_cues
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_state::SceneState;

    #[test]
    fn sound_cues_record_at_scene_time_and_clamp() {
        let mut tl = Timeline::new();
        tl.push_wait(1.5, SceneState::new());
        tl.push_sound("a.wav".into(), 0.0); // at the 1.5 s cursor
        tl.push_wait(1.0, SceneState::new()); // duration now 2.5 s
        tl.push_sound("b.wav".into(), 0.5); // 3.0 s
        tl.push_sound("c.wav".into(), -10.0); // clamps to 0

        let cues = tl.sound_cues();
        assert_eq!(cues.len(), 3);
        assert!((cues[0].start - 1.5).abs() < 1e-6);
        assert!((cues[1].start - 3.0).abs() < 1e-6);
        assert_eq!(cues[2].start, 0.0);
        assert_eq!(cues[0].path, PathBuf::from("a.wav"));
        // Cues don't extend the timeline.
        assert!((tl.duration() - 2.5).abs() < 1e-6);
    }
}
