//! Pure interaction state machines for the widgets (FE-139): [`DragSet`] (a set
//! of draggable scene handles with hover/grab/pointer-capture) and [`OrbitState`]
//! (turntable camera angles + zoom from pointer/wheel deltas).
//!
//! Like [`RenderSchedule`](crate::RenderSchedule) and
//! [`PlayerState`](crate::PlayerState), these hold **no** dioxus, GPU, or wasm
//! types — the whole interaction *policy* is here and unit-tested headlessly; the
//! browser glue (a `Figure`'s pointer/wheel handlers, a `LiveUpdater`) is a thin
//! driver over them.

use manim_core::prelude::Point;

use crate::constraint::DragConstraint;

/// A set of draggable handles at scene-space positions, with hover detection and
/// pointer capture.
///
/// The driver feeds it `(pointer_position, pressed)` each frame via
/// [`update`](Self::update); it returns which handle (if any) moved, so the
/// caller can rebuild/resample. Grabbing captures the pointer: once a handle is
/// grabbed it follows the cursor until release, even outside its hit radius.
///
/// ```
/// use manim_dioxus::interaction::DragSet;
/// use manim_core::prelude::Point;
/// let mut d = DragSet::new(vec![Point::new(0.0, 0.0, 0.0)], 0.3);
/// // Press on the handle center (no move yet), then drag it.
/// assert_eq!(d.update(Point::new(0.0, 0.0, 0.0), true), None); // grab
/// assert_eq!(d.update(Point::new(1.0, 0.0, 0.0), true), Some(0)); // dragged by (1,0)
/// assert_eq!(d.position(0).x, 1.0);
/// ```
#[derive(Debug, Clone)]
pub struct DragSet {
    positions: Vec<Point>,
    radius: f32,
    hovered: Option<usize>,
    grabbed: Option<usize>,
    /// Offset (handle − cursor) captured at grab, so the handle keeps its
    /// relative position under the cursor instead of snapping to it.
    grab_offset: Point,
    /// The grabbed handle's position at grab time — the anchor a
    /// [`DragConstraint::Axis`] rail passes through.
    grab_anchor: Point,
    was_pressed: bool,
    /// Per-handle movement constraints (FE-145), parallel to `positions`.
    constraints: Vec<DragConstraint>,
}

impl DragSet {
    /// A drag set over `positions`, each grabbable within `radius` scene units.
    pub fn new(positions: Vec<Point>, radius: f32) -> Self {
        let constraints = vec![DragConstraint::Free; positions.len()];
        Self {
            positions,
            radius: radius.max(0.0),
            hovered: None,
            grabbed: None,
            grab_offset: Point::ZERO,
            grab_anchor: Point::ZERO,
            was_pressed: false,
            constraints,
        }
    }

    /// Constrains handle `i` (builder form) — see [`DragConstraint`].
    ///
    /// ```
    /// use manim_dioxus::{DragSet, constraint::DragConstraint};
    /// use manim_core::prelude::Point;
    /// // A handle that only slides horizontally.
    /// let mut d = DragSet::new(vec![Point::ZERO], 0.3)
    ///     .with_constraint(0, DragConstraint::Axis(Point::new(1.0, 0.0, 0.0)));
    /// d.update(Point::ZERO, true);
    /// d.update(Point::new(2.0, 5.0, 0.0), true);
    /// assert_eq!(d.position(0), Point::new(2.0, 0.0, 0.0));
    /// ```
    pub fn with_constraint(mut self, i: usize, c: DragConstraint) -> Self {
        self.set_constraint(i, c);
        self
    }

    /// Constrains handle `i` in place. Out-of-range indices are ignored.
    pub fn set_constraint(&mut self, i: usize, c: DragConstraint) {
        if let Some(slot) = self.constraints.get_mut(i) {
            *slot = c;
        }
    }

    /// Handle `i`'s constraint.
    pub fn constraint(&self, i: usize) -> &DragConstraint {
        &self.constraints[i]
    }

    /// Current handle positions.
    pub fn positions(&self) -> &[Point] {
        &self.positions
    }

    /// The position of handle `i`.
    pub fn position(&self, i: usize) -> Point {
        self.positions[i]
    }

    /// Overwrites handle `i`'s position (e.g. a slider set it externally).
    pub fn set_position(&mut self, i: usize, p: Point) {
        self.positions[i] = p;
    }

    /// The handle under the cursor (grabbed one while dragging), for a hover
    /// affordance. `None` when the cursor is over empty space.
    pub fn hovered(&self) -> Option<usize> {
        self.hovered
    }

    /// The handle currently being dragged, if any.
    pub fn grabbed(&self) -> Option<usize> {
        self.grabbed
    }

    /// Whether a handle is being dragged this frame.
    pub fn is_dragging(&self) -> bool {
        self.grabbed.is_some()
    }

    /// The **nearest** handle within `radius` of `p`, or `None`. Nearest wins, so
    /// overlapping handles resolve to the closest one deterministically.
    pub fn hit_test(&self, p: Point) -> Option<usize> {
        self.hit_test_dist(p).map(|(i, _)| i)
    }

    /// [`hit_test`](Self::hit_test) plus the distance to that handle's center,
    /// for arbitrating against a mobject hit
    /// ([`resolve_hit`](crate::constraint::resolve_hit)).
    pub fn hit_test_dist(&self, p: Point) -> Option<(usize, f32)> {
        self.positions
            .iter()
            .enumerate()
            .map(|(i, q)| (i, dist2(*q, p)))
            .filter(|(_, d2)| *d2 <= self.radius * self.radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, d2)| (i, d2.sqrt()))
    }

    /// Advances the state machine one frame from `(pointer, pressed)`. Returns
    /// `Some(i)` when handle `i` moved (so the caller resamples), else `None`.
    ///
    /// State: a press grabs the nearest handle under the cursor; while grabbed
    /// the handle tracks the pointer (capture — no radius check); release lets
    /// go. Hover is reported only when not grabbing.
    pub fn update(&mut self, pointer: Point, pressed: bool) -> Option<usize> {
        let just_pressed = pressed && !self.was_pressed;
        let just_released = !pressed && self.was_pressed;
        self.was_pressed = pressed;

        if just_pressed {
            self.grabbed = self.hit_test(pointer);
            if let Some(i) = self.grabbed {
                self.grab_offset = self.positions[i] - pointer;
                self.grab_anchor = self.positions[i];
            }
        }
        if just_released {
            self.grabbed = None;
        }

        self.hovered = match self.grabbed {
            Some(i) => Some(i),
            None => self.hit_test(pointer),
        };

        if let Some(i) = self.grabbed {
            // Pointer capture: follow the cursor (keeping the grab offset)
            // regardless of radius.
            // The cursor asks; the constraint decides (a free handle takes the
            // request verbatim, so the unconstrained path is unchanged).
            let target = self.constraints[i].apply(self.grab_anchor, pointer + self.grab_offset);
            if self.positions[i] != target {
                self.positions[i] = target;
                return Some(i);
            }
        }
        None
    }
}

/// Squared xy-distance (handles live in the z=0 plane).
fn dist2(a: Point, b: Point) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

/// Turntable-camera orbit state: polar/azimuth angles plus a zoom factor, driven
/// by pointer drags and wheel notches.
///
/// Feeds `Camera2D::set_camera_orientation(phi, theta)` and `ThreeDParams.zoom`.
/// Angles are clamped so the camera can't flip over the pole; zoom is clamped to
/// a sane band.
///
/// ```
/// use manim_dioxus::interaction::OrbitState;
/// let mut o = OrbitState::new(1.0, 0.0);
/// let (phi0, theta0) = (o.phi, o.theta);
/// o.drag(0.4, 0.0); // horizontal drag spins azimuth
/// assert!(o.theta != theta0 && (o.phi - phi0).abs() < 1e-6);
/// o.zoom_by(1.0); // one notch in
/// assert!(o.zoom > 1.0);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct OrbitState {
    /// Polar angle from `+z` (radians), clamped to `(phi_min, phi_max)`.
    pub phi: f32,
    /// Azimuth (radians), free to wrap.
    pub theta: f32,
    /// Frame zoom factor (`ThreeDParams.zoom`), clamped to `(zoom_min, zoom_max)`.
    pub zoom: f32,
    sensitivity: f32,
    zoom_step: f32,
    phi_min: f32,
    phi_max: f32,
    zoom_min: f32,
    zoom_max: f32,
}

impl OrbitState {
    /// An orbit at `(phi, theta)` with default sensitivity, zoom `1.0`, and a
    /// half-space polar clamp `(0.05, π/2)` (top-down to horizon).
    pub fn new(phi: f32, theta: f32) -> Self {
        Self {
            phi,
            theta,
            zoom: 1.0,
            sensitivity: 3.5,
            zoom_step: 1.15,
            phi_min: 0.05,
            phi_max: std::f32::consts::FRAC_PI_2,
            zoom_min: 0.4,
            zoom_max: 4.0,
        }
    }

    /// Sets the drag sensitivity (radians per unit of pointer travel; drivers
    /// feed element-fraction deltas, so units are full canvas traversals).
    pub fn with_sensitivity(mut self, s: f32) -> Self {
        self.sensitivity = s.max(0.0);
        self
    }

    /// Sets the polar-angle clamp (radians). Use `(0.05, π−0.05)` for a full
    /// over-the-top orbit, `(0.05, π/2)` (default) to stay above the horizon.
    pub fn with_phi_range(mut self, min: f32, max: f32) -> Self {
        self.phi_min = min;
        self.phi_max = max;
        self.phi = self.phi.clamp(min, max);
        self
    }

    /// Applies a pointer drag `(dx, dy)`: horizontal spins the azimuth,
    /// vertical tilts the polar angle (clamped). Feed deltas of a
    /// camera-independent coordinate (element fractions, y up) — deltas of
    /// camera-derived scene positions feed back into the orbit and oscillate.
    pub fn drag(&mut self, dx: f32, dy: f32) {
        self.theta -= dx * self.sensitivity;
        self.phi = (self.phi + dy * self.sensitivity).clamp(self.phi_min, self.phi_max);
    }

    /// Applies `notches` of wheel zoom (positive = zoom in), multiplicatively and
    /// clamped. One notch multiplies/divides by the zoom step.
    pub fn zoom_by(&mut self, notches: f32) {
        self.zoom = (self.zoom * self.zoom_step.powf(notches)).clamp(self.zoom_min, self.zoom_max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y, 0.0)
    }

    #[test]
    fn hit_test_nearest_wins() {
        let d = DragSet::new(vec![p(0.0, 0.0), p(1.0, 0.0)], 0.6);
        // Closer to handle 1.
        assert_eq!(d.hit_test(p(0.9, 0.0)), Some(1));
        // Closer to handle 0.
        assert_eq!(d.hit_test(p(0.2, 0.0)), Some(0));
    }

    #[test]
    fn hit_test_misses_outside_radius() {
        let d = DragSet::new(vec![p(0.0, 0.0)], 0.3);
        assert_eq!(d.hit_test(p(1.0, 1.0)), None);
    }

    #[test]
    fn press_grabs_nearest_then_drag_moves_it() {
        let mut d = DragSet::new(vec![p(0.0, 0.0), p(2.0, 0.0)], 0.4);
        // Press on handle 0's center → grab, no move (offset zero).
        assert_eq!(d.update(p(0.0, 0.0), true), None);
        assert_eq!(d.grabbed(), Some(0));
        // Drag it to a new spot.
        assert_eq!(d.update(p(0.5, 0.5), true), Some(0));
        assert_eq!(d.position(0), p(0.5, 0.5));
        // Release lets go.
        assert_eq!(d.update(p(0.5, 0.5), false), None);
        assert_eq!(d.grabbed(), None);
    }

    #[test]
    fn pointer_capture_follows_outside_radius() {
        let mut d = DragSet::new(vec![p(0.0, 0.0)], 0.2);
        d.update(p(0.0, 0.0), true); // grab at center (offset zero)
                                     // Yank far past the radius — capture keeps it attached.
        assert_eq!(d.update(p(5.0, 5.0), true), Some(0));
        assert_eq!(d.position(0), p(5.0, 5.0));
    }

    #[test]
    fn pressing_empty_space_grabs_nothing() {
        let mut d = DragSet::new(vec![p(0.0, 0.0)], 0.2);
        assert_eq!(d.update(p(3.0, 3.0), true), None);
        assert_eq!(d.grabbed(), None);
        assert_eq!(d.update(p(3.5, 3.5), true), None); // still nothing captured
    }

    #[test]
    fn hover_reports_without_press() {
        let mut d = DragSet::new(vec![p(0.0, 0.0)], 0.3);
        d.update(p(0.1, 0.0), false);
        assert_eq!(d.hovered(), Some(0));
        assert!(!d.is_dragging());
        d.update(p(2.0, 0.0), false);
        assert_eq!(d.hovered(), None);
    }

    #[test]
    fn a_constrained_handle_snaps_to_its_grid_while_dragging() {
        let mut d =
            DragSet::new(vec![p(0.0, 0.0)], 0.4).with_constraint(0, DragConstraint::Grid(0.5));
        d.update(p(0.0, 0.0), true);
        d.update(p(1.2, -0.9), true);
        assert_eq!(d.position(0), p(1.0, -1.0));
    }

    #[test]
    fn a_region_constrained_handle_cannot_leave_its_box() {
        use manim_core::mobject::BoundingBox;
        let mut d = DragSet::new(vec![p(0.0, 0.0)], 0.4).with_constraint(
            0,
            DragConstraint::Region(BoundingBox::new(p(-1.0, -1.0), p(1.0, 1.0))),
        );
        d.update(p(0.0, 0.0), true);
        d.update(p(9.0, 9.0), true);
        assert_eq!(d.position(0), p(1.0, 1.0));
    }

    #[test]
    fn a_curve_constrained_handle_rides_the_rail() {
        use crate::constraint::CurveRail;
        let rail = CurveRail::new(|t: f32| p(t.cos(), t.sin()), 0.0, std::f32::consts::TAU);
        let mut d =
            DragSet::new(vec![p(1.0, 0.0)], 0.4).with_constraint(0, DragConstraint::Curve(rail));
        d.update(p(1.0, 0.0), true);
        d.update(p(0.1, 3.0), true); // yanked far off the circle
        let q = d.position(0);
        assert!(
            (q.x * q.x + q.y * q.y - 1.0).abs() < 1e-3,
            "{q:?} left the unit circle"
        );
        assert!(q.y > 0.9, "it should ride round to the top: {q:?}");
    }

    #[test]
    fn the_axis_rail_passes_through_the_grab_point_not_the_origin() {
        let mut d = DragSet::new(vec![p(0.0, 2.0)], 0.5)
            .with_constraint(0, DragConstraint::Axis(p(1.0, 0.0)));
        // Grab slightly off-center: the offset is kept, the rail is y = 2.
        d.update(p(0.2, 2.0), true);
        d.update(p(3.2, -5.0), true);
        assert_eq!(d.position(0), p(3.0, 2.0));
    }

    #[test]
    fn hit_test_dist_reports_the_distance_for_arbitration() {
        let d = DragSet::new(vec![p(0.0, 0.0)], 0.5);
        let (i, dist) = d.hit_test_dist(p(0.3, 0.0)).expect("hit");
        assert_eq!(i, 0);
        assert!((dist - 0.3).abs() < 1e-6);
        assert!(d.hit_test_dist(p(0.6, 0.0)).is_none());
    }

    #[test]
    fn orbit_drag_spins_azimuth_and_clamps_phi() {
        let mut o = OrbitState::new(1.0, 0.0).with_sensitivity(0.5);
        o.drag(1.0, 0.0); // dx=1 → theta -= 0.5
        assert!((o.theta - (-0.5)).abs() < 1e-6);
        assert!((o.phi - 1.0).abs() < 1e-6);
        // Push phi past the top clamp.
        for _ in 0..100 {
            o.drag(0.0, 1.0);
        }
        assert!((o.phi - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn orbit_zoom_is_multiplicative_and_clamped() {
        let mut o = OrbitState::new(1.0, 0.0);
        let z0 = o.zoom;
        o.zoom_by(1.0);
        assert!(o.zoom > z0);
        // Zoom in hard → clamps at max.
        for _ in 0..100 {
            o.zoom_by(1.0);
        }
        assert!((o.zoom - 4.0).abs() < 1e-6);
        // And out to the min.
        for _ in 0..100 {
            o.zoom_by(-1.0);
        }
        assert!((o.zoom - 0.4).abs() < 1e-6);
    }
}
