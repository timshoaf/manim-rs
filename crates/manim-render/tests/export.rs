//! Video-export smoke test.
//!
//! Renders a tiny animation at `Config::low()` to a temporary MP4 and asserts
//! the file exists and is non-trivial. Skips (with a notice) when `ffmpeg` or a
//! GPU adapter is unavailable, so it is safe in headless/CI environments.

#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use manim_core::animations::TransformInto;
use manim_core::config::Config;
use manim_core::prelude::*;
use manim_render::export::{ffmpeg_available, VideoExporter};
use manim_render::renderer::{GpuContext, RenderError};

/// Serializes the GPU-backed tests: creating multiple headless wgpu contexts
/// concurrently can segfault in some drivers, so these tests take this lock
/// (a no-op cost when run single-threaded).
static GPU_LOCK: Mutex<()> = Mutex::new(());

/// A minimal square→circle animation.
struct SquareToCircle;

impl SceneBuilder for SquareToCircle {
    fn construct(&self, scene: &mut Scene) -> manim_core::error::Result<()> {
        let sq = scene.add(Square::new().with_fill(BLUE, 0.7));
        scene.play(TransformInto::new(sq, Circle::new().with_fill(RED, 0.7)))?;
        scene.wait(0.2);
        Ok(())
    }
}

/// Writes a minimal 16-bit PCM mono WAV of `secs` seconds (a 440 Hz sine) — no
/// audio-crate dependency, just the RIFF header plus samples.
fn write_wav(path: &Path, secs: f32) {
    let rate: u32 = 8000;
    let n = (rate as f32 * secs) as u32;
    let data_len = n * 2; // 16-bit mono
    let mut buf: Vec<u8> = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // audio format: PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // channels: mono
    buf.extend_from_slice(&rate.to_le_bytes()); // sample rate
    buf.extend_from_slice(&(rate * 2).to_le_bytes()); // byte rate
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let s = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 8000.0;
        buf.extend_from_slice(&(s as i16).to_le_bytes());
    }
    std::fs::write(path, buf).unwrap();
}

/// Whether `path` has an audio stream, per `ffmpeg -i` (which prints stream info
/// to stderr, e.g. `Stream #0:1 ... Audio: aac`).
fn has_audio_stream(path: &Path) -> bool {
    match Command::new("ffmpeg").arg("-i").arg(path).output() {
        Ok(o) => String::from_utf8_lossy(&o.stderr).contains("Audio:"),
        Err(_) => false,
    }
}

/// A scene that schedules two sounds around a transform.
struct Sounded {
    a: PathBuf,
    b: PathBuf,
}

impl SceneBuilder for Sounded {
    fn construct(&self, scene: &mut Scene) -> manim_core::error::Result<()> {
        let sq = scene.add(Square::new().with_fill(BLUE, 0.7));
        scene.add_sound(self.a.clone()); // at t = 0
        scene.play(TransformInto::new(sq, Circle::new().with_fill(RED, 0.7)))?;
        scene.add_sound(self.b.clone()); // at the end of the play
        scene.wait(0.2);
        Ok(())
    }
}

#[test]
fn render_to_mp4_smoke() {
    let _gpu = GPU_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !ffmpeg_available() {
        eprintln!("SKIP render_to_mp4_smoke: ffmpeg not found on PATH");
        return;
    }
    if GpuContext::new_headless().is_err() {
        eprintln!("SKIP render_to_mp4_smoke: no GPU adapter available");
        return;
    }

    let config = Config::low();
    let mut scene = Scene::build(&SquareToCircle, config.clone()).unwrap();

    let path = std::env::temp_dir().join(format!("manim_render_smoke_{}.mp4", std::process::id()));
    let _ = std::fs::remove_file(&path);

    VideoExporter::render_to_mp4(&mut scene, &path, &config).expect("mp4 export");

    let meta = std::fs::metadata(&path).expect("output mp4 exists");
    assert!(
        meta.len() > 1024,
        "mp4 suspiciously small: {} bytes",
        meta.len()
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn render_to_mp4_muxes_sound() {
    let _gpu = GPU_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !ffmpeg_available() {
        eprintln!("SKIP render_to_mp4_muxes_sound: ffmpeg not found on PATH");
        return;
    }
    if GpuContext::new_headless().is_err() {
        eprintln!("SKIP render_to_mp4_muxes_sound: no GPU adapter available");
        return;
    }

    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let wav_a = dir.join(format!("manim_snd_a_{pid}.wav"));
    let wav_b = dir.join(format!("manim_snd_b_{pid}.wav"));
    write_wav(&wav_a, 0.2);
    write_wav(&wav_b, 0.2);

    let config = Config::low();
    let mut scene = Scene::build(
        &Sounded {
            a: wav_a.clone(),
            b: wav_b.clone(),
        },
        config.clone(),
    )
    .unwrap();
    assert_eq!(scene.sound_cues().len(), 2, "two cues scheduled");

    let out = dir.join(format!("manim_snd_out_{pid}.mp4"));
    let _ = std::fs::remove_file(&out);
    VideoExporter::render_to_mp4(&mut scene, &out, &config).expect("mp4 export with sound");

    assert!(
        has_audio_stream(&out),
        "exported mp4 has no audio stream (cues were dropped)"
    );

    for p in [&out, &wav_a, &wav_b] {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn render_to_mp4_reports_missing_sound() {
    let _gpu = GPU_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if GpuContext::new_headless().is_err() {
        eprintln!("SKIP render_to_mp4_reports_missing_sound: no GPU adapter available");
        return;
    }

    struct BadSound;
    impl SceneBuilder for BadSound {
        fn construct(&self, scene: &mut Scene) -> manim_core::error::Result<()> {
            scene.add(Square::new().with_fill(BLUE, 1.0));
            scene.add_sound("/definitely/not/a/real/sound.wav");
            scene.wait(0.1);
            Ok(())
        }
    }

    let config = Config::low();
    let mut scene = Scene::build(&BadSound, config.clone()).unwrap();
    let out = std::env::temp_dir().join(format!("manim_badsnd_{}.mp4", std::process::id()));
    let _ = std::fs::remove_file(&out);

    let err = VideoExporter::render_to_mp4(&mut scene, &out, &config).unwrap_err();
    assert!(
        matches!(err, RenderError::SoundNotFound(_)),
        "expected SoundNotFound, got {err:?}"
    );
    // Must fail before ffmpeg writes anything.
    assert!(
        !out.exists(),
        "no output should be produced on a missing sound"
    );
}
