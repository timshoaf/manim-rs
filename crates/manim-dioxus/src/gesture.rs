//! Multi-touch gesture routing (FE-144): [`GestureRouter`] arbitrates between a
//! one-finger drag, a two-finger pinch/pan, and their desktop equivalents, and
//! [`PinchState`] turns two moving contacts into a zoom factor plus a pan.
//!
//! Like the rest of [`interaction`](crate::interaction), this is pure: it holds
//! pointer ids and element-pixel coordinates, no DOM types, so the whole gesture
//! *policy* is unit-tested headlessly and the browser handlers are a thin driver
//! (`onpointerdown` → [`GestureRouter::on_down`], and so on).
//!
//! # Why deltas, and why element fractions
//!
//! Pan is reported as a **element-fraction** delta and zoom as a *ratio*, both
//! differenced frame-to-frame in a camera-independent space. Differencing
//! camera-derived scene coordinates would feed the camera motion this very
//! gesture applies back into the next delta, and the view oscillates — the same
//! trap the orbit drag hit (see [`PointerState::frac`](crate::PointerState::frac)).
//!
//! # The gesture ladder
//!
//! One contact drags (handles, orbit — unchanged from the single-pointer
//! discipline). A second contact promotes the gesture to a pinch and *cancels*
//! the drag. Lifting one finger of a pinch does **not** fall back to dragging
//! with the survivor: the router latches [`GestureMode::Locked`] until every
//! contact is up, because a finger that merely happened to leave last would
//! otherwise yank whatever handle it is resting on.

/// One frame's accumulated zoom + pan intent, in camera-independent units.
///
/// `scale` is multiplicative (`1.0` = no zoom, `>1` = zoom in), `pan` is an
/// element-fraction translation of the *content* (finger direction, y down), and
/// `anchor` is the element-fraction point the zoom should keep fixed (a pinch's
/// centroid, or the cursor under a ctrl+wheel).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GestureDelta {
    /// Multiplicative zoom since the last poll. `1.0` when there was none.
    pub scale: f32,
    /// Element-fraction pan of the content since the last poll, y **down**.
    pub pan: (f32, f32),
    /// The element-fraction point the zoom is about (centroid / cursor).
    pub anchor: (f32, f32),
    /// Whether a zoom/pan gesture is in progress (a pinch or a pan drag).
    pub active: bool,
}

impl Default for GestureDelta {
    /// The identity gesture: no zoom, no pan, anchored at the element center.
    fn default() -> Self {
        Self {
            scale: 1.0,
            pan: (0.0, 0.0),
            anchor: (0.5, 0.5),
            active: false,
        }
    }
}

impl GestureDelta {
    /// Whether this frame carries no zoom and no pan (so the consumer can skip
    /// touching the camera at all).
    pub fn is_identity(&self) -> bool {
        self.scale == 1.0 && self.pan == (0.0, 0.0)
    }

    /// Folds another delta in: zoom composes multiplicatively, pan additively,
    /// and the latest anchor wins (the gesture's current centroid).
    pub fn compose(&mut self, other: GestureDelta) {
        self.scale *= other.scale;
        self.pan.0 += other.pan.0;
        self.pan.1 += other.pan.1;
        self.anchor = other.anchor;
        self.active |= other.active;
    }
}

/// Two-contact pinch math: frame-to-frame span **ratio** and centroid delta.
///
/// Fed the two live contact positions each move, it emits the zoom factor and
/// the pan since the previous move. Absolute (start-referenced) tracking would
/// be equivalent for a pure pinch, but the incremental form composes with a
/// clamped camera without ratcheting: if the zoom clamps at its limit, backing
/// off responds immediately instead of replaying the whole accumulated span.
///
/// ```
/// use manim_dioxus::gesture::PinchState;
/// let mut p = PinchState::new();
/// // First frame only latches the reference span/centroid.
/// assert!(p.update((0.0, 0.0), (100.0, 0.0)).is_none());
/// // Fingers spread to twice the span about the same centroid → 2× zoom.
/// let d = p.update((-50.0, 0.0), (150.0, 0.0)).unwrap();
/// assert!((d.scale - 2.0).abs() < 1e-5);
/// assert_eq!(d.pan, (0.0, 0.0));
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct PinchState {
    /// The previous `(span, centroid)` in element pixels, once seen.
    prev: Option<(f32, (f32, f32))>,
}

/// The smallest span (element px) treated as a real pinch. Two contacts closer
/// than this give a span ratio dominated by touch jitter, which reads as a
/// violent zoom spike.
const MIN_PINCH_SPAN: f32 = 8.0;

/// One pinch step: a span ratio, a centroid translation, and the centroid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinchDelta {
    /// Span ratio since the previous update (`>1` = fingers spreading).
    pub scale: f32,
    /// Centroid translation since the previous update, element pixels, y down.
    pub pan: (f32, f32),
    /// The current centroid, element pixels.
    pub centroid: (f32, f32),
}

impl PinchState {
    /// A pinch that has not seen a contact pair yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets the reference pair, so the next [`update`](Self::update) only
    /// re-latches. Call when the contact set changes (a finger lifted or a third
    /// landed) — otherwise the span jumps between two different finger pairs and
    /// the view snaps.
    pub fn reset(&mut self) {
        self.prev = None;
    }

    /// Whether a reference pair is latched (a pinch is under way).
    pub fn is_active(&self) -> bool {
        self.prev.is_some()
    }

    /// Advances from the two live contact positions (element px). Returns the
    /// step since the previous call, or `None` on the first call after a
    /// [`reset`](Self::reset) (which only latches the reference).
    pub fn update(&mut self, a: (f32, f32), b: (f32, f32)) -> Option<PinchDelta> {
        let span = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
        let centroid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
        let out = match self.prev {
            Some((prev_span, prev_c)) if prev_span >= MIN_PINCH_SPAN && span >= MIN_PINCH_SPAN => {
                Some(PinchDelta {
                    scale: span / prev_span,
                    pan: (centroid.0 - prev_c.0, centroid.1 - prev_c.1),
                    centroid,
                })
            }
            _ => None,
        };
        self.prev = Some((span, centroid));
        out
    }
}

/// What the router is currently doing with the contacts it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureMode {
    /// Nothing is pressed (the pointer may still hover).
    Idle,
    /// One contact is dragging — handles/orbit see a normal pressed pointer.
    Drag,
    /// A pan drag (middle button, or a modifier-held drag) moves the view.
    Pan,
    /// Two contacts are pinching — zoom + pan; the drag is cancelled.
    Pinch,
    /// A pinch ended but contacts remain: no drag, no pinch, until all lift.
    Locked,
}

/// What a pointer press is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerRole {
    /// A normal drag contact (touch, or the primary mouse button).
    Drag,
    /// A view-pan press (middle button, or space/modifier-held primary).
    Pan,
}

/// A tracked contact: its pointer id and last element-pixel position.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Contact {
    id: i32,
    x: f32,
    y: f32,
}

/// Routes up to two pointers into a drag, a pan, or a pinch (FE-144).
///
/// The driver calls [`on_down`](Self::on_down) / [`on_move`](Self::on_move) /
/// [`on_up`](Self::on_up) from the DOM handlers and, once per rendered frame,
/// [`take_gesture`](Self::take_gesture) to drain the accumulated zoom/pan.
/// [`pointer`](Self::pointer) and [`pressed`](Self::pressed) feed the existing
/// single-pointer path ([`DragSet`](crate::DragSet), orbit) unchanged.
///
/// ```
/// use manim_dioxus::gesture::{GestureMode, GestureRouter, PointerRole};
/// let mut r = GestureRouter::new();
/// r.set_size(100.0, 100.0);
/// r.on_down(1, 10.0, 10.0, PointerRole::Drag);
/// assert!(r.pressed()); // one finger still drags
/// r.on_down(2, 90.0, 10.0, PointerRole::Drag);
/// assert_eq!(r.mode(), GestureMode::Pinch);
/// assert!(!r.pressed()); // ...and the drag is cancelled, not left latched
/// ```
#[derive(Debug, Clone, Default)]
pub struct GestureRouter {
    contacts: Vec<Contact>,
    mode: Option<GestureMode>,
    pinch: PinchState,
    /// Accumulated zoom/pan since the last [`take_gesture`](Self::take_gesture).
    pending: Option<GestureDelta>,
    /// Last position any admitted pointer reported (drives hover when idle).
    last: (f32, f32),
    /// Element size in CSS px, for converting pan px → fractions.
    size: (f32, f32),
}

impl GestureRouter {
    /// A router with no contacts, in [`GestureMode::Idle`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the element size (CSS px) used to express pan in element fractions.
    /// Degenerate sizes are ignored, so pan simply stays zero until a real size
    /// arrives.
    pub fn set_size(&mut self, w: f32, h: f32) {
        if w > 0.0 && h > 0.0 {
            self.size = (w, h);
        }
    }

    /// The current mode.
    pub fn mode(&self) -> GestureMode {
        self.mode.unwrap_or(GestureMode::Idle)
    }

    /// Whether a one-finger drag is in progress (what handle dragging reads).
    /// `false` during a pinch, a pan, and the post-pinch lock.
    pub fn pressed(&self) -> bool {
        self.mode() == GestureMode::Drag
    }

    /// The pointer position to report (element px): the dragging contact while
    /// dragging, else the last position seen (hover).
    pub fn pointer(&self) -> (f32, f32) {
        match self.contacts.first() {
            Some(c) if self.mode() == GestureMode::Drag => (c.x, c.y),
            _ => self.last,
        }
    }

    /// [`pointer`](Self::pointer) as an element fraction — the anchor a
    /// cursor-centred desktop zoom needs.
    pub fn pointer_fraction(&self) -> (f32, f32) {
        self.anchor_fraction(self.pointer())
    }

    /// How many contacts are down.
    pub fn contact_count(&self) -> usize {
        self.contacts.len()
    }

    /// A press. `role` distinguishes a normal drag from a view pan (middle
    /// button / modifier). Returns whether the press was adopted; a third
    /// contact, or any press while the router is locked, is ignored.
    pub fn on_down(&mut self, id: i32, x: f32, y: f32, role: PointerRole) -> bool {
        self.last = (x, y);
        if self.mode() == GestureMode::Locked || self.contacts.iter().any(|c| c.id == id) {
            return false;
        }
        match (self.contacts.len(), role) {
            (0, PointerRole::Pan) => {
                self.contacts.push(Contact { id, x, y });
                self.mode = Some(GestureMode::Pan);
                true
            }
            (0, PointerRole::Drag) => {
                self.contacts.push(Contact { id, x, y });
                self.mode = Some(GestureMode::Drag);
                true
            }
            (1, PointerRole::Drag) if self.mode() == GestureMode::Drag => {
                self.contacts.push(Contact { id, x, y });
                // Promote to a pinch: the drag is cancelled *this instant*, so a
                // handle grabbed by the first finger is dropped where it lies
                // rather than being flung by the centroid.
                self.mode = Some(GestureMode::Pinch);
                // Latch the reference span/centroid *now*, from the true
                // two-finger start. Latching lazily on the first move instead
                // would silently swallow that first step, and a symmetric
                // spread would come out as a small net pan.
                self.pinch.reset();
                if let [a, b] = self.contacts[..] {
                    self.pinch.update((a.x, a.y), (b.x, b.y));
                }
                true
            }
            // A third contact (or a pan press mid-gesture) is ignored outright.
            _ => false,
        }
    }

    /// A move. Returns whether it changed anything the driver should redraw for.
    pub fn on_move(&mut self, id: i32, x: f32, y: f32) -> bool {
        if self.contacts.is_empty() {
            // Hover: any pointer may move the cursor while nothing is pressed.
            self.last = (x, y);
            return true;
        }
        let Some(c) = self.contacts.iter_mut().find(|c| c.id == id) else {
            return false; // not part of the active gesture
        };
        let (px, py) = (c.x, c.y);
        c.x = x;
        c.y = y;
        self.last = (x, y);
        match self.mode() {
            GestureMode::Pinch => {
                if let [a, b] = self.contacts[..] {
                    if let Some(d) = self.pinch.update((a.x, a.y), (b.x, b.y)) {
                        let g = GestureDelta {
                            scale: d.scale,
                            pan: self.to_fraction(d.pan),
                            anchor: self.anchor_fraction(d.centroid),
                            active: true,
                        };
                        self.push(g);
                    }
                }
                true
            }
            GestureMode::Pan => {
                let g = GestureDelta {
                    scale: 1.0,
                    pan: self.to_fraction((x - px, y - py)),
                    anchor: self.anchor_fraction((x, y)),
                    active: true,
                };
                self.push(g);
                true
            }
            _ => true,
        }
    }

    /// A release (or a `pointercancel`, which is identical as far as gesture
    /// state goes). Lifting one finger of a pinch latches
    /// [`GestureMode::Locked`] until every contact is up.
    pub fn on_up(&mut self, id: i32) {
        let was = self.mode();
        self.contacts.retain(|c| c.id != id);
        self.pinch.reset();
        self.mode = Some(if self.contacts.is_empty() {
            GestureMode::Idle
        } else if was == GestureMode::Pinch || was == GestureMode::Locked {
            GestureMode::Locked
        } else {
            was
        });
    }

    /// Drops every contact (a lost pointer, an unmount): back to idle with no
    /// pending gesture.
    pub fn cancel_all(&mut self) {
        self.contacts.clear();
        self.pinch.reset();
        self.mode = Some(GestureMode::Idle);
    }

    /// Queues a desktop zoom: `notches` of ctrl+wheel about the element-fraction
    /// `anchor`, applied with the same `step` per notch a pinch would reach.
    /// Positive `notches` zoom in.
    pub fn push_wheel_zoom(&mut self, notches: f32, step: f32, anchor: (f32, f32)) {
        self.push(GestureDelta {
            scale: step.powf(notches),
            pan: (0.0, 0.0),
            anchor,
            active: false,
        });
    }

    /// Drains the zoom/pan accumulated since the last call. Returns the identity
    /// delta when nothing happened.
    pub fn take_gesture(&mut self) -> GestureDelta {
        self.pending.take().unwrap_or_default()
    }

    /// Whether a zoom/pan is pending (so the driver knows to keep drawing).
    pub fn has_pending_gesture(&self) -> bool {
        self.pending.is_some()
    }

    /// Folds a delta into the pending accumulator.
    fn push(&mut self, g: GestureDelta) {
        match &mut self.pending {
            Some(p) => p.compose(g),
            None => self.pending = Some(g),
        }
    }

    /// Element-pixel delta → element fraction.
    fn to_fraction(&self, (dx, dy): (f32, f32)) -> (f32, f32) {
        if self.size.0 > 0.0 && self.size.1 > 0.0 {
            (dx / self.size.0, dy / self.size.1)
        } else {
            (0.0, 0.0)
        }
    }

    /// Element-pixel position → element fraction (the zoom anchor).
    fn anchor_fraction(&self, (x, y): (f32, f32)) -> (f32, f32) {
        if self.size.0 > 0.0 && self.size.1 > 0.0 {
            (x / self.size.0, y / self.size.1)
        } else {
            (0.5, 0.5)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> GestureRouter {
        let mut r = GestureRouter::new();
        r.set_size(100.0, 100.0);
        r
    }

    #[test]
    fn pinch_first_update_only_latches() {
        let mut p = PinchState::new();
        assert!(p.update((0.0, 0.0), (50.0, 0.0)).is_none());
        assert!(p.is_active());
        assert!(p.update((0.0, 0.0), (50.0, 0.0)).is_some());
    }

    #[test]
    fn pinch_spread_zooms_in_and_squeeze_zooms_out() {
        let mut p = PinchState::new();
        p.update((0.0, 0.0), (100.0, 0.0));
        let out = p.update((-50.0, 0.0), (150.0, 0.0)).unwrap();
        assert!((out.scale - 2.0).abs() < 1e-5);
        p.reset();
        p.update((0.0, 0.0), (100.0, 0.0));
        let inn = p.update((25.0, 0.0), (75.0, 0.0)).unwrap();
        assert!((inn.scale - 0.5).abs() < 1e-5);
    }

    #[test]
    fn pinch_centroid_translation_is_the_pan() {
        let mut p = PinchState::new();
        p.update((0.0, 0.0), (100.0, 0.0)); // centroid (50, 0)
                                            // Both fingers slide +10x/+4y: span unchanged, centroid moves.
        let d = p.update((10.0, 4.0), (110.0, 4.0)).unwrap();
        assert!((d.scale - 1.0).abs() < 1e-6);
        assert!((d.pan.0 - 10.0).abs() < 1e-5 && (d.pan.1 - 4.0).abs() < 1e-5);
        assert!((d.centroid.0 - 60.0).abs() < 1e-5);
    }

    #[test]
    fn degenerate_span_does_not_spike_the_zoom() {
        let mut p = PinchState::new();
        // Two contacts on top of each other: no ratio is reported at all.
        p.update((0.0, 0.0), (1.0, 0.0));
        assert!(p.update((0.0, 0.0), (2.0, 0.0)).is_none());
    }

    #[test]
    fn one_contact_still_drags() {
        let mut r = router();
        assert!(r.on_down(7, 20.0, 30.0, PointerRole::Drag));
        assert_eq!(r.mode(), GestureMode::Drag);
        assert!(r.pressed());
        r.on_move(7, 25.0, 30.0);
        assert_eq!(r.pointer(), (25.0, 30.0));
        assert!(r.take_gesture().is_identity()); // a drag is not a view gesture
        r.on_up(7);
        assert_eq!(r.mode(), GestureMode::Idle);
        assert!(!r.pressed());
    }

    #[test]
    fn second_contact_promotes_to_pinch_and_cancels_the_drag() {
        let mut r = router();
        r.on_down(1, 10.0, 50.0, PointerRole::Drag);
        assert!(r.pressed());
        r.on_down(2, 90.0, 50.0, PointerRole::Drag);
        assert_eq!(r.mode(), GestureMode::Pinch);
        assert!(!r.pressed(), "the drag must be released, not left latched");
        // Spread about the centroid → zoom, no pan.
        r.on_move(1, 0.0, 50.0);
        r.on_move(2, 100.0, 50.0);
        let g = r.take_gesture();
        assert!(g.active && g.scale > 1.0);
        assert!(g.pan.0.abs() < 1e-5 && g.pan.1.abs() < 1e-5);
        assert!((g.anchor.0 - 0.5).abs() < 1e-5);
    }

    #[test]
    fn lifting_one_finger_of_a_pinch_does_not_start_a_drag() {
        let mut r = router();
        r.on_down(1, 10.0, 50.0, PointerRole::Drag);
        r.on_down(2, 90.0, 50.0, PointerRole::Drag);
        r.on_up(2);
        assert_eq!(r.mode(), GestureMode::Locked);
        assert!(!r.pressed(), "the survivor must not grab anything");
        // ...and moving it neither drags nor pans.
        r.on_move(1, 40.0, 50.0);
        assert!(!r.pressed());
        assert!(r.take_gesture().is_identity());
        // Only lifting everything unlocks.
        r.on_up(1);
        assert_eq!(r.mode(), GestureMode::Idle);
        r.on_down(3, 10.0, 10.0, PointerRole::Drag);
        assert!(r.pressed());
    }

    #[test]
    fn a_press_while_locked_is_ignored() {
        let mut r = router();
        r.on_down(1, 10.0, 50.0, PointerRole::Drag);
        r.on_down(2, 90.0, 50.0, PointerRole::Drag);
        r.on_up(1);
        assert_eq!(r.mode(), GestureMode::Locked);
        assert!(!r.on_down(9, 50.0, 50.0, PointerRole::Drag));
        assert_eq!(r.mode(), GestureMode::Locked);
    }

    #[test]
    fn a_third_contact_is_ignored() {
        let mut r = router();
        r.on_down(1, 10.0, 50.0, PointerRole::Drag);
        r.on_down(2, 90.0, 50.0, PointerRole::Drag);
        assert!(!r.on_down(3, 50.0, 10.0, PointerRole::Drag));
        assert_eq!(r.contact_count(), 2);
        // The stray contact's moves do not perturb the pinch.
        r.on_move(3, 55.0, 10.0);
        assert!(r.take_gesture().is_identity());
    }

    #[test]
    fn pan_role_pans_without_pressing() {
        let mut r = router();
        r.on_down(1, 50.0, 50.0, PointerRole::Pan);
        assert_eq!(r.mode(), GestureMode::Pan);
        assert!(!r.pressed(), "a pan drag must not grab handles");
        r.on_move(1, 60.0, 40.0);
        let g = r.take_gesture();
        assert!(g.active);
        assert!((g.pan.0 - 0.1).abs() < 1e-5 && (g.pan.1 + 0.1).abs() < 1e-5);
        assert!((g.scale - 1.0).abs() < 1e-6);
    }

    #[test]
    fn wheel_zoom_anchors_at_the_cursor() {
        let mut r = router();
        r.push_wheel_zoom(1.0, 1.2, (0.25, 0.75));
        let g = r.take_gesture();
        assert!((g.scale - 1.2).abs() < 1e-5);
        assert_eq!(g.anchor, (0.25, 0.75));
        // Drained: the next poll is the identity.
        assert!(r.take_gesture().is_identity());
    }

    #[test]
    fn gestures_accumulate_between_frames() {
        let mut r = router();
        r.push_wheel_zoom(1.0, 1.5, (0.5, 0.5));
        r.push_wheel_zoom(1.0, 1.5, (0.5, 0.5));
        let g = r.take_gesture();
        assert!(
            (g.scale - 2.25).abs() < 1e-5,
            "zoom composes multiplicatively"
        );
    }

    #[test]
    fn hover_moves_are_tracked_while_idle() {
        let mut r = router();
        assert!(r.on_move(4, 33.0, 44.0));
        assert_eq!(r.pointer(), (33.0, 44.0));
        assert!(!r.pressed());
    }

    #[test]
    fn cancel_all_returns_to_idle() {
        let mut r = router();
        r.on_down(1, 10.0, 50.0, PointerRole::Drag);
        r.on_down(2, 90.0, 50.0, PointerRole::Drag);
        r.cancel_all();
        assert_eq!(r.mode(), GestureMode::Idle);
        assert_eq!(r.contact_count(), 0);
        assert!(r.on_down(5, 1.0, 1.0, PointerRole::Drag));
    }
}
