//! Rendering back-end for the asset harness: a still is one frame sampled at a
//! time code; a clip is a frame span streamed into `ffmpeg`.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use manim_core::config::Config;
use manim_core::scene::{Frame, Scene};
use manim_render::export::build_ffmpeg_args;
use manim_render::renderer::{OffscreenRenderer, RenderError};

/// Anything that can go wrong rendering one manifest entry.
#[derive(Debug)]
pub enum HarnessError {
    /// The scene failed to construct (a broken example).
    Build(String),
    /// The renderer or an I/O step failed.
    Render(String),
    /// `ffmpeg` is needed for this entry but is not on `PATH`.
    NoFfmpeg,
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Build(m) => write!(f, "scene construction failed: {m}"),
            Self::Render(m) => write!(f, "render failed: {m}"),
            Self::NoFfmpeg => write!(f, "ffmpeg not found on PATH (needed for clips)"),
        }
    }
}

impl From<RenderError> for HarnessError {
    fn from(e: RenderError) -> Self {
        Self::Render(e.to_string())
    }
}

impl From<std::io::Error> for HarnessError {
    fn from(e: std::io::Error) -> Self {
        Self::Render(e.to_string())
    }
}

/// Builds an [`OffscreenRenderer`], or `None` when no GPU adapter is available.
///
/// Mirrors the golden tests: `REQUIRE_GPU=1` (which CI sets, backed by a
/// software rasterizer) turns a missing adapter into a hard failure, so an asset
/// job can never "succeed" by quietly rendering nothing.
pub fn try_renderer(config: &Config) -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new(config) {
        Ok(r) => {
            let info = r.context().adapter_info();
            eprintln!(
                "render-examples: {:?} backend, adapter {:?}",
                info.backend, info.name
            );
            Some(r)
        }
        Err(e) => {
            if std::env::var("REQUIRE_GPU").is_ok_and(|v| v != "0" && !v.is_empty()) {
                panic!(
                    "REQUIRE_GPU is set but no GPU adapter is available ({e}); \
                     install a software rasterizer (e.g. mesa lavapipe) or unset REQUIRE_GPU"
                );
            }
            eprintln!("SKIP render-examples: no GPU adapter available ({e})");
            None
        }
    }
}

/// Samples every frame of a freshly built scene.
///
/// The whole timeline is materialised because [`Scene::frames_with_camera`]
/// borrows the scene mutably; stills then index into it and clips slice it.
fn frames(
    builder: &dyn manim_core::prelude::SceneBuilder,
    config: &Config,
) -> Result<Vec<Frame>, HarnessError> {
    let mut scene =
        Scene::build(builder, config.clone()).map_err(|e| HarnessError::Build(format!("{e:?}")))?;
    let frames: Vec<Frame> = scene.frames_with_camera().collect();
    if frames.is_empty() {
        return Err(HarnessError::Build("empty timeline".into()));
    }
    Ok(frames)
}

/// Renders a single frame at (or nearest to) `t` seconds and writes a PNG.
///
/// `t` is clamped to the timeline, so `t: f32::INFINITY` is a legitimate way to
/// ask for "the final frame".
pub fn still(
    builder: &dyn manim_core::prelude::SceneBuilder,
    config: &Config,
    t: f32,
    out: &Path,
    renderer: &mut OffscreenRenderer,
) -> Result<(), HarnessError> {
    let frames = frames(builder, config)?;
    // Clamp into the timeline first: without this, `t = INFINITY` ("the final
    // frame") makes every |frame.t - t| equal to infinity, and the nearest-frame
    // search below degenerates to picking frame 0.
    let t = t.clamp(0.0, frames.last().map_or(0.0, |f| f.t));
    // Nearest sampled frame to the requested time code.
    let pick = frames
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (a.t - t)
                .abs()
                .partial_cmp(&(b.t - t).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0);

    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let image = renderer.render_frame(&frames[pick])?;
    image
        .save(out)
        .map_err(|e| HarnessError::Render(e.to_string()))?;
    eprintln!(
        "  still t={:.2}s (frame {pick}/{}) -> {}",
        frames[pick].t,
        frames.len() - 1,
        out.display()
    );
    Ok(())
}

/// Renders the frames in `[t0, t1]` and muxes them into an H.264 MP4.
///
/// Frames are piped to `ffmpeg` as raw RGBA, reusing
/// [`build_ffmpeg_args`] so the encoder settings match the main exporter.
pub fn clip(
    builder: &dyn manim_core::prelude::SceneBuilder,
    config: &Config,
    t0: f32,
    t1: f32,
    out: &Path,
    renderer: &mut OffscreenRenderer,
) -> Result<(), HarnessError> {
    if !ffmpeg_on_path() {
        return Err(HarnessError::NoFfmpeg);
    }
    let all = frames(builder, config)?;
    let span: Vec<&Frame> = all.iter().filter(|f| f.t >= t0 && f.t <= t1).collect();
    // A window past the end of a short timeline still yields something to encode.
    let span = if span.is_empty() {
        all.iter().collect()
    } else {
        span
    };

    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let duration = span.len() as f32 / config.fps.max(1) as f32;
    let args = build_ffmpeg_args(&[], config, duration, out);

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                HarnessError::NoFfmpeg
            } else {
                HarnessError::Render(e.to_string())
            }
        })?;
    let mut stdin = child.stdin.take().expect("ffmpeg stdin was piped");

    let mut result = Ok(());
    for frame in &span {
        match renderer.render_frame(frame) {
            Ok(image) => {
                if let Err(e) = stdin.write_all(image.as_raw()) {
                    result = Err(HarnessError::Render(e.to_string()));
                    break;
                }
            }
            Err(e) => {
                result = Err(e.into());
                break;
            }
        }
    }
    drop(stdin); // Close ffmpeg's input so it finalises the file.

    let status = child.wait()?;
    result?;
    if !status.success() {
        return Err(HarnessError::Render(format!("ffmpeg exited with {status}")));
    }
    eprintln!(
        "  clip {t0:.2}–{t1:.2}s ({} frames, {duration:.2}s @ {}fps) -> {}",
        span.len(),
        config.fps,
        out.display()
    );
    Ok(())
}

/// Whether `ffmpeg` is available on `PATH`.
pub fn ffmpeg_on_path() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
