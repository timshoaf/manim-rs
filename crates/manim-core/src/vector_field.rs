//! Vector fields: [`VectorField`] (the RK4 integrator), [`ArrowVectorField`],
//! and [`StreamLines`]. Port of manim CE's `vector_field`.
//!
//! Color-by-magnitude and color-by-speed give each arrow / streamline its own
//! color, which a single mobject's one style cannot express — so
//! [`ArrowVectorField::add_to`] and [`StreamLines::add_to`] build a
//! [`VGroup`] of per-element children (the arena-child
//! pattern). The pure [`VectorField`] integrator is standalone and needs no
//! scene.

use std::sync::Arc;

use manim_color::gradient::interpolate_color;
use manim_color::{Color, BLUE_E, GREEN, RED, YELLOW};
use manim_math::{Point, FRAME_HEIGHT, FRAME_WIDTH};

use crate::geometry::{Arrow, VGroup, VMobject};
use crate::mobject::{Buildable, MobjectId};
use crate::scene_state::SceneState;
use crate::style::Style;

/// A `Point -> Point` field, shared so config structs stay `Clone`.
pub(crate) type FieldFn = Arc<dyn Fn(Point) -> Point + Send + Sync>;

/// The default arrow-length cap in scene units (manim CE's `max_length`).
pub const DEFAULT_MAX_LENGTH: f32 = 0.45;

/// manim CE's default magnitude color ramp (low → high).
pub fn default_field_colors() -> Vec<Color> {
    vec![BLUE_E, GREEN, YELLOW, RED]
}

/// Interpolates a color ramp by `t ∈ [0, 1]` (linear space).
fn ramp_color(t: f32, colors: &[Color]) -> Color {
    if colors.is_empty() {
        return Color::from_rgba(1.0, 1.0, 1.0, 1.0);
    }
    if colors.len() == 1 {
        return colors[0];
    }
    let t = t.clamp(0.0, 1.0);
    let seg = t * (colors.len() - 1) as f32;
    let i = (seg.floor() as usize).min(colors.len() - 2);
    interpolate_color(colors[i], colors[i + 1], seg - i as f32)
}

/// A continuous vector field with an RK4 integrator. The integrator is the reuse
/// point behind [`ArrowVectorField`] and [`StreamLines`].
///
/// ```
/// use manim_core::vector_field::VectorField;
/// use manim_math::Point;
/// // Rotational field f(x, y) = (-y, x): trajectories are circles.
/// let field = VectorField::new(|p| Point::new(-p.y, p.x, 0.0));
/// let start = Point::new(1.0, 0.0, 0.0);
/// let mut p = start;
/// // Integrate one full revolution; the radius is preserved.
/// let steps = (std::f32::consts::TAU / 0.05).round() as usize;
/// for _ in 0..steps {
///     p = field.nudge(p, 0.05);
/// }
/// assert!((p.length() - 1.0).abs() < 1e-3);
/// ```
#[derive(Clone)]
pub struct VectorField {
    func: FieldFn,
}

impl VectorField {
    /// Wraps a field function.
    pub fn new(func: impl Fn(Point) -> Point + Send + Sync + 'static) -> Self {
        Self {
            func: Arc::new(func),
        }
    }

    /// The field vector at `p`.
    pub fn sample(&self, p: Point) -> Point {
        (self.func)(p)
    }

    /// Advances `p` by one classical RK4 step of size `dt` along the field
    /// (manim's `nudge`).
    pub fn nudge(&self, p: Point, dt: f32) -> Point {
        let f = &self.func;
        let k1 = f(p);
        let k2 = f(p + k1 * (dt * 0.5));
        let k3 = f(p + k2 * (dt * 0.5));
        let k4 = f(p + k3 * dt);
        p + (k1 + k2 * 2.0 + k3 * 2.0 + k4) * (dt / 6.0)
    }
}

/// A grid `[min, max, step]` sampled into points over `x_range × y_range`.
fn grid_points(x_range: [f32; 3], y_range: [f32; 3]) -> Vec<Point> {
    let mut pts = Vec::new();
    let xs = axis_samples(x_range);
    let ys = axis_samples(y_range);
    for &y in &ys {
        for &x in &xs {
            pts.push(Point::new(x, y, 0.0));
        }
    }
    pts
}

/// The sample values of `[min, max, step]`, inclusive of `min`.
fn axis_samples(range: [f32; 3]) -> Vec<f32> {
    let [min, max, step] = range;
    let step = if step.abs() > 1e-9 { step.abs() } else { 1.0 };
    let n = (((max - min) / step).floor() as i64).max(0);
    (0..=n).map(|i| min + i as f32 * step).collect()
}

/// A grid of arrows whose length and color encode the field's magnitude. Port of
/// manim CE's `ArrowVectorField`.
///
/// Build it, then [`add_to`](Self::add_to) a scene to materialize the colored
/// arrow children.
///
/// ```
/// use manim_core::vector_field::ArrowVectorField;
/// use manim_core::scene_state::SceneState;
/// use manim_math::Point;
/// let field = ArrowVectorField::new(|p| Point::new(-p.y, p.x, 0.0))
///     .with_x_range([-2.0, 2.0, 1.0])
///     .with_y_range([-2.0, 2.0, 1.0]);
/// let mut scene = SceneState::new();
/// let group = field.add_to(&mut scene);
/// // 5×5 grid, minus the zero-magnitude center = 24 arrows under one group.
/// assert_eq!(scene.family(group.erase()).len(), 1 + 24);
/// ```
#[derive(Clone)]
pub struct ArrowVectorField {
    field: VectorField,
    x_range: [f32; 3],
    y_range: [f32; 3],
    max_length: f32,
    colors: Vec<Color>,
}

impl ArrowVectorField {
    /// A field over the default frame (`14.222 × 8`) sampled every `0.5`, with
    /// the default color ramp.
    pub fn new(func: impl Fn(Point) -> Point + Send + Sync + 'static) -> Self {
        let hw = FRAME_WIDTH / 2.0;
        let hh = FRAME_HEIGHT / 2.0;
        Self {
            field: VectorField::new(func),
            x_range: [-hw, hw, 0.5],
            y_range: [-hh, hh, 0.5],
            max_length: DEFAULT_MAX_LENGTH,
            colors: default_field_colors(),
        }
    }

    /// Sets the x sampling range `[min, max, step]`.
    pub fn with_x_range(mut self, x_range: [f32; 3]) -> Self {
        self.x_range = x_range;
        self
    }

    /// Sets the y sampling range `[min, max, step]`.
    pub fn with_y_range(mut self, y_range: [f32; 3]) -> Self {
        self.y_range = y_range;
        self
    }

    /// Sets the magnitude color ramp (low → high).
    pub fn with_colors(mut self, colors: Vec<Color>) -> Self {
        self.colors = colors;
        self
    }

    /// The grid sample points.
    pub fn seed_points(&self) -> Vec<Point> {
        grid_points(self.x_range, self.y_range)
    }

    /// The drawn length for a vector of magnitude `mag`: a sqrt-scaled ramp
    /// capped at the field's `max_length` so dense fields stay legible.
    pub fn arrow_length(&self, mag: f32) -> f32 {
        (mag.sqrt() * 0.3).min(self.max_length)
    }

    /// Adds the field to `scene` as a [`VGroup`] of colored arrows and returns
    /// the group handle. Each arrow is centered on its grid point, oriented
    /// along the field, sized by [`arrow_length`](Self::arrow_length), and
    /// colored by normalized magnitude.
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        let points = self.seed_points();
        let mags: Vec<f32> = points
            .iter()
            .map(|&p| self.field.sample(p).length())
            .collect();
        let max_mag = mags.iter().cloned().fold(0.0_f32, f32::max).max(1e-6);

        let group = scene.add(VGroup::new());
        for (&p, &mag) in points.iter().zip(&mags) {
            if mag <= 1e-9 {
                continue;
            }
            let v = self.field.sample(p);
            let dir = v / v.length();
            let len = self.arrow_length(mag);
            let half = dir * (len * 0.5);
            let color = ramp_color(mag / max_mag, &self.colors);
            let arrow = Arrow::new(p - half, p + half).with_color(color);
            let child = scene.add(arrow);
            scene.add_child(group.erase(), child.erase());
        }
        group
    }
}

/// Integrated flow lines of a field, colored by speed. Port of manim CE's
/// `StreamLines`.
///
/// ```
/// use manim_core::vector_field::StreamLines;
/// use manim_core::scene_state::SceneState;
/// use manim_math::Point;
/// let lines = StreamLines::new(|p| Point::new(-p.y, p.x, 0.0))
///     .with_x_range([-2.0, 2.0, 1.0])
///     .with_y_range([-2.0, 2.0, 1.0]);
/// let mut scene = SceneState::new();
/// let group = lines.add_to(&mut scene);
/// // One streamline per (non-degenerate) seed point.
/// assert!(scene.family(group.erase()).len() > 1);
/// ```
#[derive(Clone)]
pub struct StreamLines {
    field: VectorField,
    x_range: [f32; 3],
    y_range: [f32; 3],
    dt: f32,
    max_steps: usize,
    padding: f32,
    colors: Vec<Color>,
}

impl StreamLines {
    /// A stream-line field over the default frame sampled every `0.5`.
    pub fn new(func: impl Fn(Point) -> Point + Send + Sync + 'static) -> Self {
        let hw = FRAME_WIDTH / 2.0;
        let hh = FRAME_HEIGHT / 2.0;
        Self {
            field: VectorField::new(func),
            x_range: [-hw, hw, 0.5],
            y_range: [-hh, hh, 0.5],
            dt: 0.05,
            max_steps: 200,
            padding: 1.0,
            colors: default_field_colors(),
        }
    }

    /// Sets the seed x range.
    pub fn with_x_range(mut self, x_range: [f32; 3]) -> Self {
        self.x_range = x_range;
        self
    }

    /// Sets the seed y range.
    pub fn with_y_range(mut self, y_range: [f32; 3]) -> Self {
        self.y_range = y_range;
        self
    }

    /// Sets the integration step and maximum step count.
    pub fn with_integration(mut self, dt: f32, max_steps: usize) -> Self {
        self.dt = dt;
        self.max_steps = max_steps;
        self
    }

    /// The seed grid.
    pub fn seed_points(&self) -> Vec<Point> {
        grid_points(self.x_range, self.y_range)
    }

    /// Integrates one streamline from `seed` with RK4, stopping at `max_steps`
    /// or when it leaves the frame (plus padding). Returns the polyline points.
    pub fn streamline(&self, seed: Point) -> Vec<Point> {
        let x_bound = FRAME_WIDTH / 2.0 + self.padding;
        let y_bound = FRAME_HEIGHT / 2.0 + self.padding;
        let mut pts = vec![seed];
        let mut p = seed;
        for _ in 0..self.max_steps {
            p = self.field.nudge(p, self.dt);
            if !p.is_finite() || p.x.abs() > x_bound || p.y.abs() > y_bound {
                break;
            }
            pts.push(p);
        }
        pts
    }

    /// Adds the streamlines to `scene` as a [`VGroup`], each colored by its mean
    /// speed. Returns the group handle.
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        use manim_math::path::{Path, SubPath};

        let seeds = self.seed_points();
        let speeds: Vec<f32> = seeds
            .iter()
            .map(|&p| self.field.sample(p).length())
            .collect();
        let max_speed = speeds.iter().cloned().fold(0.0_f32, f32::max).max(1e-6);

        let group = scene.add(VGroup::new());
        for (&seed, &speed) in seeds.iter().zip(&speeds) {
            let pts = self.streamline(seed);
            if pts.len() < 2 {
                continue;
            }
            let color = ramp_color(speed / max_speed, &self.colors);
            let path = Path {
                subpaths: vec![SubPath::from_corners(&pts)],
            };
            let mut style = Style::stroked(color);
            style.stroke_width = 2.0;
            let child = scene.add(VMobject::new(path, style));
            scene.add_child(group.erase(), child.erase());
        }
        group
    }

    /// Builds the streamlines **and** animates a continuous flow along them: a
    /// short dash window travels down each line, phase-offset per line for the
    /// flowing look, wrapping forever. Port (simplified) of manim CE's
    /// `StreamLines.start_animation` — driven by an updater, so it plays whenever
    /// the scene ticks. Returns the group.
    pub fn animate_flow(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        use manim_math::path::{Path, SubPath};

        let seeds = self.seed_points();
        let speeds: Vec<f32> = seeds
            .iter()
            .map(|&p| self.field.sample(p).length())
            .collect();
        let max_speed = speeds.iter().cloned().fold(0.0_f32, f32::max).max(1e-6);

        let group = scene.add(VGroup::new());
        let n = seeds.len().max(1);
        for (i, (&seed, &speed)) in seeds.iter().zip(&speeds).enumerate() {
            let pts = self.streamline(seed);
            if pts.len() < 2 {
                continue;
            }
            let color = ramp_color(speed / max_speed, &self.colors);
            let full = Path {
                subpaths: vec![SubPath::from_corners(&pts)],
            };
            let mut style = Style::stroked(color);
            style.stroke_width = 2.0;
            let child = scene.add(VMobject::new(full.clone(), style));
            scene.add_child(group.erase(), child.erase());

            let offset = i as f32 / n as f32;
            scene.add_updater(child.erase(), move |s, id, ctx| {
                // Sliding window [a, a+W] in path proportion, wrapping via time.
                let a = (ctx.time * FLOW_SPEED + offset).rem_euclid(1.0);
                let b = (a + FLOW_WINDOW).min(1.0);
                let window = full.get_subcurve(a, b);
                let data = s.get_dyn_mut(id).data_mut();
                data.path = window;
                data.bump_generation();
            });
        }
        group
    }
}

/// Flow speed for [`StreamLines::animate_flow`], in path-proportions per second.
const FLOW_SPEED: f32 = 0.25;
/// Length of the moving dash window, as a path proportion.
const FLOW_WINDOW: f32 = 0.18;

#[cfg(test)]
mod tests {
    use super::*;

    fn rotational() -> impl Fn(Point) -> Point + Send + Sync + 'static {
        |p: Point| Point::new(-p.y, p.x, 0.0)
    }

    #[test]
    fn stream_flow_window_advances_and_wraps() {
        use crate::scene_state::{SceneState, UpdaterCtx};

        let mut scene = SceneState::new();
        let lines = StreamLines::new(rotational())
            .with_x_range([-2.0, 2.0, 1.0])
            .with_y_range([-2.0, 2.0, 1.0]);
        let group = lines.animate_flow(&mut scene);
        let child = scene.get_dyn(group.erase()).data().children[0];

        let tick = |scene: &mut SceneState, t: f32| {
            scene.run_updaters(UpdaterCtx { dt: 0.0, time: t });
            let p = &scene.get_dyn(child).data().path;
            (p.point_from_proportion(0.0), p.n_curves())
        };
        let (p0, len0) = tick(&mut scene, 0.0);
        let (p1, _) = tick(&mut scene, 1.0);
        // The window (a short arc) has slid along the streamline.
        assert!((p1 - p0).length() > 1e-3, "window should advance");
        // And it's only a window, not the whole line.
        assert!(len0 >= 1);
        // After a full period (1/FLOW_SPEED s) it returns near the start.
        let (pw, _) = tick(&mut scene, 1.0 / FLOW_SPEED);
        assert!(
            (pw - p0).length() < 0.2,
            "window should wrap: {pw:?} vs {p0:?}"
        );
    }

    #[test]
    fn rk4_preserves_radius_over_a_revolution() {
        let field = VectorField::new(rotational());
        let start = Point::new(2.0, 0.0, 0.0);
        let mut p = start;
        let steps = (std::f32::consts::TAU / 0.05).round() as usize;
        for _ in 0..steps {
            p = field.nudge(p, 0.05);
        }
        // Radius preserved and returned near the start.
        assert!(
            (p.length() - 2.0).abs() < 1e-3,
            "radius drift: {}",
            p.length()
        );
        assert!((p - start).length() < 5e-2, "did not close the loop");
    }

    #[test]
    fn arrow_length_is_capped() {
        let f = ArrowVectorField::new(rotational());
        assert!(f.arrow_length(1e6) <= DEFAULT_MAX_LENGTH + 1e-6);
        // Monotonic in magnitude below the cap.
        assert!(f.arrow_length(0.1) < f.arrow_length(1.0));
    }

    #[test]
    fn seed_grid_count() {
        let f = ArrowVectorField::new(rotational())
            .with_x_range([-2.0, 2.0, 1.0])
            .with_y_range([-1.0, 1.0, 1.0]);
        // 5 columns × 3 rows.
        assert_eq!(f.seed_points().len(), 15);
    }

    #[test]
    fn arrow_field_adds_one_child_per_arrow() {
        let f = ArrowVectorField::new(rotational())
            .with_x_range([-1.0, 1.0, 1.0])
            .with_y_range([-1.0, 1.0, 1.0]);
        let mut scene = SceneState::new();
        let g = f.add_to(&mut scene);
        // 3×3 = 9 grid points; the center has zero magnitude and is skipped.
        assert_eq!(scene.family(g.erase()).len(), 1 + 8);
    }

    #[test]
    fn streamline_stops_within_bounds() {
        // An outward field drives every seed out of the frame quickly.
        let lines = StreamLines::new(|p| p).with_integration(0.1, 500);
        let sl = lines.streamline(Point::new(1.0, 1.0, 0.0));
        let bound = FRAME_WIDTH / 2.0 + 1.0;
        // It terminated before max_steps (left the frame) and stayed finite.
        assert!(sl.len() < 500);
        assert!(sl.iter().all(|p| p.is_finite()));
        let last = *sl.last().unwrap();
        assert!(last.x.abs() <= bound + 1e-3 && last.y.abs() <= (FRAME_HEIGHT / 2.0 + 1.0) + 1e-3);
    }

    #[test]
    fn rotational_streamline_stays_bounded() {
        // A closed orbit never leaves; it runs to max_steps and stays on-radius.
        let lines = StreamLines::new(rotational()).with_integration(0.05, 100);
        let sl = lines.streamline(Point::new(2.0, 0.0, 0.0));
        assert_eq!(sl.len(), 101);
        assert!(sl.iter().all(|p| (p.length() - 2.0).abs() < 1e-2));
    }
}
