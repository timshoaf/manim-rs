//! [`Code`]: a syntax-highlighted code block. Behind the `code` feature.
//!
//! Highlighting is done by [`syntect`] (bundled default syntaxes + themes,
//! pure-Rust `fancy-regex` engine). Each highlighted token becomes a colored
//! monospace span, rendered through [`MarkupText`](crate::MarkupText) (so it
//! reuses the same shaping / per-glyph children), on a rounded background rect —
//! matching manim CE's `Code` look.

use manim_color::Color;
use manim_core::geometry::{RoundedRectangle, VGroup};
use manim_core::mobject::{Buildable, MobjectExt, MobjectId};
use manim_core::scene_state::SceneState;
use manim_math::Point;

use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::markup::MarkupText;

/// Default dark theme (matches manim's dark canvas).
pub const DEFAULT_CODE_THEME: &str = "base16-ocean.dark";
/// Padding (scene units) between the code and its background edge.
pub const CODE_PADDING: f32 = 0.25;
/// Corner radius of the background rectangle.
pub const CODE_CORNER_RADIUS: f32 = 0.15;

/// A syntax-highlighted, monospace code block on a rounded background. Port of
/// manim CE's `Code`.
///
/// A *builder*: [`Code::add_to`] renders it into the scene as a [`VGroup`]
/// (background rect + colored glyph children + optional line numbers).
pub struct Code {
    source: String,
    language: Option<String>,
    theme: String,
    line_numbers: bool,
}

impl Code {
    /// A code block for `source`, highlighted as `language` (a syntect token such
    /// as `"rust"`, `"python"`, `"js"`; unknown/`None` falls back to plain text).
    pub fn new(source: impl Into<String>, language: Option<&str>) -> Self {
        Self {
            source: source.into(),
            language: language.map(str::to_string),
            theme: DEFAULT_CODE_THEME.to_string(),
            line_numbers: false,
        }
    }

    /// Uses a specific syntect theme (default [`DEFAULT_CODE_THEME`]).
    pub fn with_theme(mut self, theme: impl Into<String>) -> Self {
        self.theme = theme.into();
        self
    }

    /// Shows a gray line-number gutter to the left of the code.
    pub fn with_line_numbers(mut self) -> Self {
        self.line_numbers = true;
        self
    }

    /// Highlights the source into a Pango-markup string (monospace, one colored
    /// span per token) plus the theme's background color.
    fn highlight(&self) -> (String, Color) {
        let ss = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let theme = ts
            .themes
            .get(&self.theme)
            .or_else(|| ts.themes.get(DEFAULT_CODE_THEME))
            .or_else(|| ts.themes.values().next())
            .expect("syntect ships default themes");
        let syntax = self
            .language
            .as_deref()
            .and_then(|l| ss.find_syntax_by_token(l))
            .unwrap_or_else(|| ss.find_syntax_plain_text());

        let source = self.source.replace('\t', "    "); // expand tabs
        let mut hl = HighlightLines::new(syntax, theme);
        let mut markup = String::from("<tt>");
        for line in LinesWithEndings::from(&source) {
            let ranges = hl.highlight_line(line, &ss).unwrap_or_default();
            for (style, text) in ranges {
                let c = style.foreground;
                markup.push_str(&format!(
                    "<span foreground=\"#{:02X}{:02X}{:02X}\">{}</span>",
                    c.r,
                    c.g,
                    c.b,
                    escape(text)
                ));
            }
        }
        markup.push_str("</tt>");

        let bg = theme
            .settings
            .background
            .map(|c| {
                Color::from_rgba(
                    c.r as f32 / 255.0,
                    c.g as f32 / 255.0,
                    c.b as f32 / 255.0,
                    1.0,
                )
            })
            .unwrap_or_else(|| Color::from_rgba(0.12, 0.14, 0.17, 1.0));
        (markup, bg)
    }

    /// The number of source lines.
    pub fn line_count(&self) -> usize {
        self.source.lines().count().max(1)
    }

    /// Renders the code block into `scene`, returning the group (background +
    /// glyphs [+ line numbers]).
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        let (markup, bg_color) = self.highlight();
        let code = MarkupText::new(&markup)
            .unwrap_or_else(|_| MarkupText::new("<tt>?</tt>").expect("fallback markup"));
        let bbox = code.bounding_box();
        let (w, h) = (bbox.width(), bbox.height());
        let center = bbox.center();

        let group = scene.add(VGroup::new());

        // Background: a rounded rect behind everything.
        let mut bg = RoundedRectangle::with_params(
            w + 2.0 * CODE_PADDING,
            h + 2.0 * CODE_PADDING,
            CODE_CORNER_RADIUS,
        )
        .with_fill(bg_color, 1.0);
        bg.set_z_index(-1);
        bg.move_to(center);
        let bg_id = scene.add(bg).erase();
        scene.add_child(group.erase(), bg_id);

        let code_id = code.add_to(scene).erase();
        scene.add_child(group.erase(), code_id);

        if self.line_numbers {
            let n = self.line_count();
            let nums = (1..=n)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            let gutter = MarkupText::new(&format!(
                "<tt><span foreground=\"#8A9199\">{nums}</span></tt>"
            ))
            .expect("line-number markup");
            let gutter_id = gutter.add_to(scene).erase();
            // Place the gutter just left of the code block, top-aligned.
            let gb = scene.family_bounding_box(gutter_id);
            let target = Point::new(
                bbox.min.x - CODE_PADDING - gb.width() / 2.0,
                bbox.max.y - gb.height() / 2.0,
                0.0,
            );
            scene.move_to(gutter_id, target);
            scene.add_child(group.erase(), gutter_id);
        }

        group
    }
}

/// Escapes markup-significant characters in token text.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_string_comment_differ_in_color() {
        let src = "// a comment\nfn main() { let s = \"hi\"; }\n";
        let mut scene = SceneState::new();
        let g = Code::new(src, Some("rust")).add_to(&mut scene);
        // Collect distinct fill colors across the code's glyph children.
        let mut colors = std::collections::HashSet::new();
        for member in scene.family(g.erase()) {
            if let Some(fc) = scene.get_dyn(member).data().style.fill_color {
                colors.insert(fc.to_hex());
            }
        }
        // A keyword, a string, and a comment should not all share one color.
        assert!(
            colors.len() >= 3,
            "expected varied token colors, got {colors:?}"
        );
    }

    #[test]
    fn line_numbers_and_layout() {
        let src = "a\nb\nc\n";
        let mut scene = SceneState::new();
        let plain = Code::new(src, None);
        assert_eq!(plain.line_count(), 3);
        let g = plain.with_line_numbers().add_to(&mut scene);
        // Background + code + gutter under the group.
        assert!(scene.get_dyn(g.erase()).data().children.len() >= 3);
    }

    #[test]
    fn background_is_behind_the_code() {
        let mut scene = SceneState::new();
        let g = Code::new("x = 1\n", Some("python")).add_to(&mut scene);
        let bg = scene.get_dyn(g.erase()).data().children[0];
        assert!(scene.get_dyn(bg).data().z_index < 0);
    }
}
