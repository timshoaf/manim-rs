//! [`RenderSchedule`]: the render-on-demand state machine for a
//! [`Figure`](crate::Figure) or a
//! shared-device player.
//!
//! Textbook pages hold many figures; idle ones must cost ~0. This decides, each
//! animation frame, whether to **draw** (`should_render`) and whether to keep the
//! frame loop **running** (`wants_frame`). It holds no dioxus, GPU, or wasm types
//! — pure and unit-tested headlessly — so the browser driver is a thin loop over
//! it (see [`FigureController`](crate::FigureController)).
//!
//! A figure starts *dirty* (draw once on mount), then stays idle until something
//! wakes it:
//! - [`mark_dirty`](RenderSchedule::mark_dirty) — a parameter changed → **one**
//!   frame, then back to idle;
//! - [`set_pointer_active`](RenderSchedule::set_pointer_active)`(true)` — draw
//!   every frame while dragging, plus a short *settle* window after release (for
//!   inertia);
//! - [`set_animating`](RenderSchedule::set_animating)`(true)` — draw every frame
//!   until the animation window closes.
//!
//! When none hold, [`wants_frame`](RenderSchedule::wants_frame) is `false` and the
//! driver stops scheduling frames.

/// The default settle window (seconds) kept alive after a pointer release, so
/// inertia/damping can finish before the loop idles.
pub const DEFAULT_SETTLE_SECS: f32 = 0.25;

/// A pure render-on-demand scheduler. See the [module docs](self).
///
/// ```
/// use manim_dioxus::schedule::RenderSchedule;
/// let mut s = RenderSchedule::new();
/// // Mounts dirty: the first frame draws, then it idles.
/// assert!(s.should_render(0.0));
/// assert!(!s.wants_frame());
/// // A parameter change wakes it for exactly one frame.
/// s.mark_dirty();
/// assert!(s.wants_frame());
/// assert!(s.should_render(0.016));
/// assert!(!s.wants_frame());
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct RenderSchedule {
    /// A pending one-shot draw (mount, parameter change).
    dirty: bool,
    /// An animation window is open — draw continuously.
    animating: bool,
    /// A pointer interaction is in progress — draw continuously.
    pointer_active: bool,
    /// Keep drawing until this time (a settle window after a pointer release).
    settle_until: f32,
    /// The settle window length in seconds.
    settle_secs: f32,
    /// The last time seen by [`should_render`](Self::should_render).
    now: f32,
}

impl Default for RenderSchedule {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderSchedule {
    /// A fresh scheduler: dirty (so it draws once on mount) and otherwise idle.
    pub fn new() -> Self {
        Self {
            dirty: true,
            animating: false,
            pointer_active: false,
            settle_until: 0.0,
            settle_secs: DEFAULT_SETTLE_SECS,
            now: 0.0,
        }
    }

    /// Sets the post-release settle window (seconds); clamped non-negative.
    pub fn with_settle(mut self, secs: f32) -> Self {
        self.settle_secs = secs.max(0.0);
        self
    }

    /// Requests a single redraw (a parameter/pointer event changed the scene).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Opens or closes an animation window. Either edge marks one draw, so the
    /// first animated frame and the final settled frame both render.
    pub fn set_animating(&mut self, animating: bool) {
        self.animating = animating;
        self.dirty = true;
    }

    /// Marks a pointer interaction active or finished. Releasing (`true → false`)
    /// opens a settle window of [`with_settle`](Self::with_settle) seconds.
    pub fn set_pointer_active(&mut self, active: bool) {
        if self.pointer_active && !active {
            self.settle_until = self.now + self.settle_secs;
        }
        self.pointer_active = active;
        self.dirty = true;
    }

    /// Advances the clock to `now` (seconds) and reports whether to **draw** this
    /// frame — consuming the one-shot dirty flag.
    pub fn should_render(&mut self, now: f32) -> bool {
        self.now = now;
        let render = self.is_active_at(now);
        self.dirty = false;
        render
    }

    /// Whether the driver should schedule **another** frame: a continuous source
    /// (animating / pointer) is active, a settle window is open, or a one-shot
    /// redraw is still pending.
    pub fn wants_frame(&self) -> bool {
        self.is_active_at(self.now)
    }

    /// Whether anything asks for a draw at time `t`.
    fn is_active_at(&self, t: f32) -> bool {
        self.dirty || self.animating || self.pointer_active || t < self.settle_until
    }

    /// Whether a one-shot redraw is pending.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether an animation window is open.
    pub fn is_animating(&self) -> bool {
        self.animating
    }

    /// Whether a pointer interaction is in progress.
    pub fn is_pointer_active(&self) -> bool {
        self.pointer_active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mounts_dirty_then_idles() {
        let mut s = RenderSchedule::new();
        assert!(s.is_dirty());
        assert!(s.wants_frame());
        // First frame draws...
        assert!(s.should_render(0.0));
        // ...and it goes idle: no draw, no further frames.
        assert!(!s.wants_frame());
        assert!(!s.should_render(0.016));
    }

    #[test]
    fn mark_dirty_is_one_shot() {
        let mut s = RenderSchedule::new();
        s.should_render(0.0); // consume mount frame
        s.mark_dirty();
        assert!(s.wants_frame());
        assert!(s.should_render(0.1)); // the one redraw
        assert!(!s.wants_frame()); // and back to idle
        assert!(!s.should_render(0.2));
    }

    #[test]
    fn pointer_drag_renders_continuously_then_settles() {
        let mut s = RenderSchedule::new().with_settle(0.25);
        s.should_render(0.0);
        s.set_pointer_active(true);
        // Every frame draws while dragging.
        assert!(s.should_render(0.1));
        assert!(s.wants_frame());
        assert!(s.should_render(0.2));
        // Release at t=0.2 → settle until 0.45.
        s.set_pointer_active(false);
        assert!(s.should_render(0.3)); // within settle
        assert!(s.wants_frame());
        assert!(s.should_render(0.44)); // still within settle
                                        // Past the settle window → idle.
        assert!(!s.should_render(0.5));
        assert!(!s.wants_frame());
    }

    #[test]
    fn animation_window_renders_until_closed() {
        let mut s = RenderSchedule::new();
        s.should_render(0.0);
        s.set_animating(true);
        assert!(s.is_animating());
        for t in 1..=5 {
            assert!(s.should_render(t as f32 * 0.016));
            assert!(s.wants_frame());
        }
        // Closing draws one final frame, then idles.
        s.set_animating(false);
        assert!(s.should_render(0.2));
        assert!(!s.wants_frame());
        assert!(!s.should_render(0.3));
    }

    #[test]
    fn idle_stays_idle_across_many_ticks() {
        let mut s = RenderSchedule::new();
        s.should_render(0.0);
        // A clean figure never asks to draw or to schedule a frame.
        for i in 0..1000 {
            let t = i as f32 * 0.016;
            assert!(!s.wants_frame(), "woke at tick {i}");
            assert!(!s.should_render(t), "drew at tick {i}");
        }
    }

    #[test]
    fn negative_settle_clamps_to_zero() {
        let mut s = RenderSchedule::new().with_settle(-1.0);
        s.should_render(0.0);
        s.set_pointer_active(true);
        s.should_render(0.1);
        s.set_pointer_active(false); // one final (dirty) render, zero settle window
        assert!(s.should_render(0.1)); // the final render
        assert!(!s.wants_frame()); // no settle → straight to idle
        assert!(!s.should_render(0.2));
    }
}
