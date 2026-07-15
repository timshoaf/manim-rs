//! A Rust + WebGPU reimplementation of
//! [Manim Community Edition](https://docs.manim.community): declarative,
//! real-time mathematical animation.
//!
//! Most users want the prelude:
//!
//! ```
//! use manim::prelude::*;
//! ```

pub use manim_color as color;
pub use manim_math as math;

/// Everything you need to build scenes, in one import.
pub mod prelude {
    pub use manim_color::Color;
    pub use manim_math::{
        Point, DEGREES, DL, DOWN, DR, IN, LEFT, ORIGIN, OUT, PI, RIGHT, TAU, UL, UP, UR,
    };
}
