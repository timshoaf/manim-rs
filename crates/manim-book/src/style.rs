//! The single embedded stylesheet for a [`Book`](crate::Book).
//!
//! One string, injected once by [`Book`](crate::Book). Everything is namespaced
//! under `.mb-*` class names so it cannot collide with a host site's CSS, and
//! every colour comes from a custom property so a host can re-theme the book by
//! overriding `--mb-*` on `.mb-book`.
//!
//! Design contract:
//!
//! * **Reading measure** — prose is capped at `--mb-measure` (65ch).
//! * **Mobile-first** — the single-column layout is the base; the margin-note
//!   gutter only appears above 60rem, where there is room for it.
//! * **Light + dark** — the default is light; `prefers-color-scheme: dark` flips
//!   the custom properties, so figures (which render dark) sit on a dark page.

/// The book stylesheet. Injected by [`Book`](crate::Book); authors never touch it.
pub const BOOK_CSS: &str = r#"
.mb-book {
  --mb-measure: 65ch;
  --mb-gutter: 15rem;
  --mb-bg: #fbfaf7;
  --mb-fg: #1c1b19;
  --mb-muted: #6b6864;
  --mb-rule: #e0dcd4;
  --mb-accent: #1b6f6a;
  --mb-accent-soft: #e5f1ef;
  --mb-figure-bg: #000;
  --mb-serif: Iowan Old Style, Palatino, Palatino Linotype, Georgia, serif;
  --mb-sans: system-ui, -apple-system, Segoe UI, Helvetica, Arial, sans-serif;
  --mb-mono: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;

  background: var(--mb-bg);
  color: var(--mb-fg);
  font-family: var(--mb-serif);
  font-size: 1.05rem;
  line-height: 1.62;
  min-height: 100vh;
  box-sizing: border-box;
  padding: 2.5rem 1.15rem 5rem;
  -webkit-text-size-adjust: 100%;
}
.mb-book *, .mb-book *::before, .mb-book *::after { box-sizing: inherit; }

@media (prefers-color-scheme: dark) {
  .mb-book {
    --mb-bg: #14161a;
    --mb-fg: #e6e4e0;
    --mb-muted: #98a0a8;
    --mb-rule: #2c3138;
    --mb-accent: #6fd6cb;
    --mb-accent-soft: #17302f;
  }
}

/* --- shell ------------------------------------------------------------- */
.mb-book-title {
  font-size: clamp(1.6rem, 5vw, 2.3rem);
  line-height: 1.15;
  margin: 0 auto 2.5rem;
  max-width: var(--mb-measure);
  font-weight: 600;
  letter-spacing: -0.01em;
}
.mb-flow > * { max-width: var(--mb-measure); margin-left: auto; margin-right: auto; }

/* --- headings ----------------------------------------------------------- */
.mb-chapter { margin: 0 auto; }
.mb-chapter-head {
  max-width: var(--mb-measure);
  margin: 0 auto 2rem;
  padding-bottom: 0.9rem;
  border-bottom: 1px solid var(--mb-rule);
}
.mb-chapter-eyebrow {
  font-family: var(--mb-sans);
  font-size: 0.74rem;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  color: var(--mb-muted);
  margin: 0 0 0.4rem;
}
.mb-chapter-title { font-size: clamp(1.5rem, 4.5vw, 2.05rem); line-height: 1.18; margin: 0; font-weight: 600; }
.mb-section { margin: 2.6rem auto 0; }
.mb-section-title {
  max-width: var(--mb-measure);
  margin: 0 auto 0.9rem;
  font-size: clamp(1.15rem, 3.4vw, 1.4rem);
  line-height: 1.25;
  font-weight: 600;
  scroll-margin-top: 1.5rem;
}
.mb-section-number { color: var(--mb-muted); font-variant-numeric: tabular-nums; margin-right: 0.55em; }
.mb-anchor { color: inherit; text-decoration: none; }
.mb-anchor:hover { color: var(--mb-accent); }

/* --- prose -------------------------------------------------------------- */
.mb-prose { max-width: var(--mb-measure); margin: 0 auto 1.15rem; }
.mb-prose p { margin: 0 0 1.05rem; }
.mb-prose p:last-child { margin-bottom: 0; }
.mb-prose a { color: var(--mb-accent); text-decoration-thickness: 1px; text-underline-offset: 2px; }
.mb-prose code { font-family: var(--mb-mono); font-size: 0.88em; background: var(--mb-accent-soft); padding: 0.1em 0.32em; border-radius: 4px; }
.mb-prose ul, .mb-prose ol { margin: 0 0 1.05rem; padding-left: 1.4rem; }
.mb-prose li { margin-bottom: 0.35rem; }

/* --- math (placeholder typography until typst→SVG lands) ---------------- */
.mb-math { font-family: var(--mb-serif); font-style: italic; white-space: nowrap; }
.mb-math-block {
  display: block;
  max-width: var(--mb-measure);
  margin: 1.4rem auto;
  text-align: center;
  font-family: var(--mb-serif);
  font-style: italic;
  font-size: 1.12rem;
  overflow-x: auto;
}

/* --- figures ------------------------------------------------------------ */
.mb-figure {
  max-width: min(var(--mb-measure), 100%);
  margin: 1.9rem auto;
  scroll-margin-top: 1.5rem;
}
.mb-figure-frame {
  border: 1px solid var(--mb-rule);
  border-radius: 10px;
  overflow: hidden;
  background: var(--mb-figure-bg);
  line-height: 0;
}
.mb-figure .manim-figure { width: 100%; }
.mb-caption {
  font-family: var(--mb-sans);
  font-size: 0.85rem;
  line-height: 1.5;
  color: var(--mb-muted);
  margin: 0.6rem 0 0;
}
.mb-caption-label { color: var(--mb-fg); font-weight: 600; margin-right: 0.4em; }

/* --- cross-references --------------------------------------------------- */
.mb-ref { color: var(--mb-accent); text-decoration: none; border-bottom: 1px solid currentColor; white-space: nowrap; }
.mb-ref:hover { background: var(--mb-accent-soft); }
.mb-ref-unresolved { color: #b4342b; border-bottom: 1px dotted currentColor; white-space: nowrap; }

/* --- callouts ----------------------------------------------------------- */
.mb-callout {
  max-width: var(--mb-measure);
  margin: 1.5rem auto;
  padding: 0.95rem 1.1rem;
  border-left: 3px solid var(--mb-callout-color, var(--mb-accent));
  border-radius: 0 8px 8px 0;
  background: var(--mb-callout-bg, var(--mb-accent-soft));
}
.mb-callout-head {
  font-family: var(--mb-sans);
  font-size: 0.72rem;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  font-weight: 700;
  color: var(--mb-callout-color, var(--mb-accent));
  margin: 0 0 0.4rem;
}
.mb-callout-title { text-transform: none; letter-spacing: 0; font-size: 0.95rem; margin-left: 0.5em; color: var(--mb-fg); }
.mb-callout-body > *:last-child { margin-bottom: 0; }
.mb-callout-definition { --mb-callout-color: #1b6f6a; --mb-callout-bg: #e5f1ef; }
.mb-callout-theorem    { --mb-callout-color: #4a4ac2; --mb-callout-bg: #e9e9f8; }
.mb-callout-example    { --mb-callout-color: #7a5b12; --mb-callout-bg: #f6efdd; }
.mb-callout-warning    { --mb-callout-color: #a63a2a; --mb-callout-bg: #f9e7e3; }
@media (prefers-color-scheme: dark) {
  .mb-callout-definition { --mb-callout-color: #6fd6cb; --mb-callout-bg: #16302e; }
  .mb-callout-theorem    { --mb-callout-color: #9aa0ff; --mb-callout-bg: #1e1f38; }
  .mb-callout-example    { --mb-callout-color: #e0bd6a; --mb-callout-bg: #322a17; }
  .mb-callout-warning    { --mb-callout-color: #f0917f; --mb-callout-bg: #351f1b; }
}

/* --- margin notes ------------------------------------------------------- */
/* Mobile-first: an indented aside in the flow. Widescreen: the right gutter. */
.mb-marginnote {
  max-width: var(--mb-measure);
  margin: 1.1rem auto;
  padding-left: 0.9rem;
  border-left: 2px solid var(--mb-rule);
  font-family: var(--mb-sans);
  font-size: 0.82rem;
  line-height: 1.5;
  color: var(--mb-muted);
}

/* --- navigation --------------------------------------------------------- */
.mb-toc {
  max-width: var(--mb-measure);
  margin: 0 auto 2.5rem;
  padding: 1rem 1.2rem;
  border: 1px solid var(--mb-rule);
  border-radius: 10px;
  font-family: var(--mb-sans);
  font-size: 0.9rem;
}
.mb-toc-head { font-size: 0.72rem; letter-spacing: 0.14em; text-transform: uppercase; color: var(--mb-muted); margin: 0 0 0.6rem; }
.mb-toc ol { list-style: none; margin: 0; padding: 0; }
.mb-toc li { margin: 0.25rem 0; }
.mb-toc-sections { padding-left: 1.1rem !important; margin-top: 0.2rem !important; }
.mb-toc a { color: var(--mb-fg); text-decoration: none; }
.mb-toc a:hover { color: var(--mb-accent); text-decoration: underline; }
.mb-toc-num { color: var(--mb-muted); font-variant-numeric: tabular-nums; margin-right: 0.5em; }
.mb-toc-current > a { font-weight: 700; color: var(--mb-accent); }

.mb-nav {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
  justify-content: space-between;
  max-width: var(--mb-measure);
  margin: 3rem auto 0;
  padding-top: 1.2rem;
  border-top: 1px solid var(--mb-rule);
  font-family: var(--mb-sans);
  font-size: 0.9rem;
}
.mb-nav a {
  flex: 1 1 12rem;
  display: block;
  padding: 0.7rem 0.9rem;
  border: 1px solid var(--mb-rule);
  border-radius: 8px;
  color: var(--mb-fg);
  text-decoration: none;
}
.mb-nav a:hover { border-color: var(--mb-accent); color: var(--mb-accent); }
.mb-nav-next { text-align: right; }
.mb-nav-dir { display: block; font-size: 0.7rem; letter-spacing: 0.12em; text-transform: uppercase; color: var(--mb-muted); margin-bottom: 0.2rem; }

/* --- widescreen: margin notes move into the gutter ---------------------- */
@media (min-width: 60rem) {
  .mb-book { padding-left: 2rem; padding-right: 2rem; }
  .mb-flow { position: relative; }
  .mb-marginnote {
    position: absolute;
    left: calc(50% + var(--mb-measure) / 2 + 2rem);
    width: var(--mb-gutter);
    max-width: var(--mb-gutter);
    margin: 0;
    padding-left: 0.85rem;
    transform: translateY(-0.35rem);
  }
  /* Wide figures may exceed the measure, up to the gutter edge. */
  .mb-figure-wide { max-width: calc(var(--mb-measure) + 12rem); }
}
"#;
