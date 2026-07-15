//! Offline video and image-sequence export.
//!
//! [`VideoExporter`] renders a built [`Scene`](manim_core::scene::Scene) frame by
//! frame (via the deterministic [`Scene::frames`](manim_core::scene::Scene::frames)
//! sampler) through an offscreen [`OffscreenRenderer`], then either streams the
//! raw RGBA frames into an `ffmpeg` subprocess for an `.mp4`
//! ([`render_to_mp4`](VideoExporter::render_to_mp4)) or writes a numbered PNG
//! sequence ([`render_to_png_sequence`](VideoExporter::render_to_png_sequence)).
//!
//! `ffmpeg` must be on `PATH` for MP4 export; its absence is reported as
//! [`RenderError::FfmpegNotFound`] rather than a generic I/O error.
//!
//! ```no_run
//! use manim_core::config::Config;
//! use manim_core::prelude::*;
//! use manim_core::animations::Create;
//! use manim_render::export::VideoExporter;
//!
//! struct Dot;
//! impl SceneBuilder for Dot {
//!     fn construct(&self, scene: &mut Scene) -> manim_core::error::Result<()> {
//!         let c = scene.add(Circle::new());
//!         scene.play(Create::new(c))?;
//!         Ok(())
//!     }
//! }
//!
//! let config = Config::low();
//! let mut scene = Scene::build(&Dot, config.clone())?;
//! VideoExporter::render_to_mp4(&mut scene, "dot.mp4", &config)?;
//! # Ok::<(), manim_render::RenderError>(())
//! ```

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use manim_core::config::Config;
use manim_core::scene::Scene;

use crate::renderer::{OffscreenRenderer, RenderError};

/// Renders built scenes to video files or PNG sequences.
///
/// A stateless entry point; each call builds its own offscreen renderer sized to
/// the supplied [`Config`].
pub struct VideoExporter;

impl VideoExporter {
    /// Renders every frame of `scene` and pipes them into `ffmpeg`, writing an
    /// H.264 MP4 at `path`.
    ///
    /// Frames come from [`Scene::frames`] at the config's frame rate; each is
    /// rendered to RGBA and streamed to `ffmpeg`'s stdin as raw video. The
    /// tessellation cache is shared across frames, so static mobjects tessellate
    /// once.
    ///
    /// # Errors
    ///
    /// - [`RenderError::FfmpegNotFound`] if `ffmpeg` is not on `PATH`.
    /// - [`RenderError::FfmpegFailed`] if `ffmpeg` exits non-zero.
    /// - GPU/readback errors from the renderer.
    ///
    /// ```no_run
    /// use manim_core::config::Config;
    /// use manim_core::scene::Scene;
    /// # use manim_core::prelude::SceneBuilder;
    /// use manim_render::export::VideoExporter;
    /// # fn go(builder: &dyn SceneBuilder) -> Result<(), manim_render::RenderError> {
    /// let config = Config::low();
    /// let mut scene = Scene::build(builder, config.clone())?;
    /// VideoExporter::render_to_mp4(&mut scene, "out.mp4", &config)?;
    /// # Ok(()) }
    /// ```
    pub fn render_to_mp4(
        scene: &mut Scene,
        path: impl AsRef<Path>,
        config: &Config,
    ) -> Result<(), RenderError> {
        let path = path.as_ref();
        let mut renderer = OffscreenRenderer::new(config)?;
        let (w, h) = (config.pixel_width, config.pixel_height);

        let mut child = Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error"])
            // Raw RGBA input on stdin.
            .args(["-f", "rawvideo", "-pixel_format", "rgba"])
            .args(["-video_size", &format!("{w}x{h}")])
            .args(["-framerate", &config.fps.to_string()])
            .args(["-i", "-"])
            // H.264 output, widely-playable pixel format.
            .args(["-pix_fmt", "yuv420p"])
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    RenderError::FfmpegNotFound
                } else {
                    RenderError::Io(e)
                }
            })?;

        let mut stdin = child
            .stdin
            .take()
            .expect("ffmpeg stdin was piped and is available");

        // Collect frames first (frames() borrows the scene mutably), then render.
        let frames: Vec<_> = scene.frames().map(|(_, dl)| dl).collect();
        let mut result = Ok(());
        for list in &frames {
            let image = match renderer.render_display_list(list) {
                Ok(img) => img,
                Err(e) => {
                    result = Err(e);
                    break;
                }
            };
            if let Err(e) = stdin.write_all(image.as_raw()) {
                // A broken pipe means ffmpeg died; surface its status below.
                result = Err(RenderError::Io(e));
                break;
            }
        }
        drop(stdin); // Close ffmpeg's input so it finalizes the file.

        let status = child.wait()?;
        result?;
        if !status.success() {
            return Err(RenderError::FfmpegFailed(format!(
                "ffmpeg exited with {status}"
            )));
        }
        Ok(())
    }

    /// Renders every frame of `scene` to `dir/frame_00000.png`, `…001.png`, ….
    ///
    /// Creates `dir` if needed. Useful without `ffmpeg`, or as input to a custom
    /// encoder.
    ///
    /// # Errors
    ///
    /// I/O errors creating the directory or writing PNGs, or renderer errors.
    ///
    /// ```no_run
    /// use manim_core::config::Config;
    /// use manim_core::scene::Scene;
    /// # use manim_core::prelude::SceneBuilder;
    /// use manim_render::export::VideoExporter;
    /// # fn go(builder: &dyn SceneBuilder) -> Result<(), manim_render::RenderError> {
    /// let config = Config::low();
    /// let mut scene = Scene::build(builder, config.clone())?;
    /// VideoExporter::render_to_png_sequence(&mut scene, "frames/")?;
    /// # Ok(()) }
    /// ```
    pub fn render_to_png_sequence(
        scene: &mut Scene,
        dir: impl AsRef<Path>,
    ) -> Result<(), RenderError> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
        let mut renderer = OffscreenRenderer::new(scene.config())?;
        let frames: Vec<_> = scene.frames().map(|(_, dl)| dl).collect();
        for (i, list) in frames.iter().enumerate() {
            renderer.render_to_png(list, dir.join(format!("frame_{i:05}.png")))?;
        }
        Ok(())
    }
}

/// Whether `ffmpeg` is available on `PATH`.
///
/// Lets callers (and tests) skip MP4 export cleanly when it is missing.
///
/// ```no_run
/// use manim_render::export::ffmpeg_available;
/// if !ffmpeg_available() {
///     eprintln!("install ffmpeg for video export");
/// }
/// ```
pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}
