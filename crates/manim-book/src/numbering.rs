//! Pure numbering, anchoring and cross-reference logic (no Dioxus).
//!
//! The components in [`crate::components`] are a thin reactive shell over this
//! module: every counter, label and table-of-contents tree is computed here by
//! plain functions on plain data, so the interesting logic is unit-testable
//! natively without a browser or a virtual DOM.
//!
//! The model is deliberately small:
//!
//! * A **chapter** carries an explicit number (the author writes `number: 2`).
//! * A **section** is auto-numbered *within* its chapter → `2.3`.
//! * A **figure** is auto-numbered *within* its chapter → `Figure 2.3`.
//! * A figure may carry a **label**; a [`Ref`](crate::Ref) elsewhere resolves
//!   that label to `Fig 2.3` plus an anchor link.
//!
//! All of it lives in one [`BookIndex`], which components fill in as they mount
//! and read back to render the TOC and cross-references.

use std::collections::BTreeMap;

/// Turns a heading into a URL-safe anchor fragment.
///
/// Lowercases, keeps ASCII alphanumerics, and collapses every run of other
/// characters into a single `-` (with no leading or trailing dashes).
///
/// ```
/// # use manim_book::numbering::slug;
/// assert_eq!(slug("Complex Functions as Mappings"), "complex-functions-as-mappings");
/// assert_eq!(slug("  z ↦ z², really?  "), "z-z-really");
/// ```
pub fn slug(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut pending_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out
}

/// The anchor id for a chapter heading, e.g. `ch-2`.
pub fn chapter_anchor(chapter: u32) -> String {
    format!("ch-{chapter}")
}

/// The anchor id for a section heading, e.g. `sec-2-3-limits`.
pub fn section_anchor(chapter: u32, index: u32, title: &str) -> String {
    let s = slug(title);
    if s.is_empty() {
        format!("sec-{chapter}-{index}")
    } else {
        format!("sec-{chapter}-{index}-{s}")
    }
}

/// The anchor id for a labeled figure, e.g. `fig-domain-coloring`.
pub fn figure_anchor(label: &str) -> String {
    format!("fig-{}", slug(label))
}

/// A dotted number such as `2.3` — a chapter/index pair.
///
/// ```
/// # use manim_book::numbering::dotted;
/// assert_eq!(dotted(2, 3), "2.3");
/// ```
pub fn dotted(chapter: u32, index: u32) -> String {
    format!("{chapter}.{index}")
}

/// The caption prefix rendered above a figure, e.g. `Figure 2.3`.
pub fn figure_caption_prefix(chapter: u32, index: u32) -> String {
    format!("Figure {}", dotted(chapter, index))
}

/// The short form a [`Ref`](crate::Ref) renders, e.g. `Fig 2.3`.
pub fn figure_ref_text(chapter: u32, index: u32) -> String {
    format!("Fig {}", dotted(chapter, index))
}

/// Where a figure lives: its chapter and its 1-based index within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FigureRef {
    /// The enclosing chapter's number.
    pub chapter: u32,
    /// 1-based position among the figures of that chapter.
    pub index: u32,
}

impl FigureRef {
    /// The dotted figure number, e.g. `2.3`.
    pub fn number(&self) -> String {
        dotted(self.chapter, self.index)
    }

    /// The cross-reference text, e.g. `Fig 2.3`.
    pub fn ref_text(&self) -> String {
        figure_ref_text(self.chapter, self.index)
    }
}

/// A chapter as declared in a [`Book`](crate::Book)'s outline.
///
/// The outline is the whole book — including chapters that are not currently
/// mounted — which is what lets the table of contents and prev/next navigation
/// work on a site that renders one chapter per page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterEntry {
    /// The chapter number, as written by the author.
    pub number: u32,
    /// The chapter title.
    pub title: String,
    /// Where to navigate for this chapter (a route, a file, or `#ch-2` for a
    /// single-page book).
    pub href: String,
}

impl ChapterEntry {
    /// A chapter entry linking to `href`.
    pub fn new(number: u32, title: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            number,
            title: title.into(),
            href: href.into(),
        }
    }

    /// A chapter entry for a single-page book: links to the in-page anchor.
    pub fn anchored(number: u32, title: impl Into<String>) -> Self {
        let href = format!("#{}", chapter_anchor(number));
        Self {
            number,
            title: title.into(),
            href,
        }
    }
}

/// A section as registered by a mounted [`Section`](crate::Section).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionEntry {
    /// 1-based position within the chapter.
    pub index: u32,
    /// The section title.
    pub title: String,
    /// The heading's anchor id.
    pub anchor: String,
}

impl SectionEntry {
    /// The dotted section number within `chapter`, e.g. `2.3`.
    pub fn number(&self, chapter: u32) -> String {
        dotted(chapter, self.index)
    }
}

/// One chapter node of the rendered table of contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocChapter {
    /// The chapter number.
    pub number: u32,
    /// The chapter title.
    pub title: String,
    /// Link target for the chapter.
    pub href: String,
    /// Its sections, in document order. Empty for chapters that are declared in
    /// the outline but not currently mounted.
    pub sections: Vec<SectionEntry>,
}

/// Everything the book knows about itself: the chapter outline, the sections
/// mounted so far, and the label → figure map backing cross-references.
///
/// Components mutate this as they mount (each registration is **idempotent**, so
/// re-renders converge instead of double-counting) and read it back to render
/// the TOC, prev/next navigation and [`Ref`](crate::Ref) links.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookIndex {
    outline: Vec<ChapterEntry>,
    sections: BTreeMap<u32, Vec<SectionEntry>>,
    figures: BTreeMap<String, FigureRef>,
}

impl BookIndex {
    /// An empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// An index seeded with a book outline, sorted by chapter number.
    pub fn with_outline(outline: impl IntoIterator<Item = ChapterEntry>) -> Self {
        let mut idx = Self::new();
        for c in outline {
            idx.declare_chapter(c);
        }
        idx
    }

    /// Adds (or replaces, by number) a chapter in the outline, keeping the
    /// outline sorted by chapter number.
    pub fn declare_chapter(&mut self, entry: ChapterEntry) {
        match self.outline.iter_mut().find(|c| c.number == entry.number) {
            Some(existing) => *existing = entry,
            None => {
                self.outline.push(entry);
                self.outline.sort_by_key(|c| c.number);
            }
        }
    }

    /// Registers a mounted section under `chapter`. Idempotent per
    /// `(chapter, index)`.
    pub fn register_section(&mut self, chapter: u32, entry: SectionEntry) {
        let list = self.sections.entry(chapter).or_default();
        match list.iter_mut().find(|s| s.index == entry.index) {
            Some(existing) => *existing = entry,
            None => {
                list.push(entry);
                list.sort_by_key(|s| s.index);
            }
        }
    }

    /// Registers a labeled figure so [`resolve`](Self::resolve) can find it.
    /// Idempotent per label.
    pub fn register_figure(&mut self, label: impl Into<String>, figure: FigureRef) {
        self.figures.insert(label.into(), figure);
    }

    /// Looks up a figure by label.
    pub fn figure(&self, label: &str) -> Option<FigureRef> {
        self.figures.get(label).copied()
    }

    /// Resolves a label to its cross-reference text (`Fig 2.3`), or `None` if no
    /// figure with that label has mounted yet.
    pub fn resolve(&self, label: &str) -> Option<String> {
        self.figure(label).map(|f| f.ref_text())
    }

    /// The declared chapter outline, sorted by number.
    pub fn outline(&self) -> &[ChapterEntry] {
        &self.outline
    }

    /// The sections registered for `chapter`, in order.
    pub fn sections_of(&self, chapter: u32) -> &[SectionEntry] {
        self.sections
            .get(&chapter)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The full table-of-contents tree: every outlined chapter with whatever
    /// sections have mounted.
    pub fn toc(&self) -> Vec<TocChapter> {
        self.outline
            .iter()
            .map(|c| TocChapter {
                number: c.number,
                title: c.title.clone(),
                href: c.href.clone(),
                sections: self.sections_of(c.number).to_vec(),
            })
            .collect()
    }

    /// The chapters immediately before and after `chapter` in the outline.
    ///
    /// Neighbours are positional, not arithmetic: a book numbered 1, 2, 5 gives
    /// chapter 2 the neighbours 1 and 5.
    pub fn neighbours(&self, chapter: u32) -> (Option<&ChapterEntry>, Option<&ChapterEntry>) {
        let Some(pos) = self.outline.iter().position(|c| c.number == chapter) else {
            return (None, None);
        };
        let prev = pos.checked_sub(1).and_then(|i| self.outline.get(i));
        (prev, self.outline.get(pos + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_lowercases_and_collapses_separators() {
        assert_eq!(
            slug("Complex Functions as Mappings"),
            "complex-functions-as-mappings"
        );
        assert_eq!(slug("Möbius  maps!!"), "m-bius-maps");
        assert_eq!(slug("  padded  "), "padded");
        assert_eq!(slug("—"), "");
        assert_eq!(slug("A1"), "a1");
    }

    #[test]
    fn anchors_are_stable_and_namespaced() {
        assert_eq!(chapter_anchor(2), "ch-2");
        assert_eq!(
            section_anchor(2, 3, "The Argument Principle"),
            "sec-2-3-the-argument-principle"
        );
        assert_eq!(section_anchor(2, 3, "∮"), "sec-2-3");
        assert_eq!(figure_anchor("Domain Coloring"), "fig-domain-coloring");
    }

    #[test]
    fn numbers_are_chapter_dotted() {
        assert_eq!(dotted(2, 3), "2.3");
        assert_eq!(figure_caption_prefix(2, 3), "Figure 2.3");
        assert_eq!(figure_ref_text(2, 3), "Fig 2.3");
        let f = FigureRef {
            chapter: 4,
            index: 1,
        };
        assert_eq!(f.number(), "4.1");
        assert_eq!(f.ref_text(), "Fig 4.1");
    }

    #[test]
    fn section_entry_numbers_within_its_chapter() {
        let s = SectionEntry {
            index: 2,
            title: "Conformality".into(),
            anchor: "sec-1-2".into(),
        };
        assert_eq!(s.number(1), "1.2");
        assert_eq!(s.number(7), "7.2");
    }

    #[test]
    fn outline_is_sorted_and_replaceable_by_number() {
        let mut idx = BookIndex::new();
        idx.declare_chapter(ChapterEntry::new(3, "Third", "/c3"));
        idx.declare_chapter(ChapterEntry::new(1, "First", "/c1"));
        idx.declare_chapter(ChapterEntry::new(2, "Second", "/c2"));
        assert_eq!(
            idx.outline().iter().map(|c| c.number).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        // Re-declaring replaces rather than duplicating (idempotent mounting).
        idx.declare_chapter(ChapterEntry::new(2, "Second, revised", "/c2"));
        assert_eq!(idx.outline().len(), 3);
        assert_eq!(idx.outline()[1].title, "Second, revised");
    }

    #[test]
    fn anchored_entries_link_in_page() {
        let c = ChapterEntry::anchored(2, "Mappings");
        assert_eq!(c.href, "#ch-2");
        assert_eq!(c.number, 2);
    }

    #[test]
    fn sections_register_idempotently_and_sort() {
        let mut idx = BookIndex::new();
        let mk = |i: u32, t: &str| SectionEntry {
            index: i,
            title: t.into(),
            anchor: section_anchor(1, i, t),
        };
        idx.register_section(1, mk(2, "Second"));
        idx.register_section(1, mk(1, "First"));
        idx.register_section(1, mk(2, "Second")); // remount: no duplicate
        assert_eq!(idx.sections_of(1).len(), 2);
        assert_eq!(idx.sections_of(1)[0].title, "First");
        assert_eq!(idx.sections_of(1)[1].number(1), "1.2");
        assert!(idx.sections_of(9).is_empty());
    }

    #[test]
    fn figure_labels_resolve_to_cross_reference_text() {
        let mut idx = BookIndex::new();
        idx.register_figure(
            "coloring",
            FigureRef {
                chapter: 2,
                index: 1,
            },
        );
        idx.register_figure(
            "grid",
            FigureRef {
                chapter: 2,
                index: 2,
            },
        );
        assert_eq!(idx.resolve("coloring").as_deref(), Some("Fig 2.1"));
        assert_eq!(idx.resolve("grid").as_deref(), Some("Fig 2.2"));
        assert_eq!(idx.resolve("nope"), None);
        // Remount with the same numbers is a no-op.
        idx.register_figure(
            "coloring",
            FigureRef {
                chapter: 2,
                index: 1,
            },
        );
        assert_eq!(
            idx.figure("coloring"),
            Some(FigureRef {
                chapter: 2,
                index: 1
            })
        );
    }

    #[test]
    fn toc_tree_pairs_outline_with_mounted_sections() {
        let mut idx = BookIndex::with_outline([
            ChapterEntry::new(1, "Mappings", "/ch1"),
            ChapterEntry::new(2, "Coming soon", "/ch2"),
        ]);
        idx.register_section(
            1,
            SectionEntry {
                index: 1,
                title: "Domain coloring".into(),
                anchor: "sec-1-1".into(),
            },
        );
        idx.register_section(
            1,
            SectionEntry {
                index: 2,
                title: "Conformality".into(),
                anchor: "sec-1-2".into(),
            },
        );
        let toc = idx.toc();
        assert_eq!(toc.len(), 2);
        assert_eq!(toc[0].title, "Mappings");
        assert_eq!(toc[0].sections.len(), 2);
        assert_eq!(toc[0].sections[1].anchor, "sec-1-2");
        // An outlined-but-unmounted chapter still appears, with no sections.
        assert_eq!(toc[1].number, 2);
        assert!(toc[1].sections.is_empty());
    }

    #[test]
    fn neighbours_are_positional_not_arithmetic() {
        let idx = BookIndex::with_outline([
            ChapterEntry::new(1, "One", "/1"),
            ChapterEntry::new(2, "Two", "/2"),
            ChapterEntry::new(5, "Five", "/5"),
        ]);
        let (prev, next) = idx.neighbours(2);
        assert_eq!(prev.map(|c| c.number), Some(1));
        assert_eq!(next.map(|c| c.number), Some(5));

        let (prev, next) = idx.neighbours(1);
        assert!(prev.is_none());
        assert_eq!(next.map(|c| c.number), Some(2));

        let (prev, next) = idx.neighbours(5);
        assert_eq!(prev.map(|c| c.number), Some(2));
        assert!(next.is_none());

        // Unknown chapter: no neighbours at all.
        assert_eq!(idx.neighbours(99), (None, None));
    }
}
