//! Text typesetting for `manim_rust`: shaped, vectorized [`Text`] and
//! [`Paragraph`].
//!
//! Text is shaped with [`cosmic-text`](https://docs.rs/cosmic-text) over a
//! bundled copy of DejaVu Sans, then each glyph's outline is extracted with
//! `ttf-parser` into cubic Bézier paths — one child mobject per non-space glyph,
//! grouped under a parent `Text`, exactly like manim CE's `Text` submobject
//! structure (`text[i]` is the `i`-th glyph). Everything downstream
//! (`Create`, `Transform`, `Write`, per-glyph color) then works with no special
//! cases. See `docs/design/07-text.md`.
//!
//! Layout is **deterministic**: the bundled font is loaded explicitly and system
//! font discovery is off by default (opt in with
//! [`Text::with_system_fonts`], native only).
//!
//! ```
//! use manim_core::prelude::*;
//! use manim_text::{Text, Write};
//!
//! let mut scene = Scene::new(Config::low());
//! let title = Text::new("Hello, manim!")
//!     .color(BLUE)
//!     .add_to(scene.state_mut());
//! scene.play(Write::new(title)).unwrap();
//! assert!(scene.total_duration() > 0.0);
//! ```
//!
//! # Font license
//!
//! The bundled DejaVu Sans is under the Bitstream Vera / DejaVu license (a
//! permissive free license); see `assets/DejaVu-LICENSE.txt`.
//!
//! # Milestone
//!
//! M4, Linear issue FE-98. `MarkupText`, `MathTex`/`Tex` (typst), and
//! `SVGMobject` are later issues (FE-99–102).

mod decimal;
mod digits;
mod extras;
mod font;
pub mod latex;
mod markup;
mod math;
mod outline;
mod paragraph;
mod text;
mod write;

pub use decimal::{ChangeDecimalToValue, ChangingDecimal, DecimalNumber, Integer, Variable};
pub use extras::{BulletedList, Title, LIST_BUFF};
pub use font::DEFAULT_FONT;
pub use latex::MathError;
pub use markup::{MarkupError, MarkupText};
pub use math::{MathTex, Tex, Typst, DEFAULT_MATH_FONT_SIZE};
pub use paragraph::Paragraph;
pub use text::{Alignment, Slant, Text, Weighting, DEFAULT_FONT_SIZE, SCENE_UNITS_PER_PIXEL};
pub use write::Write;
