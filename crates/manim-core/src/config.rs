//! Scene configuration, mirroring manim CE's `config` defaults.
//!
//! Unlike manim CE there is no global mutable config: a [`Config`] is a plain
//! value passed to the scene runtime, which keeps tests hermetic.

use manim_color::Color;
use manim_math::{FRAME_HEIGHT, FRAME_WIDTH};

/// Rendering and playback configuration.
///
/// Defaults match manim CE: a 14.222 × 8 scene-unit frame, 1920×1080 pixels,
/// 60 fps, black background.
///
/// ```
/// use manim_core::config::Config;
/// let config = Config::default();
/// assert_eq!(config.frame_height, 8.0);
/// assert_eq!(config.fps, 60);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Height of the visible frame in scene units.
    pub frame_height: f32,
    /// Width of the visible frame in scene units.
    pub frame_width: f32,
    /// Output height in pixels.
    pub pixel_height: u32,
    /// Output width in pixels.
    pub pixel_width: u32,
    /// Frames per second for offline rendering and playback.
    pub fps: u32,
    /// Background color of the scene.
    pub background_color: Color,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            frame_height: FRAME_HEIGHT,
            frame_width: FRAME_WIDTH,
            pixel_height: 1080,
            pixel_width: 1920,
            fps: 60,
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 1.0),
        }
    }
}

impl Config {
    /// manim CE's `-ql` quality preset: 854×480 at 15 fps.
    ///
    /// ```
    /// use manim_core::config::Config;
    /// assert_eq!(Config::low().pixel_width, 854);
    /// ```
    pub fn low() -> Self {
        Self {
            pixel_width: 854,
            pixel_height: 480,
            fps: 15,
            ..Self::default()
        }
    }

    /// manim CE's `-qm` quality preset: 1280×720 at 30 fps.
    pub fn medium() -> Self {
        Self {
            pixel_width: 1280,
            pixel_height: 720,
            fps: 30,
            ..Self::default()
        }
    }

    /// manim CE's `-qh` quality preset: 1920×1080 at 60 fps.
    pub fn high() -> Self {
        Self::default()
    }

    /// manim CE's `-qk` quality preset: 3840×2160 at 60 fps.
    pub fn fourk() -> Self {
        Self {
            pixel_width: 3840,
            pixel_height: 2160,
            fps: 60,
            ..Self::default()
        }
    }

    /// Sets the background color (builder style).
    ///
    /// ```
    /// use manim_core::config::Config;
    /// use manim_color::Color;
    /// let c = Config::default().background(Color::from_rgba(1.0, 1.0, 1.0, 1.0));
    /// assert_eq!(c.background_color.r, 1.0);
    /// ```
    pub fn background(mut self, color: Color) -> Self {
        self.background_color = color;
        self
    }

    /// Seconds per frame at this configuration's [`fps`](Self::fps).
    pub fn frame_dt(&self) -> f32 {
        1.0 / self.fps as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_manim_ce() {
        let c = Config::default();
        assert_eq!(c.frame_height, 8.0);
        assert!((c.frame_width - 8.0 * 16.0 / 9.0).abs() < 1e-6);
        assert_eq!((c.pixel_width, c.pixel_height), (1920, 1080));
    }

    #[test]
    fn presets() {
        assert_eq!(Config::low().fps, 15);
        assert_eq!(Config::medium().pixel_height, 720);
        assert_eq!(Config::fourk().pixel_width, 3840);
    }
}
