//! The depth-tested, per-pixel-shaded mesh pass: WGSL, pipelines, GPU buffer
//! cache, and the opaque/translucent queue split.
//!
//! This is the second, parallel render path described in
//! `docs/design/12-mesh-pipeline.md`. Where [`tessellate`](crate::tessellate)
//! flattens bezier paths to triangles for the painter's-algorithm vector pass,
//! this module uploads [`TriMesh`](manim_core::mesh::TriMesh) geometry straight
//! to the GPU and draws it with a depth buffer and a Blinn-Phong fragment
//! shader.
//!
//! # Pass structure
//!
//! A frame with meshes runs **mesh pass → vector pass → HUD**. The mesh pass
//! owns the [depth attachment](DEPTH_FORMAT) and clears both color and depth;
//! the vector pass then *loads* the color target and draws over it with no depth
//! test at all (2D content is annotation — see the design doc §2). A frame with
//! **no** meshes skips this module entirely and is byte-identical to the
//! pre-mesh renderer.
//!
//! # One pipeline, two paths
//!
//! Every draw is instanced. A plain [`MeshItem`] is drawn as a single
//! identity-transform, white-tinted instance ([`MeshInstance::IDENTITY`]), so an
//! [`InstancedMesh`](manim_core::mesh::InstancedMesh) needs no second pipeline,
//! shader, or code path — it just supplies a longer instance buffer.
//!
//! # Coordinate and color conventions
//!
//! - Vertex colors arrive **premultiplied linear** (matching
//!   [`Vertex`](crate::tessellate::Vertex)); the vertex shader un-premultiplies
//!   them to a straight albedo, multiplies in the instance tint and the
//!   material's base color, and the fragment shader premultiplies again on
//!   output. Blending is the 2D pipeline's `(One, OneMinusSrcAlpha)`.
//! - Instance tints are **straight linear** — they are a multiplicative tint, so
//!   premultiplying them would double-apply alpha.
//! - Shading happens in linear space; the sRGB target encodes on store.
//!
//! ```
//! use manim_render::mesh_pipeline::{MeshInstance, SceneLight};
//! // The default light is CE's over-the-shoulder key light, normalized.
//! assert!((SceneLight::default().direction.length() - 1.0).abs() < 1e-6);
//! // A plain mesh draws as one identity instance.
//! assert_eq!(MeshInstance::IDENTITY.color, [1.0, 1.0, 1.0, 1.0]);
//! ```

use std::collections::HashMap;

use glam::{Mat3, Mat4, Vec3};
use manim_core::display::MeshItem;
use manim_core::mesh::Shading;
use manim_core::mobject::AnyId;
use wgpu::util::DeviceExt;

use crate::camera::Camera2D;

/// The depth attachment format. `Depth32Float` is core in both WebGPU and
/// WebGL2 (`DEPTH_COMPONENT32F`), so the mesh path stays portable.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The default scene light direction: the unit vector pointing **from the
/// surface toward the light**.
///
/// This is manim CE's `light_source_start = 7·LEFT + 9·DOWN + 10·OUT`
/// (`(-7, -9, 10)`) normalized — the over-the-shoulder key light that sits up
/// and behind the default camera, giving 3D solids CE's familiar shading.
///
/// ```
/// use manim_render::mesh_pipeline::DEFAULT_LIGHT_DIR;
/// // Over the viewer's left shoulder, from above the scene.
/// assert!(DEFAULT_LIGHT_DIR.x < 0.0 && DEFAULT_LIGHT_DIR.y < 0.0 && DEFAULT_LIGHT_DIR.z > 0.0);
/// assert!((DEFAULT_LIGHT_DIR.length() - 1.0).abs() < 1e-6);
/// ```
pub const DEFAULT_LIGHT_DIR: Vec3 = Vec3::new(-0.461_566_33, -0.593_442_43, 0.659_380_5);

/// The default scene ambient level: full strength, so a material's own
/// [`ambient`](manim_core::mesh::MeshMaterial::ambient) coefficient is used
/// as-is.
pub const DEFAULT_AMBIENT: f32 = 1.0;

/// The scene's single directional light.
///
/// The renderer multiplies [`ambient`](Self::ambient) into each material's own
/// ambient coefficient, so this is a scene-wide dimmer over per-material
/// settings. Configurable without shader changes, per
/// `docs/design/12-mesh-pipeline.md` §4.
///
/// ```
/// use manim_render::mesh_pipeline::{SceneLight, DEFAULT_AMBIENT, DEFAULT_LIGHT_DIR};
/// let l = SceneLight::default();
/// assert_eq!(l.direction, DEFAULT_LIGHT_DIR);
/// assert_eq!(l.ambient, DEFAULT_AMBIENT);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneLight {
    /// Unit direction from the shaded surface **toward** the light.
    pub direction: Vec3,
    /// Scene-wide ambient multiplier.
    pub ambient: f32,
}

impl Default for SceneLight {
    fn default() -> Self {
        Self {
            direction: DEFAULT_LIGHT_DIR,
            ambient: DEFAULT_AMBIENT,
        }
    }
}

/// The mesh pass's `@group(0)` uniform block: camera and light.
///
/// `camera_pos` and `light` are `vec4`s rather than `vec3`s because WGSL aligns
/// a `vec3<f32>` to 16 bytes anyway; spelling the padding out keeps the Rust
/// layout obviously identical to the shader's.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshGlobals {
    /// The camera view-projection matrix, column-major.
    pub view_proj: [[f32; 4]; 4],
    /// `xyz` = the camera eye in world space; `w` is padding.
    pub camera_pos: [f32; 4],
    /// `xyz` = the surface→light direction; `w` = the scene ambient level.
    pub light: [f32; 4],
}

impl MeshGlobals {
    /// Packs the uniform for `camera` lit by `light`.
    ///
    /// ```
    /// use manim_core::config::Config;
    /// use manim_render::camera::Camera2D;
    /// use manim_render::mesh_pipeline::{MeshGlobals, SceneLight};
    /// let cam = Camera2D::from(&Config::default());
    /// let g = MeshGlobals::new(&cam, SceneLight::default());
    /// // The ambient level rides in the light vector's w slot.
    /// assert_eq!(g.light[3], SceneLight::default().ambient);
    /// ```
    pub fn new(camera: &Camera2D, light: SceneLight) -> Self {
        let eye = camera.eye_position();
        let dir = light.direction.normalize_or_zero();
        Self {
            // Not `view_proj`: the mesh pass has a depth buffer, and a 2-D
            // camera's plain ortho matrix would clip everything off the z = 0
            // plane. See `Camera2D::mesh_view_proj`.
            view_proj: camera.mesh_view_proj().to_cols_array_2d(),
            camera_pos: [eye.x, eye.y, eye.z, 0.0],
            light: [dir.x, dir.y, dir.z, light.ambient],
        }
    }
}

/// The mesh pass's `@group(1)` uniform block: one mobject's model transform and
/// material.
///
/// Model *and* material share one block because they share a lifetime — both are
/// per-[`MeshItem`], both are tiny, and one buffer plus one bind group per item
/// per frame is the simplest correct way to feed them (a scene has tens of mesh
/// items at most; a 10k-atom molecule is *one*).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshItemUniform {
    /// The mobject's local→world matrix, column-major.
    pub model: [[f32; 4]; 4],
    /// The inverse-transpose of [`model`](Self::model)'s linear part, padded to
    /// a `mat4x4` (a `mat3x3<f32>` in a uniform block pads each column to 16
    /// bytes regardless, so this costs nothing and keeps the layout obvious).
    pub normal_matrix: [[f32; 4]; 4],
    /// The material base color, straight linear RGBA, with
    /// [`opacity`](manim_core::mesh::MeshMaterial::opacity) folded into `a`.
    pub base_color: [f32; 4],
    /// `(ambient, diffuse, specular, shininess)`.
    pub params: [f32; 4],
    /// `x` = 1.0 for [`Shading::Flat`], 0.0 for [`Shading::Smooth`]; `yzw` pad.
    pub flags: [f32; 4],
}

impl MeshItemUniform {
    /// Packs the uniform for `item`.
    ///
    /// ```
    /// use manim_core::mesh::{Mesh, MeshMaterial};
    /// use manim_core::scene_state::SceneState;
    /// use manim_color::BLUE;
    /// use manim_render::mesh_pipeline::MeshItemUniform;
    ///
    /// let mut scene = SceneState::new();
    /// scene.add(Mesh::sphere().with_material(MeshMaterial::new(BLUE).with_opacity(0.5)));
    /// let dl = scene.display_list();
    /// let u = MeshItemUniform::new(&dl.meshes()[0]);
    /// // Opacity is folded into the base color's alpha.
    /// assert!((u.base_color[3] - 0.5).abs() < 1e-6);
    /// ```
    pub fn new(item: &MeshItem) -> Self {
        let m = item.material;
        let normal = Mat3::from_mat4(item.transform).inverse().transpose();
        let base = m.base_color;
        Self {
            model: item.transform.to_cols_array_2d(),
            normal_matrix: Mat4::from_mat3(normal).to_cols_array_2d(),
            base_color: [base.r, base.g, base.b, base.a * m.opacity],
            params: [m.ambient, m.diffuse, m.specular, m.shininess.max(1.0)],
            flags: [
                match m.shading {
                    Shading::Flat => 1.0,
                    Shading::Smooth => 0.0,
                },
                0.0,
                0.0,
                0.0,
            ],
        }
    }
}

/// One mesh vertex: 48 interleaved bytes, matching
/// `docs/design/12-mesh-pipeline.md` §4.
///
/// ```
/// use manim_render::mesh_pipeline::MeshVertex;
/// assert_eq!(std::mem::size_of::<MeshVertex>(), 48);
/// ```
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshVertex {
    /// Mobject-local position.
    pub position: [f32; 3],
    /// Mobject-local unit normal.
    pub normal: [f32; 3],
    /// Per-vertex tint, **premultiplied linear** (white when the
    /// [`TriMesh`](manim_core::mesh::TriMesh) has no colors).
    pub color: [f32; 4],
    /// Texture coordinates (`(0, 0)` when the mesh has no UVs).
    pub uv: [f32; 2],
}

/// One instance: 80 bytes — a `mat4` as four `vec4`s plus a tint, matching
/// `docs/design/12-mesh-pipeline.md` §6.
///
/// ```
/// use manim_render::mesh_pipeline::MeshInstance;
/// assert_eq!(std::mem::size_of::<MeshInstance>(), 80);
/// ```
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshInstance {
    /// The instance's local→mobject matrix, column-major.
    pub model: [[f32; 4]; 4],
    /// The instance tint, **straight linear** RGBA.
    pub color: [f32; 4],
}

impl MeshInstance {
    /// The single instance a non-instanced [`MeshItem`] draws with: no
    /// transform, no tint. This is what lets one pipeline serve both paths.
    pub const IDENTITY: Self = Self {
        model: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        color: [1.0, 1.0, 1.0, 1.0],
    };

    /// Packs a core [`Instance`](manim_core::mesh::Instance).
    pub fn from_core(i: &manim_core::mesh::Instance) -> Self {
        let c = i.color;
        Self {
            model: i.transform.to_cols_array_2d(),
            color: [c.r, c.g, c.b, c.a],
        }
    }
}

/// Builds the [`MeshVertex`] buffer contents for `mesh`.
///
/// Missing per-vertex colors become opaque white and missing UVs become
/// `(0, 0)`, so the shader has no branches. Colors are premultiplied here, once,
/// at upload time.
fn vertices_of(mesh: &manim_core::mesh::TriMesh) -> Vec<MeshVertex> {
    (0..mesh.positions.len())
        .map(|i| {
            let p = mesh.positions[i];
            let n = mesh.normals.get(i).copied().unwrap_or(Vec3::Z);
            let c = mesh
                .colors
                .as_ref()
                .and_then(|cs| cs.get(i))
                .map(|c| c.premultiplied())
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let uv = mesh
                .uvs
                .as_ref()
                .and_then(|us| us.get(i))
                .map(|u| [u.x, u.y])
                .unwrap_or([0.0, 0.0]);
            MeshVertex {
                position: [p.x, p.y, p.z],
                normal: [n.x, n.y, n.z],
                color: c,
                uv,
            }
        })
        .collect()
}

/// The mesh pass's shader: instanced Blinn-Phong in linear space.
///
/// Stored inline as a `&str` to match [`renderer`](crate::renderer)'s `SHADER`
/// and `IMAGE_SHADER`.
const MESH_SHADER: &str = r#"
struct Globals {
    view_proj: mat4x4<f32>,
    // xyz = camera eye (world), w = padding.
    camera_pos: vec4<f32>,
    // xyz = unit direction surface -> light, w = scene ambient level.
    light: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

struct Item {
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    base_color: vec4<f32>,
    // (ambient, diffuse, specular, shininess)
    params: vec4<f32>,
    // x = 1.0 for flat shading
    flags: vec4<f32>,
};
@group(1) @binding(0) var<uniform> item: Item;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Premultiplied linear per-vertex tint.
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
    // Per-instance: mat4 as four vec4 columns, then a straight-linear tint.
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    @location(8) tint: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Straight-linear albedo (rgb) and final alpha (a).
    @location(2) color: vec4<f32>,
};

/// Recovers a straight-alpha color from a premultiplied one.
fn unpremultiply(c: vec4<f32>) -> vec4<f32> {
    if (c.a <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    return vec4<f32>(c.rgb / c.a, c.a);
}

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let instance = mat4x4<f32>(in.m0, in.m1, in.m2, in.m3);
    // Instances are local -> mobject space; the model matrix is mobject -> world.
    let world = item.model * instance * vec4<f32>(in.position, 1.0);

    // The instance's linear part transforms the normal directly rather than via
    // its inverse-transpose: exact for the rigid and axis-aligned scales the
    // instance helpers produce (a cylinder scaled (r, r, l) keeps radial and cap
    // normals), and only approximate under a skewing non-uniform scale.
    let inst3 = mat3x3<f32>(in.m0.xyz, in.m1.xyz, in.m2.xyz);
    let model3 = mat3x3<f32>(
        item.normal_matrix[0].xyz,
        item.normal_matrix[1].xyz,
        item.normal_matrix[2].xyz,
    );

    let vertex_tint = unpremultiply(in.color);
    var out: VsOut;
    out.clip = globals.view_proj * world;
    out.world = world.xyz;
    out.normal = model3 * (inst3 * in.normal);
    out.color = vec4<f32>(
        vertex_tint.rgb * in.tint.rgb * item.base_color.rgb,
        vertex_tint.a * in.tint.a * item.base_color.a,
    );
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let view = normalize(globals.camera_pos.xyz - in.world);

    // Flat shading takes the geometric face normal from screen-space
    // derivatives of the world position, so faceting needs no separate mesh.
    var n: vec3<f32>;
    if (item.flags.x > 0.5) {
        n = normalize(cross(dpdx(in.world), dpdy(in.world)));
    } else {
        n = normalize(in.normal);
    }
    // Two-sided: open surfaces (a saddle, a grid) are lit from whichever side
    // the camera is on. Back faces are not culled, so this is what makes the
    // underside of a Surface3D shade instead of going black.
    if (dot(n, view) < 0.0) {
        n = -n;
    }

    let light_dir = normalize(globals.light.xyz);
    let half_vec = normalize(light_dir + view);
    let n_dot_l = max(dot(n, light_dir), 0.0);
    let n_dot_h = max(dot(n, half_vec), 0.0);

    let ambient = globals.light.w * item.params.x;
    let diffuse = item.params.y * n_dot_l;
    // Gate the highlight on the surface facing the light, so back faces don't
    // pick up a rim of specular they could never receive.
    var specular = 0.0;
    if (n_dot_l > 0.0) {
        specular = item.params.z * pow(n_dot_h, item.params.w);
    }

    let alpha = in.color.a;
    let rgb = (ambient + diffuse) * in.color.rgb + vec3<f32>(specular);
    // Premultiplied out, for the (One, OneMinusSrcAlpha) blend the 2D pipeline
    // also uses; the sRGB target encodes on store.
    return vec4<f32>(rgb * alpha, alpha);
}
"#;

/// The mesh pass's two render pipelines (opaque and translucent) and their
/// bind-group layouts.
///
/// The pipelines differ only in depth-write: opaque writes depth, translucent
/// tests against it read-only (design doc §5). Both use `cull_mode: None` —
/// [`Surface3D`](manim_core::mesh::Surface3D) grids and translucent shells are
/// open geometry whose back faces must draw.
pub struct MeshPipeline {
    /// Depth write + `LessEqual` test: the opaque queue.
    pub opaque: wgpu::RenderPipeline,
    /// Depth test only, `LessEqual`, no write: the translucent queue.
    pub translucent: wgpu::RenderPipeline,
    /// The `@group(0)` layout: [`MeshGlobals`].
    pub globals_layout: wgpu::BindGroupLayout,
    /// The `@group(1)` layout: [`MeshItemUniform`].
    pub item_layout: wgpu::BindGroupLayout,
}

impl MeshPipeline {
    /// Builds the mesh pipelines for a `format` color target at `sample_count`×
    /// MSAA.
    ///
    /// ```no_run
    /// use manim_render::mesh_pipeline::MeshPipeline;
    /// use manim_render::renderer::{GpuContext, SAMPLE_COUNT, TARGET_FORMAT};
    /// let ctx = GpuContext::new_headless().unwrap();
    /// let p = MeshPipeline::new(&ctx.device, TARGET_FORMAT, SAMPLE_COUNT);
    /// let _ = p;
    /// ```
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, sample_count: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("manim-render mesh shader"),
            source: wgpu::ShaderSource::Wgsl(MESH_SHADER.into()),
        });

        let uniform_entry = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("manim-render mesh globals layout"),
            entries: &[uniform_entry(
                0,
                wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            )],
        });
        let item_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("manim-render mesh item layout"),
            entries: &[uniform_entry(
                0,
                wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            )],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("manim-render mesh pipeline layout"),
            bind_group_layouts: &[&globals_layout, &item_layout],
            push_constant_ranges: &[],
        });

        const VERTEX_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x2];
        const INSTANCE_ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
            4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4
        ];
        let buffers = [
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<MeshVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &VERTEX_ATTRS,
            },
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<MeshInstance>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &INSTANCE_ATTRS,
            },
        ];

        // The 2D pipeline's premultiplied blend, verbatim.
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

        let make = |label: &str, depth_write: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &buffers,
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
                    // Open surfaces must show both sides; the fragment shader
                    // flips the normal toward the viewer instead.
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: depth_write,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: sample_count,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            })
        };

        Self {
            opaque: make("manim-render mesh opaque pipeline", true),
            translucent: make("manim-render mesh translucent pipeline", false),
            globals_layout,
            item_layout,
        }
    }

    /// Creates a [`MeshGlobals`] uniform buffer and its `@group(0)` bind group.
    pub fn make_globals(
        &self,
        device: &wgpu::Device,
        label: &str,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: std::mem::size_of::<MeshGlobals>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        (buffer, bind_group)
    }
}

/// Creates the depth attachment view: [`DEPTH_FORMAT`], `width × height`, at
/// `sample_count`× MSAA so it matches the color target it pairs with.
///
/// Call it again on resize; the old texture drops with its view.
///
/// ```no_run
/// use manim_render::mesh_pipeline::create_depth_view;
/// use manim_render::renderer::{GpuContext, SAMPLE_COUNT};
/// let ctx = GpuContext::new_headless().unwrap();
/// let view = create_depth_view(&ctx.device, 854, 480, SAMPLE_COUNT);
/// let _ = view;
/// ```
pub fn create_depth_view(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    sample_count: u32,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("manim-render depth target"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// The local-space centroid a [`MeshItem`] sorts by: the center of its
/// geometry's bounding box.
fn local_centroid(item: &MeshItem) -> Vec3 {
    item.mesh
        .bounds()
        .map(|(lo, hi)| (lo + hi) * 0.5)
        .unwrap_or(Vec3::ZERO)
}

/// One entry in the translucent queue: which item to draw, how far away it is,
/// and (for an instanced item) the order to draw its instances in.
///
/// [`depth`](Self::depth) is camera-space `z`. The view matrix is right-handed
/// and looks down `-z`, so **more negative is farther** and the queue is sorted
/// ascending — the same convention
/// [`tessellate_ops_layered`](crate::tessellate::TessellationCache::tessellate_ops_layered)
/// uses for the vector pass.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslucentDraw {
    /// Index into the display list's mesh channel.
    pub item: usize,
    /// The sort key: camera-space `z` of the item's centroid (the mean of its
    /// instances' centroids, when instanced).
    pub depth: f32,
    /// For an instanced item, its instance indices back-to-front; `None` for a
    /// plain item.
    pub instances: Option<Vec<u32>>,
}

/// A frame's mesh items split into the two queues of `docs/design/12-mesh-pipeline.md` §5.
///
/// ```
/// use manim_core::mesh::{Mesh, MeshMaterial};
/// use manim_core::scene_state::SceneState;
/// use manim_render::camera::Camera2D;
/// use manim_render::mesh_pipeline::MeshQueues;
/// use manim_color::{BLUE, RED};
///
/// let mut scene = SceneState::new();
/// scene.add(Mesh::sphere().with_material(MeshMaterial::new(RED)));
/// scene.add(Mesh::sphere().with_material(MeshMaterial::new(BLUE).with_opacity(0.4)));
/// let dl = scene.display_list();
/// let cam = Camera2D::from(&manim_core::config::Config::default());
/// let q = MeshQueues::split(dl.meshes(), &cam.view_matrix());
/// assert_eq!(q.opaque, vec![0]);
/// assert_eq!(q.translucent.len(), 1);
/// assert_eq!(q.translucent[0].item, 1);
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshQueues {
    /// Opaque item indices, in display-list order (no sort needed — the depth
    /// buffer resolves them).
    pub opaque: Vec<usize>,
    /// Translucent items, farthest first.
    pub translucent: Vec<TranslucentDraw>,
}

impl MeshQueues {
    /// Splits `meshes` on [`MeshItem::is_translucent`] and sorts the translucent
    /// half back-to-front under `view`
    /// ([`Camera2D::view_matrix`](crate::camera::Camera2D::view_matrix)).
    ///
    /// An instanced *translucent* item also gets its instances ordered
    /// back-to-front, at the cost of re-uploading that item's instance buffer
    /// each frame (opaque instanced items keep their cached buffer, which is why
    /// a 10k-atom molecule pays nothing for this). Per-item sorting cannot fix
    /// self-intersecting translucent geometry; weighted-blended OIT is the
    /// recorded upgrade path (design doc §5).
    pub fn split(meshes: &[MeshItem], view: &Mat4) -> Self {
        let mut opaque = Vec::new();
        let mut translucent: Vec<TranslucentDraw> = Vec::new();

        for (i, item) in meshes.iter().enumerate() {
            if !item.is_translucent() {
                opaque.push(i);
                continue;
            }
            let centroid = local_centroid(item);
            let to_view = *view * item.transform;
            match item.instances.as_ref() {
                Some(instances) if !instances.is_empty() => {
                    let mut keyed: Vec<(f32, u32)> = instances
                        .iter()
                        .enumerate()
                        .map(|(k, inst)| {
                            let p = (to_view * inst.transform).transform_point3(centroid);
                            (p.z, k as u32)
                        })
                        .collect();
                    let mean = keyed.iter().map(|(z, _)| *z).sum::<f32>() / keyed.len() as f32;
                    keyed.sort_by(|a, b| {
                        a.0.partial_cmp(&b.0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then(a.1.cmp(&b.1))
                    });
                    translucent.push(TranslucentDraw {
                        item: i,
                        depth: mean,
                        instances: Some(keyed.into_iter().map(|(_, k)| k).collect()),
                    });
                }
                _ => translucent.push(TranslucentDraw {
                    item: i,
                    depth: to_view.transform_point3(centroid).z,
                    instances: None,
                }),
            }
        }

        // Ascending camera-space z draws the farthest first; ties keep
        // display-list order so the split is deterministic frame to frame.
        translucent.sort_by(|a, b| {
            a.depth
                .partial_cmp(&b.depth)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.item.cmp(&b.item))
        });

        Self {
            opaque,
            translucent,
        }
    }

    /// Whether there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.opaque.is_empty() && self.translucent.is_empty()
    }
}

/// One mobject's uploaded GPU buffers, keyed by its geometry generation.
struct GpuMesh {
    generation: u64,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    n_indices: u32,
    /// The instance buffer in display-list order. Also present for plain items,
    /// holding the single [`MeshInstance::IDENTITY`].
    instances: wgpu::Buffer,
    n_instances: u32,
}

/// Memoizes each mesh mobject's GPU buffers, keyed on `(source, generation)`.
///
/// This is [`TessellationCache`](crate::tessellate::TessellationCache)'s
/// counterpart for the mesh pass, with the same eviction policy: a mobject whose
/// generation is unchanged reuses its uploaded buffers, and entries for mobjects
/// that vanish from the display list are dropped. Static geometry therefore
/// uploads exactly once — the per-frame CPU cost of a still 10k-atom molecule is
/// zero.
#[derive(Default)]
pub struct MeshBufferCache {
    entries: HashMap<AnyId, GpuMesh>,
    hits: u64,
    misses: u64,
}

impl MeshBufferCache {
    /// An empty cache.
    ///
    /// ```
    /// use manim_render::mesh_pipeline::MeshBufferCache;
    /// assert_eq!(MeshBufferCache::new().len(), 0);
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of cached mobjects.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is cached.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Uploads for cache hits.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Uploads for cache misses.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Uploads any mesh whose `(source, generation)` is new, then evicts
    /// mobjects absent from `meshes`.
    fn refresh(&mut self, device: &wgpu::Device, meshes: &[MeshItem]) {
        let mut present: Vec<AnyId> = Vec::with_capacity(meshes.len());
        for item in meshes {
            present.push(item.source);
            let fresh = self
                .entries
                .get(&item.source)
                .is_some_and(|e| e.generation == item.generation);
            if fresh {
                self.hits += 1;
                continue;
            }
            self.misses += 1;

            let verts = vertices_of(&item.mesh);
            let instances: Vec<MeshInstance> = match item.instances.as_ref() {
                Some(xs) if !xs.is_empty() => xs.iter().map(MeshInstance::from_core).collect(),
                // One identity instance keeps plain meshes on the instanced path.
                _ => vec![MeshInstance::IDENTITY],
            };
            let entry = GpuMesh {
                generation: item.generation,
                vbuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("manim-render mesh vertices"),
                    contents: bytemuck::cast_slice(&verts),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                ibuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("manim-render mesh indices"),
                    contents: bytemuck::cast_slice(&item.mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                n_indices: item.mesh.indices.len() as u32,
                instances: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("manim-render mesh instances"),
                    contents: bytemuck::cast_slice(&instances),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                n_instances: instances.len() as u32,
            };
            self.entries.insert(item.source, entry);
        }
        self.entries.retain(|id, _| present.contains(id));
    }
}

/// One recorded mesh draw: its `@group(1)` bind group plus the buffers to bind.
///
/// wgpu buffer handles are reference-counted, so cloning them out of the cache
/// costs nothing and lets the draws outlive the borrow.
struct MeshDraw {
    bind_group: wgpu::BindGroup,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    instances: wgpu::Buffer,
    n_indices: u32,
    n_instances: u32,
}

/// A frame's mesh draws, prepared and ready to record: the opaque ones first,
/// then the translucent ones farthest-first.
///
/// Build one with [`MeshBufferCache::prepare`], record it with
/// [`MeshFrame::record`].
#[derive(Default)]
pub struct MeshFrame {
    opaque: Vec<MeshDraw>,
    translucent: Vec<MeshDraw>,
}

impl MeshFrame {
    /// Whether the frame draws no meshes — the signal to skip the mesh pass
    /// entirely and leave the frame byte-identical to a mesh-less renderer.
    pub fn is_empty(&self) -> bool {
        self.opaque.is_empty() && self.translucent.is_empty()
    }

    /// Records the mesh draws into `pass`: the opaque queue (depth write+test)
    /// then the translucent queue (depth test only, back-to-front).
    ///
    /// `globals` must bind [`MeshGlobals`] at `@group(0)`; the pass must have a
    /// [`DEPTH_FORMAT`] depth attachment.
    pub fn record<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        pipeline: &'p MeshPipeline,
        globals: &'p wgpu::BindGroup,
    ) {
        for (queue, rp) in [
            (&self.opaque, &pipeline.opaque),
            (&self.translucent, &pipeline.translucent),
        ] {
            if queue.is_empty() {
                continue;
            }
            pass.set_pipeline(rp);
            pass.set_bind_group(0, globals, &[]);
            for d in queue {
                pass.set_bind_group(1, &d.bind_group, &[]);
                pass.set_vertex_buffer(0, d.vbuf.slice(..));
                pass.set_vertex_buffer(1, d.instances.slice(..));
                pass.set_index_buffer(d.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..d.n_indices, 0, 0..d.n_instances);
            }
        }
    }
}

impl MeshBufferCache {
    /// Uploads what changed, splits the queues under `camera`, and returns the
    /// frame's draws.
    ///
    /// Cached buffers are reused for anything whose generation is unchanged. The
    /// one thing rebuilt per frame is a *translucent instanced* item's instance
    /// buffer, which must follow the camera's back-to-front order.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        pipeline: &MeshPipeline,
        meshes: &[MeshItem],
        camera: &Camera2D,
    ) -> MeshFrame {
        if meshes.is_empty() {
            self.entries.clear();
            return MeshFrame::default();
        }
        self.refresh(device, meshes);
        let queues = MeshQueues::split(meshes, &camera.view_matrix());

        let make_item_bind_group = |item: &MeshItem| {
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("manim-render mesh item uniform"),
                contents: bytemuck::bytes_of(&MeshItemUniform::new(item)),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("manim-render mesh item bind group"),
                layout: &pipeline.item_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            })
        };

        let mut frame = MeshFrame::default();
        for &i in &queues.opaque {
            let item = &meshes[i];
            let Some(entry) = self.entries.get(&item.source) else {
                continue;
            };
            if entry.n_indices == 0 {
                continue;
            }
            frame.opaque.push(MeshDraw {
                bind_group: make_item_bind_group(item),
                vbuf: entry.vbuf.clone(),
                ibuf: entry.ibuf.clone(),
                instances: entry.instances.clone(),
                n_indices: entry.n_indices,
                n_instances: entry.n_instances,
            });
        }
        for draw in &queues.translucent {
            let item = &meshes[draw.item];
            let Some(entry) = self.entries.get(&item.source) else {
                continue;
            };
            if entry.n_indices == 0 {
                continue;
            }
            // Only a sorted instanced item needs a fresh buffer; everything else
            // reuses the cached one.
            let instances = match (&draw.instances, item.instances.as_ref()) {
                (Some(order), Some(src)) if src.len() > 1 => {
                    let sorted: Vec<MeshInstance> = order
                        .iter()
                        .filter_map(|&k| src.get(k as usize))
                        .map(MeshInstance::from_core)
                        .collect();
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("manim-render mesh sorted instances"),
                        contents: bytemuck::cast_slice(&sorted),
                        usage: wgpu::BufferUsages::VERTEX,
                    })
                }
                _ => entry.instances.clone(),
            };
            frame.translucent.push(MeshDraw {
                bind_group: make_item_bind_group(item),
                vbuf: entry.vbuf.clone(),
                ibuf: entry.ibuf.clone(),
                instances,
                n_indices: entry.n_indices,
                n_instances: entry.n_instances,
            });
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use manim_color::{BLUE, RED, WHITE};
    use manim_core::config::Config;
    use manim_core::mesh::{Instance, InstancedMesh, Mesh, MeshMaterial, Shading, TriMesh};
    use manim_core::scene_state::SceneState;

    fn camera() -> Camera2D {
        let mut cam = Camera2D::from(&Config::default());
        cam.three_d = Some(manim_core::camera::ThreeDParams::default());
        cam
    }

    #[test]
    fn default_light_is_ces_key_light_normalized() {
        // The constant is written out longhand, so pin it to what it claims to
        // be: CE's `light_source_start = 7·LEFT + 9·DOWN + 10·OUT`.
        let expected = Vec3::new(-7.0, -9.0, 10.0).normalize();
        assert!(
            (DEFAULT_LIGHT_DIR - expected).length() < 1e-6,
            "{DEFAULT_LIGHT_DIR} vs {expected}"
        );
        assert!((DEFAULT_LIGHT_DIR.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn vertex_and_instance_layouts_match_the_design_doc() {
        assert_eq!(std::mem::size_of::<MeshVertex>(), 48);
        assert_eq!(std::mem::size_of::<MeshInstance>(), 80);
    }

    #[test]
    fn uniform_blocks_are_std140_sized() {
        // mat4 (64) + vec4 (16) + vec4 (16).
        assert_eq!(std::mem::size_of::<MeshGlobals>(), 96);
        // mat4 + mat4 + 3 × vec4.
        assert_eq!(std::mem::size_of::<MeshItemUniform>(), 176);
        // Uniform blocks must be 16-byte multiples.
        assert_eq!(std::mem::size_of::<MeshGlobals>() % 16, 0);
        assert_eq!(std::mem::size_of::<MeshItemUniform>() % 16, 0);
    }

    #[test]
    fn globals_pack_camera_light_and_ambient() {
        let cam = camera();
        let light = SceneLight {
            direction: Vec3::new(0.0, 0.0, 2.0),
            ambient: 0.25,
        };
        let g = MeshGlobals::new(&cam, light);
        assert_eq!(g.view_proj, cam.mesh_view_proj().to_cols_array_2d());
        assert_eq!(g.camera_pos[0..3], cam.eye_position().to_array());
        // The direction is normalized on the way in; ambient rides in w.
        assert_eq!(g.light, [0.0, 0.0, 1.0, 0.25]);
    }

    #[test]
    fn item_uniform_folds_opacity_and_flags_shading() {
        let mut scene = SceneState::new();
        scene.add(
            Mesh::sphere().with_material(
                MeshMaterial::new(BLUE)
                    .with_opacity(0.25)
                    .with_shading(Shading::Flat)
                    .with_lighting(0.1, 0.8, 0.4)
                    .with_shininess(16.0),
            ),
        );
        let dl = scene.display_list();
        let u = MeshItemUniform::new(&dl.meshes()[0]);
        assert_eq!(u.base_color, [BLUE.r, BLUE.g, BLUE.b, BLUE.a * 0.25]);
        assert_eq!(u.params, [0.1, 0.8, 0.4, 16.0]);
        assert_eq!(u.flags[0], 1.0);

        let mut smooth = SceneState::new();
        smooth.add(Mesh::sphere());
        assert_eq!(
            MeshItemUniform::new(&smooth.display_list().meshes()[0]).flags[0],
            0.0
        );
    }

    #[test]
    fn item_uniform_normal_matrix_is_the_inverse_transpose() {
        let mut scene = SceneState::new();
        // A non-uniform scale is where the inverse-transpose actually matters.
        scene.add(Mesh::sphere().with_transform(Mat4::from_scale(Vec3::new(2.0, 1.0, 1.0))));
        let dl = scene.display_list();
        let u = MeshItemUniform::new(&dl.meshes()[0]);
        let n = Mat4::from_cols_array_2d(&u.normal_matrix);
        // Scaling x by 2 squashes the x-normal by 2.
        let transformed = Mat3::from_mat4(n) * Vec3::X;
        assert!((transformed.x - 0.5).abs() < 1e-5, "got {transformed}");
    }

    #[test]
    fn shininess_never_reaches_pow_zero() {
        let mut scene = SceneState::new();
        scene.add(Mesh::sphere().with_material(MeshMaterial::new(RED).with_shininess(0.0)));
        let dl = scene.display_list();
        // pow(x, 0) is 1 everywhere — a full-strength highlight over the whole
        // surface. Clamp to 1 so a zeroed material degrades gracefully.
        assert_eq!(MeshItemUniform::new(&dl.meshes()[0]).params[3], 1.0);
    }

    #[test]
    fn queues_split_on_translucency() {
        let mut scene = SceneState::new();
        scene.add(Mesh::sphere().with_material(MeshMaterial::new(RED)));
        scene.add(Mesh::sphere().with_material(MeshMaterial::new(BLUE).with_opacity(0.4)));
        scene.add(Mesh::sphere().with_material(MeshMaterial::new(WHITE)));
        let dl = scene.display_list();
        let q = MeshQueues::split(dl.meshes(), &camera().view_matrix());
        assert_eq!(q.opaque, vec![0, 2]);
        assert_eq!(q.translucent.len(), 1);
        assert_eq!(q.translucent[0].item, 1);
        assert!(q.translucent[0].instances.is_none());
    }

    #[test]
    fn per_vertex_alpha_puts_an_opaque_material_in_the_translucent_queue() {
        let mut mesh = TriMesh::grid(2, 2);
        let n = mesh.len();
        mesh.set_colors(Some(vec![RED.with_opacity(0.5); n]))
            .unwrap();
        let mut scene = SceneState::new();
        scene.add(Mesh::new(mesh));
        let dl = scene.display_list();
        let q = MeshQueues::split(dl.meshes(), &camera().view_matrix());
        assert!(q.opaque.is_empty());
        assert_eq!(q.translucent.len(), 1);
    }

    #[test]
    fn translucent_queue_sorts_farthest_first() {
        // The default 3-D camera sits on +z looking at the origin, so a smaller
        // z is farther away and must draw first.
        let mut scene = SceneState::new();
        let near = MeshMaterial::new(RED).with_opacity(0.5);
        scene.add(
            Mesh::sphere()
                .with_material(near)
                .with_transform(Mat4::from_translation(Vec3::Z * 3.0)),
        );
        scene.add(
            Mesh::sphere()
                .with_material(near)
                .with_transform(Mat4::from_translation(Vec3::Z * -3.0)),
        );
        scene.add(Mesh::sphere().with_material(near));

        let dl = scene.display_list();
        let q = MeshQueues::split(dl.meshes(), &camera().view_matrix());
        // Farthest (z = -3) first, then the origin, then nearest (z = +3).
        assert_eq!(
            q.translucent.iter().map(|d| d.item).collect::<Vec<_>>(),
            vec![1, 2, 0]
        );
        // And the keys really do ascend.
        assert!(q.translucent[0].depth < q.translucent[1].depth);
        assert!(q.translucent[1].depth < q.translucent[2].depth);
    }

    #[test]
    fn translucent_instances_sort_within_an_item() {
        let mut scene = SceneState::new();
        // Three atoms strung along the view axis, nearest first in the source.
        let cloud = InstancedMesh::new(
            TriMesh::uv_sphere(6, 8),
            vec![
                Instance::new(Mat4::from_translation(Vec3::Z * 2.0), RED),
                Instance::new(Mat4::from_translation(Vec3::Z * -2.0), RED),
                Instance::new(Mat4::IDENTITY, RED),
            ],
        )
        .with_material(MeshMaterial::new(WHITE).with_opacity(0.5));
        scene.add(cloud);

        let dl = scene.display_list();
        let q = MeshQueues::split(dl.meshes(), &camera().view_matrix());
        assert_eq!(q.translucent.len(), 1);
        // Farthest instance (z = -2) draws first.
        assert_eq!(
            q.translucent[0].instances.as_deref(),
            Some(&[1u32, 2, 0][..])
        );
    }

    #[test]
    fn opaque_instanced_items_are_not_instance_sorted() {
        let mut scene = SceneState::new();
        scene.add(InstancedMesh::spheres(&[Vec3::ZERO, Vec3::Z * 2.0], 0.3));
        let dl = scene.display_list();
        let q = MeshQueues::split(dl.meshes(), &camera().view_matrix());
        // Opaque: the depth buffer sorts it, so the cached instance buffer stands.
        assert_eq!(q.opaque, vec![0]);
        assert!(q.translucent.is_empty());
    }

    #[test]
    fn split_is_stable_for_coincident_translucent_items() {
        let mut scene = SceneState::new();
        let m = MeshMaterial::new(RED).with_opacity(0.5);
        for _ in 0..4 {
            scene.add(Mesh::sphere().with_material(m));
        }
        let dl = scene.display_list();
        let q = MeshQueues::split(dl.meshes(), &camera().view_matrix());
        // Equal depths keep display-list order, so the frame is deterministic.
        assert_eq!(
            q.translucent.iter().map(|d| d.item).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn empty_display_list_yields_empty_queues() {
        let q = MeshQueues::split(&[], &camera().view_matrix());
        assert!(q.is_empty());
    }

    #[test]
    fn vertices_default_missing_colors_and_uvs() {
        // A bare triangle: no per-vertex colors, no UVs. The shader has no
        // branches for either, so the upload has to supply the defaults.
        let mesh = TriMesh {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            normals: vec![Vec3::Z; 3],
            colors: None,
            uvs: None,
            indices: vec![0, 1, 2],
        };
        let vs = vertices_of(&mesh);
        assert_eq!(vs.len(), 3);
        assert!(vs.iter().all(|v| v.color == [1.0, 1.0, 1.0, 1.0]));
        assert!(vs.iter().all(|v| v.uv == [0.0, 0.0]));
    }

    #[test]
    fn vertex_colors_upload_premultiplied() {
        let mut mesh = TriMesh::grid(1, 1);
        let n = mesh.len();
        mesh.set_colors(Some(vec![WHITE.with_opacity(0.5); n]))
            .unwrap();
        let vs = vertices_of(&mesh);
        assert_eq!(vs[0].color, WHITE.with_opacity(0.5).premultiplied());
    }
}
