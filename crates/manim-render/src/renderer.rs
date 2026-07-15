//! The wgpu backend: GPU context, render pipeline, offscreen texture target.
//!
//! The rendering path is deliberately small. [`GpuContext::new_headless`] brings
//! up an adapter/device with no window (software adapters like llvmpipe work).
//! A single [pipeline](Pipeline) draws premultiplied-alpha triangles through a
//! trivial shader into a 4× MSAA texture, resolved to an
//! [`Rgba8UnormSrgb`](wgpu::TextureFormat::Rgba8UnormSrgb) [`TextureTarget`] and
//! read back to an [`image::RgbaImage`]. [`OffscreenRenderer`] ties it together:
//! give it a [`Config`] and a [`DisplayList`], get a PNG.
//!
//! The draw path renders into any [`wgpu::TextureView`]
//! ([`Pipeline::draw`]), so a windowed `SurfaceTarget` can be added later
//! without touching the pipeline (tracked as FE-95).
//!
//! wasm targets cannot block on the device; a future async constructor will
//! mirror [`GpuContext::new_headless`] for them.
//!
//! ```no_run
//! use manim_core::config::Config;
//! use manim_core::geometry::Circle;
//! use manim_core::scene_state::SceneState;
//! use manim_render::renderer::OffscreenRenderer;
//!
//! let mut scene = SceneState::new();
//! scene.add(Circle::new());
//!
//! let mut renderer = OffscreenRenderer::new(&Config::low()).unwrap();
//! let image = renderer.render_display_list(&scene.display_list()).unwrap();
//! assert_eq!(image.width(), 854);
//! ```

use image::RgbaImage;
use manim_color::Color;
use manim_core::config::Config;
use manim_core::display::DisplayList;
use wgpu::util::DeviceExt;

use crate::camera::Camera2D;
use crate::tessellate::{FrameMesh, TessellationCache, Vertex};

/// Multisample count for the MSAA render target (4× is broadly supported).
pub const SAMPLE_COUNT: u32 = 4;

/// The offscreen texture format: 8-bit RGBA, sRGB-encoded on store.
///
/// Vertex and clear colors are linear; the GPU blends in linear space and
/// encodes to sRGB when writing this target, so readback bytes are display-ready
/// PNG pixels.
pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Errors from bringing up or driving the GPU.
///
/// ```
/// use manim_render::renderer::RenderError;
/// let e = RenderError::NoAdapter("none".into());
/// assert!(e.to_string().contains("adapter"));
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RenderError {
    /// No GPU adapter could be acquired (no drivers, no software fallback).
    #[error("no suitable GPU adapter: {0}")]
    NoAdapter(String),
    /// An adapter was found but a device/queue could not be created.
    #[error("failed to create GPU device: {0}")]
    NoDevice(String),
    /// Mapping the readback buffer failed.
    #[error("failed to read back rendered pixels: {0}")]
    Readback(String),
    /// The readback bytes did not form a valid image buffer.
    #[error("rendered pixels did not form a valid image")]
    InvalidImage,
    /// Writing a PNG (or other I/O) failed.
    #[error("i/o error writing image: {0}")]
    Io(#[from] std::io::Error),
    /// Encoding the output image failed.
    #[error("image encode error: {0}")]
    Image(#[from] image::ImageError),
    /// `ffmpeg` was not found on `PATH` (needed for video export).
    #[error("ffmpeg not found on PATH; install ffmpeg to export video")]
    FfmpegNotFound,
    /// `ffmpeg` ran but exited with a failure status.
    #[error("ffmpeg failed: {0}")]
    FfmpegFailed(String),
    /// Building the scene (running its `construct`) failed.
    #[error(transparent)]
    Core(#[from] manim_core::error::CoreError),
}

/// The trivial vertex+fragment shader: transform by the camera, pass color.
const SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(in.position, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Colors are already premultiplied linear; the pipeline blends them with
    // (One, OneMinusSrcAlpha) and the sRGB target encodes on store.
    return in.color;
}
"#;

/// The camera uniform block: a single `mat4x4<f32>`.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

impl From<&Camera2D> for CameraUniform {
    fn from(camera: &Camera2D) -> Self {
        Self {
            view_proj: camera.view_proj().to_cols_array_2d(),
        }
    }
}

/// A headless (windowless) wgpu context: instance, adapter, device, queue.
///
/// Construct one with [`GpuContext::new_headless`]; it drives every offscreen
/// render. The device is created with downlevel default limits so software and
/// GL-backend adapters (llvmpipe/lavapipe) qualify.
///
/// ```no_run
/// use manim_render::renderer::GpuContext;
/// let ctx = GpuContext::new_headless().unwrap();
/// println!("rendering on {}", ctx.backend_name());
/// ```
pub struct GpuContext {
    /// The wgpu instance the adapter came from.
    pub instance: wgpu::Instance,
    /// The selected adapter.
    pub adapter: wgpu::Adapter,
    /// The logical device.
    pub device: wgpu::Device,
    /// The command queue.
    pub queue: wgpu::Queue,
}

impl GpuContext {
    /// Brings up a headless context, preferring a high-performance adapter but
    /// accepting any (including software) so it works in CI without a GPU.
    ///
    /// Blocks internally via [`pollster`]; do not call from an async runtime's
    /// thread. wasm needs the (future) async constructor instead.
    ///
    /// # Errors
    ///
    /// [`RenderError::NoAdapter`] if no adapter is available at all, or
    /// [`RenderError::NoDevice`] if the adapter cannot create a device.
    pub fn new_headless() -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| RenderError::NoAdapter(e.to_string()))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("manim-render device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        }))
        .map_err(|e| RenderError::NoDevice(e.to_string()))?;

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }

    /// The human-readable name of the active backend (e.g. `"Vulkan"`, `"Gl"`).
    ///
    /// ```no_run
    /// use manim_render::renderer::GpuContext;
    /// let ctx = GpuContext::new_headless().unwrap();
    /// assert!(!ctx.backend_name().is_empty());
    /// ```
    pub fn backend_name(&self) -> String {
        format!("{:?}", self.adapter.get_info().backend)
    }

    /// The full adapter info (name, backend, device type, driver).
    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.adapter.get_info()
    }
}

/// The single render pipeline plus its camera bind-group layout.
///
/// Reusable across targets: [`Pipeline::draw`] records a pass into any
/// [`wgpu::TextureView`], which is what lets a window surface reuse it later.
pub struct Pipeline {
    /// The compiled render pipeline.
    pub pipeline: wgpu::RenderPipeline,
    /// The bind-group layout for the camera uniform (`@group(0)`).
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl Pipeline {
    /// Builds the pipeline for `format` with premultiplied-alpha blending and
    /// [`SAMPLE_COUNT`]× MSAA.
    ///
    /// ```no_run
    /// use manim_render::renderer::{GpuContext, Pipeline, TARGET_FORMAT};
    /// let ctx = GpuContext::new_headless().unwrap();
    /// let pipeline = Pipeline::new(&ctx.device, TARGET_FORMAT);
    /// let _ = pipeline;
    /// ```
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("manim-render shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("manim-render camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("manim-render pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        const ATTRS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        };

        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("manim-render pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layout,
        }
    }

    /// Records a clear-and-draw pass for `mesh` into `msaa_view`, resolving to
    /// `resolve_view`.
    ///
    /// `bind_group` must bind the camera uniform at `@group(0)`. An empty mesh
    /// still clears to `background`. This is the surface-agnostic core: pass any
    /// resolve target (offscreen texture today, swapchain view tomorrow).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        msaa_view: &wgpu::TextureView,
        resolve_view: &wgpu::TextureView,
        bind_group: &wgpu::BindGroup,
        mesh: &FrameMesh,
        background: Color,
    ) {
        record_draw(
            device,
            encoder,
            &self.pipeline,
            msaa_view,
            resolve_view,
            bind_group,
            mesh,
            background,
        );
    }
}

/// The clear-and-draw pass shared by [`Pipeline::draw`] and [`TextureTarget`].
#[allow(clippy::too_many_arguments)]
fn record_draw(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    msaa_view: &wgpu::TextureView,
    resolve_view: &wgpu::TextureView,
    bind_group: &wgpu::BindGroup,
    mesh: &FrameMesh,
    background: Color,
) {
    let bg = background.premultiplied();
    let buffers = (!mesh.indices.is_empty()).then(|| {
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("manim-render vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("manim-render indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        (vb, ib)
    });

    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("manim-render pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: msaa_view,
            resolve_target: Some(resolve_view),
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
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });

    if let Some((vb, ib)) = &buffers {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.indices.len() as u32, 0, 0..1);
    }
}

/// Rounds `value` up to the next multiple of `align`.
fn align_up(value: u32, align: u32) -> u32 {
    value.div_ceil(align) * align
}

/// An offscreen render target: an MSAA texture resolved to an sRGB texture, plus
/// a padded readback buffer, sized once and reused frame to frame.
///
/// [`TextureTarget::render`] draws a [`FrameMesh`] and returns the pixels as an
/// [`image::RgbaImage`]. It owns clones of the device/queue/pipeline (wgpu
/// handles are cheap, reference-counted clones), so it is self-contained.
pub struct TextureTarget {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    color_texture: wgpu::Texture,
    color_view: wgpu::TextureView,
    msaa_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_bytes_per_row: u32,
    unpadded_bytes_per_row: u32,
}

impl TextureTarget {
    /// Allocates a `width × height` target driven by `pipeline`.
    ///
    /// ```no_run
    /// use manim_render::renderer::{GpuContext, Pipeline, TextureTarget, TARGET_FORMAT};
    /// let ctx = GpuContext::new_headless().unwrap();
    /// let pipeline = Pipeline::new(&ctx.device, TARGET_FORMAT);
    /// let target = TextureTarget::new(&ctx.device, &ctx.queue, &pipeline, 854, 480);
    /// assert_eq!(target.size(), (854, 480));
    /// ```
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &Pipeline,
        width: u32,
        height: u32,
    ) -> Self {
        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("manim-render color target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let msaa_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("manim-render msaa target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("manim-render camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("manim-render camera bind group"),
            layout: &pipeline.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let unpadded_bytes_per_row = width * 4;
        let padded_bytes_per_row =
            align_up(unpadded_bytes_per_row, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("manim-render readback"),
            size: (padded_bytes_per_row * height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device: device.clone(),
            queue: queue.clone(),
            pipeline: pipeline.pipeline.clone(),
            uniform_buffer,
            bind_group,
            width,
            height,
            color_texture,
            color_view,
            msaa_view,
            readback,
            padded_bytes_per_row,
            unpadded_bytes_per_row,
        }
    }

    /// The target size in pixels, `(width, height)`.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Renders `mesh` under `camera` over `background`, returning the pixels.
    ///
    /// Clears to `background`, draws the mesh (premultiplied blending, MSAA),
    /// resolves, and reads the sRGB pixels back into an [`RgbaImage`].
    ///
    /// # Errors
    ///
    /// [`RenderError::Readback`] if buffer mapping fails, or
    /// [`RenderError::InvalidImage`] if the bytes do not form an image.
    pub fn render(
        &mut self,
        mesh: &FrameMesh,
        camera: &Camera2D,
        background: Color,
    ) -> Result<RgbaImage, RenderError> {
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform::from(camera)),
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("manim-render encoder"),
            });

        record_draw(
            &self.device,
            &mut encoder,
            &self.pipeline,
            &self.msaa_view,
            &self.color_view,
            &self.bind_group,
            mesh,
            background,
        );

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.color_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(Some(encoder.finish()));

        // Map the readback buffer and wait for the GPU.
        let slice = self.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        self.device
            .poll(wgpu::PollType::Wait)
            .map_err(|e| RenderError::Readback(e.to_string()))?;
        rx.recv()
            .map_err(|e| RenderError::Readback(e.to_string()))?
            .map_err(|e| RenderError::Readback(e.to_string()))?;

        // Strip row padding into a tight RGBA buffer.
        let data = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((self.unpadded_bytes_per_row * self.height) as usize);
        for row in 0..self.height {
            let start = (row * self.padded_bytes_per_row) as usize;
            let end = start + self.unpadded_bytes_per_row as usize;
            pixels.extend_from_slice(&data[start..end]);
        }
        drop(data);
        self.readback.unmap();

        RgbaImage::from_raw(self.width, self.height, pixels).ok_or(RenderError::InvalidImage)
    }
}

/// The high-level offscreen renderer: context + pipeline + cache + target.
///
/// One per output size. [`render_display_list`](Self::render_display_list)
/// tessellates (with generation caching) and rasterizes a [`DisplayList`];
/// [`render_to_png`](Self::render_to_png) writes the result to disk.
///
/// ```no_run
/// use manim_core::config::Config;
/// use manim_core::geometry::Square;
/// use manim_core::scene_state::SceneState;
/// use manim_render::renderer::OffscreenRenderer;
///
/// let mut scene = SceneState::new();
/// scene.add(Square::new());
/// let mut renderer = OffscreenRenderer::new(&Config::low()).unwrap();
/// renderer.render_to_png(&scene.display_list(), "/tmp/square.png").unwrap();
/// ```
pub struct OffscreenRenderer {
    context: GpuContext,
    cache: TessellationCache,
    target: TextureTarget,
    camera: Camera2D,
    background: Color,
}

impl OffscreenRenderer {
    /// Builds a renderer sized and colored by `config`.
    ///
    /// # Errors
    ///
    /// Propagates [`GpuContext::new_headless`] failures ([`RenderError::NoAdapter`]
    /// / [`RenderError::NoDevice`]).
    ///
    /// ```no_run
    /// use manim_core::config::Config;
    /// use manim_render::renderer::OffscreenRenderer;
    /// let renderer = OffscreenRenderer::new(&Config::low()).unwrap();
    /// assert_eq!(renderer.size(), (854, 480));
    /// ```
    pub fn new(config: &Config) -> Result<Self, RenderError> {
        let context = GpuContext::new_headless()?;
        let pipeline = Pipeline::new(&context.device, TARGET_FORMAT);
        let target = TextureTarget::new(
            &context.device,
            &context.queue,
            &pipeline,
            config.pixel_width,
            config.pixel_height,
        );
        Ok(Self {
            context,
            cache: TessellationCache::new(),
            target,
            camera: Camera2D::from(config),
            background: config.background_color,
        })
    }

    /// The output size in pixels, `(width, height)`.
    pub fn size(&self) -> (u32, u32) {
        self.target.size()
    }

    /// The GPU context (backend/adapter introspection).
    pub fn context(&self) -> &GpuContext {
        &self.context
    }

    /// The camera; mutate it to pan/zoom/roll between frames.
    pub fn camera_mut(&mut self) -> &mut Camera2D {
        &mut self.camera
    }

    /// Tessellates and renders `list` to an [`RgbaImage`].
    ///
    /// Unchanged mobjects reuse cached tessellation across calls, so animating a
    /// scene only re-tessellates what moved.
    ///
    /// # Errors
    ///
    /// Propagates [`TextureTarget::render`] failures.
    pub fn render_display_list(&mut self, list: &DisplayList) -> Result<RgbaImage, RenderError> {
        let mesh = self.cache.tessellate(list);
        self.target.render(&mesh, &self.camera, self.background)
    }

    /// Renders `list` and writes it to `path` as a PNG.
    ///
    /// # Errors
    ///
    /// Propagates render failures, or [`RenderError::Image`] / [`RenderError::Io`]
    /// on encode/write failure.
    pub fn render_to_png(
        &mut self,
        list: &DisplayList,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(), RenderError> {
        let image = self.render_display_list(list)?;
        image.save(path)?;
        Ok(())
    }
}
