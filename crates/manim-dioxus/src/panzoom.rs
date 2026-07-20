//! Pinch/scroll pan-and-zoom for a 2-D figure (FE-144): [`PanZoomState`] is the
//! pure camera-frame math, [`PanZoom`] the thin glue that applies it to a live
//! scene's [`Camera2D`] each frame.
//!
//! The gesture arrives as a [`GestureDelta`] — a zoom ratio about an
//! element-fraction anchor plus an element-fraction pan — from the
//! [`GestureRouter`](crate::gesture::GestureRouter), which is fed by a pinch,
//! ctrl+wheel, or a middle-button drag alike. Everything here is expressed in
//! frame *fractions*, never in scene coordinates read back from the camera, so
//! applying a gesture cannot feed its own motion into the next one.

use manim_core::camera::Camera2D;
use manim_core::prelude::Point;
use manim_core::scene_state::SceneState;

use crate::gesture::GestureDelta;
use crate::{LiveUpdater, PointerState};

/// The visible 2-D frame under pan/zoom: a center and a width, with the height
/// following the figure's aspect.
///
/// Zoom is *about an anchor*: the scene point under the pinch centroid (or the
/// cursor) stays put, which is the whole difference between a map that zooms
/// where you are looking and one that zooms to its own middle.
///
/// ```
/// use manim_dioxus::panzoom::PanZoomState;
/// use manim_dioxus::gesture::GestureDelta;
/// let mut pz = PanZoomState::new(8.0, 4.5);
/// // Pinch out 2× about the element center: the frame halves, center unmoved.
/// pz.apply(GestureDelta { scale: 2.0, pan: (0.0, 0.0), anchor: (0.5, 0.5), active: true });
/// assert!((pz.width - 4.0).abs() < 1e-5);
/// assert_eq!(pz.center.x, 0.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanZoomState {
    /// The frame center in scene units.
    pub center: Point,
    /// The visible frame width in scene units.
    pub width: f32,
    /// The visible frame height in scene units.
    pub height: f32,
    /// The width at zoom 1 (what [`reset`](Self::reset) returns to).
    base_width: f32,
    /// The height at zoom 1.
    base_height: f32,
    min_zoom: f32,
    max_zoom: f32,
}

impl PanZoomState {
    /// A frame of `width × height` scene units, centered at the origin, zoomable
    /// between ⅛× and 32× (a range that covers "the whole plane" to "one pixel
    /// of the domain coloring" without letting a fast pinch fling it to nothing).
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            center: Point::ZERO,
            width,
            height,
            base_width: width,
            base_height: height,
            min_zoom: 0.125,
            max_zoom: 32.0,
        }
    }

    /// Reads the initial frame from a camera (so a figure picks up its scene's
    /// own framing rather than a guess).
    pub fn from_camera(cam: &Camera2D) -> Self {
        let mut s = Self::new(cam.frame_width, cam.frame_height);
        s.center = cam.frame_center;
        s
    }

    /// Sets the zoom clamp band (multiples of the base frame).
    pub fn with_zoom_range(mut self, min: f32, max: f32) -> Self {
        self.min_zoom = min.max(f32::EPSILON);
        self.max_zoom = max.max(self.min_zoom);
        self
    }

    /// The current zoom factor relative to the base frame (`>1` = zoomed in).
    pub fn zoom(&self) -> f32 {
        self.base_width / self.width
    }

    /// Returns to the initial framing.
    pub fn reset(&mut self) {
        self.center = Point::ZERO;
        self.width = self.base_width;
        self.height = self.base_height;
    }

    /// The scene point under an element-fraction position (`(0,0)` = top-left,
    /// y **down** — the convention [`PointerState::frac`] uses).
    ///
    /// [`PointerState::frac`]: crate::PointerState::frac
    pub fn scene_at(&self, (fx, fy): (f32, f32)) -> Point {
        Point::new(
            self.center.x + (fx - 0.5) * self.width,
            self.center.y + (0.5 - fy) * self.height,
            self.center.z,
        )
    }

    /// Applies one frame's gesture: zoom about its anchor, then pan.
    ///
    /// Zoom first, pan second, both against the *post-zoom* frame — the order
    /// matters because a pinch that spreads and slides at once should track the
    /// fingers, and the pan fraction is a fraction of what you can now see.
    pub fn apply(&mut self, g: GestureDelta) {
        if g.scale > 0.0 && g.scale != 1.0 {
            let anchor = self.scene_at(g.anchor);
            // Clamp in zoom space so both limits are honoured exactly.
            let wanted = (self.zoom() * g.scale).clamp(self.min_zoom, self.max_zoom);
            let new_width = self.base_width / wanted;
            let k = new_width / self.width;
            self.height *= k;
            self.width = new_width;
            // Keep the anchored scene point under the same element fraction.
            self.center = anchor + (self.center - anchor) * k;
        }
        if g.pan != (0.0, 0.0) {
            // Content follows the fingers, so the camera moves the other way;
            // fraction y grows down while scene y grows up, hence the flip.
            self.center.x -= g.pan.0 * self.width;
            self.center.y += g.pan.1 * self.height;
        }
    }

    /// Writes the frame into a camera.
    pub fn apply_to_camera(&self, cam: &mut Camera2D) {
        cam.frame_center = self.center;
        cam.frame_width = self.width;
        cam.frame_height = self.height;
    }
}

/// Pan/zoom driver for a live 2-D [`Figure`](crate::Figure) (FE-144).
///
/// Hold one in a [`LiveUpdater`] closure and call [`sync`](Self::sync) each
/// frame *before* the rest of the scene work; it consumes the frame's gesture
/// and writes the camera. Handle dragging is unaffected: the router reports
/// `pressed = false` for the whole two-finger gesture, so a [`DragSet`] simply
/// sees the press end.
///
/// [`DragSet`]: crate::DragSet
#[derive(Debug, Clone, Copy)]
pub struct PanZoom {
    state: Option<PanZoomState>,
    zoom_range: (f32, f32),
}

impl Default for PanZoom {
    fn default() -> Self {
        Self::new()
    }
}

impl PanZoom {
    /// A driver that adopts the scene camera's own framing on its first frame.
    pub fn new() -> Self {
        Self {
            state: None,
            zoom_range: (0.125, 32.0),
        }
    }

    /// Sets the zoom clamp band (multiples of the initial frame).
    pub fn with_zoom_range(mut self, min: f32, max: f32) -> Self {
        self.zoom_range = (min, max);
        self
    }

    /// The live frame state, once the first frame has adopted the camera.
    pub fn state(&self) -> Option<PanZoomState> {
        self.state
    }

    /// The current zoom factor (1.0 before the first frame).
    pub fn zoom(&self) -> f32 {
        self.state.map(|s| s.zoom()).unwrap_or(1.0)
    }

    /// Consumes `pointer`'s gesture for this frame and applies it to the scene
    /// camera. Returns whether the view actually changed (so the caller can skip
    /// resampling anything view-dependent).
    pub fn sync(&mut self, state: &mut SceneState, pointer: &PointerState) -> bool {
        let pz = self.state.get_or_insert_with(|| {
            PanZoomState::from_camera(state.camera())
                .with_zoom_range(self.zoom_range.0, self.zoom_range.1)
        });
        let g = pointer.gesture;
        if g.is_identity() {
            return false;
        }
        pz.apply(g);
        pz.apply_to_camera(state.camera_mut());
        true
    }

    /// Restores the initial framing on the next frame.
    pub fn reset(&mut self) {
        if let Some(s) = &mut self.state {
            s.reset();
        }
    }

    /// A standalone [`LiveUpdater`] that does nothing but pan/zoom the camera —
    /// the 2-D counterpart of [`OrbitControls::updater`](crate::OrbitControls::updater).
    pub fn updater(self) -> LiveUpdater {
        let cell = std::cell::Cell::new(self);
        LiveUpdater::new(move |state, pointer, _t| {
            let mut pz = cell.get();
            pz.sync(state, pointer);
            cell.set(pz);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(scale: f32, pan: (f32, f32), anchor: (f32, f32)) -> GestureDelta {
        GestureDelta {
            scale,
            pan,
            anchor,
            active: true,
        }
    }

    #[test]
    fn zoom_about_the_center_keeps_the_center() {
        let mut pz = PanZoomState::new(8.0, 4.0);
        pz.apply(g(2.0, (0.0, 0.0), (0.5, 0.5)));
        assert!((pz.width - 4.0).abs() < 1e-5);
        assert!((pz.height - 2.0).abs() < 1e-5);
        assert_eq!(pz.center, Point::ZERO);
        assert!((pz.zoom() - 2.0).abs() < 1e-5);
    }

    #[test]
    fn zoom_about_a_corner_keeps_that_scene_point_under_the_finger() {
        let mut pz = PanZoomState::new(8.0, 4.0);
        let anchor = (0.25, 0.25);
        let before = pz.scene_at(anchor);
        pz.apply(g(3.0, (0.0, 0.0), anchor));
        let after = pz.scene_at(anchor);
        assert!(
            (before.x - after.x).abs() < 1e-4 && (before.y - after.y).abs() < 1e-4,
            "{before:?} vs {after:?}"
        );
    }

    #[test]
    fn zoom_out_widens_the_frame() {
        let mut pz = PanZoomState::new(8.0, 4.0);
        pz.apply(g(0.5, (0.0, 0.0), (0.5, 0.5)));
        assert!((pz.width - 16.0).abs() < 1e-4);
        assert!((pz.zoom() - 0.5).abs() < 1e-5);
    }

    #[test]
    fn zoom_is_clamped_both_ways() {
        let mut pz = PanZoomState::new(8.0, 4.0).with_zoom_range(0.5, 4.0);
        for _ in 0..20 {
            pz.apply(g(2.0, (0.0, 0.0), (0.5, 0.5)));
        }
        assert!((pz.zoom() - 4.0).abs() < 1e-4);
        for _ in 0..40 {
            pz.apply(g(0.5, (0.0, 0.0), (0.5, 0.5)));
        }
        assert!((pz.zoom() - 0.5).abs() < 1e-4);
        // The aspect survives the round trip.
        assert!((pz.width / pz.height - 2.0).abs() < 1e-4);
    }

    #[test]
    fn pan_moves_the_content_with_the_fingers() {
        let mut pz = PanZoomState::new(8.0, 4.0);
        // Fingers drag a quarter-width to the right → camera moves left.
        pz.apply(g(1.0, (0.25, 0.0), (0.5, 0.5)));
        assert!((pz.center.x + 2.0).abs() < 1e-5, "{:?}", pz.center);
        // Fingers drag down (frac y grows down) → camera moves up.
        pz.apply(g(1.0, (0.0, 0.5), (0.5, 0.5)));
        assert!((pz.center.y - 2.0).abs() < 1e-5, "{:?}", pz.center);
    }

    #[test]
    fn pan_is_a_fraction_of_the_zoomed_frame() {
        let mut pz = PanZoomState::new(8.0, 4.0);
        pz.apply(g(4.0, (0.0, 0.0), (0.5, 0.5))); // width 2
        pz.apply(g(1.0, (0.5, 0.0), (0.5, 0.5)));
        // Half of the *current* 2-unit frame, not of the original 8.
        assert!((pz.center.x + 1.0).abs() < 1e-4, "{:?}", pz.center);
    }

    #[test]
    fn a_combined_pinch_zooms_then_pans() {
        let mut pz = PanZoomState::new(8.0, 4.0);
        pz.apply(g(2.0, (0.1, 0.0), (0.5, 0.5)));
        assert!((pz.width - 4.0).abs() < 1e-5);
        assert!(
            (pz.center.x + 0.4).abs() < 1e-5,
            "pan uses the post-zoom width"
        );
    }

    #[test]
    fn the_identity_gesture_changes_nothing() {
        let mut pz = PanZoomState::new(8.0, 4.0);
        let before = pz;
        pz.apply(GestureDelta::default());
        assert_eq!(pz, before);
    }

    #[test]
    fn a_nonsense_scale_is_ignored() {
        let mut pz = PanZoomState::new(8.0, 4.0);
        pz.apply(g(0.0, (0.0, 0.0), (0.5, 0.5)));
        assert!((pz.width - 8.0).abs() < 1e-6);
    }

    #[test]
    fn reset_restores_the_initial_framing() {
        let mut pz = PanZoomState::new(8.0, 4.0);
        pz.apply(g(3.0, (0.3, -0.2), (0.2, 0.8)));
        pz.reset();
        assert_eq!(pz.center, Point::ZERO);
        assert!((pz.width - 8.0).abs() < 1e-6 && (pz.height - 4.0).abs() < 1e-6);
    }

    #[test]
    fn panzoom_adopts_the_scene_camera_and_writes_it_back() {
        let mut state = SceneState::new();
        state.camera_mut().frame_width = 10.0;
        state.camera_mut().frame_height = 5.0;
        let mut pz = PanZoom::new();
        let quiet = PointerState::default();
        assert!(
            !pz.sync(&mut state, &quiet),
            "an idle frame is not a view change"
        );
        let zoomed = PointerState {
            gesture: g(2.0, (0.0, 0.0), (0.5, 0.5)),
            ..PointerState::default()
        };
        assert!(pz.sync(&mut state, &zoomed));
        assert!((state.camera().frame_width - 5.0).abs() < 1e-4);
        assert!((pz.zoom() - 2.0).abs() < 1e-4);
    }
}
