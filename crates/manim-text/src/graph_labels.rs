//! Coordinate labels for manim-core's graphing types.
//!
//! manim-core cannot depend on manim-text, so numeric/tex axis labels live here
//! as extension traits over `NumberLine`, `Axes`, and `NumberPlane`. Labels are
//! placed in the graphing object's **local** coordinate frame (its geometry is
//! centered at the origin until you move it), so add labels first and then move
//! the axes and labels together — the standard manim workflow.

use manim_core::geometry::VGroup;
use manim_core::graphing::{Axes, BarChart, CoordSystem, FunctionGraph, NumberLine, NumberPlane};
use manim_core::mobject::{AnyId, MobjectExt, MobjectId};
use manim_core::scene_state::SceneState;
use manim_math::{Point, DOWN, LEFT};

use crate::decimal::{DecimalNumber, Integer};
use crate::latex::MathError;
use crate::math::MathTex;

/// The default font size for coordinate labels (manim CE's ~36).
pub const LABEL_FONT_SIZE: f32 = 36.0;

/// Builds a centered numeric label mobject for `value` and adds it to `scene`.
fn add_number(scene: &mut SceneState, value: f32, integral: bool, anchor: Point) -> AnyId {
    let mut number = if integral {
        Integer::new(value.round() as i64).font_size(LABEL_FONT_SIZE)
    } else {
        DecimalNumber::new(value)
            .num_decimal_places(1)
            .font_size(LABEL_FONT_SIZE)
    };
    number.move_to(anchor);
    scene.add(number).erase()
}

/// Whether every value in `values` is a whole number.
fn all_integral(values: &[f32]) -> bool {
    values.iter().all(|v| (v - v.round()).abs() < 1e-4)
}

/// Numeric labels for a [`NumberLine`] (manim's `add_numbers`).
pub trait CoordinateLabels {
    /// Adds a numeric label at every tick, returning the group of labels.
    fn add_numbers(&self, scene: &mut SceneState) -> MobjectId<VGroup>;
}

impl CoordinateLabels for NumberLine {
    /// ```
    /// use manim_core::graphing::NumberLine;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// use manim_text::CoordinateLabels;
    /// let mut scene = SceneState::new();
    /// let nl = scene.add(NumberLine::new(0.0, 5.0, 1.0));
    /// let labels = scene.get(nl).clone().add_numbers(&mut scene);
    /// // One label per tick (0 … 5).
    /// assert_eq!(scene.get_dyn(labels.erase()).data().children.len(), 6);
    /// ```
    fn add_numbers(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        let ticks = self.get_tick_range();
        let integral = all_integral(&ticks);
        let ids: Vec<AnyId> = ticks
            .iter()
            .map(|&x| add_number(scene, x, integral, self.number_label_point(x)))
            .collect();
        VGroup::of(scene, ids)
    }
}

/// The data-space position of an axis (clamped `0`, matching core's axes).
fn axis_position(range: [f32; 3]) -> f32 {
    0.0_f32.clamp(range[0], range[1])
}

/// Tick values of a `[min, max, step]` range, aligned to the step.
fn ticks(range: [f32; 3]) -> Vec<f32> {
    let [min, max, step] = range;
    let mut out = Vec::new();
    if step <= 0.0 {
        return out;
    }
    let mut i = (min / step).ceil() as i64;
    loop {
        let v = i as f32 * step;
        if v > max + 1e-6 {
            break;
        }
        if v >= min - 1e-6 {
            out.push(v);
        }
        i += 1;
    }
    out
}

/// Adds coordinate numbers for a coordinate system, skipping the origin value on
/// each axis, and returns the label group.
fn add_coordinates_for(
    scene: &mut SceneState,
    cs: &CoordSystem,
    x_label_point: impl Fn(f32) -> Point,
    y_label_point: impl Fn(f32) -> Point,
) -> MobjectId<VGroup> {
    let x_ticks = ticks(cs.x_range);
    let y_ticks = ticks(cs.y_range);
    let x_integral = all_integral(&x_ticks);
    let y_integral = all_integral(&y_ticks);
    let mut ids = Vec::new();
    for &x in &x_ticks {
        if x.abs() < 1e-6 {
            continue; // skip the origin to avoid overlap
        }
        ids.push(add_number(scene, x, x_integral, x_label_point(x)));
    }
    for &y in &y_ticks {
        if y.abs() < 1e-6 {
            continue;
        }
        ids.push(add_number(scene, y, y_integral, y_label_point(y)));
    }
    VGroup::of(scene, ids)
}

/// Numeric and tex labels for [`Axes`] / [`NumberPlane`] (manim's
/// `add_coordinates`, `get_axis_labels`, `get_graph_label`).
pub trait AxesLabels {
    /// Adds coordinate numbers on both axes (origin excluded), returning the
    /// label group.
    fn add_coordinates(&self, scene: &mut SceneState) -> MobjectId<VGroup>;

    /// Adds `x`/`y` axis labels (LaTeX) at the axis ends, returning their group.
    ///
    /// # Errors
    ///
    /// Propagates [`MathError`] if either label fails to typeset.
    fn get_axis_labels(
        &self,
        scene: &mut SceneState,
        x: &str,
        y: &str,
    ) -> Result<MobjectId<VGroup>, MathError>;
}

impl AxesLabels for Axes {
    /// ```
    /// use manim_core::graphing::Axes;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// use manim_text::AxesLabels;
    /// let mut scene = SceneState::new();
    /// let axes = Axes::new([-2.0, 2.0, 1.0], [-2.0, 2.0, 1.0]);
    /// let labels = axes.add_coordinates(&mut scene);
    /// // x ticks {-2,-1,1,2} + y ticks {-2,-1,1,2} (0 excluded) = 8.
    /// assert_eq!(scene.get_dyn(labels.erase()).data().children.len(), 8);
    /// ```
    fn add_coordinates(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        let cs = self.coords();
        add_coordinates_for(
            scene,
            &cs,
            |x| self.x_label_point(x),
            |y| self.y_label_point(y),
        )
    }

    fn get_axis_labels(
        &self,
        scene: &mut SceneState,
        x: &str,
        y: &str,
    ) -> Result<MobjectId<VGroup>, MathError> {
        let cs = self.coords();
        let x_at = self.x_label_point(cs.x_range[1]);
        let y_at = self.y_label_point(cs.y_range[1]);
        let xl = MathTex::new(x)?.font_size(LABEL_FONT_SIZE);
        let mut xl = xl;
        xl.move_to(x_at);
        let mut yl = MathTex::new(y)?.font_size(LABEL_FONT_SIZE);
        yl.move_to(y_at);
        let xid = xl.add_to(scene);
        let yid = yl.add_to(scene);
        Ok(VGroup::of(scene, [xid.erase(), yid.erase()]))
    }
}

impl AxesLabels for NumberPlane {
    fn add_coordinates(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        let cs = self.coords();
        let x_axis_y = axis_position(cs.y_range);
        let y_axis_x = axis_position(cs.x_range);
        add_coordinates_for(
            scene,
            &cs,
            |x| cs.coords_to_point(x, x_axis_y) + DOWN * 0.25,
            |y| cs.coords_to_point(y_axis_x, y) + LEFT * 0.25,
        )
    }

    fn get_axis_labels(
        &self,
        scene: &mut SceneState,
        x: &str,
        y: &str,
    ) -> Result<MobjectId<VGroup>, MathError> {
        let cs = self.coords();
        let x_axis_y = axis_position(cs.y_range);
        let y_axis_x = axis_position(cs.x_range);
        let mut xl = MathTex::new(x)?.font_size(LABEL_FONT_SIZE);
        xl.move_to(cs.coords_to_point(cs.x_range[1], x_axis_y) + DOWN * 0.3);
        let mut yl = MathTex::new(y)?.font_size(LABEL_FONT_SIZE);
        yl.move_to(cs.coords_to_point(y_axis_x, cs.y_range[1]) + LEFT * 0.3);
        let xid = xl.add_to(scene);
        let yid = yl.add_to(scene);
        Ok(VGroup::of(scene, [xid.erase(), yid.erase()]))
    }
}

/// Adds a LaTeX label to `scene` for `graph` at input `x`, offset up-right of
/// the curve (manim's `get_graph_label`). An extension over [`Axes`].
///
/// ```
/// use manim_core::graphing::Axes;
/// use manim_core::scene_state::SceneState;
/// use manim_core::mobject::MobjectExt;
/// use manim_text::GraphLabel;
/// let mut scene = SceneState::new();
/// let axes = Axes::new([-3.0, 3.0, 1.0], [-3.0, 3.0, 1.0]);
/// let graph = axes.plot(|x| x, None);
/// let label = axes.get_graph_label(&mut scene, &graph, "f(x)", 2.0).unwrap();
/// assert!(scene.contains(label.erase()));
/// ```
pub trait GraphLabel {
    /// Adds a LaTeX label for `graph` near input `x`.
    ///
    /// # Errors
    ///
    /// Propagates [`MathError`] if the label fails to typeset.
    fn get_graph_label(
        &self,
        scene: &mut SceneState,
        graph: &FunctionGraph,
        tex: &str,
        x: f32,
    ) -> Result<MobjectId<MathTex>, MathError>;
}

impl GraphLabel for Axes {
    fn get_graph_label(
        &self,
        scene: &mut SceneState,
        graph: &FunctionGraph,
        tex: &str,
        x: f32,
    ) -> Result<MobjectId<MathTex>, MathError> {
        let anchor = self.input_to_graph_point(x, graph) + Point::new(0.3, 0.4, 0.0);
        let mut label = MathTex::new(tex)?.font_size(LABEL_FONT_SIZE);
        label.move_to(anchor);
        Ok(label.add_to(scene))
    }
}

/// Numeric labels above [`BarChart`] bars (manim's `get_bar_labels`).
pub trait BarChartLabels {
    /// Adds a numeric label at each bar's end, returning the label group.
    fn get_bar_labels(&self, scene: &mut SceneState) -> MobjectId<VGroup>;
}

impl BarChartLabels for BarChart {
    /// ```
    /// use manim_core::graphing::BarChart;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// use manim_text::BarChartLabels;
    /// let mut scene = SceneState::new();
    /// let chart = BarChart::new(&[1.0, 2.0, 3.0]);
    /// let labels = chart.get_bar_labels(&mut scene);
    /// // One label per bar.
    /// assert_eq!(scene.get_dyn(labels.erase()).data().children.len(), 3);
    /// ```
    fn get_bar_labels(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        let values = self.values().to_vec();
        let integral = all_integral(&values);
        let ids: Vec<AnyId> = (0..self.len())
            .map(|i| add_number(scene, values[i], integral, self.bar_label_point(i)))
            .collect();
        VGroup::of(scene, ids)
    }
}
