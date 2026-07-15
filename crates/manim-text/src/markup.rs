//! [`MarkupText`]: a pragmatic subset of Pango markup rendered as styled glyphs.
//!
//! Supported tags: `<b>` (bold), `<i>` (italic), `<u>` (underline), `<s>`
//! (strikethrough), `<tt>` (monospace), `<sub>`/`<sup>` (subscript/superscript),
//! and `<span foreground="#hex" size="…">` (color and size). Weight/slant reuse
//! the same rich-text span shaping as [`Text`](crate::Text)'s `t2w`/`t2s`;
//! underline and strike become thin filled rules; `<tt>` selects the bundled
//! `MONO_FONT` (DejaVu Sans Mono). Unknown tags are a clear error.
//!
//! `<sub>`/`<sup>` shrink the run to 65% and offset the baseline ±0.35 em;
//! cosmic-text shapes each run at its own size (so horizontal advances stay
//! correct) and only the baseline offset is applied to the glyph outlines.
//! Nesting a script inside another (`<sub><sup>…`) is rejected. `size=` accepts
//! an absolute point size (`size="12"`) or a Pango named scale
//! (`x-small`…`x-large`), applied the same way. `<tt>` (monospace) is not
//! supported — it needs a bundled fixed-width face.

use std::fmt;

use cosmic_text::{Attrs, Buffer, Family, Metrics, Shaping, Style as CtStyle, Weight};
use manim_color::{Color, WHITE};
use manim_core::geometry::VMobject;
use manim_core::impl_mobject;
use manim_core::mobject::{AnyId, MobjectData, MobjectId};
use manim_core::scene_state::SceneState;
use manim_core::style::Style;
use manim_math::bezier::CubicBezier;
use manim_math::path::{Path, SubPath};
use manim_math::Point;

use crate::font;
use crate::outline::GlyphOutline;
use crate::text::{DEFAULT_FONT_SIZE, SCENE_UNITS_PER_PIXEL};

/// An error parsing markup.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MarkupError {
    /// A tag name with no supported meaning.
    UnknownTag(String),
    /// A `</tag>` with no matching opener.
    UnbalancedTag(String),
    /// A malformed `<span …>` attribute.
    BadSpan(String),
    /// A `<sub>`/`<sup>` nested inside another `<sub>`/`<sup>` (unsupported —
    /// offsets would compound ambiguously).
    NestedScript,
}

impl fmt::Display for MarkupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarkupError::UnknownTag(t) => write!(f, "unknown markup tag: <{t}>"),
            MarkupError::UnbalancedTag(t) => write!(f, "unbalanced markup tag: </{t}>"),
            MarkupError::BadSpan(s) => write!(f, "bad <span> attribute: {s}"),
            MarkupError::NestedScript => write!(f, "nested <sub>/<sup> is not supported"),
        }
    }
}

impl std::error::Error for MarkupError {}

/// The active style while parsing.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Attr {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<Color>,
    /// Font-size multiplier relative to the base size (`<sub>`/`<sup>`, named
    /// `size=`).
    scale: f32,
    /// Absolute size in points, overriding `scale` when set (`size="12"`).
    abs_pt: Option<f32>,
    /// Baseline offset in ems (`+` up for `<sup>`, `-` down for `<sub>`).
    baseline_em: f32,
    /// Whether a `<sub>`/`<sup>` is already open (nesting is rejected).
    script: bool,
    /// Whether this run is monospace (`<tt>`).
    mono: bool,
}

impl Default for Attr {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            color: None,
            scale: 1.0,
            abs_pt: None,
            baseline_em: 0.0,
            script: false,
            mono: false,
        }
    }
}

/// A run of text with a single style.
#[derive(Debug, Clone, PartialEq)]
struct Span {
    text: String,
    attr: Attr,
}

/// Parses a Pango-markup subset into styled spans.
fn parse(markup: &str) -> Result<Vec<Span>, MarkupError> {
    let mut spans: Vec<Span> = Vec::new();
    let mut stack: Vec<Attr> = vec![Attr::default()];
    let mut chars = markup.chars().peekable();
    let mut buf = String::new();

    let flush = |buf: &mut String, spans: &mut Vec<Span>, attr: Attr| {
        if !buf.is_empty() {
            spans.push(Span {
                text: std::mem::take(buf),
                attr,
            });
        }
    };

    while let Some(c) = chars.next() {
        if c == '<' {
            flush(&mut buf, &mut spans, *stack.last().unwrap());
            let mut tag = String::new();
            for t in chars.by_ref() {
                if t == '>' {
                    break;
                }
                tag.push(t);
            }
            let tag = tag.trim();
            if let Some(name) = tag.strip_prefix('/') {
                let name = name.trim();
                if stack.len() <= 1 {
                    return Err(MarkupError::UnbalancedTag(name.to_string()));
                }
                stack.pop();
            } else {
                let mut attr = *stack.last().unwrap();
                apply_open_tag(tag, &mut attr)?;
                stack.push(attr);
            }
        } else if c == '&' {
            // Minimal entity handling for markup-safe text.
            let mut ent = String::new();
            for t in chars.by_ref() {
                if t == ';' {
                    break;
                }
                ent.push(t);
            }
            buf.push(match ent.as_str() {
                "lt" => '<',
                "gt" => '>',
                "amp" => '&',
                "quot" => '"',
                _ => '?',
            });
        } else {
            buf.push(c);
        }
    }
    flush(&mut buf, &mut spans, *stack.last().unwrap());
    Ok(spans)
}

/// Applies an opening tag to the style.
fn apply_open_tag(tag: &str, attr: &mut Attr) -> Result<(), MarkupError> {
    let name = tag.split_whitespace().next().unwrap_or("");
    match name {
        "b" => attr.bold = true,
        "i" => attr.italic = true,
        "u" => attr.underline = true,
        "s" => attr.strike = true,
        "tt" => attr.mono = true,
        "sub" | "sup" => {
            if attr.script {
                return Err(MarkupError::NestedScript);
            }
            attr.script = true;
            attr.scale *= SCRIPT_SCALE;
            attr.baseline_em = if name == "sup" {
                SCRIPT_OFFSET_EM
            } else {
                -SCRIPT_OFFSET_EM
            };
        }
        "span" => {
            if let Some(hex) = attr_value(tag, "foreground").or_else(|| attr_value(tag, "color")) {
                attr.color = Some(
                    Color::from_hex(&hex)
                        .map_err(|_| MarkupError::BadSpan(format!("foreground={hex}")))?,
                );
            }
            if let Some(size) = attr_value(tag, "size") {
                apply_size(&size, attr)?;
            }
        }
        other => return Err(MarkupError::UnknownTag(other.to_string())),
    }
    Ok(())
}

/// `<sub>`/`<sup>` font-size multiplier.
const SCRIPT_SCALE: f32 = 0.65;
/// `<sub>`/`<sup>` baseline offset, in ems of the base font size.
const SCRIPT_OFFSET_EM: f32 = 0.35;

/// Applies a `size="…"` value: an absolute point size (a number) or one of
/// Pango's named scales.
fn apply_size(value: &str, attr: &mut Attr) -> Result<(), MarkupError> {
    if let Ok(pt) = value.trim().parse::<f32>() {
        if pt > 0.0 {
            attr.abs_pt = Some(pt);
            return Ok(());
        }
    }
    let factor = match value.trim() {
        "xx-small" => 0.5787,
        "x-small" => 0.6944,
        "small" => 0.8333,
        "medium" => 1.0,
        "large" => 1.2,
        "x-large" => 1.44,
        "xx-large" => 1.728,
        other => return Err(MarkupError::BadSpan(format!("size={other}"))),
    };
    attr.scale *= factor;
    Ok(())
}

/// Extracts `key="value"` from a tag body.
fn attr_value(tag: &str, key: &str) -> Option<String> {
    let idx = tag.find(key)?;
    let rest = &tag[idx + key.len()..];
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Per-child metadata, parallel to consecutive subpath groups in the own path.
#[derive(Debug, Clone)]
struct GlyphInfo {
    n_subpaths: usize,
    style: Style,
}

/// Styled, marked-up text as one child per glyph (plus underline / strike
/// rules), grouped under a parent. Port of manim CE's `MarkupText` (subset).
///
/// ```
/// use manim_text::MarkupText;
/// let m = MarkupText::new(r##"plain <b>bold</b> <span foreground="#FF0000">red</span>"##).unwrap();
/// assert!(m.glyph_count() > 5);
/// ```
#[derive(Clone)]
pub struct MarkupText {
    data: MobjectData,
    font_size: f32,
    glyphs: Vec<GlyphInfo>,
}
impl_mobject!(MarkupText);

impl MarkupText {
    /// Parses and shapes `markup`.
    ///
    /// # Errors
    ///
    /// Returns [`MarkupError`] for an unknown tag, unbalanced tag, or malformed
    /// `<span>` color.
    pub fn new(markup: &str) -> Result<Self, MarkupError> {
        let spans = parse(markup)?;
        let (path, glyphs) = shape(&spans, DEFAULT_FONT_SIZE);
        Ok(Self {
            data: MobjectData::new(path, Style::filled(WHITE)),
            font_size: DEFAULT_FONT_SIZE,
            glyphs,
        })
    }

    /// The number of glyph / decoration children.
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// The child handles, valid after [`add_to`](Self::add_to).
    pub fn glyph_ids(&self) -> &[AnyId] {
        &self.data.children
    }

    /// The font size.
    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Adds the markup to `scene` as a parent grouping one child per glyph /
    /// rule (same convention as [`Text::add_to`](crate::Text::add_to)).
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<MarkupText> {
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
}

/// One placed glyph's scene-space extent, for building decorations.
struct Placed {
    span: usize,
    x_left: f32,
    x_right: f32,
    baseline: f32,
    height: f32,
}

/// Shapes styled spans into one recentered path + per-child metadata.
fn shape(spans: &[Span], font_size: f32) -> (Path, Vec<GlyphInfo>) {
    if spans.iter().all(|s| s.text.is_empty()) {
        return (Path::default(), Vec::new());
    }
    let s = SCENE_UNITS_PER_PIXEL;
    let mut fs = font::shared();
    let mut buffer = Buffer::new(&mut fs, Metrics::new(font_size, font_size * 1.2));
    buffer.set_size(&mut fs, None, None);

    // Byte range of each span in the concatenated plain text.
    let mut ranges = Vec::new();
    let mut plain = String::new();
    for span in spans {
        let start = plain.len();
        plain.push_str(&span.text);
        ranges.push(start..plain.len());
    }

    let base = Attrs::new().family(Family::Name(font::DEFAULT_FONT));
    let rich: Vec<(&str, Attrs)> = spans
        .iter()
        .map(|span| {
            // Per-span size: `<sub>`/`<sup>`/named `size=` scale the base; an
            // absolute `size="pt"` overrides. cosmic-text shapes at this size so
            // advances stay correct; the baseline offset is applied to outlines.
            let eff_fs = span
                .attr
                .abs_pt
                .unwrap_or(font_size * span.attr.scale)
                .max(1.0);
            let family = if span.attr.mono {
                font::MONO_FONT
            } else {
                font::DEFAULT_FONT
            };
            (
                span.text.as_str(),
                Attrs::new()
                    .family(Family::Name(family))
                    .metrics(Metrics::new(eff_fs, eff_fs * 1.2))
                    .weight(if span.attr.bold {
                        Weight::BOLD
                    } else {
                        Weight::NORMAL
                    })
                    .style(if span.attr.italic {
                        CtStyle::Italic
                    } else {
                        CtStyle::Normal
                    }),
            )
        })
        .collect();
    buffer.set_rich_text(&mut fs, rich, base, Shaping::Advanced);
    buffer.shape_until_scroll(&mut fs, false);

    let mut subpaths: Vec<SubPath> = Vec::new();
    let mut glyphs: Vec<GlyphInfo> = Vec::new();
    let mut placed: Vec<Placed> = Vec::new();

    for run in buffer.layout_runs() {
        let baseline_px = run.line_y;
        for g in run.glyphs.iter() {
            let span_idx = ranges
                .iter()
                .position(|r| r.contains(&g.start))
                .unwrap_or(0);
            let color = spans[span_idx].attr.color.unwrap_or(WHITE);
            let size = g.font_size;
            // Superscript/subscript raise/lower the glyph by a fraction of the
            // base size (the shrink itself is already baked into `size`).
            let baseline_off = spans[span_idx].attr.baseline_em * font_size;
            let subs = fs
                .db()
                .with_face_data(g.font_id, |data, index| {
                    let face = ttf_parser::Face::parse(data, index).ok()?;
                    let upem = face.units_per_em() as f32;
                    let fscale = size / upem;
                    let pen = g.x + g.x_offset;
                    let place = move |ox: f32, oy: f32| {
                        Point::new(
                            (pen + ox * fscale) * s,
                            (oy * fscale - baseline_px + baseline_off) * s,
                            0.0,
                        )
                    };
                    let mut b = GlyphOutline::new(place);
                    face.outline_glyph(ttf_parser::GlyphId(g.glyph_id), &mut b);
                    Some(b.finish())
                })
                .flatten()
                .unwrap_or_default();
            if !subs.is_empty() {
                glyphs.push(GlyphInfo {
                    n_subpaths: subs.len(),
                    style: Style::filled(color),
                });
                subpaths.extend(subs);
            }
            placed.push(Placed {
                span: span_idx,
                x_left: g.x * s,
                x_right: (g.x + g.w) * s,
                baseline: -baseline_px * s,
                height: size * s,
            });
        }
    }

    // Underline / strike rules: one filled rectangle per decorated span.
    for (i, span) in spans.iter().enumerate() {
        if !span.attr.underline && !span.attr.strike {
            continue;
        }
        let color = span.attr.color.unwrap_or(WHITE);
        let items: Vec<&Placed> = placed.iter().filter(|p| p.span == i).collect();
        if items.is_empty() {
            continue;
        }
        let x0 = items.iter().map(|p| p.x_left).fold(f32::INFINITY, f32::min);
        let x1 = items
            .iter()
            .map(|p| p.x_right)
            .fold(f32::NEG_INFINITY, f32::max);
        let baseline = items[0].baseline;
        let h = items[0].height;
        let thickness = h * 0.05;
        if span.attr.underline {
            let y = baseline - h * 0.12;
            subpaths.push(rule_rect(x0, x1, y, thickness));
            glyphs.push(GlyphInfo {
                n_subpaths: 1,
                style: Style::filled(color),
            });
        }
        if span.attr.strike {
            let y = baseline + h * 0.25;
            subpaths.push(rule_rect(x0, x1, y, thickness));
            glyphs.push(GlyphInfo {
                n_subpaths: 1,
                style: Style::filled(color),
            });
        }
    }

    let mut path = Path { subpaths };
    if let Some((min, max)) = path.bounding_box() {
        let center = (min + max) * 0.5;
        path.apply(|p| p - center);
    }
    (path, glyphs)
}

/// A thin filled horizontal rule from `x0` to `x1` centered on `y`.
fn rule_rect(x0: f32, x1: f32, y: f32, thickness: f32) -> SubPath {
    let h = thickness / 2.0;
    let a = Point::new(x0, y - h, 0.0);
    let b = Point::new(x1, y - h, 0.0);
    let c = Point::new(x1, y + h, 0.0);
    let d = Point::new(x0, y + h, 0.0);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_tags() {
        let spans = parse("a<b>b<i>c</i></b>d").unwrap();
        assert_eq!(spans.len(), 4);
        assert!(!spans[0].attr.bold);
        assert!(spans[1].attr.bold && !spans[1].attr.italic);
        assert!(spans[2].attr.bold && spans[2].attr.italic);
        assert!(!spans[3].attr.bold);
    }

    #[test]
    fn span_color() {
        let spans = parse(r##"<span foreground="#FF0000">red</span>"##).unwrap();
        assert_eq!(
            spans[0].attr.color,
            Some(Color::from_hex("#FF0000").unwrap())
        );
    }

    #[test]
    fn underline_and_strike_flags() {
        assert!(parse("<u>x</u>").unwrap()[0].attr.underline);
        assert!(parse("<s>x</s>").unwrap()[0].attr.strike);
    }

    #[test]
    fn unknown_tag_errors() {
        assert_eq!(
            parse("<blink>x</blink>"),
            Err(MarkupError::UnknownTag("blink".to_string()))
        );
    }

    #[test]
    fn unbalanced_tag_errors() {
        assert!(matches!(parse("</b>"), Err(MarkupError::UnbalancedTag(_))));
    }

    #[test]
    fn sub_sup_parse_scale_and_offset() {
        let sub = parse("<sub>x</sub>").unwrap();
        assert!((sub[0].attr.scale - SCRIPT_SCALE).abs() < 1e-6);
        assert!((sub[0].attr.baseline_em + SCRIPT_OFFSET_EM).abs() < 1e-6); // lowered
        let sup = parse("<sup>x</sup>").unwrap();
        assert!((sup[0].attr.baseline_em - SCRIPT_OFFSET_EM).abs() < 1e-6); // raised
    }

    #[test]
    fn size_named_and_absolute() {
        let named = parse(r#"<span size="x-large">x</span>"#).unwrap();
        assert!((named[0].attr.scale - 1.44).abs() < 1e-4);
        let abs = parse(r#"<span size="12">x</span>"#).unwrap();
        assert_eq!(abs[0].attr.abs_pt, Some(12.0));
    }

    #[test]
    fn tt_is_monospace() {
        // Center-to-center gap of two identical glyphs = that glyph's advance.
        let gap = |markup: &str| -> f32 {
            let mut scene = SceneState::new();
            let m = MarkupText::new(markup).unwrap().add_to(&mut scene);
            let kids = scene.get_dyn(m.erase()).data().children.clone();
            scene.family_bounding_box(kids[1]).center().x
                - scene.family_bounding_box(kids[0]).center().x
        };
        // Monospace: 'I' and 'M' advance identically → equal gaps.
        let (mi, mm) = (gap("<tt>II</tt>"), gap("<tt>MM</tt>"));
        assert!((mi - mm).abs() < 0.05, "mono gaps {mi} vs {mm}");
        // Proportional (default): 'I' is narrower than 'M' → smaller gap.
        let (pi, pm) = (gap("II"), gap("MM"));
        assert!(pi < pm - 0.05, "prop gaps {pi} vs {pm}");
    }

    #[test]
    fn nested_script_errors() {
        assert_eq!(
            parse("<sub><sup>x</sup></sub>"),
            Err(MarkupError::NestedScript)
        );
    }

    #[test]
    fn sub_lowers_sup_raises_and_shrinks() {
        let mut scene = SceneState::new();
        // Same letter throughout so extents are comparable: base, sub, base, sup.
        let m = MarkupText::new("o<sub>o</sub>o<sup>o</sup>")
            .unwrap()
            .add_to(&mut scene);
        let kids = scene.get_dyn(m.erase()).data().children.clone();
        assert_eq!(kids.len(), 4);
        let cy = |i: usize| scene.family_bounding_box(kids[i]).center().y;
        let ch = |i: usize| scene.family_bounding_box(kids[i]).height();
        assert!(
            cy(1) < cy(0),
            "sub should sit lower: {} vs {}",
            cy(1),
            cy(0)
        );
        assert!(
            cy(3) > cy(2),
            "sup should sit higher: {} vs {}",
            cy(3),
            cy(2)
        );
        assert!(
            ch(1) < ch(0),
            "sub should be smaller: {} vs {}",
            ch(1),
            ch(0)
        );
    }

    #[test]
    fn size_scales_glyph() {
        let mut scene = SceneState::new();
        let m = MarkupText::new(r#"o<span size="x-large">o</span>"#)
            .unwrap()
            .add_to(&mut scene);
        let kids = scene.get_dyn(m.erase()).data().children.clone();
        assert_eq!(kids.len(), 2);
        let big = scene.family_bounding_box(kids[1]).height();
        let base = scene.family_bounding_box(kids[0]).height();
        assert!(big > base, "x-large should be taller: {big} vs {base}");
    }
}
