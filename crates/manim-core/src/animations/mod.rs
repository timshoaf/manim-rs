//! The animation catalog: concrete [`Animation`](crate::animation::Animation)
//! implementations ported from manim CE, grouped by CE module.
//!
//! | manim CE module | here |
//! | --- | --- |
//! | creation | [`Create`], [`Uncreate`], [`DrawBorderThenFill`], [`ShowIncreasingSubsets`], [`ShowSubmobjectsOneByOne`] |
//! | fading | [`FadeIn`], [`FadeOut`] |
//! | transform | [`Transform`], [`TransformInto`], [`ReplacementTransform`], [`TransformFromCopy`], [`FadeTransform`], [`Restore`], [`ScaleInPlace`], [`ShrinkToCenter`], [`Swap`], [`CyclicReplace`], [`TransformMatchingShapes`] |
//! | movement/rotation | [`Shift`], [`MoveTo`], [`Rotate`], [`Rotating`], [`MoveAlongPath`] |
//! | apply | [`Homotopy`], [`ApplyPointwiseFunction`], [`ApplyFunction`], [`ApplyMatrix`], [`MaintainPositionRelativeTo`] |
//! | indication | [`Indicate`], [`Flash`], [`FocusOn`], [`Circumscribe`], [`Wiggle`], [`ApplyWave`], [`ShowPassingFlash`], [`ChangeSpeed`] |
//! | camera | [`CameraMove`], [`CameraFrameHandle`] |
//! | growing | [`GrowFromPoint`], [`GrowFromCenter`], [`GrowFromEdge`], [`GrowArrow`], [`SpinInFromNothing`], [`SpiralIn`] |
//! | composition | [`AnimationGroup`], [`Succession`], [`LaggedStart`], [`LaggedStartMap`] |
//! | numbers/updaters | [`ValueTracker`], [`ComplexValueTracker`], [`SetValue`], [`UpdateFromFunc`], [`UpdateFromAlphaFunc`] |
//! | `.animate()` | [`Animate`], [`AnimBuilder`] |
//! | transform paths | [`paths`] |

mod animate;
mod apply;
mod camera_move;
mod composition;
mod creation;
mod fading;
mod growing;
mod indication;
mod movement_rotation;
pub mod paths;
mod transform;
mod transform_matching;
mod updaters;
mod value_tracker;

pub use animate::{AnimBuilder, Animate};
pub use apply::{
    ApplyFunction, ApplyMatrix, ApplyPointwiseFunction, Homotopy, MaintainPositionRelativeTo,
};
pub use camera_move::{CameraFrameHandle, CameraMove};
pub use composition::{
    AnimationGroup, LaggedStart, LaggedStartMap, Succession, DEFAULT_LAGGED_START_LAG_RATIO,
};
pub use creation::{
    Create, DrawBorderThenFill, ShowIncreasingSubsets, ShowSubmobjectsOneByOne, Uncreate,
};
pub use fading::{FadeIn, FadeOut};
pub use growing::{
    GrowArrow, GrowFromCenter, GrowFromEdge, GrowFromPoint, SpinInFromNothing, SpiralIn,
};
pub use indication::{
    ApplyWave, ChangeSpeed, Circumscribe, Flash, FocusOn, Indicate, ShowPassingFlash, Wiggle,
};
pub use movement_rotation::{MoveAlongPath, MoveTo, Rotate, Rotating, Shift};
pub use transform::{
    CyclicReplace, FadeTransform, MoveToTarget, ReplacementTransform, Restore, ScaleInPlace,
    ShrinkToCenter, Swap, Transform, TransformFromCopy, TransformInto,
};
pub use transform_matching::{match_shapes, MatchResult, TransformMatchingShapes};
pub use updaters::{UpdateFromAlphaFunc, UpdateFromFunc, UpdaterCtx};
pub use value_tracker::{ComplexValueTracker, SetValue, ValueTracker};
