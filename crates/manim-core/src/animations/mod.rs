//! The animation catalog: concrete [`Animation`](crate::animation::Animation)
//! implementations ported from manim CE, grouped by CE module.
//!
//! | manim CE module | here |
//! | --- | --- |
//! | creation | [`Create`], [`Uncreate`], [`DrawBorderThenFill`], [`ShowIncreasingSubsets`], [`ShowSubmobjectsOneByOne`] |
//! | fading | [`FadeIn`], [`FadeOut`] |
//! | transform | [`Transform`], [`TransformInto`], [`ReplacementTransform`], [`TransformFromCopy`], [`FadeTransform`], [`Restore`], [`ScaleInPlace`], [`ShrinkToCenter`] |
//! | movement/rotation | [`Shift`], [`MoveTo`], [`Rotate`], [`Rotating`], [`MoveAlongPath`] |
//! | composition | [`AnimationGroup`], [`Succession`], [`LaggedStart`] |
//! | numbers/updaters | [`ValueTracker`], [`SetValue`], [`UpdateFromFunc`] |
//! | `.animate()` | [`Animate`], [`AnimBuilder`] |

mod animate;
mod composition;
mod creation;
mod fading;
mod movement_rotation;
mod transform;
mod updaters;
mod value_tracker;

pub use animate::{AnimBuilder, Animate};
pub use composition::{AnimationGroup, LaggedStart, Succession, DEFAULT_LAGGED_START_LAG_RATIO};
pub use creation::{
    Create, DrawBorderThenFill, ShowIncreasingSubsets, ShowSubmobjectsOneByOne, Uncreate,
};
pub use fading::{FadeIn, FadeOut};
pub use movement_rotation::{MoveAlongPath, MoveTo, Rotate, Rotating, Shift};
pub use transform::{
    FadeTransform, ReplacementTransform, Restore, ScaleInPlace, ShrinkToCenter, Transform,
    TransformFromCopy, TransformInto,
};
pub use updaters::{UpdateFromFunc, UpdaterCtx};
pub use value_tracker::{SetValue, ValueTracker};
