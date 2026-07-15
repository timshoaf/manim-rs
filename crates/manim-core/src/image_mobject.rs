//! [`ImageMobject`]: a raster image drawn on a world-space quad.
//!
//! Port of manim CE's `ImageMobject`. The mobject stores its pixels (shared via
//! [`Arc`]) and a rectangular quad [`path`](crate::mobject::MobjectData::path)
//! sized to the image aspect at a default height of `2.0` scene units. Ordinary
//! transforms (`shift`, `scale`, `rotate`) move the quad's corners; the renderer
//! maps the texture onto them. The paint is carried on the display list as
//! [`ImagePaint`], drawn in `z_index` order interleaved with vector items.

use std::sync::Arc;

use manim_math::path::Path;
use manim_math::Point;

use crate::display::{ImageData, ImagePaint, Sampler};
use crate::impl_mobject;
use crate::mobject::MobjectData;
use crate::style::Style;

/// Default image height in scene units (aspect-preserving).
pub const DEFAULT_IMAGE_HEIGHT: f32 = 2.0;

/// A raster image mobject.
///
/// ```
/// use manim_core::image_mobject::ImageMobject;
/// use manim_core::mobject::MobjectExt;
/// // A 2×1 red/blue image → aspect 2, default height 2 → width 4.
/// let px = vec![255, 0, 0, 255, 0, 0, 255, 255];
/// let img = ImageMobject::from_rgba(2, 1, px);
/// let bb = img.bounding_box();
/// assert!((bb.height() - 2.0).abs() < 1e-5);
/// assert!((bb.width() - 4.0).abs() < 1e-5);
/// ```
#[derive(Clone)]
pub struct ImageMobject {
    data: MobjectData,
}
impl_mobject!(ImageMobject);

impl ImageMobject {
    /// Builds an image mobject from `width × height` straight-alpha RGBA8 pixels
    /// (`width·height·4` bytes), sized to the default height preserving aspect,
    /// centered on the origin, with linear sampling.
    ///
    /// Pixel buffers of the wrong length are padded/truncated to fit.
    pub fn from_rgba(width: u32, height: u32, mut pixels: Vec<u8>) -> Self {
        let need = width as usize * height as usize * 4;
        pixels.resize(need, 0);

        let aspect = if height > 0 {
            width as f32 / height as f32
        } else {
            1.0
        };
        let h = DEFAULT_IMAGE_HEIGHT;
        let w = h * aspect;
        let (hw, hh) = (w * 0.5, h * 0.5);
        // Corner order TL, TR, BR, BL (y-up), matching the renderer's UVs.
        let corners = [
            Point::new(-hw, hh, 0.0),
            Point::new(hw, hh, 0.0),
            Point::new(hw, -hh, 0.0),
            Point::new(-hw, -hh, 0.0),
        ];

        let style = Style {
            fill_color: None,
            fill_opacity: 0.0,
            stroke_color: None,
            stroke_opacity: 0.0,
            ..Style::default()
        };
        let mut data = MobjectData::new(Path::from_corners(&corners, true), style);
        data.image = Some(ImagePaint {
            data: Arc::new(ImageData {
                width,
                height,
                rgba: pixels,
            }),
            sampler: Sampler::Linear,
        });

        Self { data }
    }

    /// Reads and decodes an image file into an [`ImageMobject`] (native only).
    ///
    /// # Errors
    ///
    /// Propagates [`image::ImageError`] on read/decode failure.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, image::ImageError> {
        let rgba = image::open(path)?.to_rgba8();
        let (w, h) = rgba.dimensions();
        Ok(Self::from_rgba(w, h, rgba.into_raw()))
    }

    /// Switches to nearest-neighbor sampling (crisp pixels) — builder style.
    pub fn with_nearest_sampling(mut self) -> Self {
        if let Some(img) = &mut self.data.image {
            img.sampler = Sampler::Nearest;
        }
        self
    }

    /// The pixel dimensions `(width, height)`.
    pub fn pixel_dimensions(&self) -> (u32, u32) {
        self.data
            .image
            .as_ref()
            .map(|i| (i.data.width, i.data.height))
            .unwrap_or((0, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::{Mobject, MobjectExt};
    use manim_math::RIGHT;

    fn checker() -> ImageMobject {
        // 2×2 checkerboard.
        let px = vec![
            255, 255, 255, 255, 0, 0, 0, 255, // row 0
            0, 0, 0, 255, 255, 255, 255, 255, // row 1
        ];
        ImageMobject::from_rgba(2, 2, px)
    }

    #[test]
    fn quad_matches_aspect_and_height() {
        let img = checker();
        let bb = img.bounding_box();
        assert!((bb.height() - 2.0).abs() < 1e-5);
        assert!((bb.width() - 2.0).abs() < 1e-5); // square image
        assert!(bb.center().length() < 1e-6); // centered
    }

    #[test]
    fn carries_image_paint_in_data() {
        let img = checker();
        assert!(img.data().image.is_some());
        assert_eq!(img.pixel_dimensions(), (2, 2));
    }

    #[test]
    fn nearest_sampling_builder() {
        let img = checker().with_nearest_sampling();
        assert_eq!(img.data().image.as_ref().unwrap().sampler, Sampler::Nearest);
    }

    #[test]
    fn short_pixel_buffer_is_padded() {
        let img = ImageMobject::from_rgba(2, 2, vec![255, 0, 0, 255]);
        assert_eq!(img.data().image.as_ref().unwrap().data.rgba.len(), 16);
    }

    #[test]
    fn transforms_move_the_quad() {
        let mut img = checker();
        img.shift(2.0 * RIGHT);
        assert!((img.get_center() - 2.0 * RIGHT).length() < 1e-6);
    }
}
