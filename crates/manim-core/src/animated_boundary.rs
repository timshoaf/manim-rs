//! [`AnimatedBoundary`]: a stroke-only outline that follows a mobject and cycles
//! its color through a palette over time. Port (simplified) of manim CE's
//! `AnimatedBoundary`.
//!
//! # Simplification
//!
//! manim CE draws several partial boundary copies that grow and fade in sequence
//! for a "drawing" shimmer. Here the boundary is a **single** stroke-only
//! [`VMobject`] whose path tracks the target each updater tick and whose stroke
//! color is interpolated around a color palette as a function of time — the
//! recognizable cycling-outline effect without the multi-copy machinery.

use manim_color::{Color, BLUE, GREEN, RED, YELLOW};

use crate::geometry::VMobject;
use crate::mobject::{AnyId, MobjectId};
use crate::scene_state::SceneState;
use crate::style::Style;

/// Default seconds to traverse the whole palette once (manim CE's `cycle_rate`).
pub const DEFAULT_CYCLE_RATE: f32 = 0.5;
/// Default stroke width of the animated boundary.
pub const DEFAULT_BOUNDARY_WIDTH: f32 = 3.0;

/// The default boundary color palette.
pub fn default_boundary_colors() -> Vec<Color> {
    vec![BLUE, GREEN, RED, YELLOW]
}

/// A cycling stroke outline attached to a target mobject. Port of manim CE's
/// `AnimatedBoundary`.
///
/// This is a *factory*: [`AnimatedBoundary::of`] adds a stroke-only [`VMobject`]
/// to the scene, registers an updater that (1) copies the target's current path
/// so the outline follows any deformation and (2) cycles its stroke color around
/// the palette, and returns the outline's id.
pub struct AnimatedBoundary;

impl AnimatedBoundary {
    /// Adds a default animated boundary (palette
    /// [`default_boundary_colors`], rate [`DEFAULT_CYCLE_RATE`]) around `target`.
    ///
    /// ```
    /// use manim_core::animated_boundary::AnimatedBoundary;
    /// use manim_core::geometry::Circle;
    /// use manim_core::scene_state::{SceneState, UpdaterCtx};
    /// use manim_core::mobject::Mobject;
    /// let mut scene = SceneState::new();
    /// let circle = scene.add(Circle::new()).erase();
    /// let boundary = AnimatedBoundary::of(&mut scene, circle);
    /// // The boundary traces the circle's outline.
    /// assert!(scene.get(boundary).data().path.n_curves() > 0);
    /// // Ticking updates its stroke color.
    /// scene.run_updaters(UpdaterCtx { dt: 0.5, time: 0.5 });
    /// assert!(scene.get(boundary).data().style.stroke_color.is_some());
    /// ```
    #[allow(clippy::new_ret_no_self)]
    pub fn of(scene: &mut SceneState, target: AnyId) -> MobjectId<VMobject> {
        Self::of_with(
            scene,
            target,
            &default_boundary_colors(),
            DEFAULT_CYCLE_RATE,
        )
    }

    /// Adds an animated boundary with an explicit palette and cycle rate.
    ///
    /// An empty palette falls back to [`default_boundary_colors`].
    pub fn of_with(
        scene: &mut SceneState,
        target: AnyId,
        colors: &[Color],
        cycle_rate: f32,
    ) -> MobjectId<VMobject> {
        let palette = if colors.is_empty() {
            default_boundary_colors()
        } else {
            colors.to_vec()
        };
        let path = scene.get_dyn(target).data().path.clone();
        let mut style = Style::stroked(palette[0]);
        style.set_stroke_width(DEFAULT_BOUNDARY_WIDTH);
        let boundary = scene.add(VMobject::new(path, style));
        let rate = cycle_rate.max(1e-6);
        scene.add_updater(boundary.erase(), move |s, id, ctx| {
            if s.contains(target) {
                let p = s.get_dyn(target).data().path.clone();
                s.get_dyn_mut(id).data_mut().path = p;
            }
            let color = cycle_color(&palette, ctx.time / rate);
            let data = s.get_dyn_mut(id).data_mut();
            data.style.stroke_color = Some(color);
            data.bump_generation();
        });
        boundary
    }
}

/// The color at cyclic position `t` (in palette-lengths) around `palette`,
/// linearly interpolating between adjacent entries.
fn cycle_color(palette: &[Color], t: f32) -> Color {
    let n = palette.len();
    if n == 1 {
        return palette[0];
    }
    let scaled = t * n as f32;
    let base = scaled.floor();
    let frac = scaled - base;
    let i = (base as i64).rem_euclid(n as i64) as usize;
    let j = (i + 1) % n;
    palette[i].interpolate(&palette[j], frac)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Circle;
    use crate::mobject::Mobject;
    use crate::scene_state::UpdaterCtx;

    #[test]
    fn boundary_traces_target() {
        let mut scene = SceneState::new();
        let circle = scene.add(Circle::new()).erase();
        let boundary = AnimatedBoundary::of(&mut scene, circle);
        let target_curves = scene.get_dyn(circle).data().path.n_curves();
        assert_eq!(scene.get(boundary).data().path.n_curves(), target_curves);
    }

    #[test]
    fn color_cycles_over_time() {
        let mut scene = SceneState::new();
        let circle = scene.add(Circle::new()).erase();
        let boundary = AnimatedBoundary::of(&mut scene, circle);
        scene.run_updaters(UpdaterCtx { dt: 0.0, time: 0.0 });
        let c0 = scene.get(boundary).data().style.stroke_color.unwrap();
        scene.run_updaters(UpdaterCtx {
            dt: 0.25,
            time: 0.25,
        });
        let c1 = scene.get(boundary).data().style.stroke_color.unwrap();
        // A quarter-cycle later the stroke color has moved.
        assert!(c0 != c1, "boundary color should change over time");
    }

    #[test]
    fn cycle_color_wraps() {
        let palette = vec![BLUE, GREEN];
        // t=0 and t=1 (one full loop) land on the same color.
        let a = cycle_color(&palette, 0.0);
        let b = cycle_color(&palette, 1.0);
        assert!((a.r - b.r).abs() < 1e-6 && (a.g - b.g).abs() < 1e-6);
    }
}
