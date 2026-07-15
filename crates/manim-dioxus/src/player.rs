//! [`PlayerState`]: the framework-independent playback state machine.
//!
//! This holds no dioxus, wasm, or GPU types, so it is unit-testable headless.
//! The [`ManimPlayer`](crate::ManimPlayer) component owns one behind an
//! `Rc<RefCell<…>>` and drives it from a `requestAnimationFrame` loop; the
//! [`SceneController`](crate::SceneController) mutates it through the same handle.

/// Playback transport for a fixed sequence of precomputed frames.
///
/// Time is tracked as a `playhead` in seconds against the scene's total
/// duration; the visible frame is [`frame_index`](Self::frame_index) =
/// `round(playhead × fps)`.
///
/// ```
/// use manim_dioxus::PlayerState;
/// // 2 s scene at 30 fps → 61 frames, autoplaying.
/// let mut p = PlayerState::new(2.0, 30, 61, true, false);
/// assert!(p.is_playing());
/// p.advance(0.5); // wall-clock half second
/// assert!((p.playhead() - 0.5).abs() < 1e-6);
/// assert_eq!(p.frame_index(), 15);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerState {
    total: f32,
    fps: u32,
    frame_count: usize,
    playhead: f32,
    playing: bool,
    playback_rate: f32,
    looping: bool,
}

impl PlayerState {
    /// Builds a state for a `total`-second scene at `fps` with `frame_count`
    /// precomputed frames. `autoplay` starts it playing; `looping` restarts at
    /// the end instead of stopping.
    pub fn new(total: f32, fps: u32, frame_count: usize, autoplay: bool, looping: bool) -> Self {
        Self {
            total: total.max(0.0),
            fps: fps.max(1),
            frame_count: frame_count.max(1),
            playhead: 0.0,
            playing: autoplay,
            playback_rate: 1.0,
            looping,
        }
    }

    /// Advances the playhead by `dt` wall-clock seconds (scaled by the playback
    /// rate) when playing. At the end it either wraps (if looping) or stops.
    ///
    /// ```
    /// use manim_dioxus::PlayerState;
    /// let mut p = PlayerState::new(1.0, 60, 61, true, false);
    /// p.advance(2.0); // overshoot
    /// assert_eq!(p.playhead(), 1.0); // clamped to the end
    /// assert!(!p.is_playing());      // and stopped (not looping)
    /// ```
    pub fn advance(&mut self, dt: f32) {
        if !self.playing || self.total <= 0.0 {
            return;
        }
        self.playhead += dt * self.playback_rate;
        if self.playhead >= self.total {
            if self.looping {
                // Wrap, preserving the overshoot for smooth looping.
                self.playhead %= self.total;
            } else {
                self.playhead = self.total;
                self.playing = false;
            }
        } else if self.playhead < 0.0 {
            self.playhead = 0.0;
        }
    }

    /// Seeks to absolute time `t` (clamped to `[0, total]`), without changing the
    /// play/pause state.
    pub fn seek(&mut self, t: f32) {
        self.playhead = t.clamp(0.0, self.total);
    }

    /// Starts playback; if paused at the end, restarts from the top.
    pub fn play(&mut self) {
        if self.playhead >= self.total {
            self.playhead = 0.0;
        }
        self.playing = true;
    }

    /// Pauses playback.
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Toggles play/pause (restarting from the top if paused at the end).
    pub fn toggle(&mut self) {
        if self.playing {
            self.pause();
        } else {
            self.play();
        }
    }

    /// Restarts from the beginning and plays.
    pub fn restart(&mut self) {
        self.playhead = 0.0;
        self.playing = true;
    }

    /// Sets the playback rate (e.g. `2.0` for double speed); clamped non-negative.
    pub fn set_playback_rate(&mut self, rate: f32) {
        self.playback_rate = rate.max(0.0);
    }

    /// Sets whether playback loops.
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// The index of the frame to show now: `round(playhead × fps)`, clamped to
    /// the frame range.
    ///
    /// ```
    /// use manim_dioxus::PlayerState;
    /// let mut p = PlayerState::new(1.0, 10, 11, false, false);
    /// p.seek(0.44);
    /// assert_eq!(p.frame_index(), 4); // 0.44 × 10 = 4.4 → 4
    /// p.seek(1.0);
    /// assert_eq!(p.frame_index(), 10); // last frame
    /// ```
    pub fn frame_index(&self) -> usize {
        let idx = (self.playhead * self.fps as f32).round() as isize;
        idx.clamp(0, self.frame_count as isize - 1) as usize
    }

    /// Progress through the scene in `[0, 1]`.
    pub fn progress(&self) -> f32 {
        if self.total > 0.0 {
            (self.playhead / self.total).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Sets the playhead from a `[0, 1]` progress fraction (for a scrubber).
    pub fn set_progress(&mut self, fraction: f32) {
        self.seek(fraction.clamp(0.0, 1.0) * self.total);
    }

    /// Whether playback is currently running.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// The playhead in seconds.
    pub fn playhead(&self) -> f32 {
        self.playhead
    }

    /// The total duration in seconds.
    pub fn total(&self) -> f32 {
        self.total
    }

    /// The playback rate.
    pub fn playback_rate(&self) -> f32 {
        self.playback_rate
    }
}

/// Formats a duration in seconds as `m:ss` for the progress readout. Negative or
/// non-finite inputs clamp to `0:00`.
///
/// ```
/// use manim_dioxus::player::format_time;
/// assert_eq!(format_time(0.0), "0:00");
/// assert_eq!(format_time(5.0), "0:05");
/// assert_eq!(format_time(65.4), "1:05");
/// assert_eq!(format_time(-3.0), "0:00");
/// ```
pub fn format_time(secs: f32) -> String {
    let s = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
    let total = s.floor() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_time_mm_ss() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(9.0), "0:09");
        assert_eq!(format_time(65.4), "1:05");
        assert_eq!(format_time(600.0), "10:00");
        assert_eq!(format_time(-3.0), "0:00");
        assert_eq!(format_time(f32::NAN), "0:00");
    }

    #[test]
    fn play_pause_toggle() {
        let mut p = PlayerState::new(1.0, 30, 31, false, false);
        assert!(!p.is_playing());
        p.play();
        assert!(p.is_playing());
        p.toggle();
        assert!(!p.is_playing());
    }

    #[test]
    fn advance_stops_at_end_without_loop() {
        let mut p = PlayerState::new(1.0, 30, 31, true, false);
        p.advance(5.0);
        assert_eq!(p.playhead(), 1.0);
        assert!(!p.is_playing());
    }

    #[test]
    fn advance_wraps_when_looping() {
        let mut p = PlayerState::new(1.0, 30, 31, true, true);
        p.advance(1.25);
        assert!(p.is_playing());
        assert!((p.playhead() - 0.25).abs() < 1e-5);
    }

    #[test]
    fn seek_clamps() {
        let mut p = PlayerState::new(2.0, 30, 61, false, false);
        p.seek(-1.0);
        assert_eq!(p.playhead(), 0.0);
        p.seek(99.0);
        assert_eq!(p.playhead(), 2.0);
    }

    #[test]
    fn playback_rate_scales_advance() {
        let mut p = PlayerState::new(10.0, 30, 301, true, false);
        p.set_playback_rate(2.0);
        p.advance(1.0);
        assert!((p.playhead() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn frame_index_rounds_and_clamps() {
        let mut p = PlayerState::new(1.0, 10, 11, false, false);
        p.seek(0.44);
        assert_eq!(p.frame_index(), 4);
        p.seek(0.46);
        assert_eq!(p.frame_index(), 5);
        p.seek(2.0);
        assert_eq!(p.frame_index(), 10);
    }

    #[test]
    fn play_from_end_restarts() {
        let mut p = PlayerState::new(1.0, 30, 31, false, false);
        p.seek(1.0);
        p.play();
        assert_eq!(p.playhead(), 0.0);
        assert!(p.is_playing());
    }

    #[test]
    fn progress_round_trips() {
        let mut p = PlayerState::new(4.0, 30, 121, false, false);
        p.set_progress(0.25);
        assert!((p.playhead() - 1.0).abs() < 1e-6);
        assert!((p.progress() - 0.25).abs() < 1e-6);
    }
}
