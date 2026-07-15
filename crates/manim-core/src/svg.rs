//! [`SVGMobject`]: import an SVG as a group of vectorized mobjects.
//!
//! Parsing and normalization are delegated to [`usvg`](https://docs.rs/usvg)
//! (pure Rust — it compiles to wasm). Each usvg `Path` becomes one child
//! [`VMobject`]; its absolute transform is baked into the points, and its
//! fill/stroke map to a [`Style`]. Gradient and pattern paints fall back to a
//! solid color (documented). By manim CE convention the whole import is scaled
//! to a default height of `2.0` scene units (preserving aspect) and centered,
//! with the SVG's y-down axis flipped to manim's y-up.
//!
//! Because per-path fills differ, an SVG is materialized as a [`VGroup`] of
//! children via [`add_to`](SVGMobject::add_to) — a single mobject carries only
//! one style.

use manim_color::Color;
use manim_math::bezier::CubicBezier;
use manim_math::path::{Path, SubPath};
use manim_math::Point;
use usvg::tiny_skia_path::PathSegment;

use crate::geometry::{VGroup, VMobject};
use crate::mobject::MobjectId;
use crate::scene_state::SceneState;
use crate::style::{Style, DEFAULT_STROKE_WIDTH};

/// manim CE's default SVG height in scene units.
pub const DEFAULT_SVG_HEIGHT: f32 = 2.0;

/// The error type for SVG parsing.
#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    /// The SVG source could not be parsed.
    #[error("failed to parse SVG: {0}")]
    Parse(String),
    /// The file could not be read (native `from_file`).
    #[error("failed to read SVG file: {0}")]
    Io(#[from] std::io::Error),
}

/// One resolved SVG element: a vector path plus its paint.
#[derive(Clone)]
struct SvgElement {
    path: Path,
    style: Style,
}

/// A parsed SVG document as a set of vector elements, normalized to manim's
/// coordinate conventions.
///
/// ```
/// use manim_core::svg::SVGMobject;
/// let svg = r##"<svg viewBox="0 0 10 10" xmlns="http://www.w3.org/2000/svg">
///   <rect x="0" y="0" width="10" height="10" fill="#ff0000"/>
/// </svg>"##;
/// let m = SVGMobject::from_str(svg).unwrap();
/// assert_eq!(m.element_count(), 1);
/// ```
#[derive(Clone)]
pub struct SVGMobject {
    elements: Vec<SvgElement>,
    height: f32,
}

impl SVGMobject {
    /// Parses SVG `source`, normalizing to the default height.
    ///
    /// Named `from_str` for parity with `usvg::Tree::from_str` and manim CE;
    /// it is not a [`std::str::FromStr`] impl (it carries a custom error).
    ///
    /// # Errors
    ///
    /// [`SvgError::Parse`] if usvg cannot parse the document.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(source: &str) -> Result<Self, SvgError> {
        let tree = usvg::Tree::from_str(source, &usvg::Options::default())
            .map_err(|e| SvgError::Parse(e.to_string()))?;
        let mut elements = Vec::new();
        collect(tree.root(), &mut elements);
        let mut m = Self {
            elements,
            height: DEFAULT_SVG_HEIGHT,
        };
        m.normalize();
        Ok(m)
    }

    /// Reads and parses an SVG file (native only).
    ///
    /// # Errors
    ///
    /// [`SvgError::Io`] on read failure, [`SvgError::Parse`] on parse failure.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, SvgError> {
        let source = std::fs::read_to_string(path)?;
        Self::from_str(&source)
    }

    /// The number of vector elements (one per usvg path).
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// The resolved style of element `i` (for inspection/tests).
    pub fn element_style(&self, i: usize) -> &Style {
        &self.elements[i].style
    }

    /// Adds the SVG to `scene` as a [`VGroup`] of vector children; returns the
    /// group handle.
    ///
    /// ```
    /// use manim_core::svg::SVGMobject;
    /// use manim_core::scene_state::SceneState;
    /// let svg = r##"<svg viewBox="0 0 4 4" xmlns="http://www.w3.org/2000/svg">
    ///   <rect width="2" height="2" fill="#00f"/><rect x="2" y="2" width="2" height="2" fill="#0f0"/>
    /// </svg>"##;
    /// let m = SVGMobject::from_str(svg).unwrap();
    /// let mut scene = SceneState::new();
    /// let g = m.add_to(&mut scene);
    /// assert_eq!(scene.family(g.erase()).len(), 1 + 2); // group + 2 rects
    /// ```
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        let group = scene.add(VGroup::new());
        for el in &self.elements {
            let child = scene.add(VMobject::new(el.path.clone(), el.style.clone()));
            scene.add_child(group.erase(), child.erase());
        }
        group
    }

    /// Scales the whole import to [`height`](Self::height) preserving aspect,
    /// centers it on the origin, and flips SVG's y-down to manim's y-up.
    fn normalize(&mut self) {
        let Some((min, max)) = self.bounding_box() else {
            return;
        };
        let w = (max.x - min.x).max(1e-6);
        let h = (max.y - min.y).max(1e-6);
        let scale = self.height / h;
        let cx = 0.5 * (min.x + max.x);
        let cy = 0.5 * (min.y + max.y);
        let _ = w;
        for el in &mut self.elements {
            for sp in &mut el.path.subpaths {
                for c in &mut sp.curves {
                    for p in [&mut c.p0, &mut c.p1, &mut c.p2, &mut c.p3] {
                        p.x = (p.x - cx) * scale;
                        // Flip y (SVG is y-down; manim is y-up).
                        p.y = -(p.y - cy) * scale;
                    }
                }
            }
        }
    }

    /// The bounding box of all element control points, or `None` if empty.
    fn bounding_box(&self) -> Option<(Point, Point)> {
        let mut it = self
            .elements
            .iter()
            .flat_map(|e| e.path.subpaths.iter())
            .flat_map(|s| s.curves.iter())
            .flat_map(|c| [c.p0, c.p1, c.p2, c.p3]);
        let first = it.next()?;
        let mut min = first;
        let mut max = first;
        for p in it {
            min = min.min(p);
            max = max.max(p);
        }
        Some((min, max))
    }
}

/// Recursively collects vector elements from a usvg group.
fn collect(group: &usvg::Group, out: &mut Vec<SvgElement>) {
    for node in group.children() {
        match node {
            usvg::Node::Path(p) => {
                if let Some(el) = convert_path(p) {
                    out.push(el);
                }
            }
            usvg::Node::Group(g) => collect(g, out),
            // Nested raster images and text are not imported as vector paths.
            usvg::Node::Image(_) | usvg::Node::Text(_) => {}
        }
    }
}

/// Converts one usvg path (with its absolute transform baked in) to an element.
fn convert_path(p: &usvg::Path) -> Option<SvgElement> {
    let t = p.abs_transform();
    // Apply the absolute affine transform to a tiny_skia point.
    let tp = |x: f32, y: f32| -> Point {
        Point::new(t.sx * x + t.kx * y + t.tx, t.ky * x + t.sy * y + t.ty, 0.0)
    };

    let mut subpaths: Vec<SubPath> = Vec::new();
    let mut cur: Vec<CubicBezier> = Vec::new();
    let mut start = Point::ZERO;
    let mut pen = Point::ZERO;

    for seg in p.data().segments() {
        match seg {
            PathSegment::MoveTo(pt) => {
                if !cur.is_empty() {
                    subpaths.push(SubPath {
                        curves: std::mem::take(&mut cur),
                        closed: false,
                    });
                }
                start = tp(pt.x, pt.y);
                pen = start;
            }
            PathSegment::LineTo(pt) => {
                let to = tp(pt.x, pt.y);
                cur.push(CubicBezier::line(pen, to));
                pen = to;
            }
            PathSegment::QuadTo(c, pt) => {
                let ctrl = tp(c.x, c.y);
                let to = tp(pt.x, pt.y);
                // Elevate the quadratic to a cubic.
                let c1 = pen + (ctrl - pen) * (2.0 / 3.0);
                let c2 = to + (ctrl - to) * (2.0 / 3.0);
                cur.push(CubicBezier::new(pen, c1, c2, to));
                pen = to;
            }
            PathSegment::CubicTo(c1, c2, pt) => {
                let to = tp(pt.x, pt.y);
                cur.push(CubicBezier::new(pen, tp(c1.x, c1.y), tp(c2.x, c2.y), to));
                pen = to;
            }
            PathSegment::Close => {
                if pen != start {
                    cur.push(CubicBezier::line(pen, start));
                }
                subpaths.push(SubPath {
                    curves: std::mem::take(&mut cur),
                    closed: true,
                });
                pen = start;
            }
        }
    }
    if !cur.is_empty() {
        subpaths.push(SubPath {
            curves: cur,
            closed: false,
        });
    }
    if subpaths.is_empty() {
        return None;
    }

    Some(SvgElement {
        path: Path { subpaths },
        style: convert_style(p),
    })
}

/// Maps a usvg path's fill/stroke to a manim [`Style`]. Gradient/pattern paints
/// fall back to a mid-grey solid (documented).
fn convert_style(p: &usvg::Path) -> Style {
    let mut style = Style {
        fill_color: None,
        fill_opacity: 0.0,
        stroke_color: None,
        stroke_opacity: 0.0,
        stroke_width: DEFAULT_STROKE_WIDTH,
        ..Style::default()
    };
    if let Some(fill) = p.fill() {
        style.fill_color = Some(paint_color(fill.paint()));
        style.fill_opacity = fill.opacity().get();
    }
    if let Some(stroke) = p.stroke() {
        style.stroke_color = Some(paint_color(stroke.paint()));
        style.stroke_opacity = stroke.opacity().get();
        style.stroke_width = stroke.width().get();
    }
    style
}

/// Resolves a usvg paint to a solid manim color (gradients/patterns → grey).
fn paint_color(paint: &usvg::Paint) -> Color {
    match paint {
        usvg::Paint::Color(c) => Color::from_srgb_u8(c.red, c.green, c.blue),
        // Gradients and patterns are not resolved per-vertex here.
        _ => Color::from_srgb_u8(128, 128, 128),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_RECTS: &str = r##"<svg viewBox="0 0 10 10" xmlns="http://www.w3.org/2000/svg">
        <rect x="0" y="0" width="4" height="10" fill="#ff0000"/>
        <rect x="6" y="0" width="4" height="10" fill="#0000ff" stroke="#00ff00" stroke-width="1"/>
    </svg>"##;

    #[test]
    fn parses_multiple_paths() {
        let m = SVGMobject::from_str(TWO_RECTS).unwrap();
        assert_eq!(m.element_count(), 2);
    }

    #[test]
    fn maps_fill_and_stroke_colors() {
        let m = SVGMobject::from_str(TWO_RECTS).unwrap();
        assert_eq!(
            m.element_style(0).fill_color,
            Some(Color::from_srgb_u8(255, 0, 0))
        );
        let s1 = m.element_style(1);
        assert_eq!(s1.fill_color, Some(Color::from_srgb_u8(0, 0, 255)));
        assert_eq!(s1.stroke_color, Some(Color::from_srgb_u8(0, 255, 0)));
    }

    #[test]
    fn normalizes_to_default_height_and_centers() {
        let m = SVGMobject::from_str(TWO_RECTS).unwrap();
        let (min, max) = m.bounding_box().unwrap();
        // Height normalized to 2.0, centered on the origin.
        assert!(((max.y - min.y) - 2.0).abs() < 1e-3);
        assert!((0.5 * (min.x + max.x)).abs() < 1e-4);
        assert!((0.5 * (min.y + max.y)).abs() < 1e-4);
    }

    #[test]
    fn transform_and_viewbox_scaling_applied() {
        // A group transform shifts the rect; normalization still centers it.
        let svg = r##"<svg viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg">
            <g transform="translate(5,5)"><rect width="10" height="10" fill="#fff"/></g>
        </svg>"##;
        let m = SVGMobject::from_str(svg).unwrap();
        assert_eq!(m.element_count(), 1);
        let (min, max) = m.bounding_box().unwrap();
        // Square aspect preserved: width == height after scaling.
        assert!(((max.x - min.x) - (max.y - min.y)).abs() < 1e-3);
    }

    #[test]
    fn add_to_creates_a_child_per_element() {
        let m = SVGMobject::from_str(TWO_RECTS).unwrap();
        let mut scene = SceneState::new();
        let g = m.add_to(&mut scene);
        assert_eq!(scene.family(g.erase()).len(), 1 + 2);
    }
}
