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

/// Maps a pointer position from client (element) pixels to **scene** coordinates,
/// inverting the letterbox fit and the camera projection.
///
/// `client_x`/`client_y` are measured from the element's top-left (y grows down);
/// `view_w`/`view_h` are the element's size in the same units. Because the
/// [`letterbox`] fit scales linearly with the element size, these may be in CSS
/// pixels or backing-store pixels interchangeably — the normalized fraction is
/// the same. `view_proj` is the camera's
/// [`view_proj`](crate::camera::Camera2D::view_proj) (scene → NDC); its inverse
/// takes the pointer's NDC back to a scene point.
///
/// Returns `None` only when the fitted viewport is degenerate (zero-sized). A
/// pointer over the letterbox bars yields a scene point outside the frame rather
/// than `None`, so callers can still track it.
///
/// ```
/// use manim_render::camera::Camera2D;
/// use manim_render::layout::client_to_scene;
///
/// // A 14.222×8 frame filling a same-aspect 1422.2×800 element.
/// let cam = Camera2D {
///     frame_center: manim_math::ORIGIN,
///     frame_width: 14.222,
///     frame_height: 8.0,
///     rotation: 0.0,
///     three_d: None,
/// };
/// // The element center maps to the scene origin.
/// let p = client_to_scene(711.1, 400.0, 1422.2, 800.0, 14.222 / 8.0, cam.view_proj()).unwrap();
/// assert!(p.x.abs() < 1e-2 && p.y.abs() < 1e-2);
/// ```
pub fn client_to_scene(
    client_x: f32,
    client_y: f32,
    view_w: f32,
    view_h: f32,
    aspect: f32,
    view_proj: glam::Mat4,
) -> Option<glam::Vec3> {
    let vp = letterbox(view_w, view_h, aspect);
    if vp.w <= 0.0 || vp.h <= 0.0 {
        return None;
    }
    // Fraction within the fitted frame rect, then to NDC (y flips: down → up).
    let fx = (client_x - vp.x) / vp.w;
    let fy = (client_y - vp.y) / vp.h;
    let ndc = glam::Vec3::new(fx * 2.0 - 1.0, 1.0 - fy * 2.0, 0.0);
    Some(view_proj.inverse().project_point3(ndc))
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

    fn frame_camera() -> crate::camera::Camera2D {
        crate::camera::Camera2D {
            frame_center: manim_math::ORIGIN,
            frame_width: 14.222,
            frame_height: 8.0,
            rotation: 0.0,
            three_d: None,
        }
    }

    #[test]
    fn client_center_maps_to_scene_center() {
        let cam = frame_camera();
        // Same-aspect element: the frame fills it, so center → origin.
        let p =
            client_to_scene(711.1, 400.0, 1422.2, 800.0, 14.222 / 8.0, cam.view_proj()).unwrap();
        assert!(p.x.abs() < 1e-2, "x = {}", p.x);
        assert!(p.y.abs() < 1e-2, "y = {}", p.y);
    }

    #[test]
    fn client_corners_map_to_frame_corners() {
        let cam = frame_camera();
        let vpj = cam.view_proj();
        // Top-left client → top-left scene corner (−w/2, +h/2) (y flips).
        let tl = client_to_scene(0.0, 0.0, 1422.2, 800.0, 14.222 / 8.0, vpj).unwrap();
        assert!((tl.x - (-7.111)).abs() < 1e-2, "tl.x = {}", tl.x);
        assert!((tl.y - 4.0).abs() < 1e-2, "tl.y = {}", tl.y);
        // Bottom-right client → (+w/2, −h/2).
        let br = client_to_scene(1422.2, 800.0, 1422.2, 800.0, 14.222 / 8.0, vpj).unwrap();
        assert!((br.x - 7.111).abs() < 1e-2, "br.x = {}", br.x);
        assert!((br.y - (-4.0)).abs() < 1e-2, "br.y = {}", br.y);
    }

    #[test]
    fn client_to_scene_accounts_for_letterbox_bars() {
        let cam = frame_camera();
        // A taller element (square) letter-boxes: bars top & bottom. The frame
        // rect is centered, so the element center still maps to the scene origin,
        // and scale is invariant to the element's pixel size.
        let p =
            client_to_scene(500.0, 500.0, 1000.0, 1000.0, 14.222 / 8.0, cam.view_proj()).unwrap();
        assert!(p.x.abs() < 1e-2 && p.y.abs() < 1e-2, "center off: {p:?}");
        // A point on the top bar (above the fitted frame) has scene y above +h/2.
        let above =
            client_to_scene(500.0, 10.0, 1000.0, 1000.0, 14.222 / 8.0, cam.view_proj()).unwrap();
        assert!(
            above.y > 4.0,
            "top-bar point should be above the frame: {}",
            above.y
        );
    }

    #[test]
    fn client_to_scene_none_on_degenerate() {
        let cam = frame_camera();
        assert!(client_to_scene(0.0, 0.0, 0.0, 100.0, 1.5, cam.view_proj()).is_none());
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
