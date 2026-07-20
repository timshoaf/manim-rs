//! Drag constraints and mobject hit-testing (FE-145).
//!
//! [`DragConstraint`] projects a handle's desired position onto whatever it is
//! allowed to move on — a rail, a lattice, a curve, a box — and is applied
//! inside [`DragSet::update`](crate::DragSet::update), so every driver gets the
//! constrained behaviour for free. [`hit_mobject`] and [`nearest_mobject`] widen
//! hit-testing from "handles only" to *any* mobject already in the scene, which
//! is what turns a plotted graph or a vector tip into something you can grab.
//!
//! All of it is pure geometry over [`Point`] / [`SceneState`] — no DOM, no GPU —
//! so the projection math is unit-tested natively.

use std::rc::Rc;

use manim_core::mobject::{AnyId, BoundingBox};
use manim_core::prelude::Point;
use manim_core::scene_state::SceneState;

/// How many samples per cubic segment [`hit_mobject`] walks when measuring the
/// distance from the cursor to a mobject's outline. Enough to resolve a tight
/// curve without making a hit test cost a redraw.
const HIT_SAMPLES_PER_CURVE: usize = 8;

/// A parametric rail a handle can be pinned to: `t ↦ point`, over `t_range`.
///
/// Projection is a coarse scan over `samples` followed by a ternary refinement
/// in the winning bracket. A curve rail is *not* required to be
/// arc-length-parameterized or even monotone — the scan is what makes an
/// arbitrary closure safe here.
#[derive(Clone)]
pub struct CurveRail {
    f: Rc<dyn Fn(f32) -> Point>,
    t_range: (f32, f32),
    samples: usize,
}

impl CurveRail {
    /// A rail over `t ∈ [t0, t1]`, scanned at 64 samples.
    pub fn new(f: impl Fn(f32) -> Point + 'static, t0: f32, t1: f32) -> Self {
        Self {
            f: Rc::new(f),
            t_range: (t0, t1),
            samples: 64,
        }
    }

    /// Sets the coarse-scan resolution (clamped to at least 2). Raise it for a
    /// wiggly rail whose nearest point the default scan could bracket wrongly.
    pub fn with_samples(mut self, n: usize) -> Self {
        self.samples = n.max(2);
        self
    }

    /// The rail point at parameter `t` (clamped to the range).
    pub fn point(&self, t: f32) -> Point {
        let (t0, t1) = self.t_range;
        (self.f)(t.clamp(t0.min(t1), t0.max(t1)))
    }

    /// The parameter of the nearest rail point to `p`, and that point.
    pub fn project(&self, p: Point) -> (f32, Point) {
        let (t0, t1) = self.t_range;
        let step = (t1 - t0) / self.samples as f32;
        // Coarse scan for the best sample...
        let mut best = (t0, dist2(self.point(t0), p));
        for i in 1..=self.samples {
            let t = t0 + step * i as f32;
            let d = dist2(self.point(t), p);
            if d < best.1 {
                best = (t, d);
            }
        }
        // ...then a ternary search inside the bracketing interval. The distance
        // is not globally unimodal, but it is within one scan cell of the coarse
        // minimum, which is all the refinement assumes.
        let (mut lo, mut hi) = (
            (best.0 - step).max(t0.min(t1)),
            (best.0 + step).min(t0.max(t1)),
        );
        for _ in 0..40 {
            let m1 = lo + (hi - lo) / 3.0;
            let m2 = hi - (hi - lo) / 3.0;
            if dist2(self.point(m1), p) < dist2(self.point(m2), p) {
                hi = m2;
            } else {
                lo = m1;
            }
        }
        let t = (lo + hi) * 0.5;
        (t, self.point(t))
    }
}

impl std::fmt::Debug for CurveRail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CurveRail({:?}, {} samples)", self.t_range, self.samples)
    }
}

/// What a dragged handle is allowed to do (FE-145).
///
/// Applied by [`DragSet`](crate::DragSet) to the position the cursor asks for,
/// relative to the `anchor` captured when the handle was grabbed.
///
/// ```
/// use manim_dioxus::constraint::DragConstraint;
/// use manim_core::prelude::Point;
/// let rail = DragConstraint::Axis(Point::new(1.0, 0.0, 0.0));
/// let anchor = Point::new(0.0, 2.0, 0.0);
/// // Only the along-axis component survives; the handle stays on its rail.
/// let got = rail.apply(anchor, Point::new(3.0, 9.0, 0.0));
/// assert_eq!(got, Point::new(3.0, 2.0, 0.0));
/// ```
#[derive(Clone, Debug, Default)]
pub enum DragConstraint {
    /// Unconstrained: the handle goes wherever the cursor does.
    #[default]
    Free,
    /// Pinned to the line through the grab anchor along this direction. A zero
    /// direction degenerates to "cannot move", which is the honest reading.
    Axis(Point),
    /// Snapped to an absolute lattice of the given pitch (both x and y). Pitches
    /// `<= 0` are ignored (treated as [`Free`](Self::Free)).
    Grid(f32),
    /// Pinned to the nearest point of a parametric rail.
    Curve(CurveRail),
    /// Clamped into an axis-aligned box.
    Region(BoundingBox),
}

impl DragConstraint {
    /// Projects `desired` onto whatever this constraint allows, given the
    /// position the handle was grabbed at (`anchor`, which only
    /// [`Axis`](Self::Axis) consults).
    pub fn apply(&self, anchor: Point, desired: Point) -> Point {
        match self {
            Self::Free => desired,
            Self::Axis(dir) => {
                let len = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt();
                if len <= f32::EPSILON {
                    return anchor;
                }
                let unit = *dir / len;
                let along = (desired - anchor).dot(unit);
                anchor + unit * along
            }
            Self::Grid(step) => {
                if *step <= 0.0 {
                    desired
                } else {
                    Point::new(
                        (desired.x / step).round() * step,
                        (desired.y / step).round() * step,
                        desired.z,
                    )
                }
            }
            Self::Curve(rail) => rail.project(desired).1,
            Self::Region(b) => Point::new(
                desired.x.clamp(b.min.x, b.max.x),
                desired.y.clamp(b.min.y, b.max.y),
                desired.z.clamp(b.min.z, b.max.z),
            ),
        }
    }
}

/// The distance from `p` to a mobject family's outline, or `None` if the cursor
/// is farther than `tol` from it.
///
/// Two stages, cheap first: the family bounding box grown by `tol` rejects
/// almost everything, and only survivors pay for walking sampled path points.
/// The reported distance is to the **outline**, not the center, so a big
/// thin-stroked shape is grabbable along its edge rather than only at its
/// middle.
pub fn hit_mobject(state: &SceneState, id: impl Into<AnyId>, p: Point, tol: f32) -> Option<f32> {
    let id = id.into();
    if !state.contains(id) {
        return None;
    }
    let bb = state.family_bounding_box(id);
    if p.x < bb.min.x - tol || p.x > bb.max.x + tol || p.y < bb.min.y - tol || p.y > bb.max.y + tol
    {
        return None;
    }
    let mut best = f32::INFINITY;
    for member in state.family(id) {
        let path = &state.get_dyn(member).data().path;
        for q in path.points(HIT_SAMPLES_PER_CURVE) {
            best = best.min(dist2(q, p));
        }
    }
    let d = best.sqrt();
    (d <= tol).then_some(d)
}

/// The nearest of `ids` whose outline is within `tol` of `p`, with its distance.
///
/// Ties break toward the earlier id, so a caller's ordering is a deterministic
/// priority (put the thing that should win first).
pub fn nearest_mobject(
    state: &SceneState,
    ids: &[AnyId],
    p: Point,
    tol: f32,
) -> Option<(AnyId, f32)> {
    ids.iter()
        .filter_map(|id| hit_mobject(state, *id, p, tol).map(|d| (*id, d)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// What a press landed on when handles and plain mobjects compete (FE-145).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    /// A drag handle, by index.
    Handle(usize),
    /// A scene mobject.
    Mobject(AnyId),
}

/// Resolves a press between a hit handle and a hit mobject.
///
/// **Handles win ties and near-ties.** A handle is a deliberate affordance
/// drawn *on top of* the geometry it controls, so a press that is within reach
/// of both must grab the handle — otherwise every handle sitting on its own
/// curve becomes ungrabbable. Only a mobject that is meaningfully closer (by
/// more than `bias` scene units) takes the press.
pub fn resolve_hit(
    handle: Option<(usize, f32)>,
    mobject: Option<(AnyId, f32)>,
    bias: f32,
) -> Option<HitTarget> {
    match (handle, mobject) {
        (Some((i, hd)), Some((id, md))) => Some(if md + bias < hd {
            HitTarget::Mobject(id)
        } else {
            HitTarget::Handle(i)
        }),
        (Some((i, _)), None) => Some(HitTarget::Handle(i)),
        (None, Some((id, _))) => Some(HitTarget::Mobject(id)),
        (None, None) => None,
    }
}

/// Squared distance in the xy plane (handles and 2-D figures live at z=0; a
/// z-difference from a projected 3-D mobject should not push it out of reach).
fn dist2(a: Point, b: Point) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_core::prelude::{Circle, Line};

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y, 0.0)
    }

    #[test]
    fn free_passes_through() {
        assert_eq!(
            DragConstraint::Free.apply(p(0.0, 0.0), p(3.0, 4.0)),
            p(3.0, 4.0)
        );
    }

    #[test]
    fn axis_keeps_only_the_along_component() {
        let c = DragConstraint::Axis(p(0.0, 2.0)); // unnormalized: still a y-rail
        let got = c.apply(p(1.0, 1.0), p(7.0, 4.0));
        assert_eq!(got, p(1.0, 4.0));
    }

    #[test]
    fn axis_works_on_a_diagonal_rail() {
        let c = DragConstraint::Axis(p(1.0, 1.0));
        // The cursor sits perpendicular to the rail → projects back to the anchor.
        let got = c.apply(p(0.0, 0.0), p(1.0, -1.0));
        assert!(got.x.abs() < 1e-6 && got.y.abs() < 1e-6, "{got:?}");
        // Straight along it → travels the full projection.
        let got = c.apply(p(0.0, 0.0), p(2.0, 2.0));
        assert!(
            (got.x - 2.0).abs() < 1e-5 && (got.y - 2.0).abs() < 1e-5,
            "{got:?}"
        );
    }

    #[test]
    fn zero_axis_freezes_the_handle() {
        let c = DragConstraint::Axis(Point::ZERO);
        assert_eq!(c.apply(p(2.0, 3.0), p(9.0, 9.0)), p(2.0, 3.0));
    }

    #[test]
    fn grid_snaps_to_an_absolute_lattice() {
        let c = DragConstraint::Grid(0.5);
        assert_eq!(c.apply(Point::ZERO, p(1.24, -0.8)), p(1.0, -1.0));
        assert_eq!(c.apply(Point::ZERO, p(1.26, 0.76)), p(1.5, 1.0));
        // Non-positive pitch is a no-op, not a divide-by-zero.
        assert_eq!(
            DragConstraint::Grid(0.0).apply(Point::ZERO, p(1.3, 2.7)),
            p(1.3, 2.7)
        );
    }

    #[test]
    fn curve_projects_to_the_nearest_rail_point() {
        // The unit circle, parameterized by angle.
        let rail = CurveRail::new(|t| p(t.cos(), t.sin()), 0.0, std::f32::consts::TAU);
        let c = DragConstraint::Curve(rail);
        let got = c.apply(Point::ZERO, p(3.0, 0.0));
        assert!((got.x - 1.0).abs() < 1e-3 && got.y.abs() < 1e-3, "{got:?}");
        let got = c.apply(Point::ZERO, p(0.0, -2.0));
        assert!(got.x.abs() < 1e-3 && (got.y + 1.0).abs() < 1e-3, "{got:?}");
        // A point already on the rail stays put.
        let got = c.apply(Point::ZERO, p(0.6, 0.8));
        assert!(
            (got.x - 0.6).abs() < 1e-3 && (got.y - 0.8).abs() < 1e-3,
            "{got:?}"
        );
    }

    #[test]
    fn curve_projection_returns_a_usable_parameter() {
        let rail = CurveRail::new(|t| p(t, t * t), -2.0, 2.0);
        let (t, q) = rail.project(p(1.0, 1.0));
        assert!((t - 1.0).abs() < 1e-2, "t = {t}");
        assert!((q.y - t * t).abs() < 1e-3);
    }

    #[test]
    fn region_clamps_into_the_box() {
        let c = DragConstraint::Region(BoundingBox::new(p(-1.0, -1.0), p(1.0, 1.0)));
        assert_eq!(c.apply(Point::ZERO, p(5.0, 0.2)), p(1.0, 0.2));
        assert_eq!(c.apply(Point::ZERO, p(-0.3, -9.0)), p(-0.3, -1.0));
        assert_eq!(c.apply(Point::ZERO, p(0.5, 0.5)), p(0.5, 0.5));
    }

    #[test]
    fn hit_mobject_measures_to_the_outline_not_the_center() {
        let mut state = SceneState::new();
        let circle = state.add(Circle::new()).erase(); // unit circle at origin
                                                       // Dead center is *far* from the outline (distance 1).
        assert!(hit_mobject(&state, circle, Point::ZERO, 0.2).is_none());
        // Just outside the rim is a hit.
        let d = hit_mobject(&state, circle, p(1.05, 0.0), 0.2).expect("rim hit");
        assert!(d < 0.1, "d = {d}");
        // Well away is a miss (and takes the cheap bbox path).
        assert!(hit_mobject(&state, circle, p(9.0, 9.0), 0.2).is_none());
    }

    #[test]
    fn nearest_mobject_picks_the_closest_of_several() {
        let mut state = SceneState::new();
        let a = state.add(Line::new(p(-2.0, 0.0), p(-1.0, 0.0))).erase();
        let b = state.add(Line::new(p(1.0, 0.0), p(2.0, 0.0))).erase();
        let (hit, _) = nearest_mobject(&state, &[a, b], p(1.5, 0.05), 0.3).expect("hit b");
        assert_eq!(hit, b);
        assert!(nearest_mobject(&state, &[a, b], p(0.0, 0.0), 0.3).is_none());
    }

    #[test]
    fn a_removed_mobject_is_never_hit() {
        let mut state = SceneState::new();
        let c = state.add(Circle::new()).erase();
        state.remove(c);
        assert!(hit_mobject(&state, c, p(1.0, 0.0), 0.5).is_none());
    }

    #[test]
    fn handles_win_near_ties_against_mobjects() {
        let id = {
            let mut s = SceneState::new();
            s.add(Circle::new()).erase()
        };
        // Equal distance → the handle.
        assert_eq!(
            resolve_hit(Some((0, 0.2)), Some((id, 0.2)), 0.05),
            Some(HitTarget::Handle(0))
        );
        // Mobject slightly closer, but inside the bias → still the handle.
        assert_eq!(
            resolve_hit(Some((0, 0.2)), Some((id, 0.17)), 0.05),
            Some(HitTarget::Handle(0))
        );
        // Meaningfully closer → the mobject takes it.
        assert_eq!(
            resolve_hit(Some((0, 0.2)), Some((id, 0.10)), 0.05),
            Some(HitTarget::Mobject(id))
        );
        // Only one candidate, or none.
        assert_eq!(
            resolve_hit(Some((3, 9.0)), None, 0.05),
            Some(HitTarget::Handle(3))
        );
        assert_eq!(
            resolve_hit(None, Some((id, 9.0)), 0.05),
            Some(HitTarget::Mobject(id))
        );
        assert_eq!(resolve_hit(None, None, 0.05), None);
    }
}
