//! [`Text`]: shaped, vectorized text as one glyph submobject per non-space glyph.

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style as CtStyle, Weight};
use manim_color::{Color, WHITE};
use manim_core::geometry::VMobject;
use manim_core::impl_mobject;
use manim_core::mobject::{AnyId, MobjectData, MobjectId};
use manim_core::scene_state::SceneState;
use manim_core::style::Style;
use manim_math::path::{Path, SubPath};
use manim_math::Point;

use crate::font;
use crate::outline::GlyphOutline;

/// Scene units per layout pixel.
///
/// With this factor a capital letter of DejaVu Sans at the default
/// [`DEFAULT_FONT_SIZE`] (48) is about `0.70` scene units tall and the full em
/// box about `0.96` — close to manim CE's `font_size = 48` text scale. The
/// mapping is: `scene = pixels * SCENE_UNITS_PER_PIXEL`, where cosmic-text lays
/// text out with `font_size` as the em size in pixels.
pub const SCENE_UNITS_PER_PIXEL: f32 = 0.02;

/// manim CE's default `Text` font size.
pub const DEFAULT_FONT_SIZE: f32 = 48.0;

/// Font weight (a small, portable subset of the embedded faces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weighting {
    /// Normal weight.
    Normal,
    /// Bold weight.
    Bold,
}

impl Weighting {
    fn cosmic(self) -> Weight {
        match self {
            Weighting::Normal => Weight::NORMAL,
            Weighting::Bold => Weight::BOLD,
        }
    }
}

/// Font slant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slant {
    /// Upright.
    Normal,
    /// Italic / oblique.
    Italic,
}

impl Slant {
    fn cosmic(self) -> CtStyle {
        match self {
            Slant::Normal => CtStyle::Normal,
            Slant::Italic => CtStyle::Italic,
        }
    }
}

/// Horizontal alignment for multi-line text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alignment {
    /// Left-align lines.
    #[default]
    Left,
    /// Center lines.
    Center,
    /// Right-align lines.
    Right,
}

/// Per-glyph metadata, parallel to consecutive groups of subpaths in the Text's
/// own path.
#[derive(Debug, Clone)]
struct GlyphInfo {
    /// Number of contours (subpaths) this glyph contributes.
    n_subpaths: usize,
    /// The glyph's color.
    color: Color,
    /// The source character.
    ch: char,
}

/// Vectorized text: one child glyph mobject per non-space glyph, grouped under a
/// parent `Text`, mirroring manim CE's `Text` submobject structure (`text[i]` is
/// the `i`-th glyph). Shaped with cosmic-text over the bundled DejaVu Sans.
///
/// The parent carries the whole outline in its own path (so its bounding box and
/// pre-add builder transforms work); [`add_to`](Text::add_to) splits that
/// outline into one child per glyph in the scene, which is what per-glyph
/// animation (`Create`, `Write`) and per-substring color rely on.
///
/// ```
/// use manim_text::Text;
/// let t = Text::new("Hi!");
/// // Three non-space glyphs: H, i, !.
/// assert_eq!(t.glyph_count(), 3);
/// ```
#[derive(Clone)]
pub struct Text {
    data: MobjectData,
    text: String,
    font_size: f32,
    color: Color,
    weight: Weighting,
    slant: Slant,
    line_spacing: f32,
    alignment: Alignment,
    t2c: Vec<(String, Color)>,
    t2w: Vec<(String, Weighting)>,
    t2s: Vec<(String, Slant)>,
    use_system_fonts: bool,
    gradient: Option<Vec<Color>>,
    glyphs: Vec<GlyphInfo>,
}
impl_mobject!(Text);

impl Text {
    /// Shapes `text` at the default size and color (white).
    ///
    /// ```
    /// use manim_text::Text;
    /// use manim_core::mobject::MobjectExt;
    /// let t = Text::new("A");
    /// // A single capital at font_size 48 is ~0.7 scene units tall.
    /// assert!(t.bounding_box().height() > 0.5 && t.bounding_box().height() < 0.9);
    /// ```
    pub fn new(text: impl Into<String>) -> Self {
        let mut me = Self {
            data: MobjectData::new(Path::default(), Style::filled(WHITE)),
            text: text.into(),
            font_size: DEFAULT_FONT_SIZE,
            color: WHITE,
            weight: Weighting::Normal,
            slant: Slant::Normal,
            line_spacing: 1.0,
            alignment: Alignment::Left,
            t2c: Vec::new(),
            t2w: Vec::new(),
            t2s: Vec::new(),
            use_system_fonts: false,
            gradient: None,
            glyphs: Vec::new(),
        };
        me.rebuild();
        me
    }

    /// Colors the text with a gradient distributed across its glyphs (manim CE's
    /// `Text(..., gradient=...)` / whole-word `set_color_by_gradient`): glyph `i`
    /// gets the color interpolated at `i/(n-1)` along `colors`. Applied when the
    /// text is added to the scene.
    ///
    /// ```
    /// use manim_text::Text;
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::Mobject;
    /// use manim_color::{BLUE, RED};
    /// let mut scene = SceneState::new();
    /// let t = Text::new("AB").with_gradient(&[BLUE, RED]).add_to(&mut scene);
    /// let kids = scene.get_dyn(t.erase()).data().children.clone();
    /// assert_eq!(scene.get_dyn(kids[0]).data().style.fill_color, Some(BLUE));
    /// assert_eq!(scene.get_dyn(*kids.last().unwrap()).data().style.fill_color, Some(RED));
    /// ```
    pub fn with_gradient(mut self, colors: &[Color]) -> Self {
        if !colors.is_empty() {
            self.gradient = Some(colors.to_vec());
        }
        self
    }

    /// Sets the font size (manim's `font_size`).
    ///
    /// ```
    /// use manim_text::Text;
    /// use manim_core::mobject::MobjectExt;
    /// let small = Text::new("A").font_size(24.0);
    /// let big = Text::new("A").font_size(96.0);
    /// assert!(big.bounding_box().height() > small.bounding_box().height() * 3.0);
    /// ```
    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self.rebuild();
        self
    }

    /// Sets the text color (manim's `color`).
    ///
    /// ```
    /// use manim_text::Text;
    /// use manim_color::RED;
    /// let t = Text::new("A").color(RED);
    /// assert_eq!(t.glyph_color(0), Some(RED));
    /// ```
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self.rebuild();
        self
    }

    /// Sets the font weight.
    pub fn weight(mut self, weight: Weighting) -> Self {
        self.weight = weight;
        self.rebuild();
        self
    }

    /// Sets the font slant.
    pub fn slant(mut self, slant: Slant) -> Self {
        self.slant = slant;
        self.rebuild();
        self
    }

    /// Sets the line spacing multiplier for multi-line text.
    pub fn line_spacing(mut self, line_spacing: f32) -> Self {
        self.line_spacing = line_spacing;
        self.rebuild();
        self
    }

    /// Sets horizontal alignment for multi-line text.
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self.rebuild();
        self
    }

    /// Colors every glyph of each given substring (manim's `t2c`).
    ///
    /// ```
    /// use manim_text::Text;
    /// use manim_color::{BLUE, RED};
    /// let t = Text::new("abcd").color(RED).t2c(&[("bc", BLUE)]);
    /// // Glyphs 1 and 2 ("b", "c") are blue; the rest red.
    /// assert_eq!(t.glyph_color(0), Some(RED));
    /// assert_eq!(t.glyph_color(1), Some(BLUE));
    /// assert_eq!(t.glyph_color(2), Some(BLUE));
    /// assert_eq!(t.glyph_color(3), Some(RED));
    /// ```
    pub fn t2c(mut self, pairs: &[(&str, Color)]) -> Self {
        self.t2c = pairs.iter().map(|(s, c)| (s.to_string(), *c)).collect();
        self.rebuild();
        self
    }

    /// Bolds (or normal-weights) every glyph of each given substring (manim's
    /// `t2w`).
    pub fn t2w(mut self, pairs: &[(&str, Weighting)]) -> Self {
        self.t2w = pairs.iter().map(|(s, w)| (s.to_string(), *w)).collect();
        self.rebuild();
        self
    }

    /// Slants every glyph of each given substring (manim's `t2s`).
    pub fn t2s(mut self, pairs: &[(&str, Slant)]) -> Self {
        self.t2s = pairs.iter().map(|(s, w)| (s.to_string(), *w)).collect();
        self.rebuild();
        self
    }

    /// Enables platform system fonts in addition to the bundled font (native
    /// only; a no-op on wasm). Off by default for deterministic layout.
    pub fn with_system_fonts(mut self, enabled: bool) -> Self {
        self.use_system_fonts = enabled;
        self.rebuild();
        self
    }

    /// The number of non-space glyphs (manim's `len(text)`).
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// The source character of the `i`-th glyph.
    pub fn glyph_char(&self, i: usize) -> Option<char> {
        self.glyphs.get(i).map(|g| g.ch)
    }

    /// The color of the `i`-th glyph.
    pub fn glyph_color(&self, i: usize) -> Option<Color> {
        self.glyphs.get(i).map(|g| g.color)
    }

    /// The text string.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The child glyph handles, valid after [`add_to`](Self::add_to).
    ///
    /// ```
    /// use manim_text::Text;
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let t = Text::new("Hi").add_to(&mut scene);
    /// assert_eq!(scene.get(t).glyph_ids().len(), 2);
    /// ```
    pub fn glyph_ids(&self) -> &[AnyId] {
        &self.data.children
    }

    /// Adds this text to `scene` as a parent grouping one child mobject per
    /// glyph, returning the parent handle (mirrors `VGroup::of` /
    /// `CurvesAsSubmobjects::of`).
    ///
    /// ```
    /// use manim_text::Text;
    /// use manim_core::scene_state::SceneState;
    /// let mut scene = SceneState::new();
    /// let t = Text::new("Hi!").add_to(&mut scene);
    /// // Parent + three glyph children.
    /// assert_eq!(scene.family(t.erase()).len(), 1 + 3);
    /// ```
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<Text> {
        // The parent keeps the metadata but draws nothing itself.
        let mut parent = self.clone();
        parent.data.path = Path::default();
        let id = scene.add(parent);

        let n = self.glyphs.len().max(1);
        let mut idx = 0;
        for (i, g) in self.glyphs.iter().enumerate() {
            let sub: Vec<SubPath> = self.data.path.subpaths[idx..idx + g.n_subpaths].to_vec();
            idx += g.n_subpaths;
            let color = match &self.gradient {
                Some(stops) => sample_gradient(stops, i as f32 / (n - 1).max(1) as f32),
                None => g.color,
            };
            let child = VMobject::new(Path { subpaths: sub }, Style::filled(color));
            let cid = scene.add(child);
            scene.add_child(id.erase(), cid.erase());
        }
        id
    }

    /// Reshapes the outline from the current settings.
    fn rebuild(&mut self) {
        let (path, glyphs) = if self.use_system_fonts {
            let mut fs = font::with_system();
            shape(&mut fs, self)
        } else {
            let mut guard = font::shared();
            shape(&mut guard, self)
        };
        self.data.path = path;
        self.data.style = Style::filled(self.color);
        self.glyphs = glyphs;
        self.data.bump_generation();
    }
}

/// A shaped glyph collected in the first pass, before outline extraction.
struct Placed {
    font_id: cosmic_text::fontdb::ID,
    glyph_id: u16,
    font_size: f32,
    pen_x: f32,
    baseline: f32,
    ch: char,
    color: Color,
}

/// Shapes `spec` with `fs` and returns the recentered outline plus per-glyph
/// metadata.
fn shape(fs: &mut FontSystem, spec: &Text) -> (Path, Vec<GlyphInfo>) {
    if spec.text.is_empty() {
        return (Path::default(), Vec::new());
    }
    let line_height = (spec.font_size * spec.line_spacing).max(1.0);
    let metrics = Metrics::new(spec.font_size, line_height);
    let mut buffer = Buffer::new(fs, metrics);
    buffer.set_size(fs, None, None);

    let base = Attrs::new()
        .family(Family::Name(font::DEFAULT_FONT))
        .weight(spec.weight.cosmic())
        .style(spec.slant.cosmic());
    let spans = build_spans(spec);
    let span_attrs: Vec<(&str, Attrs)> = spans
        .iter()
        .map(|s| {
            (
                &spec.text[s.range.clone()],
                Attrs::new()
                    .family(Family::Name(font::DEFAULT_FONT))
                    .weight(s.weight.cosmic())
                    .style(s.slant.cosmic()),
            )
        })
        .collect();
    buffer.set_rich_text(fs, span_attrs, base, Shaping::Advanced);
    buffer.shape_until_scroll(fs, false);

    // First pass: collect placements and the widest line for alignment.
    let mut lines: Vec<Vec<Placed>> = Vec::new();
    let mut line_widths: Vec<f32> = Vec::new();
    let mut max_w = 0.0_f32;
    for run in buffer.layout_runs() {
        let mut placed = Vec::new();
        for g in run.glyphs.iter() {
            let ch = spec.text[g.start..].chars().next().unwrap_or(' ');
            let color = glyph_color(spec, g.start);
            placed.push(Placed {
                font_id: g.font_id,
                glyph_id: g.glyph_id,
                font_size: g.font_size,
                pen_x: g.x + g.x_offset,
                baseline: run.line_y - g.y_offset,
                ch,
                color,
            });
        }
        max_w = max_w.max(run.line_w);
        line_widths.push(run.line_w);
        lines.push(placed);
    }

    // Second pass: outline each glyph, applying per-line alignment offset.
    let mut all_subpaths: Vec<SubPath> = Vec::new();
    let mut glyphs: Vec<GlyphInfo> = Vec::new();
    for (line, width) in lines.iter().zip(&line_widths) {
        let align_shift = match spec.alignment {
            Alignment::Left => 0.0,
            Alignment::Center => (max_w - width) / 2.0,
            Alignment::Right => max_w - width,
        };
        for p in line {
            let subs = outline_glyph(fs, p, align_shift);
            if subs.is_empty() {
                continue; // spaces and empty glyphs carry no submobject
            }
            glyphs.push(GlyphInfo {
                n_subpaths: subs.len(),
                color: p.color,
                ch: p.ch,
            });
            all_subpaths.extend(subs);
        }
    }

    let mut path = Path {
        subpaths: all_subpaths,
    };
    recenter(&mut path);
    (path, glyphs)
}

/// The color of the glyph whose text starts at byte `start`.
fn glyph_color(spec: &Text, start: usize) -> Color {
    let mut color = spec.color;
    for (sub, c) in &spec.t2c {
        if sub.is_empty() {
            continue;
        }
        for (i, _) in spec.text.match_indices(sub.as_str()) {
            if start >= i && start < i + sub.len() {
                color = *c;
            }
        }
    }
    color
}

/// A run of characters sharing a weight and slant.
struct Span {
    range: std::ops::Range<usize>,
    weight: Weighting,
    slant: Slant,
}

/// Splits the text into spans of constant weight/slant from `t2w`/`t2s`.
fn build_spans(spec: &Text) -> Vec<Span> {
    let n = spec.text.len();
    let mut weights = vec![spec.weight; n];
    let mut slants = vec![spec.slant; n];
    for (sub, w) in &spec.t2w {
        if sub.is_empty() {
            continue;
        }
        for (i, _) in spec.text.match_indices(sub.as_str()) {
            weights[i..i + sub.len()].fill(*w);
        }
    }
    for (sub, s) in &spec.t2s {
        if sub.is_empty() {
            continue;
        }
        for (i, _) in spec.text.match_indices(sub.as_str()) {
            slants[i..i + sub.len()].fill(*s);
        }
    }

    // Coalesce consecutive characters with equal attributes into spans.
    let mut spans: Vec<Span> = Vec::new();
    for (b, ch) in spec.text.char_indices() {
        let w = weights[b];
        let s = slants[b];
        let end = b + ch.len_utf8();
        match spans.last_mut() {
            Some(last) if last.weight == w && last.slant == s => last.range.end = end,
            _ => spans.push(Span {
                range: b..end,
                weight: w,
                slant: s,
            }),
        }
    }
    if spans.is_empty() {
        spans.push(Span {
            range: 0..n,
            weight: spec.weight,
            slant: spec.slant,
        });
    }
    spans
}

/// Extracts a single glyph's outline as recentered-later scene subpaths.
fn outline_glyph(fs: &FontSystem, p: &Placed, align_shift: f32) -> Vec<SubPath> {
    let s = SCENE_UNITS_PER_PIXEL;
    fs.db()
        .with_face_data(p.font_id, |data, index| {
            let face = match ttf_parser::Face::parse(data, index) {
                Ok(f) => f,
                Err(_) => return Vec::new(),
            };
            let upem = face.units_per_em() as f32;
            let fscale = p.font_size / upem;
            let pen_x = p.pen_x + align_shift;
            let baseline = p.baseline;
            let place = move |ox: f32, oy: f32| {
                Point::new((pen_x + ox * fscale) * s, (oy * fscale - baseline) * s, 0.0)
            };
            let mut builder = GlyphOutline::new(place);
            face.outline_glyph(ttf_parser::GlyphId(p.glyph_id), &mut builder);
            builder.finish()
        })
        .unwrap_or_default()
}

/// Shifts a path so its bounding-box center sits at the origin.
fn recenter(path: &mut Path) {
    if let Some((min, max)) = path.bounding_box() {
        let center = (min + max) * 0.5;
        path.apply(|pt| pt - center);
    }
}

/// Samples a multi-stop color ramp at `t` in `[0, 1]`.
fn sample_gradient(stops: &[Color], t: f32) -> Color {
    if stops.len() == 1 {
        return stops[0];
    }
    let scaled = t.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
    let i = (scaled.floor() as usize).min(stops.len() - 2);
    stops[i].interpolate(&stops[i + 1], scaled - i as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_core::mobject::{Mobject, MobjectExt};
    use manim_core::scene_state::SceneState;

    #[test]
    fn glyph_count_excludes_spaces() {
        assert_eq!(Text::new("Hi!").glyph_count(), 3);
        assert_eq!(Text::new("a b").glyph_count(), 2);
    }

    #[test]
    fn deterministic_layout() {
        let a = Text::new("Hello");
        let b = Text::new("Hello");
        assert_eq!(a.data().path, b.data().path);
    }

    #[test]
    fn add_to_creates_glyph_children() {
        let mut scene = SceneState::new();
        let t = Text::new("Hi!").add_to(&mut scene);
        assert_eq!(scene.family(t.erase()).len(), 4); // parent + 3 glyphs
                                                      // The parent itself draws nothing.
        assert!(scene
            .get(t)
            .data()
            .path
            .subpaths
            .iter()
            .all(|s| s.curves.is_empty()));
    }

    #[test]
    fn font_size_scales_height() {
        let h48 = Text::new("A").bounding_box().height();
        let h96 = Text::new("A").font_size(96.0).bounding_box().height();
        assert!((h96 / h48 - 2.0).abs() < 0.05);
    }

    #[test]
    fn centered_at_origin() {
        let t = Text::new("Hello");
        assert!(t.get_center().length() < 1e-4);
    }
}
