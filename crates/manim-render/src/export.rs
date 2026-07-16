//! Offline video and image-sequence export.
//!
//! [`VideoExporter`] renders a built [`Scene`] frame by
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
use manim_core::timeline::SoundCue;

use crate::renderer::{OffscreenRenderer, RenderError};

/// Builds the full `ffmpeg` argument list (everything after `ffmpeg`) for a
/// `duration`-second render into `output`, muxing `cues` into the audio track.
///
/// The video is raw RGBA on stdin (input `0`); each cue file is an extra input.
/// With no cues the arguments are identical to the video-only invocation (no
/// regression). With cues, a `filter_complex` delays each clip to its start time
/// (`adelay`), optionally scales it (`volume`), mixes them (`amix`), and pads the
/// result to the full `duration` (`apad=whole_dur`) so the audio track exactly
/// spans the video without truncating either.
///
/// ```
/// use std::path::Path;
/// use manim_core::config::Config;
/// use manim_core::timeline::SoundCue;
/// use manim_render::export::build_ffmpeg_args;
///
/// // No cues → the plain video-only command.
/// let args = build_ffmpeg_args(&[], &Config::low(), 2.0, Path::new("out.mp4"));
/// assert!(args.iter().any(|a| a == "rawvideo"));
/// assert!(!args.iter().any(|a| a == "-filter_complex"));
///
/// // One cue at t=1s → an extra input and an adelay of 1000 ms.
/// let cues = [SoundCue { path: "click.wav".into(), start: 1.0, gain: None }];
/// let args = build_ffmpeg_args(&cues, &Config::low(), 2.0, Path::new("out.mp4"));
/// let fc = args.iter().position(|a| a == "-filter_complex").unwrap();
/// assert!(args[fc + 1].contains("adelay=1000:all=1"));
/// assert!(args[fc + 1].contains("amix=inputs=1"));
/// ```
pub fn build_ffmpeg_args(
    cues: &[SoundCue],
    config: &Config,
    duration: f32,
    output: &Path,
) -> Vec<String> {
    use std::fmt::Write as _;

    let (w, h) = (config.pixel_width, config.pixel_height);
    let mut args: Vec<String> = [
        "-y",
        "-loglevel",
        "error",
        "-f",
        "rawvideo",
        "-pixel_format",
        "rgba",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push("-video_size".into());
    args.push(format!("{w}x{h}"));
    args.push("-framerate".into());
    args.push(config.fps.to_string());
    args.push("-i".into());
    args.push("-".into());

    // Each cue is an extra input after the raw-video stdin (input 0).
    for cue in cues {
        args.push("-i".into());
        args.push(cue.path.to_string_lossy().into_owned());
    }

    if !cues.is_empty() {
        let mut filter = String::new();
        for (i, cue) in cues.iter().enumerate() {
            let ms = (cue.start * 1000.0).round().max(0.0) as i64;
            let _ = write!(filter, "[{}:a]adelay={ms}:all=1", i + 1);
            if let Some(g) = cue.gain {
                let _ = write!(filter, ",volume={g}");
            }
            let _ = write!(filter, "[c{i}];");
        }
        for i in 0..cues.len() {
            let _ = write!(filter, "[c{i}]");
        }
        let dur = duration.max(0.0);
        let _ = write!(
            filter,
            "amix=inputs={}:normalize=0,apad=whole_dur={dur:.4}[aout]",
            cues.len()
        );
        args.push("-filter_complex".into());
        args.push(filter);
        args.push("-map".into());
        args.push("0:v".into());
        args.push("-map".into());
        args.push("[aout]".into());
        args.push("-c:a".into());
        args.push("aac".into());
    }

    // H.264 output, widely-playable pixel format.
    args.push("-pix_fmt".into());
    args.push("yuv420p".into());
    args.push(output.to_string_lossy().into_owned());
    args
}

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
    /// Scenes may schedule sounds with [`Scene::add_sound`](manim_core::scene::Scene::add_sound);
    /// when present, each is muxed into the output's audio track (see
    /// [`build_ffmpeg_args`]). With no sounds the `ffmpeg` command is unchanged.
    ///
    /// # Errors
    ///
    /// - [`RenderError::FfmpegNotFound`] if `ffmpeg` is not on `PATH`.
    /// - [`RenderError::SoundNotFound`] if a scheduled sound file is missing
    ///   (checked before `ffmpeg` is invoked).
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

        // Snapshot the sound cues (immutable borrow) before frame sampling
        // (mutable borrow), and fail early on any missing file.
        let cues: Vec<SoundCue> = scene.sound_cues().to_vec();
        for cue in &cues {
            if !cue.path.exists() {
                return Err(RenderError::SoundNotFound(cue.path.display().to_string()));
            }
        }

        // Collect frames first (the iterator borrows the scene mutably), then
        // render each following its camera.
        let frames: Vec<_> = scene.frames_with_camera().collect();
        let duration = frames.len() as f32 / config.fps.max(1) as f32;
        let args = build_ffmpeg_args(&cues, config, duration, path);

        let mut child = Command::new("ffmpeg")
            .args(&args)
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

        let mut result = Ok(());
        for frame in &frames {
            let image = match renderer.render_frame(frame) {
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
        let frames: Vec<_> = scene.frames_with_camera().collect();
        for (i, frame) in frames.iter().enumerate() {
            let image = renderer.render_frame(frame)?;
            image.save(dir.join(format!("frame_{i:05}.png")))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_cues_matches_video_only_command() {
        let args = build_ffmpeg_args(&[], &Config::low(), 2.0, Path::new("out.mp4"));
        // No audio wiring, single (stdin) input, output last.
        assert!(!args.iter().any(|a| a == "-filter_complex"));
        assert!(!args.iter().any(|a| a == "-map"));
        assert_eq!(args.iter().filter(|a| *a == "-i").count(), 1);
        assert!(args.iter().any(|a| a == "rawvideo"));
        assert_eq!(args.last().unwrap(), "out.mp4");
        // Ends with the pixel format then the path, as before.
        let pf = args.iter().position(|a| a == "-pix_fmt").unwrap();
        assert_eq!(args[pf + 1], "yuv420p");
    }

    #[test]
    fn cues_add_inputs_delay_gain_and_mix() {
        let cues = [
            SoundCue {
                path: "a.wav".into(),
                start: 0.0,
                gain: None,
            },
            SoundCue {
                path: "b.wav".into(),
                start: 1.25,
                gain: Some(0.5),
            },
        ];
        let args = build_ffmpeg_args(&cues, &Config::low(), 3.0, Path::new("out.mp4"));
        // 1 video (stdin) + 2 sound inputs.
        assert_eq!(args.iter().filter(|a| *a == "-i").count(), 3);
        assert!(args.iter().any(|a| a == "a.wav") && args.iter().any(|a| a == "b.wav"));

        let fc = args.iter().position(|a| a == "-filter_complex").unwrap();
        let f = &args[fc + 1];
        // Cue 0: input index 1, no delay, no gain.
        assert!(f.contains("[1:a]adelay=0:all=1[c0];"), "filter: {f}");
        // Cue 1: input index 2, 1250 ms delay, half volume.
        assert!(
            f.contains("[2:a]adelay=1250:all=1,volume=0.5[c1];"),
            "filter: {f}"
        );
        // Mixed and padded to the full duration.
        assert!(
            f.contains("[c0][c1]amix=inputs=2:normalize=0,apad=whole_dur=3.0000[aout]"),
            "filter: {f}"
        );
        // Video + mixed audio mapped, aac audio codec.
        assert!(args.windows(2).any(|w| w[0] == "-map" && w[1] == "0:v"));
        assert!(args.windows(2).any(|w| w[0] == "-map" && w[1] == "[aout]"));
        assert!(args.windows(2).any(|w| w[0] == "-c:a" && w[1] == "aac"));
        assert_eq!(args.last().unwrap(), "out.mp4");
    }

    #[test]
    fn negative_start_clamps_to_zero_delay() {
        let cues = [SoundCue {
            path: "x.wav".into(),
            start: -1.0,
            gain: None,
        }];
        let args = build_ffmpeg_args(&cues, &Config::low(), 1.0, Path::new("o.mp4"));
        let f = &args[args.iter().position(|a| a == "-filter_complex").unwrap() + 1];
        assert!(f.contains("adelay=0:all=1"), "filter: {f}");
    }
}
