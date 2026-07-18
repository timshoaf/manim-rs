//! The per-pixel **material** pipeline: domain coloring, heatmaps, and
//! scalar-field textures (S1, `docs/design/12-scientific-extensions.md`).
//!
//! A [`Material`](manim_core::display::Material) draws a world-space quad (the
//! field's scene-space rectangle) through this pipeline, which samples the field
//! grid in the quad's UVs and shades each pixel — a colormap LUT lookup with
//! optional iso-contours ([`FieldTexture`](manim_core::display::MaterialKind::FieldTexture)
//! / [`Heatmap`](manim_core::display::MaterialKind::Heatmap)), or complex
//! phase→hue domain coloring
//! ([`PhaseHue`](manim_core::display::MaterialKind::PhaseHue)). It mirrors the
//! image pipeline: `@group(0)` shares the camera uniform, `@group(1)` carries the
//! per-material field texture, colormap LUT, sampler, and parameters.
//!
//! Field textures are `R32Float` (scalar) or `Rg32Float` (complex) and sampled
//! with `textureLoad` + manual bilinear, so no float-filtering feature is needed
//! (WebGL2-clean, like the mesh height texture).

use manim_core::display::{Colormap, FieldChannels, Material, MaterialKind, TextureData};
use wgpu::util::DeviceExt;

/// The material shader: a full quad → per-pixel colormap / domain coloring.
const MATERIAL_SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

@group(1) @binding(0) var field: texture_2d<f32>;
@group(1) @binding(1) var lut: texture_2d<f32>;
@group(1) @binding(2) var lut_samp: sampler;
@group(1) @binding(3) var<uniform> p: Params;

struct Params {
    value_range: vec2<f32>,
    opacity: f32,
    kind: u32,
    contour_spacing: f32,
    contour_width: f32,
    modulus_contours: u32,
    grid_w: f32,
    grid_h: f32,
    pad0: f32,
    pad1: f32,
    pad2: f32,
    contour_color: vec4<f32>,
};

struct VsIn { @location(0) pos: vec3<f32>, @location(1) uv: vec2<f32> };
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = camera.view_proj * vec4<f32>(in.pos, 1.0);
    o.uv = in.uv;
    return o;
}

const TAU: f32 = 6.28318530718;

fn load_field(px: i32, py: i32) -> vec2<f32> {
    let w = i32(p.grid_w);
    let h = i32(p.grid_h);
    let x = clamp(px, 0, w - 1);
    let y = clamp(py, 0, h - 1);
    return textureLoad(field, vec2<i32>(x, y), 0).xy;
}

fn sample_field(uv: vec2<f32>) -> vec2<f32> {
    // Manual bilinear over a non-filterable float texture.
    let fx = uv.x * p.grid_w - 0.5;
    let fy = uv.y * p.grid_h - 0.5;
    let x0 = i32(floor(fx));
    let y0 = i32(floor(fy));
    let dx = fx - floor(fx);
    let dy = fy - floor(fy);
    let a = load_field(x0, y0);
    let b = load_field(x0 + 1, y0);
    let c = load_field(x0, y0 + 1);
    let d = load_field(x0 + 1, y0 + 1);
    return mix(mix(a, b, dx), mix(c, d, dx), dy);
}

fn hsv2rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let c = v * s;
    let hp = fract(h) * 6.0;
    let x = c * (1.0 - abs((hp % 2.0) - 1.0));
    var rgb: vec3<f32>;
    if (hp < 1.0) { rgb = vec3<f32>(c, x, 0.0); }
    else if (hp < 2.0) { rgb = vec3<f32>(x, c, 0.0); }
    else if (hp < 3.0) { rgb = vec3<f32>(0.0, c, x); }
    else if (hp < 4.0) { rgb = vec3<f32>(0.0, x, c); }
    else if (hp < 5.0) { rgb = vec3<f32>(x, 0.0, c); }
    else { rgb = vec3<f32>(c, 0.0, x); }
    return rgb + vec3<f32>(v - c);
}

// sRGB (display) → linear light, so the hue colors blend/encode correctly.
fn srgb2lin(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let f = sample_field(in.uv);
    var rgb: vec3<f32>;
    if (p.kind == 1u) {
        // Complex phase-hue domain coloring: hue = arg/2π, brightness from
        // modulus (dark at zeros), optional log-modulus rings.
        let arg = atan2(f.y, f.x);
        let hue = arg / TAU + 0.5;
        let modulus = length(f);
        var val = modulus / (1.0 + modulus);
        if (p.modulus_contours == 1u) {
            let lm = log2(modulus + 1e-9);
            val = 0.5 + 0.5 * fract(lm);
        }
        rgb = srgb2lin(hsv2rgb(hue, 1.0, val));
    } else {
        // Scalar field → colormap LUT (linear after the sRGB LUT is decoded).
        let range = max(p.value_range.y - p.value_range.x, 1e-9);
        let t = clamp((f.x - p.value_range.x) / range, 0.0, 1.0);
        rgb = textureSample(lut, lut_samp, vec2<f32>(t, 0.5)).rgb;
        if (p.kind == 0u && p.contour_spacing > 0.0) {
            // Antialiased iso-contour lines at multiples of `contour_spacing`.
            let d = f.x / p.contour_spacing;
            let dist = min(fract(d), 1.0 - fract(d));
            let aa = max(fwidth(d) * p.contour_width, 1e-6);
            let line = (1.0 - smoothstep(0.0, aa, dist)) * p.contour_color.a;
            rgb = mix(rgb, p.contour_color.rgb, line);
        }
    }
    let a = p.opacity;
    return vec4<f32>(rgb * a, a);
}
"#;

/// A material-quad vertex: world position + field UV.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct MaterialVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// The `@group(1)` uniform block (64 bytes, std140-compatible).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct MaterialParams {
    value_range: [f32; 2],
    opacity: f32,
    kind: u32,
    contour_spacing: f32,
    contour_width: f32,
    modulus_contours: u32,
    grid_w: f32,
    grid_h: f32,
    pad0: f32,
    pad1: f32,
    pad2: f32,
    contour_color: [f32; 4],
}

impl MaterialParams {
    /// Packs a [`Material`]'s parameters for the shader.
    pub(crate) fn from_material(m: &Material) -> Self {
        let (kind, spacing, width, color, modc) = match m.kind {
            MaterialKind::FieldTexture { contours, .. } => {
                let (s, w, c) = contours
                    .map(|c| {
                        (
                            c.spacing,
                            c.width,
                            [c.color.r, c.color.g, c.color.b, c.color.a],
                        )
                    })
                    .unwrap_or((0.0, 0.0, [0.0; 4]));
                (0u32, s, w, c, 0u32)
            }
            MaterialKind::PhaseHue { modulus_contours } => {
                (1u32, 0.0, 0.0, [0.0; 4], modulus_contours as u32)
            }
            MaterialKind::Heatmap { .. } => (2u32, 0.0, 0.0, [0.0; 4], 0u32),
        };
        Self {
            value_range: m.value_range,
            opacity: m.opacity,
            kind,
            contour_spacing: spacing,
            contour_width: width,
            modulus_contours: modc,
            grid_w: m.texture.width.max(1) as f32,
            grid_h: m.texture.height.max(1) as f32,
            pad0: 0.0,
            pad1: 0.0,
            pad2: 0.0,
            contour_color: color,
        }
    }

    /// The colormap whose LUT this material samples (`PhaseHue` computes color and
    /// binds a placeholder).
    pub(crate) fn colormap(m: &Material) -> Colormap {
        match m.kind {
            MaterialKind::FieldTexture { colormap, .. } | MaterialKind::Heatmap { colormap } => {
                colormap
            }
            MaterialKind::PhaseHue { .. } => Colormap::Viridis,
        }
    }
}

/// The material render pipeline plus its `@group(1)` layout and a shared LUT
/// sampler.
pub(crate) struct MaterialPipeline {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) group1_layout: wgpu::BindGroupLayout,
    pub(crate) lut_sampler: wgpu::Sampler,
}

impl MaterialPipeline {
    /// Builds the pipeline against the shared camera layout at `@group(0)`.
    pub(crate) fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("manim-render material shader"),
            source: wgpu::ShaderSource::Wgsl(MATERIAL_SHADER.into()),
        });
        let group1_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("manim-render material group1 layout"),
            entries: &[
                // Field texture: non-filterable float (textureLoad).
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Colormap LUT: filterable sRGB.
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("manim-render material pipeline layout"),
            bind_group_layouts: &[camera_layout, &group1_layout],
            push_constant_ranges: &[],
        });
        const ATTRS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MaterialVertex>() as wgpu::BufferAddress,
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
            label: Some("manim-render material pipeline"),
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
        let lut_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("manim-render material lut sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Self {
            pipeline,
            group1_layout,
            lut_sampler,
        }
    }
}

/// Uploads a field grid as an `R32Float`/`Rg32Float` texture and returns its view.
pub(crate) fn build_field_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    tex: &TextureData,
) -> wgpu::TextureView {
    let (w, h) = (tex.width.max(1), tex.height.max(1));
    let (format, bytes_per_texel) = match tex.channels {
        FieldChannels::R => (wgpu::TextureFormat::R32Float, 4u32),
        FieldChannels::Rg => (wgpu::TextureFormat::Rg32Float, 8u32),
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("manim-render field texture"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
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
        bytemuck::cast_slice(&tex.data),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * bytes_per_texel),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Uploads a colormap's 256×1 sRGB LUT and returns its view.
pub(crate) fn build_lut_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    colormap: Colormap,
) -> wgpu::TextureView {
    let bytes = colormap.lut_rgba8();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("manim-render colormap lut"),
        size: wgpu::Extent3d {
            width: 256,
            height: 1,
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
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256 * 4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 256,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Creates the per-material uniform buffer with `params`.
pub(crate) fn params_buffer(device: &wgpu::Device, params: &MaterialParams) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("manim-render material params"),
        contents: bytemuck::bytes_of(params),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_color::WHITE;
    use manim_core::display::ContourParams;
    use std::sync::Arc;

    fn tex(w: u32, h: u32, ch: FieldChannels) -> Arc<TextureData> {
        let n = (w * h) as usize * ch as usize;
        Arc::new(TextureData {
            width: w,
            height: h,
            channels: ch,
            data: vec![0.0; n],
            center: glam::Vec3::ZERO,
            size: [1.0, 1.0],
        })
    }

    #[test]
    fn params_pack_field_texture_with_contours() {
        let m = Material {
            kind: MaterialKind::FieldTexture {
                colormap: Colormap::Viridis,
                contours: Some(ContourParams {
                    spacing: 0.5,
                    width: 2.0,
                    color: WHITE,
                }),
            },
            texture: tex(64, 32, FieldChannels::R),
            value_range: [-1.0, 3.0],
            opacity: 0.8,
        };
        let p = MaterialParams::from_material(&m);
        assert_eq!(p.kind, 0);
        assert_eq!(p.contour_spacing, 0.5);
        assert_eq!(p.contour_width, 2.0);
        assert_eq!(p.value_range, [-1.0, 3.0]);
        assert_eq!(p.opacity, 0.8);
        assert_eq!((p.grid_w, p.grid_h), (64.0, 32.0));
        assert_eq!(MaterialParams::colormap(&m), Colormap::Viridis);
    }

    #[test]
    fn params_pack_phase_hue_and_heatmap() {
        let ph = Material {
            kind: MaterialKind::PhaseHue {
                modulus_contours: true,
            },
            texture: tex(8, 8, FieldChannels::Rg),
            value_range: [0.0, 1.0],
            opacity: 1.0,
        };
        let p = MaterialParams::from_material(&ph);
        assert_eq!(p.kind, 1);
        assert_eq!(p.modulus_contours, 1);
        assert_eq!(p.contour_spacing, 0.0);

        let hm = Material {
            kind: MaterialKind::Heatmap {
                colormap: Colormap::Magma,
            },
            texture: tex(8, 8, FieldChannels::R),
            value_range: [0.0, 1.0],
            opacity: 1.0,
        };
        assert_eq!(MaterialParams::from_material(&hm).kind, 2);
        assert_eq!(MaterialParams::colormap(&hm), Colormap::Magma);
    }

    #[test]
    fn params_uniform_is_64_bytes() {
        assert_eq!(std::mem::size_of::<MaterialParams>(), 64);
    }
}
