//! The Dioxus components an author writes a chapter with.
//!
//! Everything here is a thin reactive shell over [`crate::numbering`]: the
//! components claim counter slots at mount, register themselves into the shared
//! [`BookIndex`], and render semantic HTML with the [`BOOK_CSS`] class names.
//!
//! See the crate-level docs for the authoring model.

use std::cell::Cell;
use std::rc::Rc;

use dioxus::prelude::*;
use manim_core::config::Config;
use manim_core::scene::SceneBuilder;
use manim_dioxus::{Figure, LiveUpdater, ManimGpuProvider, ParametersProvider};

use crate::numbering::{
    chapter_anchor, figure_anchor, figure_caption_prefix, section_anchor, BookIndex, ChapterEntry,
    FigureRef, SectionEntry,
};
use crate::style::BOOK_CSS;

/// The book-wide context: the one [`BookIndex`] every component reads and
/// writes. Provided by [`Book`], read by [`Toc`], [`Ref`] and [`ChapterNav`].
#[derive(Clone, Copy)]
pub struct BookCtx {
    /// The live index (outline, mounted sections, figure labels).
    pub index: Signal<BookIndex>,
}

/// The per-chapter context: the chapter's number plus its section and figure
/// counters. Provided by [`Chapter`]; consumed by [`Section`], [`FigureBlock`]
/// and [`ChapterNav`].
#[derive(Clone)]
pub struct ChapterCtx {
    /// The enclosing chapter's number.
    pub number: u32,
    sections: Rc<Cell<u32>>,
    figures: Rc<Cell<u32>>,
}

impl ChapterCtx {
    fn new(number: u32) -> Self {
        Self {
            number,
            sections: Rc::new(Cell::new(0)),
            figures: Rc::new(Cell::new(0)),
        }
    }

    /// Claims the next 1-based section index in this chapter.
    fn next_section(&self) -> u32 {
        let n = self.sections.get() + 1;
        self.sections.set(n);
        n
    }

    /// Claims the next 1-based figure index in this chapter.
    fn next_figure(&self) -> u32 {
        let n = self.figures.get() + 1;
        self.figures.set(n);
        n
    }
}

fn book_ctx() -> BookCtx {
    try_consume_context::<BookCtx>()
        .expect("manim-book: this component must be used inside a `Book { .. }`")
}

fn chapter_ctx() -> ChapterCtx {
    try_consume_context::<ChapterCtx>()
        .expect("manim-book: this component must be used inside a `Chapter { .. }`")
}

/// The top-level book shell.
///
/// Provides, in one wrapper: the embedded stylesheet, a shared GPU device
/// ([`ManimGpuProvider`] — every figure on the page reuses one `wgpu` device),
/// a book-wide [`ParametersProvider`] (so sliders and figures find each other
/// with no plumbing), and the [`BookIndex`] that backs the table of contents,
/// cross-references and prev/next navigation.
///
/// `outline` declares the *whole* book — including chapters that are not
/// currently mounted — which is what lets [`Toc`] and [`ChapterNav`] work on a
/// site that renders one chapter per page.
///
/// ```no_run
/// # use dioxus::prelude::*;
/// use manim_book::{Book, Chapter, ChapterEntry, Prose};
///
/// fn app() -> Element {
///     rsx! {
///         Book {
///             title: "Visual Complex Analysis",
///             outline: vec![
///                 ChapterEntry::new(1, "Complex Functions as Mappings", "#ch-1"),
///                 ChapterEntry::new(2, "Conformality", "#ch-2"),
///             ],
///             Chapter { number: 1, title: "Complex Functions as Mappings",
///                 Prose { "A complex function is a mapping of the plane." }
///             }
///         }
///     }
/// }
/// ```
#[component]
pub fn Book(
    /// The book's title, rendered above the content. Omit for none.
    #[props(default)]
    title: Option<String>,
    /// The full chapter outline (used by [`Toc`] and [`ChapterNav`]).
    #[props(default)]
    outline: Vec<ChapterEntry>,
    children: Element,
) -> Element {
    let index = use_signal(|| BookIndex::with_outline(outline.clone()));
    use_context_provider(|| BookCtx { index });

    rsx! {
        style { dangerous_inner_html: "{BOOK_CSS}" }
        ManimGpuProvider {
            ParametersProvider {
                div { class: "mb-book",
                    if let Some(t) = title {
                        h1 { class: "mb-book-title", "{t}" }
                    }
                    {children}
                }
            }
        }
    }
}

/// A numbered chapter: an `<h1>` heading plus a positioning context for its
/// prose, figures and margin notes.
///
/// Chapters carry an explicit `number` (books get reordered; auto-numbering the
/// top level would silently renumber every cross-reference). Sections and
/// figures inside are auto-numbered relative to it.
#[component]
pub fn Chapter(
    /// The chapter number, as written by the author.
    number: u32,
    /// The chapter title.
    title: String,
    children: Element,
) -> Element {
    let ctx = use_hook(|| ChapterCtx::new(number));
    use_context_provider(|| ctx.clone());

    // Make sure the chapter appears in the index even if the author skipped the
    // `Book`'s outline (single-chapter pages).
    let mut index = book_ctx().index;
    let (n, t) = (number, title.clone());
    use_effect(move || {
        index.with_mut(|i| {
            if !i.outline().iter().any(|c| c.number == n) {
                i.declare_chapter(ChapterEntry::anchored(n, t.clone()));
            }
        });
    });

    let anchor = chapter_anchor(number);
    rsx! {
        section { class: "mb-chapter", id: "{anchor}",
            header { class: "mb-chapter-head",
                p { class: "mb-chapter-eyebrow", "Chapter {number}" }
                h1 { class: "mb-chapter-title", "{title}" }
            }
            div { class: "mb-flow", {children} }
        }
    }
}

/// An auto-numbered section heading (`2.3 Conformality`) with a linkable anchor.
///
/// Registers itself with the book so it shows up nested under its chapter in
/// [`Toc`]. Numbering follows mount order within the enclosing [`Chapter`].
#[component]
pub fn Section(
    /// The section title.
    title: String,
    children: Element,
) -> Element {
    let chapter = chapter_ctx();
    let idx = use_hook(|| chapter.next_section());
    let anchor = section_anchor(chapter.number, idx, &title);

    let mut index = book_ctx().index;
    let (ch, t, a) = (chapter.number, title.clone(), anchor.clone());
    use_effect(move || {
        index.with_mut(|i| {
            i.register_section(
                ch,
                SectionEntry {
                    index: idx,
                    title: t.clone(),
                    anchor: a.clone(),
                },
            );
        });
    });

    let number = crate::numbering::dotted(chapter.number, idx);
    rsx! {
        section { class: "mb-section",
            h2 { class: "mb-section-title", id: "{anchor}",
                span { class: "mb-section-number", "{number}" }
                a { class: "mb-anchor", href: "#{anchor}", "{title}" }
            }
            {children}
        }
    }
}

/// A block of body text held to the book's reading measure (~65ch).
///
/// Pass rsx children: bare text for a single paragraph, or `p { .. }` elements
/// for several. Links, `code`, and lists inside are styled by the book CSS.
#[component]
pub fn Prose(children: Element) -> Element {
    rsx! { div { class: "mb-prose", {children} } }
}

/// Inline math — **placeholder rendering** (see the module note).
///
/// Renders `source` in the book's serif italic face rather than typesetting it.
/// Real typesetting means running manim-text's typst pipeline to SVG at build
/// time and embedding a data URI; that is a build-harness change (see the
/// `tools/render-examples` pattern) rather than a component change, so the API
/// is fixed here and the renderer is deferred. Authors can write the API today
/// and get typeset output for free when it lands.
#[component]
pub fn MathInline(
    /// The math source (typst syntax, when typesetting lands).
    source: String,
) -> Element {
    rsx! { span { class: "mb-math", "{source}" } }
}

/// Display math on its own centred line — **placeholder rendering**, as
/// [`MathInline`].
#[component]
pub fn MathBlock(
    /// The math source (typst syntax, when typesetting lands).
    source: String,
) -> Element {
    rsx! { div { class: "mb-math-block", "{source}" } }
}

/// A numbered, captioned, optionally labelled figure wrapping
/// [`manim_dioxus::Figure`].
///
/// The figure number is claimed from the enclosing [`Chapter`] at mount, so the
/// caption reads `Figure 2.3` with no bookkeeping by the author. Giving it a
/// `label` registers it for [`Ref`] cross-references and gives the block a
/// stable `id` to link to.
///
/// ```no_run
/// # use dioxus::prelude::*;
/// # use manim_core::prelude::*; use manim_core::error::Result;
/// use manim_book::FigureBlock;
/// # #[derive(Clone, PartialEq)] struct Coloring;
/// # impl SceneBuilder for Coloring { fn construct(&self, _: &mut Scene) -> Result<()> { Ok(()) } }
/// # fn body() -> Element {
/// rsx! {
///     FigureBlock {
///         scene: Coloring,
///         label: "coloring",
///         caption: "Domain coloring of a rational map. Drag the zeros and poles.",
///     }
/// }
/// # }
/// ```
#[component]
pub fn FigureBlock<S: SceneBuilder + Clone + PartialEq + 'static>(
    /// The scene to render.
    scene: S,
    /// Caption text, rendered after the `Figure N.M` prefix.
    #[props(default)]
    caption: Option<String>,
    /// A cross-reference label; [`Ref`] resolves it to `Fig N.M`.
    #[props(default)]
    label: Option<String>,
    /// A live updater (drag handles, orbit controls, parameter reads).
    #[props(default)]
    live: Option<LiveUpdater>,
    /// Render settings; defaults to [`Config::low`], as [`Figure`] does.
    #[props(default)]
    config: Option<Config>,
    /// Freeze an animated scene at this time (seconds).
    #[props(default)]
    time: Option<f32>,
    /// CSS width of the figure; defaults to `100%` (responsive).
    #[props(default)]
    width: Option<String>,
    /// CSS height override; by default the aspect ratio decides.
    #[props(default)]
    height: Option<String>,
    /// Defer the first render until scrolled into view. Default `true`.
    #[props(default = true)]
    lazy: bool,
    /// Let the figure exceed the reading measure on wide screens.
    #[props(default = false)]
    wide: bool,
) -> Element {
    let chapter = chapter_ctx();
    let idx = use_hook(|| chapter.next_figure());
    let fig = FigureRef {
        chapter: chapter.number,
        index: idx,
    };

    let mut index = book_ctx().index;
    let lbl = label.clone();
    use_effect(move || {
        if let Some(l) = lbl.clone() {
            index.with_mut(|i| i.register_figure(l, fig));
        }
    });

    let anchor = label.as_deref().map(figure_anchor);
    let prefix = figure_caption_prefix(fig.chapter, fig.index);
    let class = if wide {
        "mb-figure mb-figure-wide"
    } else {
        "mb-figure"
    };

    rsx! {
        figure { class: "{class}", id: anchor,
            div { class: "mb-figure-frame",
                Figure {
                    scene,
                    live,
                    config,
                    time,
                    width: width.unwrap_or_else(|| "100%".to_string()),
                    height,
                    lazy,
                }
            }
            figcaption { class: "mb-caption",
                span { class: "mb-caption-label", "{prefix}." }
                if let Some(c) = caption {
                    "{c}"
                }
            }
        }
    }
}

/// A cross-reference to a labelled [`FigureBlock`], rendered as a link reading
/// `Fig 2.3`.
///
/// Resolution is reactive: if the reference appears in the prose *before* the
/// figure it points at has mounted, it renders as an unresolved marker and
/// upgrades to a link as soon as the figure registers.
#[component]
pub fn Ref(
    /// The label given to a [`FigureBlock`].
    label: String,
) -> Element {
    let index = book_ctx().index;
    let resolved = index.read().resolve(&label);
    match resolved {
        Some(text) => {
            let anchor = figure_anchor(&label);
            rsx! { a { class: "mb-ref", href: "#{anchor}", "{text}" } }
        }
        None => {
            rsx! { span { class: "mb-ref-unresolved", title: "unresolved label: {label}", "[?{label}]" } }
        }
    }
}

/// The kind of a [`Callout`] — decides its colour and its uppercase label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalloutKind {
    /// A definition of a term.
    Definition,
    /// A theorem, lemma or proposition.
    Theorem,
    /// A worked example.
    Example,
    /// A caveat, pitfall or common mistake.
    Warning,
}

impl CalloutKind {
    /// The uppercase label rendered in the callout header.
    pub fn label(self) -> &'static str {
        match self {
            Self::Definition => "Definition",
            Self::Theorem => "Theorem",
            Self::Example => "Example",
            Self::Warning => "Warning",
        }
    }

    /// The CSS modifier class for this kind.
    pub fn class(self) -> &'static str {
        match self {
            Self::Definition => "mb-callout mb-callout-definition",
            Self::Theorem => "mb-callout mb-callout-theorem",
            Self::Example => "mb-callout mb-callout-example",
            Self::Warning => "mb-callout mb-callout-warning",
        }
    }
}

/// A styled aside box: definition, theorem, example or warning.
///
/// ```no_run
/// # use dioxus::prelude::*;
/// use manim_book::{Callout, CalloutKind};
/// # fn body() -> Element {
/// rsx! {
///     Callout { kind: CalloutKind::Definition, title: "Conformal",
///         "A map is conformal where it preserves angles."
///     }
/// }
/// # }
/// ```
#[component]
pub fn Callout(
    /// Which kind of box this is.
    kind: CalloutKind,
    /// An optional title shown next to the kind label.
    #[props(default)]
    title: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        aside { class: "{kind.class()}",
            p { class: "mb-callout-head",
                "{kind.label()}"
                if let Some(t) = title {
                    span { class: "mb-callout-title", "{t}" }
                }
            }
            div { class: "mb-callout-body mb-prose", {children} }
        }
    }
}

/// A short aside. On wide screens it floats into the right gutter beside the
/// prose; on narrow screens it becomes an indented note in the flow.
///
/// Place it immediately *after* the paragraph it comments on — the widescreen
/// layout pulls it up to align with that point in the text.
#[component]
pub fn MarginNote(children: Element) -> Element {
    rsx! { aside { class: "mb-marginnote", {children} } }
}

/// The table of contents, built from the [`Book`]'s outline plus every
/// [`Section`] that has mounted.
///
/// Chapters declared in the outline but not currently mounted still appear (with
/// no sections), so a one-chapter-per-page site gets a complete TOC.
#[component]
pub fn Toc(
    /// Heading above the list. Defaults to `Contents`.
    #[props(default)]
    title: Option<String>,
) -> Element {
    let index = book_ctx().index;
    let current = try_consume_context::<ChapterCtx>().map(|c| c.number);
    let entries = index.read().toc();
    let heading = title.unwrap_or_else(|| "Contents".to_string());

    rsx! {
        nav { class: "mb-toc", aria_label: "Table of contents",
            p { class: "mb-toc-head", "{heading}" }
            ol {
                for ch in entries {
                    li { key: "{ch.number}",
                        class: if Some(ch.number) == current { "mb-toc-current" } else { "" },
                        a { href: "{ch.href}",
                            span { class: "mb-toc-num", "{ch.number}" }
                            "{ch.title}"
                        }
                        if !ch.sections.is_empty() {
                            ol { class: "mb-toc-sections",
                                for s in ch.sections {
                                    li { key: "{s.index}",
                                        a { href: "#{s.anchor}",
                                            span { class: "mb-toc-num", "{s.number(ch.number)}" }
                                            "{s.title}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Previous / next chapter links, derived from the [`Book`] outline and the
/// enclosing [`Chapter`]'s number.
///
/// Neighbours are positional, not arithmetic — a book numbered 1, 2, 5 links
/// chapter 2 to chapter 5. Place it at the end of a chapter.
#[component]
pub fn ChapterNav() -> Element {
    let index = book_ctx().index;
    let chapter = chapter_ctx();
    let idx = index.read();
    let (prev, next) = idx.neighbours(chapter.number);
    let prev = prev.cloned();
    let next = next.cloned();
    drop(idx);

    rsx! {
        nav { class: "mb-nav", aria_label: "Chapter navigation",
            if let Some(p) = prev {
                a { class: "mb-nav-prev", href: "{p.href}",
                    span { class: "mb-nav-dir", "← Previous" }
                    "{p.number}. {p.title}"
                }
            }
            if let Some(n) = next {
                a { class: "mb-nav-next", href: "{n.href}",
                    span { class: "mb-nav-dir", "Next →" }
                    "{n.number}. {n.title}"
                }
            }
        }
    }
}
