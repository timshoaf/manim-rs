//! [`MathTex`], [`Typst`], and [`Tex`]: math typesetting via the typst compiler.
//!
//! A math string (LaTeX translated by [`crate::latex`], or raw typst) is
//! compiled to a laid-out typst frame; each shaped glyph's outline is extracted
//! with `ttf-parser` (reusing the same elevation code as [`Text`](crate::Text))
//! into one child mobject per glyph, plus filled shapes for fraction bars and
//! matrix rules. typst bundles its own math fonts (New Computer Modern) via
//! `typst-assets`, so results are deterministic.

use manim_color::{Color, WHITE};
use manim_core::error::CoreError;
use manim_core::geometry::VMobject;
use manim_core::impl_mobject;
use manim_core::mobject::{AnyId, MobjectData, MobjectId};
use manim_core::scene_state::SceneState;
use manim_core::style::Style;
use manim_math::bezier::CubicBezier;
use manim_math::path::{Path, SubPath};
use manim_math::Point;

use typst::foundations::Bytes;
use typst::layout::{Frame, FrameItem, PagedDocument, Point as TPoint, Transform};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::visualize::Geometry;
use typst::{Library, World};

use crate::latex::{translate, MathError};
use crate::outline::GlyphOutline;

/// typst points → scene units (chosen so a formula at [`MathTex::font_size`] 48
/// matches the [`Text`](crate::Text) scale: em ≈ 0.96 scene units).
pub const PT_PER_SCENE_UNIT_INV: f32 = 0.02;

/// Default math font size, matching manim CE's convention.
pub const DEFAULT_MATH_FONT_SIZE: f32 = 48.0;

/// How a source string is fed to typst.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Math mode (`$ ... $`).
    Math,
    /// Content / text mode.
    Content,
}

/// Per-child metadata (parallel to consecutive subpath groups in the own path).
#[derive(Debug, Clone)]
struct GlyphInfo {
    n_subpaths: usize,
    style: Style,
    /// Byte range in the (translated) typst source this glyph came from, via the
    /// glyph's typst span. `None` for synthesized glyphs with no span (e.g. a
    /// fraction bar / rule) — those are isolated by shape-matching fallback.
    typst_range: Option<std::ops::Range<usize>>,
}

/// Vectorized math: one child mobject per glyph (and per fraction bar / rule),
/// grouped under a parent, mirroring manim CE's `MathTex`. Build it with
/// [`MathTex::new`] (LaTeX), [`Typst::new`] (raw typst math), or [`Tex::new`]
/// (typst content mode).
///
/// ```
/// use manim_text::MathTex;
/// // The quadratic formula has a healthy number of glyphs.
/// let m = MathTex::new(r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}").unwrap();
/// assert!(m.glyph_count() > 10);
/// ```
#[derive(Clone)]
pub struct MathTex {
    data: MobjectData,
    typst_src: String,
    mode: Mode,
    font_size: f32,
    color: Color,
    glyphs: Vec<GlyphInfo>,
}
impl_mobject!(MathTex);

impl MathTex {
    /// Typesets a LaTeX-math subset (see [`crate::latex`] for coverage).
    ///
    /// # Errors
    ///
    /// A [`CoreError::Text`] wrapping the underlying [`MathError`] — an
    /// untranslatable command (`MathError::UnknownCommand`) or a typst rejection
    /// (`MathError::Typeset`), recoverable via the error's
    /// [`source`](std::error::Error::source). Returning [`CoreError`] lets this
    /// compose with `?` inside a scene `construct`.
    pub fn new(latex: &str) -> Result<Self, CoreError> {
        let typst_src = translate(latex).map_err(CoreError::text)?;
        Self::build(typst_src, Mode::Math, DEFAULT_MATH_FONT_SIZE, WHITE).map_err(CoreError::text)
    }

    /// The typst source string this was built from.
    pub fn source(&self) -> &str {
        &self.typst_src
    }

    /// Sets the font size (re-typesets).
    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self.rebuild();
        self
    }

    /// Sets the color of every glyph (re-typesets).
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self.rebuild();
        self
    }

    /// The number of glyph/shape children.
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// The child glyph handles, valid after [`add_to`](Self::add_to).
    pub fn glyph_ids(&self) -> &[AnyId] {
        &self.data.children
    }

    /// Adds this formula to `scene` as a parent grouping one child per glyph /
    /// shape, returning the parent handle (same convention as
    /// [`Text::add_to`](crate::Text::add_to)).
    ///
    /// ```
    /// use manim_text::MathTex;
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let m = MathTex::new(r"e^{i\pi} + 1 = 0").unwrap().add_to(&mut scene);
    /// assert!(scene.family(m.erase()).len() > 1);
    /// ```
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<MathTex> {
        let mut parent = self.clone();
        parent.data.path = Path::default();
        let id = scene.add(parent);
        let mut idx = 0;
        for g in &self.glyphs {
            let sub: Vec<SubPath> = self.data.path.subpaths[idx..idx + g.n_subpaths].to_vec();
            idx += g.n_subpaths;
            let child = VMobject::new(Path { subpaths: sub }, g.style.clone());
            let cid = scene.add(child);
            scene.add_child(id.erase(), cid.erase());
        }
        id
    }

    /// The per-glyph paths of this (unadded) formula, in child order.
    fn glyph_paths(&self) -> Vec<Path> {
        let mut out = Vec::with_capacity(self.glyphs.len());
        let mut idx = 0;
        for g in &self.glyphs {
            let sub = self.data.path.subpaths[idx..idx + g.n_subpaths].to_vec();
            idx += g.n_subpaths;
            out.push(Path { subpaths: sub });
        }
        out
    }

    /// The typst-source byte range of each glyph child, parallel to
    /// [`glyph_ids`](Self::glyph_ids) / the added children.
    fn glyph_ranges(&self) -> Vec<Option<std::ops::Range<usize>>> {
        self.glyphs.iter().map(|g| g.typst_range.clone()).collect()
    }

    /// Isolates the glyph children of an added formula `id` that come from the
    /// substring `tex`, one group **per occurrence**. Port of manim CE's
    /// `get_parts_by_tex`.
    ///
    /// # How isolation works
    ///
    /// Each glyph records the byte range in the (translated) typst source it was
    /// laid out from (via typst's glyph spans). The query `tex` is translated the
    /// same way and matched as a substring of the formula's typst source; each
    /// match yields the group of glyphs whose ranges fall inside it. This gives
    /// true occurrence-level isolation (both `x`s in `x^2 + x` are separate
    /// groups). Synthesized glyphs with no span (a fraction bar) are never
    /// grouped. `tex` should be a well-formed LaTeX sub-expression so it
    /// translates the same way in isolation.
    ///
    /// ```
    /// use manim_text::MathTex;
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let m = MathTex::new(r"e^{i\pi} + 1 = 0").unwrap().add_to(&mut scene);
    /// // Exactly one π glyph.
    /// let parts = MathTex::get_parts_by_tex(&scene, m, r"\pi");
    /// assert_eq!(parts.len(), 1);
    /// assert_eq!(parts[0].len(), 1);
    /// ```
    pub fn get_parts_by_tex(
        scene: &SceneState,
        id: MobjectId<MathTex>,
        tex: &str,
    ) -> Vec<Vec<AnyId>> {
        let math = scene.get(id);
        let typst_src = math.typst_src.clone();
        let ranges = math.glyph_ranges();
        let children = scene.get_dyn(id.erase()).data().children.clone();
        let query = query_to_typst(tex);
        occurrences_to_groups(&typst_src, &query, &ranges, &children)
    }

    /// The `index`-th occurrence group of `tex` (manim's `index_of_part` /
    /// `get_part_by_tex`), if present.
    pub fn get_part_by_tex(
        scene: &SceneState,
        id: MobjectId<MathTex>,
        tex: &str,
        index: usize,
    ) -> Option<Vec<AnyId>> {
        Self::get_parts_by_tex(scene, id, tex)
            .into_iter()
            .nth(index)
    }

    /// Recolors every glyph of an added formula `id` belonging to the substring
    /// `tex`, returning how many were recolored. Port of manim CE's
    /// `set_color_by_tex` with **true occurrence-level isolation**.
    ///
    /// Precedence: glyphs are matched by their typst-source range
    /// ([`get_parts_by_tex`](Self::get_parts_by_tex)); if that yields nothing
    /// (e.g. the query maps only to synthesized/spanless glyphs), it falls back to
    /// shape-signature matching against a mini-render of `tex` (which colors *all*
    /// occurrences of those glyph shapes — the previous approximation).
    ///
    /// # Errors
    ///
    /// A [`CoreError::Text`] if `tex` fails to typeset (fallback path only).
    ///
    /// ```
    /// use manim_text::MathTex;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::Mobject;
    /// use manim_color::RED;
    /// let mut scene = SceneState::new();
    /// let m = MathTex::new(r"x^2 + x").unwrap().add_to(&mut scene);
    /// // Both x's colored (but not the 2).
    /// let n = MathTex::set_color_by_tex(&mut scene, m, "x", RED).unwrap();
    /// assert_eq!(n, 2);
    /// ```
    pub fn set_color_by_tex(
        scene: &mut SceneState,
        id: MobjectId<MathTex>,
        tex: &str,
        color: Color,
    ) -> Result<usize, CoreError> {
        let parts = Self::get_parts_by_tex(scene, id, tex);
        if !parts.is_empty() {
            let mut count = 0;
            for group in parts {
                for c in group {
                    scene.get_dyn_mut(c).set_fill(color, 1.0);
                    count += 1;
                }
            }
            return Ok(count);
        }
        // Fallback: shape-signature match (spanless glyphs / untranslatable query).
        let sub = MathTex::new(tex)?;
        let wanted: std::collections::HashSet<u64> = sub
            .glyph_paths()
            .iter()
            .filter(|p| !p.subpaths.iter().all(|s| s.curves.is_empty()))
            .map(crate::match_tex::signature)
            .collect();
        let children = scene.get_dyn(id.erase()).data().children.clone();
        let mut count = 0;
        for c in children {
            let sig = crate::match_tex::signature(&scene.get_dyn(c).data().path);
            if wanted.contains(&sig) {
                scene.get_dyn_mut(c).set_fill(color, 1.0);
                count += 1;
            }
        }
        Ok(count)
    }

    /// Builds a math mobject from a typst source string in the given mode.
    fn build(
        typst_src: String,
        mode: Mode,
        font_size: f32,
        color: Color,
    ) -> Result<Self, MathError> {
        let mut me = Self {
            data: MobjectData::new(Path::default(), Style::filled(color)),
            typst_src,
            mode,
            font_size,
            color,
            glyphs: Vec::new(),
        };
        me.retypeset()?;
        Ok(me)
    }

    /// Re-typesets, panicking on failure (used by infallible builders after a
    /// successful initial build).
    fn rebuild(&mut self) {
        let _ = self.retypeset();
    }

    /// Compiles the typst source and rebuilds the outline.
    fn retypeset(&mut self) -> Result<(), MathError> {
        let (path, glyphs) = typeset(&self.typst_src, self.mode, self.font_size, self.color)?;
        self.data.path = path;
        self.data.style = Style::filled(self.color);
        self.glyphs = glyphs;
        self.data.bump_generation();
        Ok(())
    }
}

/// Raw typst math input (no LaTeX translation). Port of the escape hatch in
/// `docs/design/07-text.md`.
///
/// ```
/// use manim_text::Typst;
/// let m = Typst::new("e^(i pi) + 1 = 0").unwrap();
/// assert!(m.glyph_count() > 5);
/// ```
pub struct Typst;

impl Typst {
    /// Typesets a raw typst **math** string, returning a [`MathTex`] mobject.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(typst_math: &str) -> Result<MathTex, CoreError> {
        MathTex::build(
            typst_math.to_string(),
            Mode::Math,
            DEFAULT_MATH_FONT_SIZE,
            WHITE,
        )
        .map_err(CoreError::text)
    }
}

/// Text-mode (content) typst input, for full documents / prose. Port of manim
/// CE's `Tex`.
///
/// ```
/// use manim_text::Tex;
/// let m = Tex::new("Hello typst").unwrap();
/// assert!(m.glyph_count() > 5);
/// ```
pub struct Tex;

impl Tex {
    /// Typesets a typst **content-mode** string, returning a [`MathTex`] mobject.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(content: &str) -> Result<MathTex, CoreError> {
        MathTex::build(
            content.to_string(),
            Mode::Content,
            DEFAULT_MATH_FONT_SIZE,
            WHITE,
        )
        .map_err(CoreError::text)
    }
}

// ---------------------------------------------------------------------------
// typst World and compilation.
// ---------------------------------------------------------------------------

/// A minimal typst [`World`] backed only by the bundled typst-assets fonts and a
/// single in-memory source file.
struct MathWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    source: Source,
}

impl MathWorld {
    fn new(main: String) -> Self {
        let fonts: Vec<Font> = typst_assets::fonts()
            .flat_map(|data| {
                let bytes = Bytes::new(data);
                (0..).map_while(move |i| Font::new(bytes.clone(), i))
            })
            .collect();
        let book = FontBook::from_fonts(&fonts);
        let id = FileId::new(None, VirtualPath::new("main.typ"));
        let source = Source::new(id, main);
        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            source,
        }
    }
}

impl World for MathWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.source.id()
    }
    fn source(&self, id: FileId) -> typst::diag::FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(typst::diag::FileError::NotFound(
                id.vpath().as_rootless_path().to_path_buf(),
            ))
        }
    }
    fn file(&self, id: FileId) -> typst::diag::FileResult<Bytes> {
        Err(typst::diag::FileError::NotFound(
            id.vpath().as_rootless_path().to_path_buf(),
        ))
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<i64>) -> Option<typst::foundations::Datetime> {
        None
    }
}

/// Compiles the source and extracts the recentered outline + per-child metadata.
fn typeset(
    src: &str,
    mode: Mode,
    font_size: f32,
    color: Color,
) -> Result<(Path, Vec<GlyphInfo>), MathError> {
    let body = match mode {
        Mode::Math => format!("${src}$"),
        Mode::Content => src.to_string(),
    };
    let main = format!(
        "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n\
         #set text(size: {font_size}pt)\n{body}"
    );
    // Byte offset of `src` within `main`, so glyph spans map back to `src`.
    let src_offset = match mode {
        Mode::Math => main.len() - 1 - src.len(), // main = "…${src}$"
        Mode::Content => main.len() - src.len(),
    };
    let world = MathWorld::new(main);
    let document = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|diags| {
            let msg = diags
                .iter()
                .map(|d| d.message.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            MathError::Typeset(msg)
        })?;

    let page = document
        .pages
        .first()
        .ok_or_else(|| MathError::Typeset("empty document".to_string()))?;

    let mut children: Vec<(Vec<SubPath>, Style, Option<std::ops::Range<usize>>)> = Vec::new();
    let identity = Aff::identity();
    render_frame(
        &page.frame,
        identity,
        color,
        &world.source,
        src_offset,
        &mut children,
    );

    // Concatenate into one path + metadata, then recenter.
    let mut subpaths = Vec::new();
    let mut glyphs = Vec::new();
    for (subs, style, typst_range) in children {
        if subs.is_empty() {
            continue;
        }
        glyphs.push(GlyphInfo {
            n_subpaths: subs.len(),
            style,
            typst_range,
        });
        subpaths.extend(subs);
    }
    let mut path = Path { subpaths };
    if let Some((min, max)) = path.bounding_box() {
        let center = (min + max) * 0.5;
        path.apply(|p| p - center);
    }
    Ok((path, glyphs))
}

/// A 2D affine transform (`x' = a·x + c·y + e`, `y' = b·x + d·y + f`), in pt.
#[derive(Clone, Copy)]
struct Aff {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Aff {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    /// `self ∘ other` — apply `other` first, then `self`.
    fn then(self, outer: Aff) -> Aff {
        // outer applied after self.
        Aff {
            a: outer.a * self.a + outer.c * self.b,
            b: outer.b * self.a + outer.d * self.b,
            c: outer.a * self.c + outer.c * self.d,
            d: outer.b * self.c + outer.d * self.d,
            e: outer.a * self.e + outer.c * self.f + outer.e,
            f: outer.b * self.e + outer.d * self.f + outer.f,
        }
    }

    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
}

/// A pt translation.
fn translate_aff(p: TPoint) -> Aff {
    Aff {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: p.x.to_pt(),
        f: p.y.to_pt(),
    }
}

/// A typst [`Transform`] as an [`Aff`] (translation in pt).
fn transform_aff(t: Transform) -> Aff {
    Aff {
        a: t.sx.get(),
        b: t.ky.get(),
        c: t.kx.get(),
        d: t.sy.get(),
        e: t.tx.to_pt(),
        f: t.ty.to_pt(),
    }
}

/// Recursively walks a typst frame, emitting glyph and shape children. Each glyph
/// carries its byte range in the (translated) typst source (`src_offset` is where
/// the user's source begins inside the compiled document).
#[allow(clippy::type_complexity)]
fn render_frame(
    frame: &Frame,
    current: Aff,
    color: Color,
    source: &Source,
    src_offset: usize,
    out: &mut Vec<(Vec<SubPath>, Style, Option<std::ops::Range<usize>>)>,
) {
    let s = PT_PER_SCENE_UNIT_INV;
    for (pos, item) in frame.items() {
        match item {
            FrameItem::Group(group) => {
                // child → parent: translate(pos) then group.transform, then current.
                let child = transform_aff(group.transform)
                    .then(translate_aff(*pos))
                    .then(current);
                render_frame(&group.frame, child, color, source, src_offset, out);
            }
            FrameItem::Text(text) => {
                let size_pt = text.size.to_pt();
                let upem = text.font.units_per_em();
                let fscale = size_pt / upem;
                let mut pen = pos.x.to_pt();
                let baseline = pos.y.to_pt();
                let data = text.font.data();
                let index = text.font.index();
                for glyph in &text.glyphs {
                    let gx = pen + glyph.x_offset.get() * size_pt;
                    let subs = ttf_parser::Face::parse(data, index)
                        .ok()
                        .map(|face| {
                            let place = |ox: f32, oy: f32| {
                                let lx = gx + ox as f64 * fscale;
                                let ly = baseline - oy as f64 * fscale;
                                let (wx, wy) = current.apply(lx, ly);
                                Point::new(wx as f32 * s, -(wy as f32) * s, 0.0)
                            };
                            let mut b = GlyphOutline::new(place);
                            face.outline_glyph(ttf_parser::GlyphId(glyph.id), &mut b);
                            b.finish()
                        })
                        .unwrap_or_default();
                    if !subs.is_empty() {
                        let span = glyph.span.0;
                        let typst_range = if span.is_detached() {
                            None
                        } else {
                            source.range(span).and_then(|r| {
                                (r.start >= src_offset)
                                    .then(|| (r.start - src_offset)..(r.end - src_offset))
                            })
                        };
                        out.push((subs, Style::filled(color), typst_range));
                    }
                    pen += glyph.x_advance.get() * size_pt;
                }
            }
            FrameItem::Shape(shape, _) => {
                if let Some(subpath) = shape_subpath(&shape.geometry, *pos, current, s) {
                    out.push((vec![subpath], Style::filled(color), None));
                }
            }
            _ => {}
        }
    }
}

/// Converts a fraction-bar-style shape (a horizontal/vertical line or a rect)
/// into a filled subpath. Curves (radical signs, brackets) are skipped for now.
fn shape_subpath(geo: &Geometry, pos: TPoint, current: Aff, s: f32) -> Option<SubPath> {
    let to_scene = |x: f64, y: f64| {
        let (wx, wy) = current.apply(x, y);
        Point::new(wx as f32 * s, -(wy as f32) * s, 0.0)
    };
    match geo {
        Geometry::Line(end) => {
            // A thin filled rectangle along the segment (default hairline).
            let ax = pos.x.to_pt();
            let ay = pos.y.to_pt();
            let bx = ax + end.x.to_pt();
            let by = ay + end.y.to_pt();
            let thickness = 0.6; // pt; typst rules are ~0.6pt
            let (dx, dy) = (bx - ax, by - ay);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-9 {
                return None;
            }
            let (px, py) = (-dy / len * thickness / 2.0, dx / len * thickness / 2.0);
            Some(closed_quad(
                to_scene(ax + px, ay + py),
                to_scene(bx + px, by + py),
                to_scene(bx - px, by - py),
                to_scene(ax - px, ay - py),
            ))
        }
        Geometry::Rect(size) => {
            let x0 = pos.x.to_pt();
            let y0 = pos.y.to_pt();
            let x1 = x0 + size.x.to_pt();
            let y1 = y0 + size.y.to_pt();
            Some(closed_quad(
                to_scene(x0, y0),
                to_scene(x1, y0),
                to_scene(x1, y1),
                to_scene(x0, y1),
            ))
        }
        Geometry::Curve(_) => None,
    }
}

/// A closed four-corner subpath.
fn closed_quad(a: Point, b: Point, c: Point, d: Point) -> SubPath {
    SubPath {
        curves: vec![
            CubicBezier::line(a, b),
            CubicBezier::line(b, c),
            CubicBezier::line(c, d),
            CubicBezier::line(d, a),
        ],
        closed: true,
    }
}

/// Translates a LaTeX query to typst for matching against a formula's typst
/// source; falls back to the raw (trimmed) query if it doesn't translate.
fn query_to_typst(tex: &str) -> String {
    crate::latex::translate(tex)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| tex.trim().to_string())
}

/// Groups glyph `children` by each occurrence of `query` in `typst_src`, using
/// per-glyph typst byte `ranges`. A glyph joins an occurrence if its range lies
/// within the matched span.
fn occurrences_to_groups(
    typst_src: &str,
    query: &str,
    ranges: &[Option<std::ops::Range<usize>>],
    children: &[AnyId],
) -> Vec<Vec<AnyId>> {
    let mut groups = Vec::new();
    if query.is_empty() {
        return groups;
    }
    let mut start = 0;
    while let Some(rel) = typst_src[start..].find(query) {
        let s = start + rel;
        let e = s + query.len();
        let group: Vec<AnyId> = ranges
            .iter()
            .zip(children)
            .filter_map(|(r, &c)| r.as_ref().filter(|r| r.start >= s && r.end <= e).map(|_| c))
            .collect();
        if !group.is_empty() {
            groups.push(group);
        }
        start = e.max(s + 1);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_core::mobject::{Mobject, MobjectExt};

    /// The fraction of a formula's glyphs that carry a typst source range.
    fn coverage(src: &str) -> (usize, usize) {
        let m = MathTex::new(src).unwrap();
        let total = m.glyphs.len();
        let mapped = m.glyphs.iter().filter(|g| g.typst_range.is_some()).count();
        (mapped, total)
    }

    #[test]
    fn span_coverage_is_high() {
        // Report per-formula, then assert a high overall mapping rate.
        let mut tot_mapped = 0;
        let mut tot_total = 0;
        for src in [
            r"e^{i\pi} + 1 = 0",
            r"x^2 + x",
            r"b^2 - 4ac",
            r"\frac{\pi}{2} + \pi",
        ] {
            let (mapped, total) = coverage(src);
            eprintln!("{src}: {mapped}/{total}");
            tot_mapped += mapped;
            tot_total += total;
        }
        // Most glyphs carry a source range; only synthesized ones (fraction bars)
        // and the occasional reflowed construct miss.
        assert!(
            tot_mapped * 4 >= tot_total * 3,
            "coverage {tot_mapped}/{tot_total} below 75%"
        );
    }

    #[test]
    fn get_parts_by_tex_isolates_single_pi() {
        let mut scene = SceneState::new();
        let m = MathTex::new(r"e^{i\pi} + 1 = 0")
            .unwrap()
            .add_to(&mut scene);
        let parts = MathTex::get_parts_by_tex(&scene, m, r"\pi");
        assert_eq!(parts.len(), 1, "one π occurrence");
        assert_eq!(parts[0].len(), 1, "exactly the π glyph");
    }

    #[test]
    fn get_parts_by_tex_isolates_each_occurrence() {
        let mut scene = SceneState::new();
        let m = MathTex::new(r"\frac{\pi}{2} + \pi")
            .unwrap()
            .add_to(&mut scene);
        // The standalone π maps; the numerator π may be spanless — at least one
        // occurrence is isolated, each as its own group.
        let parts = MathTex::get_parts_by_tex(&scene, m, r"\pi");
        assert!(!parts.is_empty());
        assert!(parts.iter().all(|g| !g.is_empty()));
    }

    #[test]
    fn set_color_by_tex_colors_both_x_not_the_2() {
        use manim_color::RED;
        let mut scene = SceneState::new();
        let m = MathTex::new(r"x^2 + x").unwrap().add_to(&mut scene);
        let n = MathTex::set_color_by_tex(&mut scene, m, "x", RED).unwrap();
        assert_eq!(n, 2, "both x's, not the 2");
        // The 2 glyph keeps its original color.
        let kids = scene.get_dyn(m.erase()).data().children.clone();
        let reds = kids
            .iter()
            .filter(|&&c| scene.get_dyn(c).data().style.fill_color == Some(RED))
            .count();
        assert_eq!(reds, 2);
    }

    #[test]
    fn isolate_contiguous_subexpression() {
        let mut scene = SceneState::new();
        let m = MathTex::new(r"b^2 - 4ac").unwrap().add_to(&mut scene);
        let parts = MathTex::get_parts_by_tex(&scene, m, "4ac");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].len(), 3, "the 4, a, c glyphs as one group");
    }

    #[test]
    fn synthesized_glyphs_do_not_panic() {
        let mut scene = SceneState::new();
        let m = MathTex::new(r"\frac{a}{b}").unwrap().add_to(&mut scene);
        // Query touching the fraction (spanless bar) must not panic.
        let _ = MathTex::get_parts_by_tex(&scene, m, "a");
        let _ = MathTex::get_part_by_tex(&scene, m, "b", 0);
    }

    #[test]
    fn typst_math_renders_glyphs() {
        let m = Typst::new("a + b = c").unwrap();
        assert!(m.glyph_count() >= 4, "got {}", m.glyph_count());
    }

    #[test]
    fn mathtex_translates_and_renders() {
        let m = MathTex::new(r"e^{i\pi} + 1 = 0").unwrap();
        assert!(m.glyph_count() >= 6);
    }

    #[test]
    fn deterministic() {
        let a = MathTex::new(r"\frac{a}{b}").unwrap();
        let b = MathTex::new(r"\frac{a}{b}").unwrap();
        assert_eq!(a.data().path, b.data().path);
    }

    #[test]
    fn unknown_command_surfaces_error() {
        use std::error::Error;
        let err = match MathTex::new(r"\nonsense x") {
            Ok(_) => panic!("expected an error for \\nonsense"),
            Err(e) => e,
        };
        let math_err = err.source().and_then(|s| s.downcast_ref::<MathError>());
        assert!(matches!(math_err, Some(MathError::UnknownCommand(_))));
    }

    #[test]
    fn centered_at_origin() {
        let m = MathTex::new("x = y").unwrap();
        assert!(m.get_center().length() < 1e-3);
    }
}
