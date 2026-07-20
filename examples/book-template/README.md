# book-template — an interactive textbook, ready to write in

A runnable [`manim-book`](../../crates/manim-book) site with one real chapter
("Complex Functions as Mappings") and one stub. Copy this directory, delete the
sample chapter, and start writing.

```sh
cd examples/book-template
dx serve            # http://localhost:8080
```

(`cargo install dioxus-cli` if you don't have `dx`. Plain
`cargo check --target wasm32-unknown-unknown` also works — figures render on
wasm only; a native build shows placeholder boxes.)

## The authoring model

A chapter is **prose plus scene structs**. The scaffold owns everything else:
numbering, cross-references, the table of contents, prev/next navigation,
callout styling, the reading measure, light/dark theming, the shared GPU device,
and the parameter plumbing between sliders and figures.

`src/main.rs` is organised in the four layers you'll work in:

1. **Scene structs** — ordinary `SceneBuilder`s, plus a `LiveUpdater` if the
   figure is interactive. Your actual mathematics.
2. **Figure components** — a slider (`use_parameter`) next to the `FigureBlock`
   it drives. Only needed for parameterised figures.
3. **Chapters** — the rsx: `Chapter` → `Section` → `Prose` / `FigureBlock` /
   `Callout` / `MarginNote` / `Ref`.
4. **The book** — one `Book` with an `outline`.

### Adding a chapter

Two edits, well under 20 lines:

```rust
// 1. One line in the outline.
fn outline() -> Vec<ChapterEntry> {
    vec![
        ChapterEntry::anchored(1, "Complex Functions as Mappings"),
        ChapterEntry::anchored(2, "The Riemann Sphere"),
        ChapterEntry::anchored(3, "Integration"),          // <- new
    ]
}

// 2. One chapter block, mounted in `app()`.
#[component]
fn ChapterThree() -> Element {
    rsx! {
        Chapter { number: 3, title: "Integration",
            Section { title: "Contours",
                Prose { "A contour integral sums f(z) dz along a path." }
                FigureBlock { scene: ContourScene, label: "contour", caption: "A contour in the plane." }
            }
            ChapterNav {}
        }
    }
}
```

That's it. The section numbers itself `3.1`, the figure captions itself
`Figure 3.1`, a `Ref { label: "contour" }` anywhere in the book renders
`Fig 3.1` as a link, the TOC grows a nested entry, and chapter 2's "Next →"
starts pointing here.

### What each component gives you

| Component | What it does for you |
|---|---|
| `Book { title, outline }` | Stylesheet, shared `wgpu` device, shared parameter set, the index behind TOC/refs/nav |
| `Chapter { number, title }` | `<h1>`, `#ch-N` anchor, and the counters sections and figures draw from |
| `Section { title }` | Auto-numbered `<h2>` (`1.2`), linkable anchor, TOC registration |
| `Prose` | Body text at a 65ch reading measure |
| `FigureBlock { scene, caption, label }` | `manim_dioxus::Figure` + `Figure N.M` caption + the anchor `Ref` links to |
| `Ref { label }` | `Fig N.M` cross-reference link — resolves even if written *before* the figure |
| `Callout { kind, title }` | Definition / Theorem / Example / Warning boxes |
| `MarginNote` | Right-gutter aside on wide screens, indented note on mobile |
| `Toc` / `ChapterNav` | Contents tree and prev/next, both derived from the outline |
| `MathInline` / `MathBlock` | **Placeholder**: styles the source in serif italic; typesetting is not wired up yet |

### Things worth knowing

- **Parameter names are book-wide.** `Book` provides one `ParametersProvider`,
  so two figures using `"phase"` share a slider — deliberately handy, and a
  collision hazard. Prefix them per chapter (`"ch1.phase"`) once you have many,
  or wrap a figure in its own `ParametersProvider` to isolate it.
- **Chapter numbers are explicit; everything under them is automatic.** Books
  get reordered, and auto-numbering the top level would silently renumber every
  cross-reference in the text.
- **`Ref` resolves reactively.** A reference to a figure further down the page
  renders as `[?label]` for one frame and upgrades to a link the moment that
  figure mounts. A permanently red `[?label]` means the label is misspelled.
- **Figures are lazy by default** (`lazy: true` — they render when scrolled into
  view). Interactive figures in this template pass `lazy: false` so their first
  frame is up immediately.
- **This template is a single page** — chapters use `#ch-N` anchors. For a
  one-chapter-per-page site, swap `ChapterEntry::anchored` for
  `ChapterEntry::new(2, "…", "/chapter-2")` and mount one chapter per route; the
  outline is what makes the TOC and nav complete either way.
