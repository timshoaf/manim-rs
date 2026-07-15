//! Tabular (monospaced-digit) layout for numbers, so a changing value does not
//! jitter horizontally.
//!
//! Numbers are laid out directly over the bundled DejaVu Sans with `ttf-parser`
//! (no shaping needed for ASCII digits/punctuation): every digit `0`–`9` is
//! given the **same** advance (the widest digit's), so only a change in the
//! *number* of digits shifts the width.

use manim_math::path::{Path, SubPath};
use manim_math::Point;

use crate::font;
use crate::outline::GlyphOutline;
use crate::text::SCENE_UNITS_PER_PIXEL;

/// The result of laying out a number string.
pub(crate) struct NumberLayout {
    /// The glyph outlines, baseline at `y = 0`, left edge near `x = 0`.
    pub path: Path,
    /// The number of non-blank glyphs.
    pub glyph_count: usize,
}

/// Lays out `s` with tabular digit advances at the given `font_size`.
pub(crate) fn layout(s: &str, font_size: f32) -> NumberLayout {
    let face = ttf_parser::Face::parse(font::DEJAVU_REGULAR, 0).expect("bundled DejaVu is valid");
    let upem = face.units_per_em() as f32;
    let unit_to_scene = (font_size / upem) * SCENE_UNITS_PER_PIXEL;

    // The widest digit advance defines the tabular cell width.
    let digit_adv = ('0'..='9')
        .filter_map(|c| face.glyph_index(c))
        .filter_map(|g| face.glyph_hor_advance(g))
        .fold(0.0_f32, |m, a| m.max(a as f32));

    let mut pen = 0.0_f32; // font units
    let mut subpaths: Vec<SubPath> = Vec::new();
    let mut glyph_count = 0;
    for ch in s.chars() {
        let Some(gid) = face.glyph_index(ch) else {
            pen += digit_adv;
            continue;
        };
        let pen_now = pen;
        let place = move |ox: f32, oy: f32| {
            // Font outlines are y-up and the scene is y-up: no flip.
            Point::new((pen_now + ox) * unit_to_scene, oy * unit_to_scene, 0.0)
        };
        let mut builder = GlyphOutline::new(place);
        face.outline_glyph(gid, &mut builder);
        let subs = builder.finish();
        if !subs.is_empty() {
            glyph_count += 1;
            subpaths.extend(subs);
        }
        pen += if ch.is_ascii_digit() {
            digit_adv
        } else {
            face.glyph_hor_advance(gid).unwrap_or(0) as f32
        };
    }

    NumberLayout {
        path: Path { subpaths },
        glyph_count,
    }
}
