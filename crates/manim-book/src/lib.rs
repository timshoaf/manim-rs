//! **manim-book** — an authoring scaffold for interactive, constructivist
//! textbooks (FE-146).
//!
//! The design goal is *a chapter is prose plus scene structs, with zero
//! plumbing*. You write your scenes as ordinary [`SceneBuilder`]s and your
//! chapter as rsx; this crate supplies the numbering, cross-references, table of
//! contents, navigation, callouts and typography — and wires up the shared GPU
//! device and parameter set that the figures need.
//!
//! [`SceneBuilder`]: manim_core::scene::SceneBuilder
//!
//! # The authoring model
//!
//! One [`Book`] at the root, [`Chapter`]s inside it, [`Section`]s inside those,
//! and [`Prose`] / [`FigureBlock`] / [`Callout`] / [`MarginNote`] as the body.
//! That is the whole vocabulary:
//!
//! ```no_run
//! # use dioxus::prelude::*;
//! # use manim_core::prelude::*; use manim_core::error::Result;
//! use manim_book::*;
//!
//! #[derive(Clone, PartialEq)]
//! struct Coloring;
//! impl SceneBuilder for Coloring {
//!     fn construct(&self, _scene: &mut Scene) -> Result<()> { Ok(()) }
//! }
//!
//! fn app() -> Element {
//!     rsx! {
//!         Book {
//!             title: "Visual Complex Analysis",
//!             outline: vec![
//!                 ChapterEntry::new(1, "Complex Functions as Mappings", "#ch-1"),
//!                 ChapterEntry::new(2, "Conformality", "#ch-2"),
//!             ],
//!             Toc {}
//!             Chapter { number: 1, title: "Complex Functions as Mappings",
//!                 Prose { "A complex function is a mapping of the plane to itself." }
//!                 Section { title: "Domain coloring",
//!                     Prose { "Hue is the argument; brightness is the modulus." }
//!                     FigureBlock {
//!                         scene: Coloring,
//!                         label: "coloring",
//!                         caption: "Domain coloring of a rational map.",
//!                     }
//!                     Prose { "Compare the zeros in " Ref { label: "coloring" } "." }
//!                 }
//!                 ChapterNav {}
//!             }
//!         }
//!     }
//! }
//! ```
//!
//! Adding a chapter is one [`ChapterEntry`] in the outline plus one [`Chapter`]
//! block — no counters, no id strings, no stylesheet edits.
//!
//! # What each piece does for you
//!
//! * [`Book`] injects the stylesheet and wraps everything in a
//!   [`ManimGpuProvider`](manim_dioxus::ManimGpuProvider) (one `wgpu` device for
//!   the whole page) and a
//!   [`ParametersProvider`](manim_dioxus::ParametersProvider) (sliders and
//!   figures find each other by name).
//! * [`Chapter`] carries an explicit number; [`Section`] and [`FigureBlock`]
//!   auto-number under it (`2.3`, `Figure 2.3`).
//! * [`FigureBlock`] wraps [`manim_dioxus::Figure`] — render-on-demand,
//!   responsive, and lazy by default — adding the numbered caption and the
//!   anchor a [`Ref`] links to.
//! * [`Toc`] and [`ChapterNav`] are derived from the outline plus whatever has
//!   mounted, so they work both for a single-page book and for a
//!   one-chapter-per-page site.
//!
//! All of the numbering is plain data in [`numbering`], unit-tested natively.
//!
//! # Status
//!
//! * Figures render on wasm only; native builds show
//!   [`manim_dioxus::Figure`]'s placeholder `<div>` so the workspace still
//!   builds and tests natively.
//! * [`MathInline`] / [`MathBlock`] are **API-only placeholders**: they style
//!   the source in the book's serif italic instead of typesetting it. Real
//!   output means running manim-text's typst pipeline to SVG at build time and
//!   embedding data URIs — a build-harness job, not a component one. The API is
//!   fixed now so prose written today upgrades for free.

#![allow(missing_docs)] // dioxus `#[component]` codegen; hand-written items are documented.

pub mod components;
pub mod numbering;
pub mod style;

pub use components::{
    Book, BookCtx, Callout, CalloutKind, Chapter, ChapterCtx, ChapterNav, FigureBlock, MarginNote,
    MathBlock, MathInline, Prose, Ref, Section, Toc,
};
pub use numbering::{BookIndex, ChapterEntry, FigureRef, SectionEntry, TocChapter};
pub use style::BOOK_CSS;
