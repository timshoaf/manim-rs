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
use manim_core::scene::Frame;
use web_sys::HtmlCanvasElement;

use crate::camera::Camera2D;
use crate::layout::letterbox;
use crate::mesh_pipeline::{
    create_depth_view, MeshBufferCache, MeshGlobals, MeshPipeline, SceneLight,
};
use crate::renderer::{record_draw_over, Pipeline, RenderError, SAMPLE_COUNT};
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
    /// Magnifying-camera uniform + identity (border) uniform for a zoom window.
    zoom_uniform: wgpu::Buffer,
    zoom_bind_group: wgpu::BindGroup,
    border_uniform: wgpu::Buffer,
    border_bind_group: wgpu::BindGroup,
    msaa_view: wgpu::TextureView,
    /// The depth-tested mesh pass. Everything here stays idle — the depth
    /// texture is allocated but never attached — until a display list actually
    /// carries meshes.
    mesh_pipeline: MeshPipeline,
    mesh_cache: MeshBufferCache,
    depth_view: wgpu::TextureView,
    mesh_globals: wgpu::Buffer,
    mesh_globals_bind_group: wgpu::BindGroup,
    light: SceneLight,
    cache: TessellationCache,
    camera: Camera2D,
    aspect: f32,
    background: Color,
    /// The active zoom window (from the last `render_frame`), or `None`.
    zoom_window: Option<manim_core::camera::ZoomWindow>,
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
        let (uniform, bind_group) = make_camera_bind_group(&device, &pipeline, "canvas camera");
        let (zoom_uniform, zoom_bind_group) =
            make_camera_bind_group(&device, &pipeline, "canvas zoom camera");
        let (border_uniform, border_bind_group) =
            make_camera_bind_group(&device, &pipeline, "canvas border");
        let msaa_view = make_msaa(&device, format, width, height);

        let mesh_pipeline = MeshPipeline::new(&device, format, SAMPLE_COUNT);
        let (mesh_globals, mesh_globals_bind_group) =
            mesh_pipeline.make_globals(&device, "canvas mesh globals");
        let depth_view = create_depth_view(&device, width, height, SAMPLE_COUNT);

        Ok(Self {
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
            mesh_pipeline,
            mesh_cache: MeshBufferCache::new(),
            depth_view,
            mesh_globals,
            mesh_globals_bind_group,
            light: SceneLight::default(),
            cache: TessellationCache::new(),
            camera: Camera2D::from(config),
            aspect: config.frame_width / config.frame_height,
            background: config.background_color,
            zoom_window: None,
        })
    }

    /// Reconfigures the surface (and the MSAA + depth targets) for a new canvas
    /// size.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        self.msaa_view = make_msaa(&self.device, self.surface_config.format, width, height);
        self.depth_view = create_depth_view(&self.device, width, height, SAMPLE_COUNT);
    }

    /// Mutable access to the camera (pan/zoom/roll between frames).
    pub fn camera_mut(&mut self) -> &mut Camera2D {
        &mut self.camera
    }

    /// The directional light the mesh pass shades with.
    pub fn light(&self) -> SceneLight {
        self.light
    }

    /// Sets the directional light the mesh pass shades with.
    pub fn set_light(&mut self, light: SceneLight) {
        self.light = light;
    }

    /// Converts a pointer position in element (client) pixels to scene
    /// coordinates, inverting the current letterbox fit and camera projection.
    ///
    /// `client_x`/`client_y` are relative to the canvas element's top-left, and
    /// `elem_w`/`elem_h` are its displayed size (CSS or backing pixels — the fit
    /// is scale-invariant). Returns `None` for a degenerate (zero-sized) fit. See
    /// [`layout::client_to_scene`](crate::layout::client_to_scene).
    pub fn client_to_scene(
        &self,
        client_x: f32,
        client_y: f32,
        elem_w: f32,
        elem_h: f32,
    ) -> Option<glam::Vec3> {
        crate::layout::client_to_scene(
            client_x,
            client_y,
            elem_w,
            elem_h,
            self.aspect,
            self.camera.view_proj(),
        )
    }

    /// Tessellates and draws `list` into the canvas, letterboxed with
    /// background-color bars.
    ///
    /// A list carrying [`meshes`](DisplayList::meshes) runs the depth-tested
    /// mesh pass first and composites the vector content over it; a list without
    /// them draws exactly as it always has.
    ///
    /// # Errors
    ///
    /// [`RenderError::Readback`] if the swapchain texture cannot be acquired for
    /// a reason other than a recoverable lost/outdated surface (which is
    /// reconfigured and skipped).
    pub fn render(&mut self, list: &DisplayList) -> Result<(), RenderError> {
        let mesh = self.cache.tessellate(list);
        let mesh_frame = if list.meshes().is_empty() {
            crate::mesh_pipeline::MeshFrame::default()
        } else {
            self.queue.write_buffer(
                &self.mesh_globals,
                0,
                bytemuck::bytes_of(&MeshGlobals::new(&self.camera, self.light)),
            );
            self.mesh_cache.prepare(
                &self.device,
                &self.mesh_pipeline,
                list.meshes(),
                &self.camera,
            )
        };

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

        let (out_w, out_h) = (self.surface_config.width, self.surface_config.height);
        let vp = letterbox(out_w as f32, out_h as f32, self.aspect);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("canvas encoder"),
            });

        // The mesh pass clears color + depth over the whole surface, then the
        // vector pass loads its result instead of clearing. Skipped entirely
        // when there are no meshes, so a 2-D frame is unchanged.
        let drew_meshes = !mesh_frame.is_empty();
        if drew_meshes {
            let bg = self.background.premultiplied();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas mesh pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg[0] as f64,
                            g: bg[1] as f64,
                            b: bg[2] as f64,
                            a: bg[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if vp.w > 0.0 && vp.h > 0.0 {
                pass.set_viewport(vp.x, vp.y, vp.w, vp.h, 0.0, 1.0);
            }
            mesh_frame.record(
                &mut pass,
                &self.mesh_pipeline,
                &self.mesh_globals_bind_group,
            );
        }

        if let Some(zw) = self.zoom_window {
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
            self.queue.write_buffer(
                &self.zoom_uniform,
                0,
                bytemuck::cast_slice(&zoom_cam.view_proj().to_cols_array_2d()),
            );
            self.queue.write_buffer(
                &self.border_uniform,
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
                &self.device,
                &mut encoder,
                &self.pipeline.pipeline,
                &self.msaa_view,
                &target,
                &self.bind_group,
                &self.zoom_bind_group,
                &self.border_bind_group,
                &mesh,
                &border,
                self.background,
                vp,
                inset,
                out_w,
                out_h,
                drew_meshes,
            );
        } else {
            record_draw_over(
                &self.device,
                &mut encoder,
                &self.pipeline.pipeline,
                &self.msaa_view,
                &target,
                &self.bind_group,
                &mesh,
                self.background,
                Some(vp),
                drew_meshes,
            );
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    /// Renders a [`Frame`], following its camera (center/zoom/rotation) and
    /// background — the web analogue of
    /// [`OffscreenRenderer::render_frame`](crate::renderer::OffscreenRenderer::render_frame).
    ///
    /// Adopts the frame's camera and background and adapts the tessellation
    /// tolerance to the zoom, then draws. Use this to follow an animated camera
    /// in the browser.
    ///
    /// # Errors
    ///
    /// As [`render`](Self::render).
    pub fn render_frame(&mut self, frame: &Frame) -> Result<(), RenderError> {
        self.camera = Camera2D::from(&frame.camera);
        self.background = frame.camera.background;
        self.zoom_window = frame.camera.zoom_window;
        self.cache.set_zoom(frame.camera.height);
        self.render(&frame.display_list)
    }
}

/// Creates a camera uniform buffer and its `@group(0)` bind group.
fn make_camera_bind_group(
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
