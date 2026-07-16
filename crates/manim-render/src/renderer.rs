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
#[cfg(not(target_arch = "wasm32"))]
use manim_core::config::Config;
#[cfg(not(target_arch = "wasm32"))]
use manim_core::display::DisplayList;
use wgpu::util::DeviceExt;

use crate::camera::Camera2D;
#[cfg(not(target_arch = "wasm32"))]
use crate::tessellate::TessellationCache;
use crate::tessellate::{FrameMesh, Vertex};

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
    /// A scheduled sound file (from `Scene::add_sound`) does not exist; checked
    /// before invoking `ffmpeg`.
    #[error("sound file not found: {0}")]
    SoundNotFound(String),
    /// Building the scene (running its `construct`) failed.
    #[error(transparent)]
    Core(#[from] manim_core::error::CoreError),
}

/// The trivial vertex+fragment shader: transform by the camera, pass color.
const SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(in.position, 1.0);
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
    /// Brings up a surfaceless context asynchronously, preferring a
    /// high-performance adapter but accepting any (including software).
    ///
    /// This is the portable constructor: native code reaches it through the
    /// blocking [`new_headless`](Self::new_headless), and wasm callers `.await`
    /// it (there is no blocking on the web's single thread).
    ///
    /// # Errors
    ///
    /// [`RenderError::NoAdapter`] if no adapter is available at all, or
    /// [`RenderError::NoDevice`] if the adapter cannot create a device.
    ///
    /// ```no_run
    /// # async fn demo() -> Result<(), manim_render::RenderError> {
    /// use manim_render::renderer::GpuContext;
    /// let ctx = GpuContext::new_async().await?;
    /// let _ = ctx;
    /// # Ok(()) }
    /// ```
    pub async fn new_async() -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| RenderError::NoAdapter(e.to_string()))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("manim-render device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                ..Default::default()
            })
            .await
            .map_err(|e| RenderError::NoDevice(e.to_string()))?;

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }

    /// Brings up a headless context, blocking on [`new_async`](Self::new_async)
    /// via [`pollster`]. Native-only — wasm has no blocking; use
    /// [`new_async`](Self::new_async) there.
    ///
    /// Do not call from an async runtime's thread (it blocks).
    ///
    /// # Errors
    ///
    /// [`RenderError::NoAdapter`] / [`RenderError::NoDevice`], as
    /// [`new_async`](Self::new_async).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_headless() -> Result<Self, RenderError> {
        pollster::block_on(Self::new_async())
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
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
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
            None,
        );
    }
}

/// The clear-and-draw pass shared by [`Pipeline::draw`], [`TextureTarget`], and
/// the surface renderers (preview / canvas).
///
/// When `viewport` is `Some`, the draw is confined to that pixel rectangle
/// (letterboxing); the clear still covers the whole attachment, so the margins
/// show `background`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_draw(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    msaa_view: &wgpu::TextureView,
    resolve_view: &wgpu::TextureView,
    bind_group: &wgpu::BindGroup,
    mesh: &FrameMesh,
    background: Color,
    viewport: Option<crate::layout::Viewport>,
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
        if let Some(vp) = viewport {
            if vp.w <= 0.0 || vp.h <= 0.0 {
                return;
            }
            pass.set_viewport(vp.x, vp.y, vp.w, vp.h, 0.0, 1.0);
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.indices.len() as u32, 0, 0..1);
    }
}

/// Draws `mesh` with `main_bg` over `base`, then again scissored into `inset`
/// with `zoom_bg` (the magnifier), then `border` with `border_bg` — the mesh-path
/// analogue of [`TextureTarget::render_ops_zoomed`], shared by the surface
/// renderers (canvas / preview). Clears to `background` over the whole attachment.
#[cfg(any(
    all(feature = "preview", not(target_arch = "wasm32")),
    all(feature = "web", target_arch = "wasm32")
))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_draw_zoomed(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    msaa_view: &wgpu::TextureView,
    resolve_view: &wgpu::TextureView,
    main_bg: &wgpu::BindGroup,
    zoom_bg: &wgpu::BindGroup,
    border_bg: &wgpu::BindGroup,
    mesh: &FrameMesh,
    border: &FrameMesh,
    background: Color,
    base: crate::layout::Viewport,
    inset: crate::layout::Viewport,
    out_w: u32,
    out_h: u32,
) {
    let bg = background.premultiplied();
    let make = |m: &FrameMesh| {
        (!m.indices.is_empty()).then(|| {
            let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("manim-render zoom vertices"),
                contents: bytemuck::cast_slice(&m.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("manim-render zoom indices"),
                contents: bytemuck::cast_slice(&m.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            (vb, ib, m.indices.len() as u32)
        })
    };
    let main_buf = make(mesh);
    let border_buf = make(border);

    let sx = inset.x.max(0.0).round() as u32;
    let sy = inset.y.max(0.0).round() as u32;
    let sw = (inset.w.round() as u32).min(out_w.saturating_sub(sx));
    let sh = (inset.h.round() as u32).min(out_h.saturating_sub(sy));

    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("manim-render zoom pass"),
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

    pass.set_pipeline(pipeline);
    if let Some((vb, ib, count)) = &main_buf {
        // Main pass, over the letterboxed base viewport.
        if base.w > 0.0 && base.h > 0.0 {
            pass.set_viewport(base.x, base.y, base.w, base.h, 0.0, 1.0);
        }
        pass.set_bind_group(0, main_bg, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..*count, 0, 0..1);
        // Zoom pass, confined to the inset.
        if sw > 0 && sh > 0 {
            pass.set_viewport(inset.x, inset.y, inset.w, inset.h, 0.0, 1.0);
            pass.set_scissor_rect(sx, sy, sw, sh);
            pass.set_bind_group(0, zoom_bg, &[]);
            pass.draw_indexed(0..*count, 0, 0..1);
            pass.set_viewport(0.0, 0.0, out_w as f32, out_h as f32, 0.0, 1.0);
            pass.set_scissor_rect(0, 0, out_w, out_h);
        }
    }
    if let Some((vb, ib, count)) = &border_buf {
        // Border in NDC (identity view-projection), full attachment.
        pass.set_bind_group(0, border_bg, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..*count, 0, 0..1);
    }
}

/// Rounds `value` up to the next multiple of `align`.
fn align_up(value: u32, align: u32) -> u32 {
    value.div_ceil(align) * align
}

/// Appends an axis-aligned quad (two triangles) spanning `(x0, y0)`–`(x1, y1)`
/// with solid premultiplied `col` to `mesh`.
fn push_quad(mesh: &mut FrameMesh, col: [f32; 4], x0: f32, y0: f32, x1: f32, y1: f32) {
    let base = mesh.vertices.len() as u32;
    for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
        mesh.vertices.push(Vertex {
            position: [x, y, 0.0],
            color: col,
        });
    }
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Builds the four border bars framing `inset` (in output pixels) as a mesh in
/// **NDC** clip space — drawn with an identity view-projection. The border sits
/// just inside the inset edges so it frames the magnified content.
pub(crate) fn border_mesh_ndc(
    inset: crate::layout::Viewport,
    out_w: u32,
    out_h: u32,
    thickness_px: f32,
    color: Color,
) -> FrameMesh {
    let (ow, oh) = (out_w.max(1) as f32, out_h.max(1) as f32);
    let to_ndc_x = |px: f32| px / ow * 2.0 - 1.0;
    let to_ndc_y = |py: f32| 1.0 - py / oh * 2.0;
    let xo_l = to_ndc_x(inset.x);
    let xo_r = to_ndc_x(inset.x + inset.w);
    let yo_t = to_ndc_y(inset.y);
    let yo_b = to_ndc_y(inset.y + inset.h);
    let tx = thickness_px / ow * 2.0;
    let ty = thickness_px / oh * 2.0;
    let col = color.premultiplied();

    let mut mesh = FrameMesh::default();
    // NDC y grows upward, so the top edge has the larger y.
    push_quad(&mut mesh, col, xo_l, yo_t, xo_r, yo_t - ty); // top bar
    push_quad(&mut mesh, col, xo_l, yo_b + ty, xo_r, yo_b); // bottom bar
    push_quad(&mut mesh, col, xo_l, yo_t - ty, xo_l + tx, yo_b + ty); // left bar
    push_quad(&mut mesh, col, xo_r - tx, yo_t - ty, xo_r, yo_b + ty); // right bar
    mesh
}

/// The textured-quad shader for [`ImageMobject`](manim_core::image_mobject::ImageMobject)s.
const IMAGE_SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VsIn { @location(0) pos: vec3<f32>, @location(1) uv: vec2<f32> };
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(in.pos, 1.0);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // sRGB texture → linear rgb, straight alpha; premultiply for (One, 1-srcA).
    let c = textureSample(tex, samp, in.uv);
    return vec4<f32>(c.rgb * c.a, c.a);
}
"#;

/// A textured quad vertex: position + UV.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ImageVertex {
    position: [f32; 3],
    uv: [f32; 2],
}

/// The image (textured-quad) pipeline, sharing the camera bind-group layout at
/// `@group(0)` with the vector [`Pipeline`] and adding a texture+sampler layout
/// at `@group(1)`.
struct ImagePipeline {
    pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
}

impl ImagePipeline {
    fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("manim-render image shader"),
            source: wgpu::ShaderSource::Wgsl(IMAGE_SHADER.into()),
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("manim-render image texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("manim-render image pipeline layout"),
            bind_group_layouts: &[camera_layout, &texture_layout],
            push_constant_ranges: &[],
        });
        const ATTRS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ImageVertex>() as wgpu::BufferAddress,
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
            label: Some("manim-render image pipeline"),
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
            texture_layout,
        }
    }
}

/// A cached uploaded texture, keyed by the source mobject's generation.
struct CachedTexture {
    generation: u64,
    bind_group: wgpu::BindGroup,
}

/// A GPU-resident draw op: uploaded vertex/index buffers for one vector batch or
/// one textured image quad. Built by [`TextureTarget::build_gpu_ops`] and
/// recorded by [`TextureTarget::record_ops`]; the buffers must outlive the pass.
enum GpuOp {
    /// A vector triangle batch.
    Vector {
        vb: wgpu::Buffer,
        ib: wgpu::Buffer,
        count: u32,
    },
    /// A textured quad (two triangles), keyed to a cached texture by `source`.
    Image {
        vb: wgpu::Buffer,
        ib: wgpu::Buffer,
        source: manim_core::mobject::AnyId,
    },
}

/// An offscreen render target: an MSAA texture resolved to an sRGB texture, plus
/// a padded readback buffer, sized once and reused frame to frame.
///
/// [`TextureTarget::render_ops`] draws a z-ordered list of vector batches and
/// textured quads and returns the pixels as an [`image::RgbaImage`]. It owns
/// clones of the device/queue/pipeline (wgpu handles are cheap, reference-counted
/// clones), so it is self-contained.
pub struct TextureTarget {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    image_pipeline: ImagePipeline,
    texture_cache: std::collections::HashMap<manim_core::mobject::AnyId, CachedTexture>,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// A second camera uniform holding the orthographic matrix, used to draw
    /// `fixed_in_frame` HUD content under a 3-D camera, and (as an identity
    /// matrix) the zoom-window border in NDC space.
    hud_uniform_buffer: wgpu::Buffer,
    hud_bind_group: wgpu::BindGroup,
    /// A third camera uniform holding the magnifying camera for a zoom window.
    zoom_uniform_buffer: wgpu::Buffer,
    zoom_bind_group: wgpu::BindGroup,
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

        let hud_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("manim-render hud camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let hud_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("manim-render hud camera bind group"),
            layout: &pipeline.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: hud_uniform_buffer.as_entire_binding(),
            }],
        });

        let zoom_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("manim-render zoom camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let zoom_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("manim-render zoom camera bind group"),
            layout: &pipeline.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: zoom_uniform_buffer.as_entire_binding(),
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

        let image_pipeline = ImagePipeline::new(device, &pipeline.bind_group_layout, TARGET_FORMAT);

        Self {
            device: device.clone(),
            queue: queue.clone(),
            pipeline: pipeline.pipeline.clone(),
            image_pipeline,
            texture_cache: std::collections::HashMap::new(),
            uniform_buffer,
            bind_group,
            hud_uniform_buffer,
            hud_bind_group,
            zoom_uniform_buffer,
            zoom_bind_group,
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

    /// Ensures the texture for image quad `q` is uploaded and cached (keyed by
    /// source generation), then returns its texture bind group.
    fn ensure_texture(&mut self, q: &crate::tessellate::ImageQuad) {
        if let Some(c) = self.texture_cache.get(&q.source) {
            if c.generation == q.generation {
                return;
            }
        }
        let data = &q.paint.data;
        let (w, h) = (data.width.max(1), data.height.max(1));
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("manim-render image texture"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let filter = match q.paint.sampler {
            manim_core::display::Sampler::Linear => wgpu::FilterMode::Linear,
            manim_core::display::Sampler::Nearest => wgpu::FilterMode::Nearest,
        };
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("manim-render image sampler"),
            mag_filter: filter,
            min_filter: filter,
            ..Default::default()
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("manim-render image bind group"),
            layout: &self.image_pipeline.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        self.texture_cache.insert(
            q.source,
            CachedTexture {
                generation: q.generation,
                bind_group,
            },
        );
    }

    /// The target size in pixels, `(width, height)`.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Renders a z-ordered list of `FrameOp`s (vector batches interleaved with
    /// textured image quads) under `camera` over `background`, returning the
    /// pixels.
    ///
    /// # Errors
    ///
    /// As [`render`](Self::render).
    pub fn render_ops(
        &mut self,
        ops: &[crate::tessellate::FrameOp],
        camera: &Camera2D,
        background: Color,
    ) -> Result<RgbaImage, RenderError> {
        use crate::tessellate::FrameOp;

        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform::from(camera)),
        );

        // Upload/refresh textures, then evict vanished ones.
        let mut present = Vec::new();
        for op in ops {
            if let FrameOp::Image(q) = op {
                self.ensure_texture(q);
                present.push(q.source);
            }
        }
        self.texture_cache.retain(|id, _| present.contains(id));

        let gpu_ops = self.build_gpu_ops(ops);
        let mut encoder = self.begin_ops_encoder();
        {
            let mut pass = self.begin_ops_pass(&mut encoder, background);
            self.record_ops(&mut pass, &gpu_ops, &self.bind_group);
        }
        self.copy_and_read(encoder)
    }

    /// Renders `ops` with the `main` camera full-frame, then a second time with
    /// the `zoom` camera scissored into the `inset` pixel rectangle (a magnifying
    /// window), and finally a border around the inset — all in one pass.
    ///
    /// The zoom pass reuses the same tessellated `ops`, so the inset shows the
    /// identical scene magnified. The border is drawn in NDC with an identity
    /// matrix (via the shared HUD uniform).
    ///
    /// # Errors
    ///
    /// As [`render`](Self::render).
    #[allow(clippy::too_many_arguments)]
    pub fn render_ops_zoomed(
        &mut self,
        ops: &[crate::tessellate::FrameOp],
        main: &Camera2D,
        zoom: &Camera2D,
        inset: crate::layout::Viewport,
        border_color: Color,
        border_px: f32,
        background: Color,
    ) -> Result<RgbaImage, RenderError> {
        use crate::tessellate::FrameOp;

        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform::from(main)),
        );
        self.queue.write_buffer(
            &self.zoom_uniform_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform::from(zoom)),
        );
        // The border is drawn directly in NDC → identity view-projection.
        self.queue.write_buffer(
            &self.hud_uniform_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            }),
        );

        let mut present = Vec::new();
        for op in ops {
            if let FrameOp::Image(q) = op {
                self.ensure_texture(q);
                present.push(q.source);
            }
        }
        self.texture_cache.retain(|id, _| present.contains(id));

        let gpu_ops = self.build_gpu_ops(ops);
        let border = border_mesh_ndc(inset, self.width, self.height, border_px, border_color);
        let border_gpu = self.build_gpu_ops(&[FrameOp::Vector(border)]);

        // Scissor rectangle in attachment pixels, clamped to bounds.
        let sx = inset.x.max(0.0).round() as u32;
        let sy = inset.y.max(0.0).round() as u32;
        let sw = (inset.w.round() as u32).min(self.width.saturating_sub(sx));
        let sh = (inset.h.round() as u32).min(self.height.saturating_sub(sy));

        let mut encoder = self.begin_ops_encoder();
        {
            let mut pass = self.begin_ops_pass(&mut encoder, background);
            // Main pass, full frame.
            self.record_ops(&mut pass, &gpu_ops, &self.bind_group);
            // Zoom pass, confined to the inset.
            if sw > 0 && sh > 0 {
                pass.set_viewport(inset.x, inset.y, inset.w, inset.h, 0.0, 1.0);
                pass.set_scissor_rect(sx, sy, sw, sh);
                self.record_ops(&mut pass, &gpu_ops, &self.zoom_bind_group);
                // Reset to full frame for the border.
                pass.set_viewport(0.0, 0.0, self.width as f32, self.height as f32, 0.0, 1.0);
                pass.set_scissor_rect(0, 0, self.width, self.height);
                self.record_ops(&mut pass, &border_gpu, &self.hud_bind_group);
            }
        }
        self.copy_and_read(encoder)
    }

    /// Renders a 3-D frame: depth-sorted `world` content with the perspective
    /// camera, then the `hud` (`fixed_in_frame`) overlay with the orthographic
    /// camera, in a single pass over the same target.
    ///
    /// The two camera uniforms (`view_proj` and `ortho_view_proj`) let HUD items
    /// stay flat and unmoving while the world orbits.
    ///
    /// # Errors
    ///
    /// As [`render`](Self::render).
    pub fn render_ops_layered(
        &mut self,
        world: &[crate::tessellate::FrameOp],
        hud: &[crate::tessellate::FrameOp],
        camera: &Camera2D,
        background: Color,
    ) -> Result<RgbaImage, RenderError> {
        use crate::tessellate::FrameOp;

        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj: camera.view_proj().to_cols_array_2d(),
            }),
        );
        self.queue.write_buffer(
            &self.hud_uniform_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj: camera.ortho_view_proj().to_cols_array_2d(),
            }),
        );

        // Textures across both layers, then evict vanished ones.
        let mut present = Vec::new();
        for op in world.iter().chain(hud.iter()) {
            if let FrameOp::Image(q) = op {
                self.ensure_texture(q);
                present.push(q.source);
            }
        }
        self.texture_cache.retain(|id, _| present.contains(id));

        let world_gpu = self.build_gpu_ops(world);
        let hud_gpu = self.build_gpu_ops(hud);
        let mut encoder = self.begin_ops_encoder();
        {
            let mut pass = self.begin_ops_pass(&mut encoder, background);
            self.record_ops(&mut pass, &world_gpu, &self.bind_group);
            self.record_ops(&mut pass, &hud_gpu, &self.hud_bind_group);
        }
        self.copy_and_read(encoder)
    }

    /// Creates the command encoder for an ops render.
    fn begin_ops_encoder(&self) -> wgpu::CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("manim-render ops encoder"),
            })
    }

    /// Begins the clear-and-draw pass into the MSAA target.
    fn begin_ops_pass<'e>(
        &self,
        encoder: &'e mut wgpu::CommandEncoder,
        background: Color,
    ) -> wgpu::RenderPass<'e> {
        let bg = background.premultiplied();
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("manim-render ops pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.msaa_view,
                resolve_target: Some(&self.color_view),
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
        })
    }

    /// Pre-builds GPU vertex/index buffers for each op (they must outlive the
    /// render pass). Empty vector batches are skipped.
    fn build_gpu_ops(&self, ops: &[crate::tessellate::FrameOp]) -> Vec<GpuOp> {
        use crate::tessellate::FrameOp;
        let mut gpu_ops = Vec::new();
        for op in ops {
            match op {
                FrameOp::Vector(mesh) if !mesh.indices.is_empty() => {
                    gpu_ops.push(GpuOp::Vector {
                        vb: self
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("manim-render vertices"),
                                contents: bytemuck::cast_slice(&mesh.vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            }),
                        ib: self
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("manim-render indices"),
                                contents: bytemuck::cast_slice(&mesh.indices),
                                usage: wgpu::BufferUsages::INDEX,
                            }),
                        count: mesh.indices.len() as u32,
                    });
                }
                FrameOp::Vector(_) => {}
                FrameOp::Image(q) => {
                    // Two triangles over corners [TL,TR,BR,BL] with UVs.
                    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
                    let verts: Vec<ImageVertex> = (0..4)
                        .map(|i| ImageVertex {
                            position: q.corners[i],
                            uv: uvs[i],
                        })
                        .collect();
                    let idx: [u32; 6] = [0, 1, 2, 0, 2, 3];
                    gpu_ops.push(GpuOp::Image {
                        vb: self
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("manim-render image vertices"),
                                contents: bytemuck::cast_slice(&verts),
                                usage: wgpu::BufferUsages::VERTEX,
                            }),
                        ib: self
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("manim-render image indices"),
                                contents: bytemuck::cast_slice(&idx),
                                usage: wgpu::BufferUsages::INDEX,
                            }),
                        source: q.source,
                    });
                }
            }
        }
        gpu_ops
    }

    /// Records `gpu_ops` into `pass`, binding `camera_bg` at `@group(0)` for
    /// every draw (the perspective camera for world content, the orthographic
    /// camera for HUD content).
    fn record_ops<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        gpu_ops: &'p [GpuOp],
        camera_bg: &'p wgpu::BindGroup,
    ) {
        for op in gpu_ops {
            match op {
                GpuOp::Vector { vb, ib, count } => {
                    pass.set_pipeline(&self.pipeline);
                    pass.set_bind_group(0, camera_bg, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*count, 0, 0..1);
                }
                GpuOp::Image { vb, ib, source } => {
                    if let Some(tex) = self.texture_cache.get(source) {
                        pass.set_pipeline(&self.image_pipeline.pipeline);
                        pass.set_bind_group(0, camera_bg, &[]);
                        pass.set_bind_group(1, &tex.bind_group, &[]);
                        pass.set_vertex_buffer(0, vb.slice(..));
                        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..6, 0, 0..1);
                    }
                }
            }
        }
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
            None,
        );

        self.copy_and_read(encoder)
    }

    /// Copies the resolved color texture into the readback buffer, submits, waits,
    /// and builds the tightly-packed [`RgbaImage`].
    fn copy_and_read(&self, encoder: wgpu::CommandEncoder) -> Result<RgbaImage, RenderError> {
        let mut encoder = encoder;
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
///
/// Native-only: it constructs its context with the blocking
/// [`GpuContext::new_headless`] and reads pixels back synchronously. On wasm,
/// render through `CanvasSurface` (wasm `web` feature) instead.
#[cfg(not(target_arch = "wasm32"))]
pub struct OffscreenRenderer {
    context: GpuContext,
    cache: TessellationCache,
    target: TextureTarget,
    camera: Camera2D,
    background: Color,
}

#[cfg(not(target_arch = "wasm32"))]
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

    /// Tessellates and renders `list` to an [`RgbaImage`] with the current
    /// camera and background.
    ///
    /// Unchanged mobjects reuse cached tessellation across calls, so animating a
    /// scene only re-tessellates what moved.
    ///
    /// # Errors
    ///
    /// Propagates [`TextureTarget::render`] failures.
    pub fn render_display_list(&mut self, list: &DisplayList) -> Result<RgbaImage, RenderError> {
        if self.camera.is_3d() {
            let frame = self.cache.tessellate_ops_layered(list, &self.camera);
            return self.target.render_ops_layered(
                &frame.world,
                &frame.hud,
                &self.camera,
                self.background,
            );
        }
        let ops = self.cache.tessellate_ops(list);
        self.target.render_ops(&ops, &self.camera, self.background)
    }

    /// Renders a [`Frame`](manim_core::scene::Frame), following its camera.
    ///
    /// Adopts the frame's camera (center/zoom/rotation) and background, adapts
    /// the tessellation tolerance to the camera zoom, and rasterizes. This is
    /// what makes camera-follow (`MovingCameraScene`) render correctly.
    ///
    /// ```no_run
    /// # use manim_core::scene::{Scene, Frame};
    /// # use manim_render::renderer::OffscreenRenderer;
    /// # fn go(scene: &mut Scene, r: &mut OffscreenRenderer) -> Result<(), manim_render::RenderError> {
    /// for frame in scene.frames_with_camera() {
    ///     let _img = r.render_frame(&frame)?;
    /// }
    /// # Ok(()) }
    /// ```
    pub fn render_frame(
        &mut self,
        frame: &manim_core::scene::Frame,
    ) -> Result<RgbaImage, RenderError> {
        self.camera = Camera2D::from(&frame.camera);
        self.background = frame.camera.background;
        self.cache.set_zoom(frame.camera.height);
        if self.camera.is_3d() {
            let layered = self
                .cache
                .tessellate_ops_layered(&frame.display_list, &self.camera);
            return self.target.render_ops_layered(
                &layered.world,
                &layered.hud,
                &self.camera,
                self.background,
            );
        }
        let ops = self.cache.tessellate_ops(&frame.display_list);
        if let Some(zw) = frame.camera.zoom_window {
            let (w, h) = self.target.size();
            let base = crate::layout::Viewport {
                x: 0.0,
                y: 0.0,
                w: w as f32,
                h: h as f32,
            };
            let inset =
                crate::layout::inset_viewport(base, zw.inset_x, zw.inset_y, zw.inset_w, zw.inset_h);
            let (zw_w, zw_h) = crate::layout::zoom_frame_size(zw.region_width, inset.w, inset.h);
            let zoom_cam = Camera2D {
                frame_center: zw.region_center,
                frame_width: zw_w,
                frame_height: zw_h,
                rotation: 0.0,
                three_d: None,
            };
            return self.target.render_ops_zoomed(
                &ops,
                &self.camera,
                &zoom_cam,
                inset,
                zw.border_color,
                zw.border_width,
                self.background,
            );
        }
        self.target.render_ops(&ops, &self.camera, self.background)
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
