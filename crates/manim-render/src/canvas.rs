//! Browser canvas rendering (`wasm32` + `web` feature).
//!
//! [`CanvasSurface`] wraps a wgpu surface created from an
//! [`HtmlCanvasElement`](web_sys::HtmlCanvasElement) and draws a
//! [`DisplayList`] into it, letterboxed to the scene's aspect. It is the wasm
//! analogue of the native offscreen/preview renderers: same tessellation and
//! [`Pipeline`](crate::renderer::Pipeline), a surface target instead of a
//! texture or window.
//!
//! Construction is async ([`GpuContext`](crate::renderer::GpuContext) can't
//! block on the web); drive it from a `requestAnimationFrame` loop via
//! `wasm_bindgen_futures`.
//!
//! ```no_run
//! # #[cfg(all(target_arch = "wasm32", feature = "web"))]
//! # async fn go(canvas: web_sys::HtmlCanvasElement, list: &manim_core::display::DisplayList) {
//! use manim_core::config::Config;
//! use manim_render::canvas::CanvasSurface;
//!
//! let mut surface = CanvasSurface::new(canvas, &Config::low()).await.unwrap();
//! surface.render(list).unwrap();
//! # }
//! ```

use manim_color::Color;
use manim_core::config::Config;
use manim_core::display::DisplayList;
use web_sys::HtmlCanvasElement;

use crate::camera::Camera2D;
use crate::layout::letterbox;
use crate::renderer::{record_draw, Pipeline, RenderError, SAMPLE_COUNT};
use crate::tessellate::TessellationCache;

/// Installs [`console_error_panic_hook`] so Rust panics surface in the browser
/// console with a readable message and stack. Call once at startup.
///
/// ```no_run
/// manim_render::canvas::set_panic_hook();
/// ```
pub fn set_panic_hook() {
    console_error_panic_hook::set_once();
}

/// A wgpu surface bound to an HTML `<canvas>`, rendering display lists in the
/// browser.
///
/// Holds its own device/queue/pipeline plus a [`TessellationCache`], so a
/// `requestAnimationFrame` loop can call [`render`](Self::render) each frame and
/// only re-tessellate changed mobjects.
pub struct CanvasSurface {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    pipeline: Pipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    msaa_view: wgpu::TextureView,
    cache: TessellationCache,
    camera: Camera2D,
    aspect: f32,
    background: Color,
}

impl CanvasSurface {
    /// Creates a surface from `canvas`, sized and colored by `config`.
    ///
    /// Requests a browser adapter compatible with the canvas and a device with
    /// WebGL2-downlevel limits (so it runs on both the WebGPU and WebGL
    /// backends). Uses an sRGB surface format when available so the pipeline's
    /// linear colors encode correctly.
    ///
    /// # Errors
    ///
    /// [`RenderError::NoAdapter`] / [`RenderError::NoDevice`] if the browser
    /// cannot provide a GPU surface, adapter, or device.
    pub async fn new(canvas: HtmlCanvasElement, config: &Config) -> Result<Self, RenderError> {
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| RenderError::NoAdapter(format!("create canvas surface: {e}")))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| RenderError::NoAdapter(e.to_string()))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("manim-render canvas device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                ..Default::default()
            })
            .await
            .map_err(|e| RenderError::NoDevice(e.to_string()))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let pipeline = Pipeline::new(&device, format);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("canvas camera uniform"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canvas camera bind group"),
            layout: &pipeline.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let msaa_view = make_msaa(&device, format, width, height);

        Ok(Self {
            surface,
            device,
            queue,
            surface_config,
            pipeline,
            uniform,
            bind_group,
            msaa_view,
            cache: TessellationCache::new(),
            camera: Camera2D::from(config),
            aspect: config.frame_width / config.frame_height,
            background: config.background_color,
        })
    }

    /// Reconfigures the surface (and MSAA target) for a new canvas size.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        self.msaa_view = make_msaa(&self.device, self.surface_config.format, width, height);
    }

    /// Mutable access to the camera (pan/zoom/roll between frames).
    pub fn camera_mut(&mut self) -> &mut Camera2D {
        &mut self.camera
    }

    /// Tessellates and draws `list` into the canvas, letterboxed with
    /// background-color bars.
    ///
    /// # Errors
    ///
    /// [`RenderError::Readback`] if the swapchain texture cannot be acquired for
    /// a reason other than a recoverable lost/outdated surface (which is
    /// reconfigured and skipped).
    pub fn render(&mut self, list: &DisplayList) -> Result<(), RenderError> {
        let mesh = self.cache.tessellate(list);

        let view_proj = self.camera.view_proj().to_cols_array_2d();
        self.queue
            .write_buffer(&self.uniform, 0, bytemuck::cast_slice(&view_proj));

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(());
            }
            Err(e) => return Err(RenderError::Readback(e.to_string())),
        };
        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let vp = letterbox(
            self.surface_config.width as f32,
            self.surface_config.height as f32,
            self.aspect,
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("canvas encoder"),
            });
        record_draw(
            &self.device,
            &mut encoder,
            &self.pipeline.pipeline,
            &self.msaa_view,
            &target,
            &self.bind_group,
            &mesh,
            self.background,
            Some(vp),
        );
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

/// Creates a multisampled color texture view for the canvas surface.
fn make_msaa(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("canvas msaa"),
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
