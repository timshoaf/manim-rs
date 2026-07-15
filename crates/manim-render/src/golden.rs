//! Golden-image comparison for renderer tests.
//!
//! [`assert_golden`] compares a freshly rendered [`RgbaImage`] against a
//! checked-in PNG under `tests/golden/`, tolerating the small per-channel
//! differences that different GPU drivers and anti-aliasing produce. Two knobs
//! define "close enough": a per-channel byte tolerance ([`CHANNEL_TOLERANCE`])
//! and a fraction-of-differing-pixels threshold ([`PIXEL_FRACTION_TOLERANCE`]).
//!
//! Bootstrapping and updating:
//! - A **missing** golden is written and the check passes (first run seeds it),
//!   with a notice on stderr.
//! - `BLESS=1` **overwrites** the golden with the new render and passes — use it
//!   after an intentional visual change.
//!
//! ```no_run
//! use image::RgbaImage;
//! use manim_render::golden::assert_golden;
//! let img = RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]));
//! assert_golden("black_4x4", &img);
//! ```

use std::path::PathBuf;

use image::RgbaImage;

/// Maximum absolute per-channel difference (0–255) for two pixels to count as
/// equal.
pub const CHANNEL_TOLERANCE: u8 = 3;

/// Maximum fraction of differing pixels tolerated before a comparison fails.
pub const PIXEL_FRACTION_TOLERANCE: f64 = 0.005;

/// The fraction of pixels in `a` that differ from `b` by more than
/// `channel_tol` on any channel.
///
/// Mismatched dimensions return `1.0` (everything differs).
///
/// ```
/// use image::{Rgba, RgbaImage};
/// use manim_render::golden::pixel_diff_fraction;
///
/// let a = RgbaImage::from_pixel(2, 2, Rgba([10, 10, 10, 255]));
/// let mut b = a.clone();
/// b.put_pixel(0, 0, Rgba([200, 10, 10, 255])); // one of four pixels differs
/// assert!((pixel_diff_fraction(&a, &b, 3) - 0.25).abs() < 1e-9);
/// ```
pub fn pixel_diff_fraction(a: &RgbaImage, b: &RgbaImage, channel_tol: u8) -> f64 {
    if a.dimensions() != b.dimensions() {
        return 1.0;
    }
    let total = (a.width() as u64 * a.height() as u64).max(1);
    let mut differing = 0u64;
    for (pa, pb) in a.pixels().zip(b.pixels()) {
        let diff =
            pa.0.iter()
                .zip(pb.0.iter())
                .any(|(x, y)| x.abs_diff(*y) > channel_tol);
        if diff {
            differing += 1;
        }
    }
    differing as f64 / total as f64
}

/// The absolute path of the golden PNG named `name`.
fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(format!("{name}.png"))
}

/// Writes `img` to the golden file for `name`, creating the directory.
fn write_golden(name: &str, img: &RgbaImage) {
    let path = golden_path(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create golden dir");
    }
    img.save(&path)
        .unwrap_or_else(|e| panic!("write golden {}: {e}", path.display()));
}

/// Asserts that `img` matches the checked-in golden for `name`, within
/// [`CHANNEL_TOLERANCE`] per channel on at least
/// `1 - `[`PIXEL_FRACTION_TOLERANCE`] of pixels.
///
/// On a missing golden, the image is written and the check passes (bootstrap).
/// With `BLESS=1` in the environment, the golden is overwritten and the check
/// passes.
///
/// # Panics
///
/// If the golden exists, `BLESS` is unset, and too many pixels differ (or the
/// dimensions mismatch).
///
/// ```no_run
/// use image::RgbaImage;
/// use manim_render::golden::assert_golden;
/// let img = RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]));
/// assert_golden("red_8x8", &img);
/// ```
pub fn assert_golden(name: &str, img: &RgbaImage) {
    let path = golden_path(name);
    let bless = std::env::var_os("BLESS").is_some_and(|v| v == "1");

    if bless {
        write_golden(name, img);
        eprintln!("BLESS=1: wrote golden {}", path.display());
        return;
    }
    if !path.exists() {
        write_golden(name, img);
        eprintln!(
            "golden {} did not exist; wrote it and passing (bootstrap)",
            path.display()
        );
        return;
    }

    let golden = image::open(&path)
        .unwrap_or_else(|e| panic!("open golden {}: {e}", path.display()))
        .to_rgba8();
    let fraction = pixel_diff_fraction(img, &golden, CHANNEL_TOLERANCE);
    assert!(
        fraction <= PIXEL_FRACTION_TOLERANCE,
        "golden mismatch for {name}: {:.3}% of pixels differ by > {} \
         (tolerance {:.3}%). Re-run with BLESS=1 if this change is intended.",
        fraction * 100.0,
        CHANNEL_TOLERANCE,
        PIXEL_FRACTION_TOLERANCE * 100.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn identical_images_have_zero_diff() {
        let a = RgbaImage::from_pixel(16, 16, Rgba([1, 2, 3, 255]));
        assert_eq!(pixel_diff_fraction(&a, &a, CHANNEL_TOLERANCE), 0.0);
    }

    #[test]
    fn within_channel_tolerance_is_equal() {
        let a = RgbaImage::from_pixel(4, 4, Rgba([100, 100, 100, 255]));
        let b = RgbaImage::from_pixel(4, 4, Rgba([102, 100, 98, 255]));
        // Every channel is within ±3, so nothing differs.
        assert_eq!(pixel_diff_fraction(&a, &b, CHANNEL_TOLERANCE), 0.0);
    }

    #[test]
    fn mismatched_dimensions_fully_differ() {
        let a = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 255]));
        let b = RgbaImage::from_pixel(4, 5, Rgba([0, 0, 0, 255]));
        assert_eq!(pixel_diff_fraction(&a, &b, CHANNEL_TOLERANCE), 1.0);
    }
}
