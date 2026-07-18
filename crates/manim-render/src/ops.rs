//! [`OpsRenderer`]: the shared per-pixel-quad draw path (image + material
//! `FrameOp`s), embedded by both the offscreen [`TextureTarget`](crate::renderer::TextureTarget)
//! and the browser [`CanvasSurface`](crate::canvas::CanvasSurface).
//!
//! Vector batches are drawn with the caller's vector pipeline; image quads and
//! material quads each have their own pipeline + `@group(1)` bind group here, with
//! per-`(arena, source)` caches. Pulling this out of `TextureTarget` is what lets
//! the canvas render the exact same z-ordered `FrameOp` stream as offscreen — so
//! everything renderable offscreen renders in the browser.
//!
//! It owns no device/queue: every method takes them, so the caller keeps a single
//! device (crucial for the shared-device page layout — see
//! [`SharedGpu`](crate::renderer::SharedGpu)).

use std::collections::HashMap;

use manim_core::display::{Colormap, Sampler};
use manim_core::mobject::AnyId;
use wgpu::util::DeviceExt;

use crate::material::{
    build_field_texture, build_lut_texture, params_buffer, MaterialParams, MaterialPipeline,
    MaterialVertex,
};
use crate::tessellate::{FrameOp, ImageQuad, MaterialQuad};

/// The textured-quad shader for images.
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
/// `@group(0)` with the vector pipeline and adding a texture+sampler layout at
/// `@group(1)`.
struct ImagePipeline {
    pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
}

impl ImagePipeline {
    fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        sample_count: u32,
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
                count: sample_count,
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

/// A cached uploaded image texture, keyed on `(arena, source)`, validated by the
/// source's generation.
struct CachedTexture {
    generation: u64,
    bind_group: wgpu::BindGroup,
}

/// A cached material bind group: its params uniform and the `@group(1)` bind
/// group (which keeps the uploaded field texture alive), validated by generation.
struct CachedMaterial {
    generation: u64,
    params_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// A GPU-resident quad op: uploaded vertex/index buffers for one image or
/// material quad, keyed to a cached bind group by `(arena, source)`.
enum GpuOp {
    Vector {
        vb: wgpu::Buffer,
        ib: wgpu::Buffer,
        count: u32,
    },
    Image {
        vb: wgpu::Buffer,
        ib: wgpu::Buffer,
        texture: (u64, AnyId),
    },
    Material {
        vb: wgpu::Buffer,
        ib: wgpu::Buffer,
        material: (u64, AnyId),
    },
}

/// The shared image + material draw path: two pipelines and their per-source
/// caches. Both render targets embed one; see the [module docs](self).
pub(crate) struct OpsRenderer {
    image_pipeline: ImagePipeline,
    material_pipeline: MaterialPipeline,
    texture_cache: HashMap<(u64, AnyId), CachedTexture>,
    material_cache: HashMap<(u64, AnyId), CachedMaterial>,
    lut_cache: HashMap<Colormap, wgpu::TextureView>,
}

/// A built list of GPU ops for one frame; opaque — pass it back to
/// [`OpsRenderer::record`].
pub(crate) struct GpuOps(Vec<GpuOp>);

impl OpsRenderer {
    /// Builds the image + material pipelines against the shared camera layout at
    /// `@group(0)`.
    pub(crate) fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        Self {
            image_pipeline: ImagePipeline::new(device, camera_layout, format, sample_count),
            material_pipeline: MaterialPipeline::new(device, camera_layout, format, sample_count),
            texture_cache: HashMap::new(),
            material_cache: HashMap::new(),
            lut_cache: HashMap::new(),
        }
    }

    /// Uploads/refreshes every image texture and material field for `ops`, then
    /// evicts cached entries whose source vanished from the frame.
    pub(crate) fn prepare<'a>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        arena: u64,
        ops: impl Iterator<Item = &'a FrameOp>,
    ) {
        let mut images = Vec::new();
        let mut materials = Vec::new();
        for op in ops {
            match op {
                FrameOp::Image(q) => {
                    self.ensure_texture(device, queue, arena, q);
                    images.push((arena, q.source));
                }
                FrameOp::Material(q) => {
                    self.ensure_material(device, queue, arena, q);
                    materials.push((arena, q.source));
                }
                _ => {}
            }
        }
        self.texture_cache.retain(|key, _| images.contains(key));
        self.material_cache.retain(|key, _| materials.contains(key));
    }

    /// Ensures the texture for image quad `q` is uploaded and cached.
    fn ensure_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        arena: u64,
        q: &ImageQuad,
    ) {
        if let Some(c) = self.texture_cache.get(&(arena, q.source)) {
            if c.generation == q.generation {
                return;
            }
        }
        let data = &q.paint.data;
        let (w, h) = (data.width.max(1), data.height.max(1));
        let texture = device.create_texture(&wgpu::TextureDescriptor {
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
        queue.write_texture(
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
            Sampler::Linear => wgpu::FilterMode::Linear,
            Sampler::Nearest => wgpu::FilterMode::Nearest,
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("manim-render image sampler"),
            mag_filter: filter,
            min_filter: filter,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
            (arena, q.source),
            CachedTexture {
                generation: q.generation,
                bind_group,
            },
        );
    }

    /// Ensures colormap `cm`'s LUT texture is uploaded (there are only four).
    fn ensure_lut(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, cm: Colormap) {
        self.lut_cache
            .entry(cm)
            .or_insert_with(|| build_lut_texture(device, queue, cm));
    }

    /// Ensures material `q`'s field texture, params uniform, and bind group are
    /// built and cached; a param-only change rewrites the uniform in place.
    fn ensure_material(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        arena: u64,
        q: &MaterialQuad,
    ) {
        let key = (arena, q.source);
        let params = MaterialParams::from_material(&q.material);
        if let Some(c) = self.material_cache.get(&key) {
            if c.generation == q.generation {
                queue.write_buffer(&c.params_buffer, 0, bytemuck::bytes_of(&params));
                return;
            }
        }
        let cm = MaterialParams::colormap(&q.material);
        self.ensure_lut(device, queue, cm);
        let field_view = build_field_texture(device, queue, &q.material.texture);
        let params_buffer = params_buffer(device, &params);
        let lut_view = self.lut_cache.get(&cm).expect("LUT ensured above");
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("manim-render material bind group"),
            layout: &self.material_pipeline.group1_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&field_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(lut_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.material_pipeline.lut_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        });
        self.material_cache.insert(
            key,
            CachedMaterial {
                generation: q.generation,
                params_buffer,
                bind_group,
            },
        );
    }

    /// Pre-builds GPU vertex/index buffers for each op (they must outlive the
    /// render pass). `VectorZ` batches are drawn by the caller's z-test pass, so
    /// they are skipped here.
    pub(crate) fn build_ops(
        &self,
        device: &wgpu::Device,
        arena: u64,
        ops: &[FrameOp],
    ) -> GpuOps {
        let mut gpu_ops = Vec::new();
        for op in ops {
            match op {
                FrameOp::VectorZ(_) => {}
                FrameOp::Vector(mesh) if !mesh.indices.is_empty() => {
                    gpu_ops.push(GpuOp::Vector {
                        vb: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("manim-render vertices"),
                            contents: bytemuck::cast_slice(&mesh.vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                        ib: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("manim-render indices"),
                            contents: bytemuck::cast_slice(&mesh.indices),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                        count: mesh.indices.len() as u32,
                    });
                }
                FrameOp::Vector(_) => {}
                FrameOp::Image(q) => {
                    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
                    let verts: Vec<ImageVertex> = (0..4)
                        .map(|i| ImageVertex {
                            position: q.corners[i],
                            uv: uvs[i],
                        })
                        .collect();
                    let idx: [u32; 6] = [0, 1, 2, 0, 2, 3];
                    gpu_ops.push(GpuOp::Image {
                        vb: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("manim-render image vertices"),
                            contents: bytemuck::cast_slice(&verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                        ib: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("manim-render image indices"),
                            contents: bytemuck::cast_slice(&idx),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                        texture: (arena, q.source),
                    });
                }
                FrameOp::Material(q) => {
                    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
                    let verts: Vec<MaterialVertex> = (0..4)
                        .map(|i| MaterialVertex {
                            position: q.corners[i],
                            uv: uvs[i],
                        })
                        .collect();
                    let idx: [u32; 6] = [0, 1, 2, 0, 2, 3];
                    gpu_ops.push(GpuOp::Material {
                        vb: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("manim-render material vertices"),
                            contents: bytemuck::cast_slice(&verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                        ib: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("manim-render material indices"),
                            contents: bytemuck::cast_slice(&idx),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                        material: (arena, q.source),
                    });
                }
            }
        }
        GpuOps(gpu_ops)
    }

    /// Records `gpu_ops` into `pass`, binding `camera_bg` at `@group(0)`; vector
    /// batches use `vector_pipeline` (the caller's), images/materials use the
    /// pipelines owned here.
    pub(crate) fn record<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        gpu_ops: &'p GpuOps,
        camera_bg: &'p wgpu::BindGroup,
        vector_pipeline: &'p wgpu::RenderPipeline,
    ) {
        for op in &gpu_ops.0 {
            match op {
                GpuOp::Vector { vb, ib, count } => {
                    pass.set_pipeline(vector_pipeline);
                    pass.set_bind_group(0, camera_bg, &[]);
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*count, 0, 0..1);
                }
                GpuOp::Image { vb, ib, texture } => {
                    if let Some(tex) = self.texture_cache.get(texture) {
                        pass.set_pipeline(&self.image_pipeline.pipeline);
                        pass.set_bind_group(0, camera_bg, &[]);
                        pass.set_bind_group(1, &tex.bind_group, &[]);
                        pass.set_vertex_buffer(0, vb.slice(..));
                        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..6, 0, 0..1);
                    }
                }
                GpuOp::Material { vb, ib, material } => {
                    if let Some(mat) = self.material_cache.get(material) {
                        pass.set_pipeline(&self.material_pipeline.pipeline);
                        pass.set_bind_group(0, camera_bg, &[]);
                        pass.set_bind_group(1, &mat.bind_group, &[]);
                        pass.set_vertex_buffer(0, vb.slice(..));
                        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..6, 0, 0..1);
                    }
                }
            }
        }
    }
}
