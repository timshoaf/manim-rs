//! Deformation animations and the adaptive [`DeformationGrid`].
//!
//! [`ApplyMap`] and [`FlowMap`] re-evaluate a [`Homotopy`] or a field flow at
//! every `alpha`, so a point traces the map's *actual* path — unlike core's
//! `ApplyFunction`, which linearly interpolates between start and end points.
//! For a nonlinear map the two disagree at intermediate `alpha`.

use manim_core::animation::{AnimConfig, Animation};
use manim_core::geometry::{VGroup, VMobject};
use manim_core::mobject::{AnyId, MobjectData, MobjectId};
use manim_core::prelude::{Point, BLUE, WHITE};
use manim_core::scene_state::SceneState;
use manim_core::style::Style;
use manim_math::path::Path;
use manim_math::rate_functions::RateFn;

use manim_fields::field::VectorField3;
use manim_fields::map::{Homotopy, SpaceMap};

use crate::{to_field, to_scene};

/// Snapshots the path/style of every family member (a public reimplementation of
/// core's `pub(crate)` helper).
fn family_data(state: &SceneState, id: AnyId) -> Vec<(AnyId, MobjectData)> {
    state
        .family(id)
        .into_iter()
        .map(|m| (m, state.get_dyn(m).data().clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// ApplyMap
// ---------------------------------------------------------------------------

/// Deforms a mobject by a [`SpaceMap`] (or an explicit [`Homotopy`]): every
/// original point `x` moves to `H(x, alpha)`, re-evaluated each frame.
///
/// With the default straight homotopy this matches `ApplyFunction`; with a
/// curved [`Homotopy`] (or [`FlowMap`]) the intermediate frames follow the true
/// path, not the endpoint chord.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_sci::deform::ApplyMap;
/// use manim_fields::map::SpaceMap;
/// let mut scene = Scene::new(Config::default());
/// let sq = scene.add(Square::new());
/// scene.play(ApplyMap::new(sq, &SpaceMap::scaling(2.0))).unwrap();
/// assert!((scene[sq].bounding_box().width() - 4.0).abs() < 1e-3);
/// ```
pub struct ApplyMap {
    id: AnyId,
    homotopy: Homotopy,
    config: AnimConfig,
    start: Vec<(AnyId, MobjectData)>,
}

impl ApplyMap {
    /// Deforms `id` from the identity to `map` along a straight homotopy.
    pub fn new(id: impl Into<AnyId>, map: &SpaceMap) -> Self {
        Self::with_homotopy(id, SpaceMap::identity().homotopy_to(map))
    }
    /// Deforms `id` along an explicit [`Homotopy`] (which may follow a curved
    /// path, so intermediate frames are *not* an endpoint lerp).
    pub fn with_homotopy(id: impl Into<AnyId>, homotopy: Homotopy) -> Self {
        Self {
            id: id.into(),
            homotopy,
            config: AnimConfig::default(),
            start: Vec::new(),
        }
    }
    /// Sets the run time in seconds.
    pub fn run_time(mut self, t: f32) -> Self {
        self.config.run_time = t;
        self
    }
    /// Sets the easing curve.
    pub fn rate_fn(mut self, r: RateFn) -> Self {
        self.config.rate_fn = r;
        self
    }
}

impl Animation for ApplyMap {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = family_data(state, self.id);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let a = alpha as f64;
        let h = &self.homotopy;
        for (id, data) in &self.start {
            if state.contains(*id) {
                let out = state.get_dyn_mut(*id).data_mut();
                out.path = data.path.clone();
                out.path.apply(|p| to_scene(h.at(to_field(p), a)));
                out.bump_generation();
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    fn duration(&self) -> f32 {
        self.config.run_time
    }
    fn rate_fn(&self) -> RateFn {
        self.config.rate_fn.clone()
    }
}

// ---------------------------------------------------------------------------
// FlowMap
// ---------------------------------------------------------------------------

/// Advects a mobject along a vector field's integral curves: at `alpha`, every
/// point sits at the time-`alpha·t` flow of its start position. Genuinely
/// nonlinear in `alpha` — points curve along the flow rather than sliding on a
/// straight chord.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_sci::deform::FlowMap;
/// use manim_fields::field::{ScalarField, VectorField3};
/// let v = VectorField3::from_components(
///     ScalarField::coordinate(1).scale(-1.0),
///     ScalarField::coordinate(0),
///     ScalarField::constant(0.0),
/// );
/// let mut scene = Scene::new(Config::default());
/// let d = scene.add(Dot::at(Point::new(1.0, 0.0, 0.0)));
/// scene.play(FlowMap::new(d, v, std::f64::consts::FRAC_PI_2)).unwrap();
/// // A 90° rotation about the origin: (1,0) → (0,1).
/// assert!((scene[d].get_center() - Point::new(0.0, 1.0, 0.0)).length() < 1e-3);
/// ```
pub struct FlowMap {
    id: AnyId,
    field: VectorField3,
    t_total: f64,
    config: AnimConfig,
    start: Vec<(AnyId, MobjectData)>,
}

impl FlowMap {
    /// Flows `id` along `field` for total time `t_total` over the animation.
    pub fn new(id: impl Into<AnyId>, field: VectorField3, t_total: f64) -> Self {
        Self {
            id: id.into(),
            field,
            t_total,
            config: AnimConfig::default(),
            start: Vec::new(),
        }
    }
    /// Sets the run time in seconds.
    pub fn run_time(mut self, t: f32) -> Self {
        self.config.run_time = t;
        self
    }
    /// Sets the easing curve.
    pub fn rate_fn(mut self, r: RateFn) -> Self {
        self.config.rate_fn = r;
        self
    }
}

impl Animation for FlowMap {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = family_data(state, self.id);
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let t = alpha as f64 * self.t_total;
        let steps = ((t.abs() * 100.0).ceil() as usize).max(1);
        let field = &self.field;
        for (id, data) in &self.start {
            if state.contains(*id) {
                let out = state.get_dyn_mut(*id).data_mut();
                out.path = data.path.clone();
                out.path
                    .apply(|p| to_scene(field.flow(to_field(p), t, steps)));
                out.bump_generation();
            }
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    fn duration(&self) -> f32 {
        self.config.run_time
    }
    fn rate_fn(&self) -> RateFn {
        self.config.rate_fn.clone()
    }
}

// ---------------------------------------------------------------------------
// DeformationGrid
// ---------------------------------------------------------------------------

/// The singular values `(σ_max, σ_min)` of a 2×2 matrix `[[a, b], [c, d]]`.
fn singular_values_2x2(a: f64, b: f64, c: f64, d: f64) -> (f64, f64) {
    let s = a * a + b * b + c * c + d * d;
    let disc = ((a * a + b * b - c * c - d * d).powi(2) + 4.0 * (a * c + b * d).powi(2)).sqrt();
    let smax = (0.5 * (s + disc)).max(0.0).sqrt();
    let smin = (0.5 * (s - disc)).max(0.0).sqrt();
    (smax, smin)
}

/// The local stretch factor of the map at `p` — the largest singular value of
/// its `xy` Jacobian block (how much a small segment is lengthened). Blows up
/// near poles, so it drives subdivision toward high-distortion regions.
fn stretch(map: &SpaceMap, p: manim_fields::Point) -> f64 {
    let j = map.jacobian(p);
    let (smax, _) = singular_values_2x2(j.x_axis.x, j.y_axis.x, j.x_axis.y, j.y_axis.y);
    smax
}

/// An ambient coordinate grid over a rectangle whose lines subdivide *adaptively*
/// — more densely where a supplied [`SpaceMap`] distorts most — so the deformed
/// polylines stay faithful to the true curved images.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_sci::deform::DeformationGrid;
/// let mut scene = Scene::new(Config::default());
/// let grid = DeformationGrid::new([-2.0, 2.0], [-2.0, 2.0], 1.0).add_to(scene.state_mut());
/// // A grid of straight lines (undeformed) — a non-empty group.
/// assert!(!scene.state().get_dyn(grid).data().children.is_empty());
/// ```
#[derive(Clone)]
pub struct DeformationGrid {
    x_range: [f64; 2],
    y_range: [f64; 2],
    step: f64,
    map: Option<SpaceMap>,
    pre_deform: bool,
    ghost: bool,
    main_opacity: f32,
    threshold: f64,
    max_depth: u32,
}

impl DeformationGrid {
    /// A grid over `x_range × y_range` with grid spacing `step`.
    pub fn new(x_range: [f64; 2], y_range: [f64; 2], step: f64) -> Self {
        Self {
            x_range,
            y_range,
            step,
            map: None,
            pre_deform: false,
            ghost: false,
            main_opacity: 1.0,
            threshold: 0.15,
            max_depth: 6,
        }
    }
    /// Draws the grid at reduced stroke opacity — the "ghost" look for a static
    /// undeformed reference behind an animated grid.
    pub fn faded(mut self, opacity: f32) -> Self {
        self.main_opacity = opacity;
        self
    }
    /// Carries a [`SpaceMap`]: lines subdivide adaptively by its distortion. By
    /// itself the grid is left undeformed (ready to animate with [`ApplyMap`]);
    /// call [`pre_deformed`](Self::pre_deformed) to bake the map in.
    pub fn with_map(mut self, map: &SpaceMap) -> Self {
        self.map = Some(map.clone());
        self
    }
    /// Applies the carried map to the grid immediately (the static conformal
    /// image) instead of leaving it to be animated.
    pub fn pre_deformed(mut self) -> Self {
        self.pre_deform = true;
        self
    }
    /// Also draws a faded, undeformed "ghost" copy behind the grid (like
    /// `LinearTransformationScene`'s background plane).
    pub fn with_ghost(mut self) -> Self {
        self.ghost = true;
        self
    }
    /// The target *image* segment length below which subdivision stops (smaller
    /// = finer grid). A segment splits while `stretch × length` exceeds it.
    pub fn threshold(mut self, t: f64) -> Self {
        self.threshold = t;
        self
    }

    /// Adaptive anchors along the segment `a → b` in grid space, packed where the
    /// carried map distorts (uniform two-point line when there is no map).
    fn adaptive_line(
        &self,
        a: manim_fields::Point,
        b: manim_fields::Point,
    ) -> Vec<manim_fields::Point> {
        match &self.map {
            None => vec![a, b],
            Some(map) => {
                let mut pts = vec![a];
                self.refine(map, a, b, 0, &mut pts);
                pts
            }
        }
    }
    fn refine(
        &self,
        map: &SpaceMap,
        a: manim_fields::Point,
        b: manim_fields::Point,
        depth: u32,
        out: &mut Vec<manim_fields::Point>,
    ) {
        let mid = (a + b) * 0.5;
        // Subdivide while the *image* of this segment is long (stretch × length),
        // i.e. where the map bends/stretches the grid the most.
        let image_len = stretch(map, mid) * (b - a).length();
        if depth < self.max_depth && image_len > self.threshold {
            self.refine(map, a, mid, depth + 1, out);
            self.refine(map, mid, b, depth + 1, out);
        } else {
            out.push(b);
        }
    }

    /// Builds a single polyline mobject through `anchors`, optionally mapped.
    fn line_mobject(
        &self,
        anchors: &[manim_fields::Point],
        map: Option<&SpaceMap>,
        style: Style,
    ) -> VMobject {
        let pts: Vec<Point> = anchors
            .iter()
            .map(|&p| to_scene(map.map_or(p, |m| m.apply(p))))
            .collect();
        VMobject::new(Path::from_corners(&pts, false), style)
    }

    /// Renders the grid into `scene`, returning the group of grid lines
    /// (ghost lines first, then the main grid).
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        let mut ids: Vec<AnyId> = Vec::new();

        let deform_map = if self.pre_deform {
            self.map.as_ref()
        } else {
            None
        };
        let main_style = Style::stroked(BLUE).with_opacity_scaled(self.main_opacity);
        let ghost_style = Style::stroked(WHITE).with_opacity_scaled(0.35);

        // Ghost: undeformed, faded copy (added first so it sits behind).
        if self.ghost {
            for anchors in self.grid_lines() {
                let m = self.line_mobject(&anchors, None, ghost_style.clone());
                ids.push(scene.add(m).erase());
            }
        }
        for anchors in self.grid_lines() {
            let m = self.line_mobject(&anchors, deform_map, main_style.clone());
            ids.push(scene.add(m).erase());
        }
        VGroup::of(scene, ids)
    }

    /// The adaptive anchor list for every grid line (verticals then horizontals).
    fn grid_lines(&self) -> Vec<Vec<manim_fields::Point>> {
        let mut lines = Vec::new();
        let mut x = self.x_range[0];
        while x <= self.x_range[1] + 1e-9 {
            lines.push(self.adaptive_line(
                manim_fields::Point::new(x, self.y_range[0], 0.0),
                manim_fields::Point::new(x, self.y_range[1], 0.0),
            ));
            x += self.step;
        }
        let mut y = self.y_range[0];
        while y <= self.y_range[1] + 1e-9 {
            lines.push(self.adaptive_line(
                manim_fields::Point::new(self.x_range[0], y, 0.0),
                manim_fields::Point::new(self.x_range[1], y, 0.0),
            ));
            y += self.step;
        }
        lines
    }
}

/// A tiny convenience for the ghost style (Style has public fields, but this
/// keeps the intent legible).
trait OpacityScaled {
    fn with_opacity_scaled(self, o: f32) -> Self;
}
impl OpacityScaled for Style {
    fn with_opacity_scaled(mut self, o: f32) -> Self {
        self.stroke_opacity = o;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_core::prelude::*;
    use manim_fields::field::{ScalarField, VectorField3};

    /// All anchor points of a mobject family.
    fn anchors(state: &SceneState, id: AnyId) -> Vec<Point> {
        let mut pts = Vec::new();
        for m in state.family(id) {
            for sp in &state.get_dyn(m).data().path.subpaths {
                for c in &sp.curves {
                    pts.push(c.p0);
                    pts.push(c.p3);
                }
            }
        }
        pts
    }

    #[test]
    fn apply_map_endpoints_are_identity_and_map() {
        let mut state = SceneState::new();
        let sq = state.add(Square::new()).erase();
        let start = anchors(&state, sq);

        let map = SpaceMap::scaling(2.0);
        let mut anim = ApplyMap::new(sq, &map);
        anim.begin(&mut state);

        anim.interpolate(&mut state, 0.0);
        for (s, a) in start.iter().zip(anchors(&state, sq)) {
            assert!((*s - a).length() < 1e-6, "α=0 not identity");
        }
        anim.interpolate(&mut state, 1.0);
        for (s, a) in start.iter().zip(anchors(&state, sq)) {
            assert!((*s * 2.0 - a).length() < 1e-6, "α=1 not the map");
        }
    }

    #[test]
    fn apply_map_midframe_follows_homotopy_not_endpoint_lerp() {
        // A curved (arc) homotopy from identity to a translation by (−4,0,0). At
        // α=0.5 the homotopy lifts every point by sin(π/2)=1 in +y — off the
        // straight chord (whose y stays at the start value). The distinguishing
        // property: H(x, 0.5) = lerp(x, map(x), 0.5) + (0, 1, 0).
        let mut state = SceneState::new();
        let d = state.add(Dot::at(Point::new(2.0, 0.0, 0.0))).erase();
        let start = anchors(&state, d);

        let target = SpaceMap::translation(manim_fields::Point::new(-4.0, 0.0, 0.0));
        let arc = Homotopy::with_path(SpaceMap::identity(), target, |x, y, t| {
            let base = x * (1.0 - t) + y * t;
            base + manim_fields::Point::new(0.0, (std::f64::consts::PI * t).sin(), 0.0)
        });
        let mut anim = ApplyMap::with_homotopy(d, arc);
        anim.begin(&mut state);
        anim.interpolate(&mut state, 0.5);
        let mid = anchors(&state, d);

        for (s, m) in start.iter().zip(&mid) {
            let map_pt = Point::new(s.x - 4.0, s.y, 0.0); // target(s)
            let lerp = (*s + map_pt) * 0.5; // straight endpoint lerp
            assert!(
                (m.x - lerp.x).abs() < 1e-4,
                "x should be the chord midpoint"
            );
            // The arc adds a full unit of y over the endpoint lerp — NOT a lerp.
            assert!(
                (m.y - (lerp.y + 1.0)).abs() < 1e-4,
                "midframe should follow the arc: y={} lerp.y={}",
                m.y,
                lerp.y
            );
        }
    }

    #[test]
    fn flow_map_of_rotation_preserves_radius() {
        // Rotation field: a FlowMap keeps every point on its circle (radius
        // fixed), whereas a straight lerp to the rotated endpoint cuts the chord
        // and shrinks the radius.
        let v = VectorField3::from_components(
            ScalarField::coordinate(1).scale(-1.0),
            ScalarField::coordinate(0),
            ScalarField::constant(0.0),
        );
        let mut state = SceneState::new();
        let d = state.add(Dot::at(Point::new(1.0, 0.0, 0.0))).erase();
        let start = anchors(&state, d);
        let mut anim = FlowMap::new(d, v, std::f64::consts::FRAC_PI_2);
        anim.begin(&mut state);
        anim.interpolate(&mut state, 0.5); // mid-frame = 45° rotation
        let mid = anchors(&state, d);

        for (s, m) in start.iter().zip(&mid) {
            assert!(
                (m.length() - s.length()).abs() < 1e-3,
                "radius drifted from {} to {}",
                s.length(),
                m.length()
            );
        }
        // Sanity: a chord lerp of (1,0)→(0,1) at 0.5 shrinks the radius to ~0.71.
        let chord_mid = (Point::new(1.0, 0.0, 0.0) + Point::new(0.0, 1.0, 0.0)) * 0.5;
        assert!(
            chord_mid.length() < 0.95,
            "chord midpoint radius {}",
            chord_mid.length()
        );
    }

    #[test]
    fn deformation_grid_subdivides_near_a_pole() {
        // z ↦ 1/z has a pole at the origin; anchors should crowd there.
        let inv = SpaceMap::from_parts(
            |p| {
                let r2 = p.x * p.x + p.y * p.y;
                manim_fields::Point::new(p.x / r2, -p.y / r2, 0.0)
            },
            |p| {
                // Jacobian of 1/z (conformal): scale 1/|z|² grows near 0.
                let r2 = p.x * p.x + p.y * p.y;
                let s = 1.0 / r2;
                glam::DMat3::from_diagonal(manim_fields::Point::new(s, s, 1.0))
            },
        );
        let grid = DeformationGrid::new([0.2, 2.0], [0.2, 2.0], 0.5).with_map(&inv);

        let mut near = 0usize;
        let mut far = 0usize;
        for line in grid.grid_lines() {
            for p in line {
                if p.length() < 1.0 {
                    near += 1;
                } else if p.length() > 2.0 {
                    far += 1;
                }
            }
        }
        assert!(
            near > far * 2,
            "expected denser anchors near the pole: near={near} far={far}"
        );
    }
}
