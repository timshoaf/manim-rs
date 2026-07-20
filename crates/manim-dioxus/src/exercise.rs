//! Exercise blocks (FE-147): wrap an interactive figure in a goal, and tell the
//! reader when they have met it.
//!
//! The judging is a pure state machine — [`ExerciseMachine`] over an
//! [`ExerciseState`] snapshot and a `target` predicate — so "did they solve it"
//! is unit-tested headlessly, exactly like the drag and schedule machines. The
//! [`Exercise`] component is the thin part: it owns the machine, publishes an
//! [`ExerciseHandle`] into context for the figure's live updater to report into,
//! and draws the achieved badge and the reset button.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use manim_core::prelude::Point;

/// A snapshot of what the reader has built: handle positions plus named scalar
/// parameters. This is what a `target` predicate is asked about.
///
/// ```
/// use manim_dioxus::exercise::ExerciseState;
/// use manim_core::prelude::Point;
/// let s = ExerciseState::new()
///     .with_handles(vec![Point::new(1.0, 0.0, 0.0)])
///     .with_param("phase", 0.25);
/// assert_eq!(s.param("phase"), Some(0.25));
/// assert!(s.handles_satisfy(|p| (p.length() - 1.0).abs() < 1e-3));
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExerciseState {
    handles: Vec<Point>,
    params: Vec<(String, f32)>,
}

impl ExerciseState {
    /// An empty snapshot.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the handle positions (builder form).
    pub fn with_handles(mut self, handles: impl Into<Vec<Point>>) -> Self {
        self.handles = handles.into();
        self
    }

    /// Adds a named parameter (builder form).
    pub fn with_param(mut self, name: impl Into<String>, v: f32) -> Self {
        self.set_param(name, v);
        self
    }

    /// Sets a named parameter in place.
    pub fn set_param(&mut self, name: impl Into<String>, v: f32) {
        let name = name.into();
        match self.params.iter_mut().find(|(n, _)| *n == name) {
            Some((_, slot)) => *slot = v,
            None => self.params.push((name, v)),
        }
    }

    /// The handle positions.
    pub fn handles(&self) -> &[Point] {
        &self.handles
    }

    /// Handle `i`, if it exists. Prefer this to indexing: a predicate written
    /// against a figure that later loses a handle should read `false`, not
    /// panic mid-render.
    pub fn handle(&self, i: usize) -> Option<Point> {
        self.handles.get(i).copied()
    }

    /// A named parameter, if set.
    pub fn param(&self, name: &str) -> Option<f32> {
        self.params.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
    }

    /// Whether **every** handle satisfies `f` (and there is at least one) — the
    /// shape most "put them all on …" goals take.
    pub fn handles_satisfy(&self, f: impl Fn(Point) -> bool) -> bool {
        !self.handles.is_empty() && self.handles.iter().all(|p| f(*p))
    }
}

/// The predicate deciding whether an exercise is met.
///
/// A plain `fn` pointer (not a closure) so the [`Exercise`] props stay
/// `Copy + PartialEq` and dioxus can diff them cheaply. It is a newtype rather
/// than a bare `fn` because comparing function *pointers* directly is not
/// meaningful across codegen units — the newtype compares their addresses
/// explicitly, which is an honest "probably the same predicate" and is only ever
/// used to decide whether to re-render.
#[derive(Clone, Copy)]
pub struct ExerciseTarget(fn(&ExerciseState) -> bool);

impl ExerciseTarget {
    /// Wraps a predicate.
    pub fn new(f: fn(&ExerciseState) -> bool) -> Self {
        Self(f)
    }

    /// Judges a snapshot.
    pub fn met(&self, state: &ExerciseState) -> bool {
        (self.0)(state)
    }
}

impl From<fn(&ExerciseState) -> bool> for ExerciseTarget {
    fn from(f: fn(&ExerciseState) -> bool) -> Self {
        Self(f)
    }
}

impl PartialEq for ExerciseTarget {
    fn eq(&self, other: &Self) -> bool {
        self.0 as usize == other.0 as usize
    }
}

impl std::fmt::Debug for ExerciseTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExerciseTarget(..)")
    }
}

/// What changed on an [`ExerciseMachine::evaluate`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExerciseEvent {
    /// The achieved state is what it was.
    Unchanged,
    /// The goal just became met (fire a badge / a chime).
    Achieved,
    /// The goal was met and no longer is (the reader dragged back out).
    Lost,
}

/// The pure judging machine: evaluates a target against snapshots and tracks
/// both the *live* state and whether it was **ever** met.
///
/// Two flags, because they answer different questions. `achieved` drives the
/// live badge ("you are there now"); `solved` is sticky, so a reader who nails
/// the configuration and then keeps playing does not lose the credit for it.
///
/// ```
/// use manim_dioxus::exercise::{ExerciseMachine, ExerciseState, ExerciseEvent, ExerciseTarget};
/// use manim_core::prelude::Point;
/// fn on_axis(s: &ExerciseState) -> bool {
///     s.handles_satisfy(|p| p.y.abs() < 0.1)
/// }
/// let on_axis = ExerciseTarget::new(on_axis);
/// let mut m = ExerciseMachine::new();
/// let off = ExerciseState::new().with_handles(vec![Point::new(0.0, 1.0, 0.0)]);
/// let on = ExerciseState::new().with_handles(vec![Point::new(0.0, 0.0, 0.0)]);
/// assert_eq!(m.evaluate(&off, on_axis), ExerciseEvent::Unchanged);
/// assert_eq!(m.evaluate(&on, on_axis), ExerciseEvent::Achieved);
/// assert_eq!(m.evaluate(&off, on_axis), ExerciseEvent::Lost);
/// assert!(m.solved(), "the credit is sticky");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExerciseMachine {
    achieved: bool,
    solved: bool,
    /// Consecutive satisfying evaluations seen so far.
    streak: u32,
    /// How many in a row are required to latch (1 = immediate).
    hold: u32,
}

impl ExerciseMachine {
    /// A machine that latches as soon as the target is met.
    pub fn new() -> Self {
        Self {
            hold: 1,
            ..Default::default()
        }
    }

    /// Requires the target to hold for `frames` consecutive evaluations before
    /// latching. Use it when a drag path *sweeps through* the answer — an
    /// exercise that congratulates you for passing over the target is worse
    /// than one that makes you stop on it. Clamped to at least 1.
    pub fn with_hold(mut self, frames: u32) -> Self {
        self.hold = frames.max(1);
        self
    }

    /// Whether the goal is met right now.
    pub fn achieved(&self) -> bool {
        self.achieved
    }

    /// Whether the goal has ever been met since the last [`reset`](Self::reset).
    pub fn solved(&self) -> bool {
        self.solved
    }

    /// Judges a snapshot and reports the transition.
    pub fn evaluate(&mut self, state: &ExerciseState, target: ExerciseTarget) -> ExerciseEvent {
        let met = target.met(state);
        self.streak = if met {
            self.streak.saturating_add(1)
        } else {
            0
        };
        let now = met && self.streak >= self.hold;
        let event = match (self.achieved, now) {
            (false, true) => ExerciseEvent::Achieved,
            (true, false) => ExerciseEvent::Lost,
            _ => ExerciseEvent::Unchanged,
        };
        self.achieved = now;
        self.solved |= now;
        event
    }

    /// Clears both flags (the reset button).
    pub fn reset(&mut self) {
        *self = Self {
            hold: self.hold,
            ..Default::default()
        };
    }
}

/// A shared handle to an [`Exercise`]'s machine, published into context.
///
/// A figure's live updater takes one with [`use_exercise`] and calls
/// [`report`](Self::report) each frame with the current handle/parameter state;
/// the component re-renders only when the achieved flag actually flips, so
/// reporting every frame of a drag is cheap.
#[derive(Clone)]
pub struct ExerciseHandle {
    machine: Rc<RefCell<ExerciseMachine>>,
    target: ExerciseTarget,
    achieved: Signal<bool>,
    solved: Signal<bool>,
    /// Bumped by [`reset`](Self::reset) so a figure can notice and restore its
    /// own starting configuration.
    reset_epoch: Signal<u32>,
}

impl PartialEq for ExerciseHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.machine, &other.machine)
    }
}

impl std::fmt::Debug for ExerciseHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExerciseHandle({:?})", self.machine.borrow())
    }
}

impl ExerciseHandle {
    /// Judges `state` and publishes any change. Returns the transition.
    pub fn report(&self, state: &ExerciseState) -> ExerciseEvent {
        let event = self.machine.borrow_mut().evaluate(state, self.target);
        if event != ExerciseEvent::Unchanged {
            let m = *self.machine.borrow();
            // Signals are `Copy`; taking a local mutable copy writes the same
            // underlying slot, which is what lets a rAF callback publish.
            let (mut achieved, mut solved) = (self.achieved, self.solved);
            achieved.set(m.achieved());
            solved.set(m.solved());
        }
        event
    }

    /// Whether the goal is met right now.
    pub fn is_achieved(&self) -> bool {
        self.machine.borrow().achieved()
    }

    /// Whether the goal has ever been met since the last reset.
    pub fn is_solved(&self) -> bool {
        self.machine.borrow().solved()
    }

    /// Clears the machine and bumps [`reset_epoch`](Self::reset_epoch), so a
    /// live figure watching the epoch can put its handles back.
    pub fn reset(&self) {
        self.machine.borrow_mut().reset();
        let (mut achieved, mut solved, mut epoch) = (self.achieved, self.solved, self.reset_epoch);
        achieved.set(false);
        solved.set(false);
        let next = epoch.peek().wrapping_add(1);
        epoch.set(next);
    }

    /// A counter incremented on every [`reset`](Self::reset). A live updater
    /// keeps the last value it saw and restores its starting state when it
    /// changes.
    pub fn reset_epoch(&self) -> u32 {
        *self.reset_epoch.peek()
    }
}

/// Reads the enclosing [`Exercise`]'s handle from context, if there is one.
///
/// Returns `None` outside an [`Exercise`], so the same figure component can be
/// used both inside an exercise and as a plain illustration.
pub fn use_exercise() -> Option<ExerciseHandle> {
    try_consume_context::<ExerciseHandle>()
}

/// An exercise block wrapping an interactive figure (FE-147).
///
/// Publishes an [`ExerciseHandle`] to its subtree; the figure inside reports its
/// state each frame ([`use_exercise`]), and this draws the prompt, an achieved
/// badge, and a reset button.
///
/// Props:
/// - `prompt`: what the reader is asked to do.
/// - `target`: the predicate deciding it — a plain `fn`, see [`ExerciseTarget`].
/// - `hint`: optional smaller text under the prompt.
/// - `hold`: consecutive satisfying frames required to latch (default 1).
/// - `children`: the figure (and any controls).
#[component]
pub fn Exercise(
    prompt: String,
    target: ExerciseTarget,
    #[props(default)] hint: Option<String>,
    #[props(default = 1)] hold: u32,
    children: Element,
) -> Element {
    let achieved = use_signal(|| false);
    let solved = use_signal(|| false);
    let reset_epoch = use_signal(|| 0u32);
    let handle = use_hook(|| ExerciseHandle {
        machine: Rc::new(RefCell::new(ExerciseMachine::new().with_hold(hold))),
        target,
        achieved,
        solved,
        reset_epoch,
    });
    use_context_provider(|| handle.clone());

    let is_achieved = achieved();
    let is_solved = solved();
    let reset_handle = handle.clone();
    // The badge reads "solved" once earned but dims when the reader wanders off
    // it, so the state is honest without snatching the credit back.
    let (badge_bg, badge_text) = match (is_achieved, is_solved) {
        (true, _) => ("#12492f", "✓ Solved"),
        (false, true) => ("#2a2a2a", "✓ Solved earlier"),
        (false, false) => ("#2a2a2a", "Not yet"),
    };
    rsx! {
        section {
            class: "manim-exercise",
            style: "border:1px solid #2a2a2a;border-radius:10px;overflow:hidden;background:#101010;margin-bottom:1.3rem;",
            header {
                style: "display:flex;align-items:center;gap:10px;padding:8px 12px;background:#181818;",
                span { style: "flex:1;color:#dfe6ee;font-size:0.9rem;", "{prompt}" }
                span {
                    style: "padding:3px 9px;border-radius:999px;font-size:0.75rem;color:#cfe;background:{badge_bg};white-space:nowrap;",
                    "{badge_text}"
                }
                button {
                    style: "padding:3px 9px;font-size:0.75rem;",
                    onclick: move |_| reset_handle.reset(),
                    "Reset"
                }
            }
            {children}
            if let Some(h) = hint {
                p { style: "margin:0;padding:8px 12px;color:#8b95a1;font-size:0.8rem;", "{h}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y, 0.0)
    }

    /// "Every handle sits on the unit circle" — the VCA exercise's target.
    fn on_unit_circle(s: &ExerciseState) -> bool {
        s.handles_satisfy(|p| ((p.x * p.x + p.y * p.y).sqrt() - 1.0).abs() < 0.08)
    }

    #[test]
    fn transitions_are_reported_once_each_way() {
        let mut m = ExerciseMachine::new();
        let off = ExerciseState::new().with_handles(vec![p(0.5, 0.0)]);
        let on = ExerciseState::new().with_handles(vec![p(1.0, 0.0)]);
        assert_eq!(
            m.evaluate(&off, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Unchanged
        );
        assert_eq!(
            m.evaluate(&on, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Achieved
        );
        // Staying achieved is not re-reported (no repeated fanfare).
        assert_eq!(
            m.evaluate(&on, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Unchanged
        );
        assert_eq!(
            m.evaluate(&off, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Lost
        );
        assert_eq!(
            m.evaluate(&off, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Unchanged
        );
    }

    #[test]
    fn solved_is_sticky_but_achieved_is_live() {
        let mut m = ExerciseMachine::new();
        let on = ExerciseState::new().with_handles(vec![p(0.0, 1.0)]);
        let off = ExerciseState::new().with_handles(vec![p(0.0, 3.0)]);
        m.evaluate(&on, ExerciseTarget::new(on_unit_circle));
        assert!(m.achieved() && m.solved());
        m.evaluate(&off, ExerciseTarget::new(on_unit_circle));
        assert!(!m.achieved(), "the live flag follows the reader");
        assert!(m.solved(), "the credit does not");
    }

    #[test]
    fn every_handle_must_satisfy_the_target() {
        let mut m = ExerciseMachine::new();
        let half = ExerciseState::new().with_handles(vec![p(1.0, 0.0), p(0.2, 0.0)]);
        assert_eq!(
            m.evaluate(&half, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Unchanged
        );
        let both = ExerciseState::new().with_handles(vec![p(1.0, 0.0), p(0.0, -1.0)]);
        assert_eq!(
            m.evaluate(&both, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Achieved
        );
    }

    #[test]
    fn an_empty_state_never_satisfies_an_all_handles_target() {
        let mut m = ExerciseMachine::new();
        assert_eq!(
            m.evaluate(&ExerciseState::new(), ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Unchanged
        );
        assert!(!m.achieved());
    }

    #[test]
    fn a_hold_requirement_ignores_a_drag_sweeping_through_the_answer() {
        let mut m = ExerciseMachine::new().with_hold(3);
        let on = ExerciseState::new().with_handles(vec![p(1.0, 0.0)]);
        let off = ExerciseState::new().with_handles(vec![p(2.0, 0.0)]);
        // Two frames on target, then away: never latches.
        assert_eq!(
            m.evaluate(&on, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Unchanged
        );
        assert_eq!(
            m.evaluate(&on, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Unchanged
        );
        assert_eq!(
            m.evaluate(&off, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Unchanged
        );
        assert!(!m.solved());
        // Resting on it does.
        for _ in 0..2 {
            assert_eq!(
                m.evaluate(&on, ExerciseTarget::new(on_unit_circle)),
                ExerciseEvent::Unchanged
            );
        }
        assert_eq!(
            m.evaluate(&on, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Achieved
        );
    }

    #[test]
    fn reset_clears_both_flags_and_the_streak_but_keeps_the_hold() {
        let mut m = ExerciseMachine::new().with_hold(2);
        let on = ExerciseState::new().with_handles(vec![p(1.0, 0.0)]);
        m.evaluate(&on, ExerciseTarget::new(on_unit_circle));
        m.evaluate(&on, ExerciseTarget::new(on_unit_circle));
        assert!(m.solved());
        m.reset();
        assert!(!m.achieved() && !m.solved());
        // The hold survives, so the first post-reset frame does not latch.
        assert_eq!(
            m.evaluate(&on, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Unchanged
        );
        assert_eq!(
            m.evaluate(&on, ExerciseTarget::new(on_unit_circle)),
            ExerciseEvent::Achieved
        );
    }

    #[test]
    fn parameters_participate_in_targets() {
        fn phase_is_zero(s: &ExerciseState) -> bool {
            s.param("phase").is_some_and(|v| v.abs() < 0.05)
        }
        let mut m = ExerciseMachine::new();
        let s = ExerciseState::new().with_param("phase", 1.0);
        assert_eq!(
            m.evaluate(&s, ExerciseTarget::new(phase_is_zero)),
            ExerciseEvent::Unchanged
        );
        let s = ExerciseState::new().with_param("phase", 0.01);
        assert_eq!(
            m.evaluate(&s, ExerciseTarget::new(phase_is_zero)),
            ExerciseEvent::Achieved
        );
        // A missing parameter reads false rather than defaulting to zero.
        assert_eq!(
            m.evaluate(&ExerciseState::new(), ExerciseTarget::new(phase_is_zero)),
            ExerciseEvent::Lost
        );
    }

    #[test]
    fn setting_a_parameter_twice_replaces_it() {
        let mut s = ExerciseState::new().with_param("a", 1.0);
        s.set_param("a", 2.0);
        assert_eq!(s.param("a"), Some(2.0));
        assert_eq!(s.param("missing"), None);
        assert_eq!(s.handle(0), None);
    }
}
