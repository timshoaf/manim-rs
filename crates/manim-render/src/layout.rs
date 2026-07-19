//! Letterbox layout: fitting a fixed-aspect frame inside an arbitrary window.
//!
//! A scene's frame has a fixed aspect ratio (from [`Config`](manim_core::config::Config)'s
//! `frame_width / frame_height`), but a preview window can be any size. The
//! frame is scaled to the largest rectangle of the correct aspect that fits, and
//! centered; the leftover margins become background-color bars. This math is
//! pure and window-system-independent, so it is always compiled and unit-tested
//! (the winit `RealtimePlayer` (`preview` feature) is feature-gated).

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
/// This is the special case of [`element_to_scene`] where the element and the
/// backing store share an aspect. A canvas CSS-stretched into a box of a
/// *different* aspect (a fixed-height box on a narrow phone) needs the general
/// form — the fit is per-axis there, and this uniform one mistracks.
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
    element_to_scene(
        client_x, client_y, view_w, view_h, view_w, view_h, aspect, view_proj,
    )
}

/// Maps a pointer position from element (CSS) pixels to **scene** coordinates
/// for a canvas whose backing store is `backing_w × backing_h` and is stretched
/// by CSS to fill an `elem_w × elem_h` box.
///
/// This is the general form of [`client_to_scene`], which assumes the element
/// and the backing store have the same aspect. They do not have to: a canvas
/// styled `width:100%;height:100%` inside a box whose aspect differs from
/// `backing_w / backing_h` is scaled **per axis** by the browser, so a single
/// uniform fit cannot invert it. The correct inverse is therefore two steps:
///
/// 1. element px → backing px, scaling each axis independently
///    (`backing_w / elem_w`, `backing_h / elem_h`);
/// 2. backing px → scene, inverting the [`letterbox`] fit that
///    [`render`](crate::canvas::CanvasSurface::render) applies *inside the
///    backing store* plus the camera projection.
///
/// Note both coordinate spaces here are CSS-pixel-derived on the element side
/// and device-pixel on the backing side, and the ratio between them is exactly
/// what step 1 measures — so no separate `devicePixelRatio` term belongs
/// anywhere in this mapping.
///
/// Returns `None` when the element or the fitted viewport is degenerate.
#[allow(clippy::too_many_arguments)]
pub fn element_to_scene(
    client_x: f32,
    client_y: f32,
    elem_w: f32,
    elem_h: f32,
    backing_w: f32,
    backing_h: f32,
    aspect: f32,
    view_proj: glam::Mat4,
) -> Option<glam::Vec3> {
    if !(elem_w > 0.0 && elem_h > 0.0) {
        return None;
    }
    let bx = client_x * (backing_w / elem_w);
    let by = client_y * (backing_h / elem_h);
    let vp = letterbox(backing_w, backing_h, aspect);
    if vp.w <= 0.0 || vp.h <= 0.0 {
        return None;
    }
    // Fraction within the fitted frame rect, then to NDC (y flips: down → up).
    let fx = (bx - vp.x) / vp.w;
    let fy = (by - vp.y) / vp.h;
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

/// The pixel [`Viewport`] of a zoom inset placed at the normalized rectangle
/// `(ix, iy, iw, ih)` (fractions of `base`, top-left origin) within `base`.
///
/// ```
/// use manim_render::layout::{inset_viewport, Viewport};
/// let base = Viewport { x: 0.0, y: 0.0, w: 1000.0, h: 800.0 };
/// let vp = inset_viewport(base, 0.6, 0.05, 0.35, 0.35);
/// assert_eq!((vp.x, vp.y, vp.w, vp.h), (600.0, 40.0, 350.0, 280.0));
/// ```
pub fn inset_viewport(base: Viewport, ix: f32, iy: f32, iw: f32, ih: f32) -> Viewport {
    Viewport {
        x: base.x + ix * base.w,
        y: base.y + iy * base.h,
        w: iw * base.w,
        h: ih * base.h,
    }
}

/// The zoom camera's frame `(width, height)` for a magnified region of scene
/// width `region_width` shown undistorted in an inset of pixel size
/// `inset_w × inset_h`. The height follows the inset aspect so nothing stretches.
///
/// ```
/// use manim_render::layout::zoom_frame_size;
/// // A 2-unit-wide region in a 2:1 inset → a 1-unit-tall camera frame.
/// let (w, h) = zoom_frame_size(2.0, 400.0, 200.0);
/// assert!((w - 2.0).abs() < 1e-6 && (h - 1.0).abs() < 1e-6);
/// ```
pub fn zoom_frame_size(region_width: f32, inset_w: f32, inset_h: f32) -> (f32, f32) {
    let w = region_width.max(1e-4);
    if inset_w <= 0.0 || !inset_w.is_finite() {
        return (w, w);
    }
    (w, w * (inset_h / inset_w))
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

    /// An aspect-true CSS box (the desktop case): the per-axis mapping must
    /// agree exactly with the old uniform one.
    #[test]
    fn element_to_scene_matches_client_to_scene_when_aspect_true() {
        let cam = frame_camera();
        let vpj = cam.view_proj();
        let a = 14.222 / 8.0;
        // 1422×800 backing shown in a 355.5×200 CSS box (same aspect, DPR-like
        // 4× downscale).
        for (x, y) in [(0.0, 0.0), (177.75, 100.0), (355.5, 200.0), (300.0, 40.0)] {
            let per_axis = element_to_scene(x, y, 355.5, 200.0, 1422.0, 800.0, a, vpj).unwrap();
            // Same fractional position expressed directly in the element box.
            let uniform = client_to_scene(x, y, 355.5, 200.0, a, vpj).unwrap();
            assert!(
                (per_axis.x - uniform.x).abs() < 1e-2 && (per_axis.y - uniform.y).abs() < 1e-2,
                "{per_axis:?} vs {uniform:?}"
            );
        }
    }

    /// The mobile case: a 16:9-ish backing store squashed into a nearly-square
    /// box. The uniform fit is wrong here; the per-axis mapping still sends the
    /// box corners to the frame corners.
    #[test]
    fn element_to_scene_handles_squashed_box() {
        let cam = frame_camera();
        let vpj = cam.view_proj();
        let a = 14.222 / 8.0;
        // 1422×800 backing CSS-stretched into 360×428 (the phone regression).
        let (ew, eh) = (360.0, 428.0);
        let c = element_to_scene(ew * 0.5, eh * 0.5, ew, eh, 1422.0, 800.0, a, vpj).unwrap();
        assert!(c.x.abs() < 1e-2 && c.y.abs() < 1e-2, "center off: {c:?}");
        let tl = element_to_scene(0.0, 0.0, ew, eh, 1422.0, 800.0, a, vpj).unwrap();
        assert!((tl.x - (-7.111)).abs() < 1e-2, "tl.x = {}", tl.x);
        assert!((tl.y - 4.0).abs() < 1e-2, "tl.y = {}", tl.y);
        let br = element_to_scene(ew, eh, ew, eh, 1422.0, 800.0, a, vpj).unwrap();
        assert!((br.x - 7.111).abs() < 1e-2, "br.x = {}", br.x);
        assert!((br.y - (-4.0)).abs() < 1e-2, "br.y = {}", br.y);
        // The old uniform mapping visibly misses vertically on this box: it
        // letterboxes the frame inside 360×428 instead of stretching it.
        let uniform_br = client_to_scene(ew, eh, ew, eh, a, vpj).unwrap();
        assert!(
            (uniform_br.y - (-4.0)).abs() > 0.5,
            "regression guard: uniform fit should differ here ({})",
            uniform_br.y
        );
    }

    /// Element coordinates are CSS pixels: doubling the backing store (a 2×
    /// devicePixelRatio canvas) at the same CSS size must not move the mapped
    /// point — no DPR term belongs in the mapping.
    #[test]
    fn element_to_scene_is_invariant_to_backing_resolution() {
        let cam = frame_camera();
        let vpj = cam.view_proj();
        let a = 14.222 / 8.0;
        let lo = element_to_scene(120.0, 90.0, 360.0, 428.0, 711.0, 400.0, a, vpj).unwrap();
        let hi = element_to_scene(120.0, 90.0, 360.0, 428.0, 1422.0, 800.0, a, vpj).unwrap();
        assert!(
            (lo.x - hi.x).abs() < 1e-2 && (lo.y - hi.y).abs() < 1e-2,
            "{lo:?} vs {hi:?}"
        );
    }

    #[test]
    fn element_to_scene_none_on_degenerate_element() {
        let cam = frame_camera();
        let vpj = cam.view_proj();
        assert!(element_to_scene(0.0, 0.0, 0.0, 100.0, 640.0, 360.0, 1.5, vpj).is_none());
        assert!(element_to_scene(0.0, 0.0, 100.0, 100.0, 0.0, 360.0, 1.5, vpj).is_none());
    }

    #[test]
    fn inset_viewport_places_within_base() {
        let base = Viewport {
            x: 100.0,
            y: 0.0,
            w: 800.0,
            h: 600.0,
        };
        let vp = inset_viewport(base, 0.5, 0.5, 0.25, 0.25);
        assert_eq!((vp.x, vp.y, vp.w, vp.h), (500.0, 300.0, 200.0, 150.0));
        // The inset stays inside the base rectangle.
        assert!(vp.x + vp.w <= base.x + base.w + 1e-3);
        assert!(vp.y + vp.h <= base.y + base.h + 1e-3);
    }

    #[test]
    fn zoom_frame_size_matches_inset_aspect() {
        // Square inset → square region frame.
        let (w, h) = zoom_frame_size(3.0, 300.0, 300.0);
        assert!((w - 3.0).abs() < 1e-6 && (h - 3.0).abs() < 1e-6);
        // Wide inset → short frame (undistorted magnification).
        let (w2, h2) = zoom_frame_size(4.0, 400.0, 100.0);
        assert!((w2 - 4.0).abs() < 1e-6 && (h2 - 1.0).abs() < 1e-6);
        // Degenerate inset falls back to square.
        assert_eq!(zoom_frame_size(2.0, 0.0, 100.0), (2.0, 2.0));
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
