//! Realtime preview window (winit + wgpu surface), behind the `preview` feature.
//!
//! [`RealtimePlayer`] opens a vsync'd window and plays a scene's precomputed
//! frames by wall clock. Frames are sampled once up front via
//! [`Scene::frames`](manim_core::scene::Scene::frames) (the same deterministic
//! sampler the offline path uses), so playback, scrubbing, and restarting are
//! just moving a playhead — no live re-seeking. The frame is letterboxed to the
//! scene's aspect ([`crate::layout`]) with background-color bars.
//!
//! Controls: **Space** play/pause, **←/→** seek ∓1 s, **R** restart, **Esc**
//! quit.
//!
//! This module is compiled only with `--features preview`; the offscreen path
//! and video export do not need a window system.
//!
//! ```no_run
//! use manim_core::config::Config;
//! use manim_core::scene::Scene;
//! # use manim_core::prelude::SceneBuilder;
//! use manim_render::preview::RealtimePlayer;
//!
//! # fn go(builder: &dyn SceneBuilder) -> Result<(), manim_render::RenderError> {
//! let mut scene = Scene::build(builder, Config::low())?;
//! RealtimePlayer::new(&mut scene).run()?;
//! # Ok(()) }
//! ```

use std::sync::Arc;
use std::time::Instant;

use manim_core::config::Config;
use manim_core::scene::{Frame, Scene};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::camera::Camera2D;
use crate::layout::letterbox;
use crate::renderer::{Pipeline, RenderError, SAMPLE_COUNT};
use crate::tessellate::TessellationCache;

/// Seconds moved per Left/Right arrow press.
const SEEK_STEP: f32 = 1.0;

/// A realtime scene player: a window that plays precomputed frames by wall clock.
///
/// Build with [`RealtimePlayer::new`] (which samples the scene's frames), then
/// [`run`](RealtimePlayer::run) to open the window and block until the user
/// quits.
pub struct RealtimePlayer {
    frames: Vec<Frame>,
    total: f32,
    config: Config,
    aspect: f32,
}

impl RealtimePlayer {
    /// Samples `scene` into frames and prepares a player.
    ///
    /// The scene is sampled once here; later playback never touches it, so the
    /// player owns everything it needs.
    ///
    /// ```no_run
    /// use manim_core::config::Config;
    /// use manim_core::scene::Scene;
    /// # use manim_core::prelude::SceneBuilder;
    /// use manim_render::preview::RealtimePlayer;
    /// # fn go(builder: &dyn SceneBuilder) -> Result<(), manim_render::RenderError> {
    /// let mut scene = Scene::build(builder, Config::low())?;
    /// let player = RealtimePlayer::new(&mut scene);
    /// player.run()?;
    /// # Ok(()) }
    /// ```
    pub fn new(scene: &mut Scene) -> Self {
        let config = scene.config().clone();
        // Frame aspect stays constant under camera zoom (width and height scale
        // together), so letterbox against the configured aspect.
        let aspect = config.frame_width / config.frame_height;
        let total = scene.total_duration();
        let frames: Vec<Frame> = scene.frames_with_camera().collect();
        Self {
            frames,
            total,
            config,
            aspect,
        }
    }

    /// Opens the preview window and blocks until the user quits.
    ///
    /// # Errors
    ///
    /// [`RenderError`] on event-loop or GPU-surface setup failure.
    pub fn run(self) -> Result<(), RenderError> {
        let event_loop = EventLoop::new()
            .map_err(|e| RenderError::NoDevice(format!("winit event loop: {e}")))?;
        event_loop.set_control_flow(ControlFlow::Poll);
        let mut app = PreviewApp {
            player: self,
            gfx: None,
            playhead: 0.0,
            playing: true,
            last: None,
            cache: TessellationCache::new(),
            error: None,
        };
        event_loop
            .run_app(&mut app)
            .map_err(|e| RenderError::NoDevice(format!("winit run: {e}")))?;
        match app.error.take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// The frame index nearest scene time `t` (clamped to the frame range).
    fn frame_index(&self, t: f32) -> usize {
        if self.frames.len() <= 1 {
            return 0;
        }
        let idx = (t * self.config.fps as f32).round() as isize;
        idx.clamp(0, self.frames.len() as isize - 1) as usize
    }
}

/// Live GPU state, created once the event loop hands us a window.
struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    pipeline: Pipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    zoom_uniform: wgpu::Buffer,
    zoom_bind_group: wgpu::BindGroup,
    border_uniform: wgpu::Buffer,
    border_bind_group: wgpu::BindGroup,
    msaa_view: wgpu::TextureView,
}

/// The winit application: player data plus optional live GPU state.
struct PreviewApp {
    player: RealtimePlayer,
    gfx: Option<Gfx>,
    playhead: f32,
    playing: bool,
    last: Option<Instant>,
    cache: TessellationCache,
    error: Option<RenderError>,
}

impl PreviewApp {
    /// Advances the playhead by wall-clock `dt` when playing, pausing at the end.
    fn tick(&mut self) {
        let now = Instant::now();
        let dt = self.last.map(|l| (now - l).as_secs_f32()).unwrap_or(0.0);
        self.last = Some(now);
        if self.playing {
            self.playhead += dt;
            if self.playhead >= self.player.total {
                self.playhead = self.player.total;
                self.playing = false;
            }
        }
    }

    /// Reconfigures the surface and MSAA texture for a new size.
    fn resize(&mut self, width: u32, height: u32) {
        if let Some(gfx) = &mut self.gfx {
            if width == 0 || height == 0 {
                return;
            }
            gfx.surface_config.width = width;
            gfx.surface_config.height = height;
            gfx.surface.configure(&gfx.device, &gfx.surface_config);
            gfx.msaa_view = make_msaa(&gfx.device, gfx.surface_config.format, width, height);
        }
    }

    /// Renders the current playhead frame, letterboxed, to the surface.
    fn render(&mut self) -> Result<(), RenderError> {
        let Some(gfx) = &self.gfx else {
            return Ok(());
        };
        let idx = self.player.frame_index(self.playhead);
        let frame_data = &self.player.frames[idx];
        let camera = Camera2D::from(&frame_data.camera);
        let background = frame_data.camera.background;
        self.cache.set_zoom(frame_data.camera.height);
        let mesh = self.cache.tessellate(&frame_data.display_list);

        // Update the camera uniform (follows the animated camera per frame).
        let view_proj = camera.view_proj().to_cols_array_2d();
        gfx.queue
            .write_buffer(&gfx.uniform, 0, bytemuck::cast_slice(&view_proj));

        let frame = match gfx.surface.get_current_texture() {
            Ok(f) => f,
            // Surface lost/outdated: reconfigure and skip this frame.
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                gfx.surface.configure(&gfx.device, &gfx.surface_config);
                return Ok(());
            }
            Err(e) => return Err(RenderError::Readback(e.to_string())),
        };
        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let (out_w, out_h) = (gfx.surface_config.width, gfx.surface_config.height);
        let vp = letterbox(out_w as f32, out_h as f32, self.player.aspect);

        let mut encoder = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("preview encoder"),
            });
        if let Some(zw) = frame_data.camera.zoom_window {
            // A magnifying inset: second pass with a zoom camera + border.
            let inset =
                crate::layout::inset_viewport(vp, zw.inset_x, zw.inset_y, zw.inset_w, zw.inset_h);
            let (zw_w, zw_h) = crate::layout::zoom_frame_size(zw.region_width, inset.w, inset.h);
            let zoom_cam = Camera2D {
                frame_center: zw.region_center,
                frame_width: zw_w,
                frame_height: zw_h,
                rotation: 0.0,
                three_d: None,
            };
            gfx.queue.write_buffer(
                &gfx.zoom_uniform,
                0,
                bytemuck::cast_slice(&zoom_cam.view_proj().to_cols_array_2d()),
            );
            gfx.queue.write_buffer(
                &gfx.border_uniform,
                0,
                bytemuck::cast_slice(&glam::Mat4::IDENTITY.to_cols_array_2d()),
            );
            let border = crate::renderer::border_mesh_ndc(
                inset,
                out_w,
                out_h,
                zw.border_width,
                zw.border_color,
            );
            crate::renderer::record_draw_zoomed(
                &gfx.device,
                &mut encoder,
                &gfx.pipeline.pipeline,
                &gfx.msaa_view,
                &target,
                &gfx.bind_group,
                &gfx.zoom_bind_group,
                &gfx.border_bind_group,
                &mesh,
                &border,
                background,
                vp,
                inset,
                out_w,
                out_h,
                false,
            );
        } else {
            crate::renderer::record_draw(
                &gfx.device,
                &mut encoder,
                &gfx.pipeline.pipeline,
                &gfx.msaa_view,
                &target,
                &gfx.bind_group,
                &mesh,
                background,
                Some(vp),
            );
        }
        gfx.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

/// Creates a multisampled color texture view for the surface.
fn make_msaa(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("preview msaa"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

impl PreviewApp {
    /// Brings up the window, surface, and pipeline. Records any error and asks
    /// the loop to exit on failure.
    fn init(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        match self.build_gfx(event_loop) {
            Ok(gfx) => self.gfx = Some(gfx),
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn build_gfx(&self, event_loop: &ActiveEventLoop) -> Result<Gfx, RenderError> {
        let (init_w, init_h) = (
            self.player.config.pixel_width,
            self.player.config.pixel_height,
        );
        let attrs = Window::default_attributes()
            .with_title("manim preview")
            .with_inner_size(winit::dpi::LogicalSize::new(init_w, init_h));
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .map_err(|e| RenderError::NoDevice(format!("create window: {e}")))?,
        );

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| RenderError::NoAdapter(format!("create surface: {e}")))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|e| RenderError::NoAdapter(e.to_string()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("preview device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        }))
        .map_err(|e| RenderError::NoDevice(e.to_string()))?;

        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        let caps = surface.get_capabilities(&adapter);
        // Prefer an sRGB format so the pipeline's linear colors encode correctly.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: w,
            height: h,
            present_mode: wgpu::PresentMode::Fifo, // vsync
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let pipeline = Pipeline::new(&device, format);
        let (uniform, bind_group) = camera_bind_group(&device, &pipeline, "preview camera");
        let (zoom_uniform, zoom_bind_group) =
            camera_bind_group(&device, &pipeline, "preview zoom camera");
        let (border_uniform, border_bind_group) =
            camera_bind_group(&device, &pipeline, "preview border");
        let msaa_view = make_msaa(&device, format, w, h);

        Ok(Gfx {
            window,
            surface,
            device,
            queue,
            surface_config,
            pipeline,
            uniform,
            bind_group,
            zoom_uniform,
            zoom_bind_group,
            border_uniform,
            border_bind_group,
            msaa_view,
        })
    }
}

/// Creates a camera uniform buffer and its `@group(0)` bind group.
fn camera_bind_group(
    device: &wgpu::Device,
    pipeline: &Pipeline,
    label: &str,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout: &pipeline.bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    (uniform, bind_group)
}

impl ApplicationHandler for PreviewApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.init(event_loop);
        self.last = Some(Instant::now());
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => self.on_key(event_loop, code),
            WindowEvent::RedrawRequested => {
                self.tick();
                if let Err(e) = self.render() {
                    self.error = Some(e);
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(gfx) = &self.gfx {
            gfx.window.request_redraw();
        }
    }
}

impl PreviewApp {
    /// Handles a key press: transport controls.
    fn on_key(&mut self, event_loop: &ActiveEventLoop, code: KeyCode) {
        match code {
            KeyCode::Escape => event_loop.exit(),
            KeyCode::Space => {
                // Toggle; restart from the top if paused at the end.
                if !self.playing && self.playhead >= self.player.total {
                    self.playhead = 0.0;
                }
                self.playing = !self.playing;
            }
            KeyCode::ArrowLeft => {
                self.playhead = (self.playhead - SEEK_STEP).max(0.0);
            }
            KeyCode::ArrowRight => {
                self.playhead = (self.playhead + SEEK_STEP).min(self.player.total);
            }
            KeyCode::KeyR => {
                self.playhead = 0.0;
                self.playing = true;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_core::prelude::*;

    fn player() -> RealtimePlayer {
        let mut scene = Scene::new(Config::low());
        let c = scene.add(Circle::new());
        scene.play(manim_core::animations::Create::new(c)).unwrap();
        RealtimePlayer::new(&mut scene)
    }

    #[test]
    fn frame_index_clamps_to_range() {
        let p = player();
        assert_eq!(p.frame_index(-5.0), 0);
        let last = p.frames.len() - 1;
        assert_eq!(p.frame_index(1e6), last);
    }

    #[test]
    fn frame_index_tracks_time() {
        let p = player();
        // At t=0 we get frame 0; near the end we get the last frame.
        assert_eq!(p.frame_index(0.0), 0);
        assert!(p.frame_index(p.total) >= p.frames.len().saturating_sub(1));
    }
}
