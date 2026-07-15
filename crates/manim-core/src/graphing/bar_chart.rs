//! [`BarChart`]: a categorical bar chart mobject.

use manim_color::{Color, BLUE, GREEN, ORANGE, PURPLE, RED, YELLOW};
use manim_math::path::{Path, SubPath};
use manim_math::Point;

use crate::geometry::VMobject;
use crate::impl_mobject;
use crate::mobject::MobjectData;
use crate::style::Style;

/// The default bar color cycle (manim CE's default palette order).
pub fn default_bar_colors() -> Vec<Color> {
    vec![BLUE, GREEN, RED, YELLOW, PURPLE, ORANGE]
}

/// A bar chart: one filled rectangle per value, rising (or falling, for
/// negatives) from a baseline. Port of manim CE's `BarChart`.
///
/// The chart's own path holds all bars in one fill (so it transforms as a unit
/// and [`change_bar_values`](Self::change_bar_values) is animatable); per-bar
/// colors are available via [`get_bar`](Self::get_bar) for a multicolored build.
///
/// ```
/// use manim_core::graphing::BarChart;
/// use manim_core::mobject::Mobject;
/// let chart = BarChart::new(&[1.0, 2.0, 3.0]);
/// // Three bars.
/// assert_eq!(chart.data().path.subpaths.len(), 3);
/// // Bar heights are proportional to values (2 is twice 1).
/// let h = |i| { let (b, t) = chart.get_bar_span(i); (t.y - b.y).abs() };
/// assert!((h(1) / h(0) - 2.0).abs() < 1e-4);
/// ```
#[derive(Clone)]
pub struct BarChart {
    data: MobjectData,
    values: Vec<f32>,
    y_range: [f32; 3],
    width: f32,
    height: f32,
    bar_ratio: f32,
    bar_colors: Vec<Color>,
    bar_names: Vec<String>,
}
impl_mobject!(BarChart);

impl BarChart {
    /// A bar chart of `values` with an auto y-range including `0`, default size,
    /// and the default color cycle.
    pub fn new(values: &[f32]) -> Self {
        let max = values.iter().cloned().fold(0.0_f32, f32::max);
        let min = values.iter().cloned().fold(0.0_f32, f32::min);
        let y_max = if max > 0.0 { max } else { 1.0 };
        let mut chart = Self {
            data: MobjectData::new(Path::default(), Style::filled(BLUE)),
            values: values.to_vec(),
            y_range: [min, y_max, 1.0],
            width: 6.0,
            height: 4.0,
            bar_ratio: 0.6,
            bar_colors: default_bar_colors(),
            bar_names: Vec::new(),
        };
        chart.rebuild();
        chart
    }

    /// Sets the y-axis range `[min, max, step]`.
    pub fn with_y_range(mut self, y_range: [f32; 3]) -> Self {
        self.y_range = y_range;
        self.rebuild();
        self
    }

    /// Sets the on-screen chart size in scene units.
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self.rebuild();
        self
    }

    /// Sets the per-bar color cycle.
    pub fn with_bar_colors(mut self, colors: &[Color]) -> Self {
        if !colors.is_empty() {
            self.bar_colors = colors.to_vec();
        }
        self
    }

    /// Sets the category names (used by label helpers).
    pub fn with_bar_names(mut self, names: &[&str]) -> Self {
        self.bar_names = names.iter().map(|s| s.to_string()).collect();
        self
    }

    /// The number of bars.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the chart has no bars.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The current values.
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// The color of bar `i`.
    pub fn bar_color(&self, i: usize) -> Color {
        self.bar_colors[i % self.bar_colors.len().max(1)]
    }

    /// The category name of bar `i`, if set.
    pub fn bar_name(&self, i: usize) -> Option<&str> {
        self.bar_names.get(i).map(|s| s.as_str())
    }

    /// Scene units per data unit on the y-axis.
    fn unit_y(&self) -> f32 {
        let span = self.y_range[1] - self.y_range[0];
        if span.abs() > 1e-9 {
            self.height / span
        } else {
            0.0
        }
    }

    /// The scene y for data value `v`.
    fn y_of(&self, v: f32) -> f32 {
        (v - self.y_range[0]) * self.unit_y() - self.height / 2.0
    }

    /// The center x of bar `i`.
    fn bar_x(&self, i: usize) -> f32 {
        let n = self.values.len().max(1);
        let slot = self.width / n as f32;
        -self.width / 2.0 + slot * (i as f32 + 0.5)
    }

    /// The `(bottom, top)` corner-of-the-baseline-side and value-side y points at
    /// the center x of bar `i`.
    pub fn get_bar_span(&self, i: usize) -> (Point, Point) {
        let x = self.bar_x(i);
        let baseline_v = 0.0_f32.clamp(self.y_range[0], self.y_range[1]);
        let base = Point::new(x, self.y_of(baseline_v), 0.0);
        let top = Point::new(
            x,
            self.y_of(self.values.get(i).copied().unwrap_or(0.0)),
            0.0,
        );
        (base, top)
    }

    /// The rectangle subpath of bar `i`.
    fn bar_rect(&self, i: usize) -> SubPath {
        let n = self.values.len().max(1);
        let slot = self.width / n as f32;
        let bw = slot * self.bar_ratio;
        let (base, top) = self.get_bar_span(i);
        let x0 = base.x - bw / 2.0;
        let x1 = base.x + bw / 2.0;
        super::closed_polygon(&[
            Point::new(x0, base.y, 0.0),
            Point::new(x1, base.y, 0.0),
            Point::new(x1, top.y, 0.0),
            Point::new(x0, top.y, 0.0),
        ])
    }

    /// Bar `i` as a standalone filled [`VMobject`] in its own color.
    ///
    /// ```
    /// use manim_core::graphing::BarChart;
    /// use manim_core::mobject::Mobject;
    /// let chart = BarChart::new(&[2.0, 4.0]);
    /// let bar = chart.get_bar(1);
    /// assert_eq!(bar.data().style.fill_color, Some(chart.bar_color(1)));
    /// ```
    pub fn get_bar(&self, i: usize) -> VMobject {
        let path = Path {
            subpaths: vec![self.bar_rect(i)],
        };
        VMobject::new(path, Style::filled(self.bar_color(i)))
    }

    /// The anchor point for a numeric label on bar `i` (just past the bar's end).
    pub fn bar_label_point(&self, i: usize) -> Point {
        let (base, top) = self.get_bar_span(i);
        let dir = if top.y >= base.y { 0.25 } else { -0.25 };
        Point::new(top.x, top.y + dir, 0.0)
    }

    /// Replaces the bar values and rebuilds the geometry (manim's
    /// `change_bar_values`); animatable through `Transform` / `.animate()`.
    ///
    /// ```
    /// use manim_core::graphing::BarChart;
    /// let mut chart = BarChart::new(&[1.0, 1.0]);
    /// let before = chart.get_bar_span(0);
    /// chart.change_bar_values(&[3.0, 1.0]);
    /// let after = chart.get_bar_span(0);
    /// // The first bar grew taller.
    /// assert!((after.1.y - after.0.y).abs() > (before.1.y - before.0.y).abs());
    /// ```
    pub fn change_bar_values(&mut self, values: &[f32]) {
        self.values = values.to_vec();
        self.rebuild();
    }

    /// Rebuilds the chart path (all bars) from the current config.
    fn rebuild(&mut self) {
        let subpaths = (0..self.values.len()).map(|i| self.bar_rect(i)).collect();
        self.data.path = Path { subpaths };
        self.data.bump_generation();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::Mobject;

    #[test]
    fn bar_heights_proportional_to_values() {
        let chart = BarChart::new(&[1.0, 2.0, 4.0]);
        let h = |i| {
            let (b, t) = chart.get_bar_span(i);
            (t.y - b.y).abs()
        };
        assert!((h(1) / h(0) - 2.0).abs() < 1e-4);
        assert!((h(2) / h(0) - 4.0).abs() < 1e-4);
    }

    #[test]
    fn negative_values_hang_below_baseline() {
        let chart = BarChart::new(&[2.0, -2.0]);
        let (base0, top0) = chart.get_bar_span(0);
        let (base1, top1) = chart.get_bar_span(1);
        // Shared baseline; positive rises above it, negative drops below.
        assert!((base0.y - base1.y).abs() < 1e-5);
        assert!(top0.y > base0.y);
        assert!(top1.y < base1.y);
    }

    #[test]
    fn change_bar_values_updates_and_bumps() {
        let mut chart = BarChart::new(&[1.0, 1.0]);
        let g0 = chart.data().generation;
        chart.change_bar_values(&[5.0, 1.0]);
        assert!(chart.data().generation != g0);
        assert_eq!(chart.data().path.subpaths.len(), 2);
        let (b, t) = chart.get_bar_span(0);
        assert!((t.y - b.y).abs() > 1.0);
    }
}
