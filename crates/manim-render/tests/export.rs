//! Video-export smoke test.
//!
//! Renders a tiny animation at `Config::low()` to a temporary MP4 and asserts
//! the file exists and is non-trivial. Skips (with a notice) when `ffmpeg` or a
//! GPU adapter is unavailable, so it is safe in headless/CI environments.

use manim_core::animations::TransformInto;
use manim_core::config::Config;
use manim_core::prelude::*;
use manim_render::export::{ffmpeg_available, VideoExporter};
use manim_render::renderer::GpuContext;

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

#[test]
fn render_to_mp4_smoke() {
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
