//! [`MaterialQuad`] — a world-space rectangle painted per-pixel by a GPU
//! [`Material`], the first-class domain-coloring / heatmap scene citizen.
//!
//! A field (complex or scalar) is sampled onto a [`TextureData`] grid pinned to
//! the rectangle; the attached material shades every pixel from it (phase-hue,
//! colormap, or field-texture with contours). [`MaterialQuad::resample`] swaps
//! the texture when the field or its parameters change — the mechanism a
//! draggable-zeros/poles figure re-renders through.

use std::sync::Arc;

use manim_core::display::{
    Colormap, ContourParams, FieldChannels, Material, MaterialKind, TextureData,
};
use manim_core::impl_mobject;
use manim_core::mobject::{AnyId, MobjectData, MobjectId};
use manim_core::prelude::Color;
use manim_core::scene_state::SceneState;
use manim_core::style::Style;
use manim_math::path::Path;
use manim_math::Point;

use manim_fields::complex::Complex;
use manim_fields::field::{ComplexField, ScalarField};

/// A rectangle mobject whose region is painted by a GPU [`Material`].
#[derive(Clone)]
pub struct MaterialQuad {
    data: MobjectData,
}
impl_mobject!(MaterialQuad);

/// The `i`-th of `n` sample coordinates spanning `[a, b]` inclusive.
fn axis_sample(a: f64, b: f64, i: usize, n: usize) -> f64 {
    if n <= 1 {
        0.5 * (a + b)
    } else {
        a + (b - a) * i as f64 / (n - 1) as f64
    }
}

/// A blank style (no fill, no stroke) so only the material paints the quad.
fn blank_style() -> Style {
    Style {
        fill_color: None,
        fill_opacity: 0.0,
        stroke_color: None,
        stroke_opacity: 0.0,
        ..Style::default()
    }
}

fn rect_path(x_range: [f64; 2], y_range: [f64; 2]) -> Path {
    let (x0, x1) = (x_range[0] as f32, x_range[1] as f32);
    let (y0, y1) = (y_range[0] as f32, y_range[1] as f32);
    Path::from_corners(
        &[
            Point::new(x0, y0, 0.0),
            Point::new(x1, y0, 0.0),
            Point::new(x1, y1, 0.0),
            Point::new(x0, y1, 0.0),
        ],
        true,
    )
}

fn region_center_size(x_range: [f64; 2], y_range: [f64; 2]) -> (Point, [f32; 2]) {
    let center = Point::new(
        (0.5 * (x_range[0] + x_range[1])) as f32,
        (0.5 * (y_range[0] + y_range[1])) as f32,
        0.0,
    );
    let size = [
        (x_range[1] - x_range[0]) as f32,
        (y_range[1] - y_range[0]) as f32,
    ];
    (center, size)
}

/// Samples a complex field to an `RG32F` (re, im) texture over the rectangle.
fn sample_complex(
    x_range: [f64; 2],
    y_range: [f64; 2],
    nx: usize,
    ny: usize,
    field: &ComplexField,
) -> TextureData {
    let mut data = Vec::with_capacity(nx * ny * 2);
    for j in 0..ny {
        let y = axis_sample(y_range[0], y_range[1], j, ny);
        for i in 0..nx {
            let x = axis_sample(x_range[0], x_range[1], i, nx);
            let w = field.at(Complex::new(x, y));
            data.push(w.re as f32);
            data.push(w.im as f32);
        }
    }
    let (center, size) = region_center_size(x_range, y_range);
    TextureData {
        width: nx as u32,
        height: ny as u32,
        channels: FieldChannels::Rg,
        data,
        center,
        size,
    }
}

/// Samples a scalar field to an `R32F` texture; returns it with the value range.
fn sample_scalar(
    x_range: [f64; 2],
    y_range: [f64; 2],
    nx: usize,
    ny: usize,
    field: &ScalarField,
) -> (TextureData, [f32; 2]) {
    let mut data = Vec::with_capacity(nx * ny);
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for j in 0..ny {
        let y = axis_sample(y_range[0], y_range[1], j, ny);
        for i in 0..nx {
            let x = axis_sample(x_range[0], x_range[1], i, nx);
            let v = field.at(manim_fields::Point::new(x, y, 0.0));
            lo = lo.min(v);
            hi = hi.max(v);
            data.push(v as f32);
        }
    }
    let (center, size) = region_center_size(x_range, y_range);
    let td = TextureData {
        width: nx as u32,
        height: ny as u32,
        channels: FieldChannels::R,
        data,
        center,
        size,
    };
    (td, [lo as f32, hi as f32])
}

impl MaterialQuad {
    /// A quad over the rectangle painted by an already-built [`Material`] (e.g. a
    /// texture sampled elsewhere, like a wavefunction grid).
    pub fn from_material(x_range: [f64; 2], y_range: [f64; 2], material: Material) -> Self {
        let mut data = MobjectData::new(rect_path(x_range, y_range), blank_style());
        data.material = Some(material);
        Self { data }
    }

    /// The material currently painting the quad.
    pub fn material(&self) -> &Material {
        self.data
            .material
            .as_ref()
            .expect("MaterialQuad always carries a material")
    }

    /// Adds the quad to a scene.
    pub fn add_to(self, scene: &mut SceneState) -> MobjectId<MaterialQuad> {
        scene.add(self)
    }

    /// The phase-hue [`Material`] for a complex field over the rectangle.
    pub fn domain_coloring_material(
        x_range: [f64; 2],
        y_range: [f64; 2],
        resolution: (usize, usize),
        field: &ComplexField,
    ) -> Material {
        let td = sample_complex(x_range, y_range, resolution.0, resolution.1, field);
        Material {
            kind: MaterialKind::PhaseHue {
                modulus_contours: false,
            },
            texture: Arc::new(td),
            value_range: [0.0, 1.0],
            opacity: 1.0,
        }
    }

    /// A domain-coloring quad for a complex field `f(z)` — phase → hue, modulus →
    /// brightness (the Needham / *Visual Complex Analysis* picture).
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_fields::field::ComplexField;
    /// use manim_sci::material_quad::MaterialQuad;
    /// let mut scene = SceneState::new();
    /// let f = ComplexField::new(|z| z * z);
    /// let q = MaterialQuad::domain_coloring([-2.0, 2.0], [-2.0, 2.0], (64, 64), &f).add_to(&mut scene);
    /// assert!(scene.get_dyn(q).data().material.is_some());
    /// ```
    pub fn domain_coloring(
        x_range: [f64; 2],
        y_range: [f64; 2],
        resolution: (usize, usize),
        field: &ComplexField,
    ) -> Self {
        Self::from_material(
            x_range,
            y_range,
            Self::domain_coloring_material(x_range, y_range, resolution, field),
        )
    }

    /// The heatmap [`Material`] for a scalar field over the rectangle (auto-ranged
    /// to the sampled min/max).
    pub fn heatmap_material(
        x_range: [f64; 2],
        y_range: [f64; 2],
        resolution: (usize, usize),
        field: &ScalarField,
        colormap: Colormap,
    ) -> Material {
        let (td, range) = sample_scalar(x_range, y_range, resolution.0, resolution.1, field);
        Material {
            kind: MaterialKind::Heatmap { colormap },
            texture: Arc::new(td),
            value_range: range,
            opacity: 1.0,
        }
    }

    /// A heatmap quad for a scalar field `f(x, y)`.
    pub fn heatmap(
        x_range: [f64; 2],
        y_range: [f64; 2],
        resolution: (usize, usize),
        field: &ScalarField,
        colormap: Colormap,
    ) -> Self {
        Self::from_material(
            x_range,
            y_range,
            Self::heatmap_material(x_range, y_range, resolution, field, colormap),
        )
    }

    /// The field-texture [`Material`] (colormap + iso-contour lines) for a scalar
    /// field.
    pub fn field_contours_material(
        x_range: [f64; 2],
        y_range: [f64; 2],
        resolution: (usize, usize),
        field: &ScalarField,
        colormap: Colormap,
        contour_spacing: f32,
    ) -> Material {
        let (td, range) = sample_scalar(x_range, y_range, resolution.0, resolution.1, field);
        Material {
            kind: MaterialKind::FieldTexture {
                colormap,
                contours: Some(ContourParams {
                    spacing: contour_spacing,
                    width: 1.5,
                    color: Color::from_rgba(0.0, 0.0, 0.0, 1.0),
                }),
            },
            texture: Arc::new(td),
            value_range: range,
            opacity: 1.0,
        }
    }

    /// A contoured scalar-field quad (colormap + iso-contour overlay).
    pub fn field_contours(
        x_range: [f64; 2],
        y_range: [f64; 2],
        resolution: (usize, usize),
        field: &ScalarField,
        colormap: Colormap,
        contour_spacing: f32,
    ) -> Self {
        Self::from_material(
            x_range,
            y_range,
            Self::field_contours_material(
                x_range,
                y_range,
                resolution,
                field,
                colormap,
                contour_spacing,
            ),
        )
    }

    /// Swaps the material on an existing quad in the scene — re-sample the field
    /// (parameters may have changed) and pass the fresh [`Material`]. Bumps the
    /// generation and swaps the texture `Arc`, so the renderer re-uploads.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_fields::field::ComplexField;
    /// use manim_sci::material_quad::MaterialQuad;
    /// let mut scene = SceneState::new();
    /// let f = ComplexField::new(|z| z);
    /// let q = MaterialQuad::domain_coloring([-1.0, 1.0], [-1.0, 1.0], (16, 16), &f).add_to(&mut scene);
    /// let before = scene.get_dyn(q).data().generation;
    /// let g = ComplexField::new(|z| z * z); // a dragged parameter changed the map
    /// let m = MaterialQuad::domain_coloring_material([-1.0, 1.0], [-1.0, 1.0], (16, 16), &g);
    /// MaterialQuad::resample(&mut scene, q, m);
    /// assert!(scene.get_dyn(q).data().generation > before);
    /// ```
    pub fn resample(scene: &mut SceneState, id: impl Into<AnyId>, material: Material) {
        scene.get_dyn_mut(id).set_material(material);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_coloring_builds_rg_texture() {
        let f = ComplexField::new(|z| z * z);
        let q = MaterialQuad::domain_coloring([-1.0, 1.0], [-1.0, 1.0], (8, 8), &f);
        let m = q.material();
        assert_eq!(m.texture.channels, FieldChannels::Rg);
        assert_eq!(m.texture.data.len(), 8 * 8 * 2);
        assert!(matches!(m.kind, MaterialKind::PhaseHue { .. }));
        assert!(q.data.style.fill_color.is_none() && q.data.style.stroke_color.is_none());
    }

    #[test]
    fn heatmap_auto_ranges_scalar_field() {
        // f = x over [0,2] → value range [0, 2].
        let f = ScalarField::coordinate(0);
        let q = MaterialQuad::heatmap([0.0, 2.0], [0.0, 1.0], (16, 4), &f, Colormap::Viridis);
        let m = q.material();
        assert_eq!(m.texture.channels, FieldChannels::R);
        assert!((m.value_range[0]).abs() < 1e-5 && (m.value_range[1] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn resample_bumps_generation_and_swaps_texture() {
        let mut scene = SceneState::new();
        let f = ComplexField::new(|z| z);
        let q =
            MaterialQuad::domain_coloring([-1.0, 1.0], [-1.0, 1.0], (8, 8), &f).add_to(&mut scene);
        let gen0 = scene.get_dyn(q).data().generation;
        let tex0 = scene
            .get_dyn(q)
            .data()
            .material
            .as_ref()
            .unwrap()
            .texture
            .clone();

        let g = ComplexField::new(|z| z * z);
        let m = MaterialQuad::domain_coloring_material([-1.0, 1.0], [-1.0, 1.0], (8, 8), &g);
        MaterialQuad::resample(&mut scene, q, m);

        let data = scene.get_dyn(q).data();
        assert!(data.generation > gen0, "resample must bump the generation");
        let tex1 = &data.material.as_ref().unwrap().texture;
        assert!(
            !Arc::ptr_eq(&tex0, tex1),
            "resample must swap the texture Arc"
        );
    }

    #[test]
    fn quad_reaches_the_display_list_material() {
        let mut scene = SceneState::new();
        let f = ComplexField::new(|z| z);
        MaterialQuad::domain_coloring([-1.0, 1.0], [-1.0, 1.0], (8, 8), &f).add_to(&mut scene);
        let dl = scene.display_list();
        assert!(
            dl.0.iter().any(|it| it.material.is_some()),
            "material reached the DrawItem"
        );
    }
}
