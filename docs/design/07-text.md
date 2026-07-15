# Text & Math Typesetting

All text becomes **VMobject outlines** (glyph beziers), exactly like manim —
so `Write`, `Transform`, per-letter animation, and gradients work on text with
zero special cases in the renderer.

## Plain text: `Text` / `Paragraph` / `MarkupText`

- Shaping: `cosmic-text` (harfbuzz-quality shaping, bidi, emoji, font fallback,
  system font discovery via `fontdb`; bundled Latin fallback font for wasm
  determinism).
- Outline extraction: shaped glyph ids → `ttf-parser` outline callbacks →
  cubic path per glyph (quadratics elevated to cubics) → one submobject per
  glyph, grouped word/line, mirroring CE's `Text` submobject structure
  (`text[i]` = i-th glyph) so `TransformMatchingShapes` and slicing parity hold.
- API parity: font, weight/slant, `t2c` (per-substring color), `t2w`, `t2s`,
  `line_spacing`, `disable_ligatures`. `MarkupText`: a pragmatic subset of Pango
  markup (`<b>`, `<i>`, `<span foreground=…>`, `<sub>`, `<sup>`).

## Math: `MathTex` / `Tex` via Typst

manim CE shells out to LaTeX + dvisvgm. We use the **typst** compiler crates
(pure Rust, fast, wasm-compatible) instead:

- `MathTex::new("e^{i\\pi} + 1 = 0")` — input is translated from a LaTeX-math
  subset to Typst math (a curated mapping table covering the common manim
  corpus: fractions, roots, sums/integrals, greek, accents, matrices, aligned
  environments). Escape hatch: `Typst::new(raw_typst)` for full typst syntax.
- Typst output (laid-out glyphs + positions) → same outline-extraction path as
  `Text` → submobject per glyph with CE-compatible substring indexing
  (`get_part_by_tex`, `index_of_part`, isolate-substrings via `substrings_to_isolate`).
- `Tex` (full text-mode LaTeX documents) maps to typst content mode.
- Optional `latex` cargo feature (native only, post-v1): true LaTeX+dvisvgm
  path for users who need exact LaTeX rendering; API identical.

Rationale: keeps web builds self-contained (no LaTeX in wasm), removes manim's
most painful install dependency, and typst quality is excellent. The parity
map tracks LaTeX-corpus coverage; golden tests pin popular formulas.

## SVG: `SVGMobject` / `ImageMobject`

- `usvg` normalizes SVG → path tree → VMobjects (fills, strokes, transforms,
  groups). CE-compatible: `SVGMobject::new("file.svg")`, `height` defaulting.
- `ImageMobject`: textured quad (separate `DrawItem::Image` variant; the one
  non-vector draw type). Nearest/linear sampling, opacity.

## Numbers

`DecimalNumber` / `Integer` / `Variable`: digit glyphs from `Text`, fixed-width
digit layout so `ChangingDecimal` doesn't jitter; `num_decimal_places`,
`include_sign`, `group_with_commas` parity.
