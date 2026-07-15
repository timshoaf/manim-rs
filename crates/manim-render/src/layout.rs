//! Letterbox layout: fitting a fixed-aspect frame inside an arbitrary window.
//!
//! A scene's frame has a fixed aspect ratio (from [`Config`](manim_core::config::Config)'s
//! `frame_width / frame_height`), but a preview window can be any size. The
//! frame is scaled to the largest rectangle of the correct aspect that fits, and
//! centered; the leftover margins become background-color bars. This math is
//! pure and window-system-independent, so it is always compiled and unit-tested
//! (the winit [`RealtimePlayer`](crate::preview::RealtimePlayer) is feature-gated).

/// A pixel-space rectangle: origin `(x, y)` (top-left) and size `(w, h)`.
///
/// Used as a wgpu viewport for the letterboxed frame.
///
/// ```
/// use manim_render::layout::Viewport;
/// let vp = Viewport { x: 10.0, y: 0.0, w: 100.0, h: 80.0 };
/// assert_eq!(vp.w, 100.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Left edge in pixels.
    pub x: f32,
    /// Top edge in pixels.
    pub y: f32,
    /// Width in pixels.
    pub w: f32,
    /// Height in pixels.
    pub h: f32,
}

/// Computes the centered viewport for a frame of ratio `aspect`
/// (`frame_width / frame_height`) inside a `surface_w × surface_h` window.
///
/// The result is the largest rectangle of the given aspect that fits, centered;
/// the remaining space is the letterbox bars. Zero or non-finite inputs collapse
/// to a zero-sized viewport at the origin.
///
/// ```
/// use manim_render::layout::letterbox;
/// // A 16:9 frame in a square window is pillar-boxed vertically centered.
/// let vp = letterbox(1000.0, 1000.0, 16.0 / 9.0);
/// assert_eq!(vp.w, 1000.0);
/// assert!((vp.h - 562.5).abs() < 1e-3);
/// assert_eq!(vp.x, 0.0);
/// assert!((vp.y - 218.75).abs() < 1e-3);
/// ```
pub fn letterbox(surface_w: f32, surface_h: f32, aspect: f32) -> Viewport {
    if !(surface_w > 0.0 && surface_h > 0.0 && aspect > 0.0 && aspect.is_finite()) {
        return Viewport {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
    }
    let window_aspect = surface_w / surface_h;
    if window_aspect > aspect {
        // Window is wider than the frame: full height, pillar-boxed (bars L/R).
        let h = surface_h;
        let w = surface_h * aspect;
        Viewport {
            x: (surface_w - w) * 0.5,
            y: 0.0,
            w,
            h,
        }
    } else {
        // Window is taller than the frame: full width, letter-boxed (bars T/B).
        let w = surface_w;
        let h = surface_w / aspect;
        Viewport {
            x: 0.0,
            y: (surface_h - h) * 0.5,
            w,
            h,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_aspect_fills_completely() {
        let vp = letterbox(1600.0, 900.0, 16.0 / 9.0);
        assert_eq!(vp.x, 0.0);
        assert_eq!(vp.y, 0.0);
        assert!((vp.w - 1600.0).abs() < 1e-3);
        assert!((vp.h - 900.0).abs() < 1e-3);
    }

    #[test]
    fn wider_window_pillar_boxes() {
        // 2:1 window, 16:9 frame → full height, bars left & right.
        let vp = letterbox(2000.0, 1000.0, 16.0 / 9.0);
        assert_eq!(vp.h, 1000.0);
        assert!((vp.w - 1000.0 * 16.0 / 9.0).abs() < 1e-3);
        assert_eq!(vp.y, 0.0);
        // Symmetric bars.
        assert!((vp.x - (2000.0 - vp.w) / 2.0).abs() < 1e-3);
    }

    #[test]
    fn taller_window_letter_boxes() {
        // 1:1 window, 16:9 frame → full width, bars top & bottom.
        let vp = letterbox(1000.0, 1000.0, 16.0 / 9.0);
        assert_eq!(vp.w, 1000.0);
        assert!((vp.h - 1000.0 * 9.0 / 16.0).abs() < 1e-3);
        assert_eq!(vp.x, 0.0);
        assert!((vp.y - (1000.0 - vp.h) / 2.0).abs() < 1e-3);
    }

    #[test]
    fn fitted_rect_stays_within_window() {
        for (sw, sh) in [
            (1920.0, 1080.0),
            (800.0, 1200.0),
            (640.0, 480.0),
            (1000.0, 1000.0),
        ] {
            let vp = letterbox(sw, sh, 14.222 / 8.0);
            assert!(vp.x >= -1e-3 && vp.y >= -1e-3);
            assert!(vp.x + vp.w <= sw + 1e-3);
            assert!(vp.y + vp.h <= sh + 1e-3);
        }
    }

    #[test]
    fn degenerate_inputs_are_zero() {
        assert_eq!(
            letterbox(0.0, 100.0, 1.5),
            Viewport {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0
            }
        );
        assert_eq!(
            letterbox(100.0, 100.0, 0.0),
            Viewport {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0
            }
        );
        assert_eq!(
            letterbox(100.0, 0.0, 1.5),
            Viewport {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0
            }
        );
    }
}
